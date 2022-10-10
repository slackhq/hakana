use std::sync::Arc;

use hakana_reflection_info::{
    codebase_info::{symbols::Symbol, CodebaseInfo},
    data_flow::{
        graph::DataFlowGraph,
        node::DataFlowNode,
        path::{PathExpressionKind, PathKind},
    },
    functionlike_info::FunctionLikeInfo,
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
    StrId,
};
use hakana_reflection_info::{
    functionlike_identifier::FunctionLikeIdentifier, method_identifier::MethodIdentifier,
};
use indexmap::IndexMap;
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::{template, type_combiner, wrap_atomic};

#[derive(Debug)]
pub enum StaticClassType<'a, 'b> {
    None,
    Name(&'a Symbol),
    Object(&'b TAtomic),
}

pub struct TypeExpansionOptions<'a> {
    pub self_class: Option<&'a Symbol>,
    pub static_class_type: StaticClassType<'a, 'a>,
    pub parent_class: Option<&'a Symbol>,
    pub file_path: Option<&'a StrId>,

    pub evaluate_class_constants: bool,
    pub evaluate_conditional_types: bool,
    pub function_is_final: bool,
    pub expand_generic: bool,
    pub expand_templates: bool,
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
        }
    }
}

pub fn expand_union(
    codebase: &CodebaseInfo,
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
            &options,
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

    return_type.parent_nodes.extend(
        extra_data_flow_nodes
            .into_iter()
            .map(|n| (n.get_id().clone(), n)),
    );
}

fn expand_atomic(
    return_type_part: &mut TAtomic,
    codebase: &CodebaseInfo,
    options: &TypeExpansionOptions,
    data_flow_graph: &mut DataFlowGraph,
    skip_key: &mut bool,
    new_return_type_parts: &mut Vec<TAtomic>,
    extra_data_flow_nodes: &mut Vec<DataFlowNode>,
) {
    if let TAtomic::TDict {
        ref mut known_items,
        ref mut params,
        ..
    } = return_type_part
    {
        if let Some(params) = params {
            expand_union(codebase, &mut params.0, options, data_flow_graph);
            expand_union(codebase, &mut params.1, options, data_flow_graph);
        }

        if let Some(known_items) = known_items {
            for (_, (_, item_type)) in known_items {
                expand_union(codebase, Arc::make_mut(item_type), options, data_flow_graph);
            }
        }

        return;
    } else if let TAtomic::TVec {
        ref mut known_items,
        ref mut type_param,
        ..
    } = return_type_part
    {
        expand_union(codebase, type_param, options, data_flow_graph);

        if let Some(known_items) = known_items {
            for (_, (_, item_type)) in known_items {
                expand_union(codebase, item_type, options, data_flow_graph);
            }
        }

        return;
    } else if let TAtomic::TKeyset {
        ref mut type_param, ..
    } = return_type_part
    {
        expand_union(codebase, type_param, options, data_flow_graph);

        return;
    } else if let TAtomic::TNamedObject {
        ref mut name,
        ref mut type_params,
        ref mut is_this,
        ..
    } = return_type_part
    {
        if *name == StrId::this() {
            *name = match options.static_class_type {
                StaticClassType::None => StrId::this(),
                StaticClassType::Name(this_name) => this_name.clone().clone(),
                StaticClassType::Object(obj) => {
                    *skip_key = true;
                    new_return_type_parts.push(obj.clone().clone());
                    return;
                }
            };

            if options.function_is_final {
                *is_this = false;
            }
        }

        if let Some(type_params) = type_params {
            for param_type in type_params {
                expand_union(codebase, param_type, options, data_flow_graph);
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
            expand_union(codebase, return_type, options, data_flow_graph);
        }

        for param in params {
            if let Some(ref mut param_type) = param.signature_type {
                expand_union(codebase, param_type, options, data_flow_graph);
            }
        }
    } else if let TAtomic::TTemplateParam {
        ref mut from_class,
        ref defining_entity,
        ref mut as_type,
        ..
    } = return_type_part
    {
        let defining_entity_name = codebase.interner.lookup(*defining_entity);
        *from_class = !defining_entity_name.starts_with("fn-")
            && !defining_entity_name.starts_with("typedef-");
        expand_union(codebase, as_type, options, data_flow_graph);

        return;
    } else if let TAtomic::TClassname {
        ref mut as_type, ..
    } = return_type_part
    {
        let mut atomic_return_type_parts = vec![];
        expand_atomic(
            as_type,
            codebase,
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
            expand_union(codebase, &mut constraint_union, options, data_flow_graph);
            *constraint_type = Box::new(constraint_union.get_single_owned());
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
            new_return_type_parts.push(TAtomic::TMixedAny);
            return;
        };

        let can_expand_type = if let Some(type_file_path) = &type_definition.newtype_file {
            if let Some(expanding_file_path) = options.file_path {
                expanding_file_path == type_file_path
            } else {
                false
            }
        } else {
            true
        };

        if type_definition.is_literal_string {
            *skip_key = true;
            new_return_type_parts.push(TAtomic::TStringWithFlags(false, false, true));
            return;
        }

        if can_expand_type {
            *skip_key = true;

            let mut untemplated_type = if let Some(type_params) = type_params {
                let mut new_template_types = IndexMap::new();

                let mut i: usize = 0;
                for (k, v) in &type_definition.template_types {
                    let mut h = FxHashMap::default();
                    for (kk, _) in v {
                        h.insert(kk.clone(), type_params.get(i).unwrap().clone());
                    }

                    new_template_types.insert(k.clone(), h);

                    i += 1;
                }

                template::inferred_type_replacer::replace(
                    &type_definition.actual_type,
                    &template::TemplateResult::new(IndexMap::new(), new_template_types),
                    codebase,
                )
            } else {
                type_definition.actual_type.clone()
            };

            expand_union(codebase, &mut untemplated_type, options, data_flow_graph);

            let expanded_types = untemplated_type
                .types
                .into_iter()
                .map(|mut v| {
                    if let None = type_params {
                        if let TAtomic::TDict {
                            known_items: Some(_),
                            ref mut shape_name,
                            ..
                        } = v
                        {
                            if let Some(shape_field_taints) = &type_definition.shape_field_taints {
                                let shape_node = DataFlowNode::new(
                                    codebase.interner.lookup(*type_name).to_string(),
                                    codebase.interner.lookup(*type_name).to_string(),
                                    None,
                                    None,
                                );

                                for (field_name, taints) in shape_field_taints {
                                    let label = format!(
                                        "{}[{}]",
                                        codebase.interner.lookup(*type_name),
                                        field_name.to_string(Some(&codebase.interner))
                                    );
                                    let field_node = DataFlowNode::TaintSource {
                                        id: label.clone(),
                                        label,
                                        pos: None,
                                        types: taints.clone(),
                                    };

                                    data_flow_graph.add_path(
                                        &field_node,
                                        &shape_node,
                                        PathKind::ExpressionAssignment(
                                            PathExpressionKind::ArrayValue,
                                            match field_name {
                                                DictKey::Int(i) => i.to_string(),
                                                DictKey::String(k) => k.clone(),
                                                DictKey::Enum(_, _) => todo!(),
                                            },
                                        ),
                                        None,
                                        None,
                                    );

                                    data_flow_graph.add_node(field_node);
                                }

                                extra_data_flow_nodes.push(shape_node.clone());

                                data_flow_graph.add_node(shape_node);
                            }
                            *shape_name = Some(codebase.interner.lookup(*type_name).to_string());
                        };
                    }
                    v
                })
                .collect::<Vec<_>>();

            new_return_type_parts.extend(expanded_types);
        } else {
            if let Some(definition_as_type) = &type_definition.as_type {
                let mut definition_as_type = if let Some(type_params) = type_params {
                    let mut new_template_types = IndexMap::new();

                    let mut i: usize = 0;
                    for (k, v) in &type_definition.template_types {
                        let mut h = FxHashMap::default();
                        for (kk, _) in v {
                            h.insert(kk.clone(), type_params.get(i).unwrap().clone());
                        }

                        new_template_types.insert(k.clone(), h);

                        i += 1;
                    }

                    template::inferred_type_replacer::replace(
                        &definition_as_type,
                        &template::TemplateResult::new(IndexMap::new(), new_template_types),
                        codebase,
                    )
                } else {
                    definition_as_type.clone()
                };

                expand_union(codebase, &mut definition_as_type, options, data_flow_graph);

                if definition_as_type.is_single() {
                    *as_type = Some(Box::new(definition_as_type.get_single_owned()));
                }
            }
        }

        if let Some(type_params) = type_params {
            for param_type in type_params {
                expand_union(codebase, param_type, options, data_flow_graph);
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
                    new_return_type_parts.push(TAtomic::TMixedAny);
                    return;
                };

                let mut type_ = if let Some(t) = classlike_storage.type_constants.get(member_name) {
                    t.clone()
                } else {
                    *skip_key = true;
                    new_return_type_parts.push(TAtomic::TMixedAny);
                    return;
                };

                expand_union(codebase, &mut type_, options, data_flow_graph);

                *skip_key = true;
                new_return_type_parts.extend(type_.types.into_iter().map(|mut v| {
                    if let TAtomic::TDict {
                        known_items: Some(_),
                        ref mut shape_name,
                        ..
                    } = v
                    {
                        *shape_name = Some(format!(
                            "{}::{}",
                            codebase.interner.lookup(*class_name),
                            member_name
                        ));
                    };
                    v
                }));
            }
            _ => {
                *skip_key = true;
                new_return_type_parts.push(TAtomic::TMixedAny);
                return;
            }
        };
    } else if let TAtomic::TClosureAlias { id, .. } = &return_type_part {
        if let Some(value) = get_closure_from_id(id, codebase, data_flow_graph) {
            *skip_key = true;
            new_return_type_parts.push(value);
            return;
        }
    }
}

pub fn get_closure_from_id(
    id: &FunctionLikeIdentifier,
    codebase: &CodebaseInfo,
    data_flow_graph: &mut DataFlowGraph,
) -> Option<TAtomic> {
    match id {
        FunctionLikeIdentifier::Function(name) => {
            if let Some(functionlike_info) = codebase.functionlike_infos.get(&name) {
                return Some(get_expanded_closure(
                    functionlike_info,
                    codebase,
                    data_flow_graph,
                ));
            }
        }
        FunctionLikeIdentifier::Method(classlike_name, method_name) => {
            let declaring_method_id = codebase.get_declaring_method_id(&MethodIdentifier(
                classlike_name.clone(),
                method_name.clone(),
            ));

            if let Some(classlike_info) = codebase.classlike_infos.get(&declaring_method_id.0) {
                if let Some(functionlike_info) = classlike_info.methods.get(&declaring_method_id.1)
                {
                    return Some(get_expanded_closure(
                        functionlike_info,
                        codebase,
                        data_flow_graph,
                    ));
                }
            }
        }
    }
    None
}

fn get_expanded_closure(
    functionlike_info: &FunctionLikeInfo,
    codebase: &CodebaseInfo,
    data_flow_graph: &mut DataFlowGraph,
) -> TAtomic {
    TAtomic::TClosure {
        params: functionlike_info
            .params
            .iter()
            .map(|param| {
                let mut param = param.clone();
                if let Some(ref mut t) = param.signature_type {
                    expand_union(
                        codebase,
                        t,
                        &TypeExpansionOptions {
                            ..Default::default()
                        },
                        data_flow_graph,
                    );
                }

                param
            })
            .collect(),
        return_type: if let Some(return_type) = &functionlike_info.return_type {
            let mut return_type = return_type.clone();
            expand_union(
                codebase,
                &mut return_type,
                &TypeExpansionOptions {
                    ..Default::default()
                },
                data_flow_graph,
            );
            Some(return_type)
        } else {
            None
        },
        effects: functionlike_info.effects.to_u8(),
        closure_id: functionlike_info.name,
    }
}
