use std::sync::Arc;

use crate::{
    classlike_info::ClassConstantType,
    code_location::FilePath,
    codebase_info::CodebaseInfo,
    data_flow::{
        graph::DataFlowGraph,
        node::{DataFlowNode, DataFlowNodeId, DataFlowNodeKind},
        path::{ArrayDataKind, PathKind},
    },
    functionlike_info::FunctionLikeInfo,
    functionlike_parameter::FnParameter,
    t_atomic::{DictKey, TAtomic, TClosure, TDict, TVec},
    t_union::TUnion,
    ttype::intersect_union_types_simple,
    type_definition_info::TypeDefinitionInfo,
};
use crate::{functionlike_identifier::FunctionLikeIdentifier, method_identifier::MethodIdentifier};
use hakana_str::{Interner, StrId};
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ttype::{extend_dataflow_uniquely, get_nothing, template, type_combiner, wrap_atomic};

#[derive(Debug)]
pub enum StaticClassType<'a, 'b> {
    None,
    Name(&'a StrId),
    Object(&'b TAtomic),
}

#[derive(Debug)]
pub struct TypeExpansionOptions<'a> {
    pub self_class: Option<&'a StrId>,
    pub static_class_type: StaticClassType<'a, 'a>,
    pub parent_class: Option<&'a StrId>,

    pub evaluate_class_constants: bool,
    pub evaluate_conditional_types: bool,
    pub function_is_final: bool,
    pub expand_generic: bool,
    pub expand_templates: bool,
    pub expand_hakana_types: bool,
    pub force_alias_expansion: bool,
    pub expand_type_aliases: bool,
    pub where_constraints: Option<&'a Vec<(StrId, TUnion)>>,
}

impl Default for TypeExpansionOptions<'_> {
    fn default() -> Self {
        Self {
            self_class: None,
            static_class_type: StaticClassType::None,
            parent_class: None,
            evaluate_class_constants: true,
            evaluate_conditional_types: false,
            function_is_final: false,
            expand_generic: false,
            expand_templates: true,
            expand_hakana_types: true,
            force_alias_expansion: false,
            expand_type_aliases: true,
            where_constraints: None,
        }
    }
}

pub fn expand_union(
    codebase: &CodebaseInfo,
    // interner is only used for data_flow_graph addition, so it's optional
    interner: &Option<&Interner>,
    file_path: &FilePath,
    return_type: &mut TUnion,
    options: &TypeExpansionOptions,
    data_flow_graph: &mut DataFlowGraph,
    cost: &mut u32,
) {
    let mut overall_new_atomic_types = Vec::with_capacity(return_type.types.len());
    let mut overall_extra_data_flow_nodes = vec![];

    // Take ownership of the types to process them one by one.
    let original_types = std::mem::take(&mut return_type.types);

    for mut current_atomic_being_processed in original_types {
        let mut skip_this_atomic = false;
        // This vector will receive replacements if current_atomic_being_processed is skipped.
        let mut replacements_for_current_atomic = Vec::new();

        expand_atomic(
            &mut current_atomic_being_processed, // Modified in-place if not skipped
            codebase,
            interner,
            file_path,
            options,
            data_flow_graph,
            cost,
            &mut skip_this_atomic, // expand_atomic sets this to true if it wants to replace
            &mut replacements_for_current_atomic, // expand_atomic pushes replacements here
            &mut overall_extra_data_flow_nodes, // expand_atomic can still add global extras
        );

        if skip_this_atomic {
            // current_atomic_being_processed is discarded, use replacements
            overall_new_atomic_types.extend(replacements_for_current_atomic);
        } else {
            // current_atomic_being_processed was modified in-place, keep it.
            // replacements_for_current_atomic should be empty in this case.
            overall_new_atomic_types.push(current_atomic_being_processed);
        }
    }

    if overall_new_atomic_types.len() > 1 {
        return_type.types = type_combiner::combine(overall_new_atomic_types, codebase, false);
    } else {
        return_type.types = overall_new_atomic_types;
    }

    extend_dataflow_uniquely(&mut return_type.parent_nodes, overall_extra_data_flow_nodes);
}

fn expand_atomic(
    return_type_part: &mut TAtomic,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    file_path: &FilePath,
    options: &TypeExpansionOptions,
    data_flow_graph: &mut DataFlowGraph,
    cost: &mut u32,
    skip_key: &mut bool,
    new_return_type_parts: &mut Vec<TAtomic>,
    extra_data_flow_nodes: &mut Vec<DataFlowNode>,
) {
    *cost += 1;

    if let TAtomic::TDict(TDict {
        ref mut known_items,
        ref mut params,
        ref mut shape_name,
        ..
    }) = return_type_part
    {
        if let Some(params) = params {
            expand_union(
                codebase,
                interner,
                file_path,
                &mut params.0,
                options,
                data_flow_graph,
                cost,
            );
            expand_union(
                codebase,
                interner,
                file_path,
                &mut params.1,
                options,
                data_flow_graph,
                cost,
            );
        }

        if let Some(known_items) = known_items {
            for (_, item_type) in known_items.values_mut() {
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    Arc::make_mut(item_type),
                    options,
                    data_flow_graph,
                    cost,
                );
            }
        }

        if options.force_alias_expansion {
            *shape_name = None;
        }
    } else if let TAtomic::TVec(TVec {
        ref mut known_items,
        ref mut type_param,
        ..
    }) = return_type_part
    {
        expand_union(
            codebase,
            interner,
            file_path,
            type_param,
            options,
            data_flow_graph,
            cost,
        );

        if let Some(known_items) = known_items {
            for (_, item_type) in known_items.values_mut() {
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    item_type,
                    options,
                    data_flow_graph,
                    cost,
                );
            }
        }

        return;
    } else if let TAtomic::TKeyset {
        ref mut type_param, ..
    } = return_type_part
    {
        expand_union(
            codebase,
            interner,
            file_path,
            type_param,
            options,
            data_flow_graph,
            cost,
        );

        return;
    } else if let TAtomic::TAwaitable { ref mut value } = return_type_part {
        expand_union(
            codebase,
            interner,
            file_path,
            value,
            options,
            data_flow_graph,
            cost,
        );

        return;
    } else if let TAtomic::TNamedObject {
        ref mut name,
        ref mut type_params,
        ref mut is_this,
        ..
    } = return_type_part
    {
        if *name == StrId::THIS {
            *name = match options.static_class_type {
                StaticClassType::None => StrId::THIS,
                StaticClassType::Name(this_name) => *this_name,
                StaticClassType::Object(obj) => {
                    *skip_key = true;
                    new_return_type_parts.push(obj.clone());
                    return;
                }
            };

            if options.function_is_final {
                *is_this = false;
            }
        } else if *is_this {
            if let StaticClassType::Object(obj) = options.static_class_type {
                if let TAtomic::TNamedObject {
                    name: new_this_name,
                    ..
                } = obj
                {
                    if codebase.class_extends_or_implements(new_this_name, name) {
                        *skip_key = true;
                        new_return_type_parts.push(obj.clone());
                        return;
                    }
                }
            };
        }

        if let Some(type_params) = type_params {
            for param_type in type_params {
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    param_type,
                    options,
                    data_flow_graph,
                    cost,
                );
            }
        }

        return;
    } else if let TAtomic::TClosure(ref mut closure) = return_type_part {
        if let Some(ref mut return_type) = closure.return_type {
            expand_union(
                codebase,
                interner,
                file_path,
                return_type,
                options,
                data_flow_graph,
                cost,
            );
        }

        for param in closure.params.iter_mut() {
            if let Some(ref mut param_type) = param.signature_type {
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    param_type,
                    options,
                    data_flow_graph,
                    cost,
                );
            }
        }
    } else if let TAtomic::TGenericParam {
        param_name,
        ref mut as_type,
        ..
    } = return_type_part
    {
        if let Some(where_constraints) = options.where_constraints {
            for (_, constraint_type) in where_constraints.iter().filter(|(k, _)| k == param_name) {
                *as_type = Box::new(
                    intersect_union_types_simple(as_type, constraint_type, codebase)
                        .unwrap_or(get_nothing()),
                );
            }
        }
        expand_union(
            codebase,
            interner,
            file_path,
            as_type,
            options,
            data_flow_graph,
            cost,
        );

        return;
    } else if let TAtomic::TClassname {
        ref mut as_type, ..
    }
    | TAtomic::TTypename {
        ref mut as_type, ..
    } = return_type_part
    {
        let mut atomic_return_type_parts = vec![];
        expand_atomic(
            as_type,
            codebase,
            interner,
            file_path,
            options,
            data_flow_graph,
            cost,
            &mut false,
            &mut atomic_return_type_parts,
            extra_data_flow_nodes,
        );

        if !atomic_return_type_parts.is_empty() {
            *as_type = Box::new(atomic_return_type_parts.remove(0));
        }

        return;
    } else if let TAtomic::TEnumLiteralCase {
        ref enum_name,
        ref mut as_type,
        ref mut underlying_type,
        ..
    }
    | TAtomic::TEnum {
        name: ref enum_name,
        ref mut as_type,
        ref mut underlying_type,
        ..
    } = return_type_part
    {
        if let Some(enum_storage) = codebase.classlike_infos.get(enum_name) {
            if let Some(storage_as_type) = &enum_storage.enum_as_type {
                let mut as_type_union = wrap_atomic(storage_as_type.clone());
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    &mut as_type_union,
                    options,
                    data_flow_graph,
                    cost,
                );
                *as_type = Some(Arc::new(as_type_union.get_single_owned()));
            }

            if let Some(storage_underlying_type) = &enum_storage.enum_underlying_type {
                let mut underlying_type_union = wrap_atomic(storage_underlying_type.clone());
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    &mut underlying_type_union,
                    options,
                    data_flow_graph,
                    cost,
                );
                *underlying_type = Some(Arc::new(underlying_type_union.get_single_owned()));
            }
        }

        return;
    } else if let TAtomic::TMemberReference {
        ref classlike_name,
        ref member_name,
    } = return_type_part
    {
        *skip_key = true;

        if let Some(literal_value) =
            codebase.get_classconst_literal_value(classlike_name, member_name)
        {
            let mut literal_value = literal_value.clone();

            expand_atomic(
                &mut literal_value,
                codebase,
                interner,
                file_path,
                options,
                data_flow_graph,
                cost,
                skip_key,
                new_return_type_parts,
                extra_data_flow_nodes,
            );

            new_return_type_parts.push(literal_value);
        } else {
            let const_type = codebase.get_class_constant_type(
                classlike_name,
                false,
                member_name,
                FxHashSet::default(),
            );

            if let Some(mut const_type) = const_type {
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    &mut const_type,
                    options,
                    data_flow_graph,
                    cost,
                );

                new_return_type_parts.extend(const_type.types);
            } else {
                new_return_type_parts.push(TAtomic::TMixed);
            }
        }

        return;
    } else if let TAtomic::TTypeAlias {
        name: type_name,
        type_params,
        as_type,
        ..
    } = return_type_part
    {
        if !options.expand_type_aliases {
            return;
        }

        let type_definition = if let Some(t) = codebase.type_definitions.get(type_name) {
            t
        } else {
            *skip_key = true;
            new_return_type_parts.push(TAtomic::TMixedWithFlags(true, false, false, false));
            return;
        };

        let can_expand_type =
            options.force_alias_expansion || can_expand_type_in_file(file_path, type_definition);

        if type_definition.is_literal_string && options.expand_hakana_types {
            *skip_key = true;
            new_return_type_parts.push(TAtomic::TStringWithFlags(false, false, true));
            return;
        }

        if can_expand_type {
            *skip_key = true;

            let mut actual_type = if let Some(type_params) = type_params {
                let mut new_template_types = IndexMap::new();

                for (i, (k, v)) in type_definition.template_types.iter().enumerate() {
                    if i < type_params.len() {
                        let mut h = FxHashMap::default();
                        for (kk, _) in v {
                            h.insert(*kk, type_params[i].clone());
                        }

                        new_template_types.insert(*k, h);
                    }
                }

                template::inferred_type_replacer::replace(
                    &type_definition.actual_type,
                    &template::TemplateResult::new(IndexMap::new(), new_template_types),
                    codebase,
                )
            } else {
                type_definition.actual_type.clone()
            };

            expand_union(
                codebase,
                interner,
                file_path,
                &mut actual_type,
                options,
                data_flow_graph,
                cost,
            );

            let expanded_types = actual_type
                .types
                .into_iter()
                .map(|mut v| {
                    if type_params.is_none() {
                        if let TAtomic::TDict(TDict {
                            known_items: Some(_),
                            ref mut shape_name,
                            ..
                        }) = v
                        {
                            if let (Some(shape_field_taints), Some(interner)) =
                                (&type_definition.shape_field_taints, interner)
                            {
                                let shape_node =
                                    DataFlowNode::get_for_type(type_name, type_definition.location);

                                for (field_name, taints) in shape_field_taints {
                                    let field_name_str = field_name.to_string(Some(interner));

                                    let field_node = DataFlowNode {
                                        id: DataFlowNodeId::ShapeFieldAccess(
                                            *type_name,
                                            field_name_str,
                                        ),
                                        kind: DataFlowNodeKind::TaintSource {
                                            pos: Some(taints.0),
                                            types: taints.1.clone(),
                                        },
                                    };

                                    data_flow_graph.add_path(
                                        &field_node.id,
                                        &shape_node.id,
                                        PathKind::ArrayAssignment(
                                            ArrayDataKind::ArrayValue,
                                            match field_name {
                                                DictKey::Int(i) => i.to_string(),
                                                DictKey::String(k) => k.clone(),
                                                DictKey::Enum(_, _) => todo!(),
                                            },
                                        ),
                                        vec![],
                                        vec![],
                                    );

                                    data_flow_graph.add_node(field_node);
                                }

                                extra_data_flow_nodes.push(shape_node.clone());

                                data_flow_graph.add_node(shape_node);
                            }

                            if !options.force_alias_expansion {
                                *shape_name = Some((*type_name, None));
                            }
                        };
                    }
                    v
                })
                .collect::<Vec<_>>();

            new_return_type_parts.extend(expanded_types);
        } else if let Some(definition_as_type) = &type_definition.as_type {
            let mut definition_as_type = if let Some(type_params) = type_params {
                let mut new_template_types = IndexMap::new();

                for (i, (k, v)) in type_definition.template_types.iter().enumerate() {
                    let mut h = FxHashMap::default();
                    for (kk, _) in v {
                        h.insert(
                            *kk,
                            if let Some(t) = type_params.get(i) {
                                t.clone()
                            } else {
                                get_nothing()
                            },
                        );
                    }

                    new_template_types.insert(*k, h);
                }

                template::inferred_type_replacer::replace(
                    definition_as_type,
                    &template::TemplateResult::new(IndexMap::new(), new_template_types),
                    codebase,
                )
            } else {
                definition_as_type.clone()
            };

            expand_union(
                codebase,
                interner,
                file_path,
                &mut definition_as_type,
                options,
                data_flow_graph,
                cost,
            );

            *as_type = Some(Box::new(definition_as_type));
        }

        if let Some(type_params) = type_params {
            for param_type in type_params {
                expand_union(
                    codebase,
                    interner,
                    file_path,
                    param_type,
                    options,
                    data_flow_graph,
                    cost,
                );
            }
        }

        return;
    } else if let TAtomic::TClassTypeConstant {
        class_type,
        member_name,
        as_type,
    } = return_type_part
    {
        let mut atomic_return_type_parts = vec![];
        expand_atomic(
            class_type,
            codebase,
            interner,
            file_path,
            options,
            data_flow_graph,
            cost,
            &mut false,
            &mut atomic_return_type_parts,
            extra_data_flow_nodes,
        );

        if !atomic_return_type_parts.is_empty() {
            *class_type = Box::new(atomic_return_type_parts.remove(0));
        }

        match class_type.as_ref() {
            TAtomic::TNamedObject {
                name: class_name,
                is_this,
                ..
            } => {
                let classlike_storage = if let Some(c) = codebase.classlike_infos.get(class_name) {
                    c
                } else {
                    *skip_key = true;
                    new_return_type_parts.push(TAtomic::TMixedWithFlags(true, false, false, false));
                    return;
                };

                let type_constant = if let Some(t) =
                    classlike_storage.type_constants.get(member_name)
                {
                    t.clone()
                } else {
                    *skip_key = true;
                    new_return_type_parts.push(TAtomic::TMixedWithFlags(true, false, false, false));
                    return;
                };

                let mut is_this = *is_this;

                if is_this {
                    if let StaticClassType::Object(obj) = options.static_class_type {
                        if let TAtomic::TNamedObject {
                            name: new_this_name,
                            ..
                        } = obj
                        {
                            if !codebase.class_extends_or_implements(new_this_name, class_name) {
                                is_this = false
                            }
                        }
                    } else {
                        is_this = false;
                    }
                }

                match (is_this, type_constant) {
                    (_, ClassConstantType::Concrete(mut type_))
                    | (false, ClassConstantType::Abstract(Some(mut type_))) => {
                        expand_union(
                            codebase,
                            interner,
                            file_path,
                            &mut type_,
                            options,
                            data_flow_graph,
                            cost,
                        );

                        *skip_key = true;
                        new_return_type_parts.extend(type_.types.into_iter().map(|mut v| {
                            if let TAtomic::TDict(TDict {
                                known_items: Some(_),
                                ref mut shape_name,
                                ..
                            }) = v
                            {
                                if !options.force_alias_expansion {
                                    *shape_name = Some((*class_name, Some(*member_name)));
                                }
                            };
                            v
                        }));
                    }
                    (true, ClassConstantType::Abstract(Some(mut type_))) => {
                        expand_union(
                            codebase,
                            interner,
                            file_path,
                            &mut type_,
                            options,
                            data_flow_graph,
                            cost,
                        );

                        *as_type = Box::new(type_);
                    }
                    _ => {}
                };
            }
            _ => {
                *skip_key = true;
                new_return_type_parts.push(TAtomic::TMixedWithFlags(true, false, false, false));
                return;
            }
        };
    } else if let TAtomic::TClosureAlias { id, .. } = &return_type_part {
        if let Some(value) =
            get_closure_from_id(id, codebase, interner, file_path, data_flow_graph, cost)
        {
            *skip_key = true;
            new_return_type_parts.push(value);
            return;
        }
    }
}

pub fn can_expand_type_in_file(file_path: &FilePath, type_definition: &TypeDefinitionInfo) -> bool {
    if let Some(type_file_path) = &type_definition.newtype_file {
        file_path == type_file_path
    } else {
        true
    }
}

pub fn get_closure_from_id(
    id: &FunctionLikeIdentifier,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    file_path: &FilePath,
    data_flow_graph: &mut DataFlowGraph,
    cost: &mut u32,
) -> Option<TAtomic> {
    match id {
        FunctionLikeIdentifier::Function(name) => {
            if let Some(functionlike_info) = codebase.functionlike_infos.get(&(*name, StrId::EMPTY))
            {
                return Some(get_expanded_closure(
                    functionlike_info,
                    codebase,
                    interner,
                    file_path,
                    data_flow_graph,
                    &TypeExpansionOptions::default(),
                    cost,
                ));
            }
        }
        FunctionLikeIdentifier::Method(classlike_name, method_name) => {
            let declaring_method_id =
                codebase.get_declaring_method_id(&MethodIdentifier(*classlike_name, *method_name));

            if let Some(functionlike_info) = codebase.get_method(&declaring_method_id) {
                return Some(get_expanded_closure(
                    functionlike_info,
                    codebase,
                    interner,
                    file_path,
                    data_flow_graph,
                    &TypeExpansionOptions {
                        self_class: Some(classlike_name),
                        static_class_type: StaticClassType::Name(classlike_name),
                        ..Default::default()
                    },
                    cost,
                ));
            }
        }
        _ => {
            panic!()
        }
    }
    None
}

fn get_expanded_closure(
    functionlike_info: &FunctionLikeInfo,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    file_path: &FilePath,
    data_flow_graph: &mut DataFlowGraph,
    options: &TypeExpansionOptions,
    cost: &mut u32,
) -> TAtomic {
    TAtomic::TClosure(Box::new(TClosure {
        params: functionlike_info
            .params
            .iter()
            .map(|param| FnParameter {
                signature_type: if let Some(t) = &param.signature_type {
                    let mut t = t.clone();
                    expand_union(
                        codebase,
                        interner,
                        file_path,
                        &mut t,
                        options,
                        data_flow_graph,
                        cost,
                    );
                    Some(Box::new(t))
                } else {
                    None
                },
                is_inout: param.is_inout,
                is_variadic: param.is_variadic,
                is_optional: param.is_optional,
            })
            .collect(),
        return_type: if let Some(return_type) = &functionlike_info.return_type {
            let mut return_type = return_type.clone();
            expand_union(
                codebase,
                interner,
                file_path,
                &mut return_type,
                options,
                data_flow_graph,
                cost,
            );
            Some(return_type)
        } else {
            None
        },
        effects: functionlike_info.effects.to_u8(),
        closure_id: (
            functionlike_info.def_location.file_path,
            functionlike_info.def_location.start_offset,
        ),
    }))
}
