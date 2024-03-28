use std::sync::Arc;

use hakana_reflection_info::{
    classlike_info::ClassConstantType,
    code_location::FilePath,
    codebase_info::CodebaseInfo,
    data_flow::{
        graph::DataFlowGraph,
        node::{DataFlowNode, DataFlowNodeKind},
        path::{ArrayDataKind, PathKind},
    },
    functionlike_info::FunctionLikeInfo,
    functionlike_parameter::FnParameter,
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_reflection_info::{
    functionlike_identifier::FunctionLikeIdentifier, method_identifier::MethodIdentifier,
};
use hakana_str::{Interner, StrId};
use indexmap::IndexMap;
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::{extend_dataflow_uniquely, get_nothing, template, type_combiner, wrap_atomic};

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
    pub file_path: Option<&'a FilePath>,

    pub evaluate_class_constants: bool,
    pub evaluate_conditional_types: bool,
    pub function_is_final: bool,
    pub expand_generic: bool,
    pub expand_templates: bool,
    pub expand_hakana_types: bool,
    pub expand_all_type_aliases: bool,
}

impl Default for TypeExpansionOptions<'_> {
    fn default() -> Self {
        Self {
            file_path: None,
            self_class: None,
            static_class_type: StaticClassType::None,
            parent_class: None,
            evaluate_class_constants: true,
            evaluate_conditional_types: false,
            function_is_final: false,
            expand_generic: false,
            expand_templates: true,
            expand_hakana_types: true,
            expand_all_type_aliases: false,
        }
    }
}

pub fn expand_union(
    codebase: &CodebaseInfo,
    // interner is only used for data_flow_graph addition, so it's optional
    interner: &Option<&Interner>,
    return_type: &mut TUnion,
    options: &TypeExpansionOptions,
    data_flow_graph: &mut DataFlowGraph,
) {
    let mut new_return_type_parts = vec![];

    let mut extra_data_flow_nodes = vec![];

    let mut skipped_keys = vec![];

    for (i, return_type_part) in return_type.types.iter_mut().enumerate() {
        let mut skip_key = false;
        expand_atomic(
            return_type_part,
            codebase,
            interner,
            options,
            data_flow_graph,
            &mut skip_key,
            &mut new_return_type_parts,
            &mut extra_data_flow_nodes,
        );

        if skip_key {
            skipped_keys.push(i);
        }
    }

    if !skipped_keys.is_empty() {
        let mut i = 0;
        return_type.types.retain(|_| {
            let to_retain = !skipped_keys.contains(&i);
            i += 1;
            to_retain
        });

        new_return_type_parts.extend(return_type.types.drain(..).collect_vec());

        if new_return_type_parts.len() > 1 {
            return_type.types = type_combiner::combine(new_return_type_parts, codebase, false)
        } else {
            return_type.types = new_return_type_parts;
        }
    }

    extend_dataflow_uniquely(&mut return_type.parent_nodes, extra_data_flow_nodes);
}

fn expand_atomic(
    return_type_part: &mut TAtomic,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    options: &TypeExpansionOptions,
    data_flow_graph: &mut DataFlowGraph,
    skip_key: &mut bool,
    new_return_type_parts: &mut Vec<TAtomic>,
    extra_data_flow_nodes: &mut Vec<DataFlowNode>,
) {
    if let TAtomic::TDict {
        ref mut known_items,
        ref mut params,
        ref mut shape_name,
        ..
    } = return_type_part
    {
        if let Some(params) = params {
            expand_union(codebase, interner, &mut params.0, options, data_flow_graph);
            expand_union(codebase, interner, &mut params.1, options, data_flow_graph);
        }

        if let Some(known_items) = known_items {
            for (_, item_type) in known_items.values_mut() {
                expand_union(
                    codebase,
                    interner,
                    Arc::make_mut(item_type),
                    options,
                    data_flow_graph,
                );
            }
        }

        if options.expand_all_type_aliases {
            *shape_name = None;
        }
    } else if let TAtomic::TVec {
        ref mut known_items,
        ref mut type_param,
        ..
    } = return_type_part
    {
        expand_union(codebase, interner, type_param, options, data_flow_graph);

        if let Some(known_items) = known_items {
            for (_, item_type) in known_items.values_mut() {
                expand_union(codebase, interner, item_type, options, data_flow_graph);
            }
        }

        return;
    } else if let TAtomic::TKeyset {
        ref mut type_param, ..
    } = return_type_part
    {
        expand_union(codebase, interner, type_param, options, data_flow_graph);

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
                    new_return_type_parts.push(obj.clone().clone());
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
                        new_return_type_parts.push(obj.clone().clone());
                        return;
                    }
                }
            };
        }

        if let Some(type_params) = type_params {
            for param_type in type_params {
                expand_union(codebase, interner, param_type, options, data_flow_graph);
            }
        }

        return;
    } else if let TAtomic::TClosure {
        params,
        return_type,
        ..
    } = return_type_part
    {
        if let Some(return_type) = return_type {
            expand_union(codebase, interner, return_type, options, data_flow_graph);
        }

        for param in params {
            if let Some(ref mut param_type) = param.signature_type {
                expand_union(codebase, interner, param_type, options, data_flow_graph);
            }
        }
    } else if let TAtomic::TGenericParam {
        ref mut as_type, ..
    } = return_type_part
    {
        expand_union(codebase, interner, as_type, options, data_flow_graph);

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
            options,
            data_flow_graph,
            &mut false,
            &mut atomic_return_type_parts,
            extra_data_flow_nodes,
        );

        if !atomic_return_type_parts.is_empty() {
            *as_type = Box::new(atomic_return_type_parts.remove(0));
        }

        return;
    } else if let TAtomic::TEnumLiteralCase {
        ref mut constraint_type,
        ..
    } = return_type_part
    {
        if let Some(constraint_type) = constraint_type {
            let mut constraint_union = wrap_atomic((**constraint_type).clone());
            expand_union(
                codebase,
                interner,
                &mut constraint_union,
                options,
                data_flow_graph,
            );
            *constraint_type = Box::new(constraint_union.get_single_owned());
        }

        return;
    } else if let TAtomic::TEnum {
        ref name,
        ref mut base_type,
        ..
    } = return_type_part
    {
        if let Some(enum_storage) = codebase.classlike_infos.get(name) {
            if let Some(storage_type) = &enum_storage.enum_type {
                let mut constraint_union = wrap_atomic((*storage_type).clone());
                expand_union(
                    codebase,
                    interner,
                    &mut constraint_union,
                    options,
                    data_flow_graph,
                );
                *base_type = Some(Box::new(constraint_union.get_single_owned()));
            } else {
                *base_type = Some(Box::new(TAtomic::TArraykey { from_any: true }));
            }
        }

        return;
    } else if let TAtomic::TTypeAlias {
        name: type_name,
        type_params,
        as_type,
    } = return_type_part
    {
        let type_definition = if let Some(t) = codebase.type_definitions.get(type_name) {
            t
        } else {
            *skip_key = true;
            new_return_type_parts.push(TAtomic::TMixedWithFlags(true, false, false, false));
            return;
        };

        let can_expand_type = if let Some(type_file_path) = &type_definition.newtype_file {
            if let Some(expanding_file_path) = options.file_path {
                expanding_file_path == type_file_path
            } else {
                options.expand_all_type_aliases
            }
        } else {
            true
        };

        if type_definition.is_literal_string && options.expand_hakana_types {
            *skip_key = true;
            new_return_type_parts.push(TAtomic::TStringWithFlags(false, false, true));
            return;
        }

        if can_expand_type {
            *skip_key = true;

            let mut untemplated_type = if let Some(type_params) = type_params {
                let mut new_template_types = IndexMap::new();

                for (i, (k, v)) in type_definition.template_types.iter().enumerate() {
                    let mut h = FxHashMap::default();
                    for (kk, _) in v {
                        h.insert(*kk, type_params.get(i).unwrap().clone());
                    }

                    new_template_types.insert(*k, h);
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
                &mut untemplated_type,
                options,
                data_flow_graph,
            );

            let expanded_types = untemplated_type
                .types
                .into_iter()
                .map(|mut v| {
                    if type_params.is_none() {
                        if let TAtomic::TDict {
                            known_items: Some(_),
                            ref mut shape_name,
                            ..
                        } = v
                        {
                            if let (Some(shape_field_taints), Some(interner)) =
                                (&type_definition.shape_field_taints, interner)
                            {
                                let shape_node = DataFlowNode::get_for_type(
                                    type_name,
                                    interner,
                                    type_definition.location,
                                );

                                for (field_name, taints) in shape_field_taints {
                                    let label = format!(
                                        "{}[{}]",
                                        interner.lookup(type_name),
                                        field_name.to_string(Some(interner))
                                    );
                                    let field_node = DataFlowNode {
                                        id: label.clone(),
                                        kind: DataFlowNodeKind::TaintSource {
                                            pos: Some(taints.0),
                                            label,
                                            types: taints.1.clone(),
                                        },
                                    };

                                    data_flow_graph.add_path(
                                        &field_node,
                                        &shape_node,
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

                            if !options.expand_all_type_aliases {
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
                &mut definition_as_type,
                options,
                data_flow_graph,
            );

            *as_type = Some(Box::new(definition_as_type));
        }

        if let Some(type_params) = type_params {
            for param_type in type_params {
                expand_union(codebase, interner, param_type, options, data_flow_graph);
            }
        }

        return;
    } else if let TAtomic::TClassTypeConstant {
        class_type,
        member_name,
    } = return_type_part
    {
        let mut atomic_return_type_parts = vec![];
        expand_atomic(
            class_type,
            codebase,
            interner,
            options,
            data_flow_graph,
            &mut false,
            &mut atomic_return_type_parts,
            extra_data_flow_nodes,
        );

        if !atomic_return_type_parts.is_empty() {
            *class_type = Box::new(atomic_return_type_parts.remove(0));
        }

        match class_type.as_ref() {
            TAtomic::TNamedObject {
                name: class_name, ..
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

                match type_constant {
                    ClassConstantType::Abstract(Some(mut type_))
                    | ClassConstantType::Concrete(mut type_) => {
                        expand_union(codebase, interner, &mut type_, options, data_flow_graph);

                        *skip_key = true;
                        new_return_type_parts.extend(type_.types.into_iter().map(|mut v| {
                            if let TAtomic::TDict {
                                known_items: Some(_),
                                ref mut shape_name,
                                ..
                            } = v
                            {
                                *shape_name = Some((*class_name, Some(*member_name)));
                            };
                            v
                        }));
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
        if let Some(value) = get_closure_from_id(id, codebase, interner, data_flow_graph) {
            *skip_key = true;
            new_return_type_parts.push(value);
            return;
        }
    }
}

pub fn get_closure_from_id(
    id: &FunctionLikeIdentifier,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    data_flow_graph: &mut DataFlowGraph,
) -> Option<TAtomic> {
    match id {
        FunctionLikeIdentifier::Function(name) => {
            if let Some(functionlike_info) = codebase.functionlike_infos.get(&(*name, StrId::EMPTY))
            {
                return Some(get_expanded_closure(
                    functionlike_info,
                    codebase,
                    interner,
                    data_flow_graph,
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
                    data_flow_graph,
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
    data_flow_graph: &mut DataFlowGraph,
) -> TAtomic {
    TAtomic::TClosure {
        params: functionlike_info
            .params
            .iter()
            .map(|param| FnParameter {
                signature_type: if let Some(t) = &param.signature_type {
                    let mut t = t.clone();
                    expand_union(
                        codebase,
                        interner,
                        &mut t,
                        &TypeExpansionOptions {
                            ..Default::default()
                        },
                        data_flow_graph,
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
                &mut return_type,
                &TypeExpansionOptions {
                    ..Default::default()
                },
                data_flow_graph,
            );
            Some(Box::new(return_type))
        } else {
            None
        },
        effects: functionlike_info.effects.to_u8(),
        closure_id: (
            functionlike_info.def_location.file_path,
            functionlike_info.def_location.start_offset,
        ),
    }
}
