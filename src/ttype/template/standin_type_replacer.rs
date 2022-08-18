use crate::{
    add_union_type, combine_optional_union_types, get_arrayish_params, get_mixed, get_mixed_any,
    get_mixed_maybe_from_loop, get_value_param, intersect_union_types, is_array_container,
    type_combiner,
    type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator},
    type_expander::{self, StaticClassType, TypeExpansionOptions},
    wrap_atomic,
};
use function_context::FunctionLikeIdentifier;
use hakana_reflection_info::{
    codebase_info::CodebaseInfo,
    data_flow::graph::{DataFlowGraph, GraphKind},
    t_atomic::TAtomic,
    t_union::TUnion,
};
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

use super::{inferred_type_replacer, TemplateBound, TemplateResult};

pub fn replace(
    union_type: &TUnion,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    input_type: &Option<TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&String>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,                             // true
    add_lower_bound: bool,                     // false
    bound_equality_classlike: Option<&String>, // None
    depth: usize,                              // 1
) -> TUnion {
    let mut atomic_types = Vec::new();

    let original_atomic_types = union_type.clone().types;

    let mut input_type = input_type.clone();

    if let Some(ref mut input_type_inner) = input_type {
        if !input_type_inner.is_single() {
            // here we want to subtract atomic types from the input type
            // when they're also in the union type, so those shared atomic
            // types will never be inferred as part of the generic type
            for (key, _) in &original_atomic_types {
                input_type_inner.types.remove(key);
            }

            if input_type_inner.types.is_empty() {
                return union_type.clone();
            }
        }
    }

    let mut had_template = false;

    for (key, atomic_type) in original_atomic_types.iter() {
        atomic_types.extend(handle_atomic_standin(
            atomic_type,
            key,
            template_result,
            codebase,
            &input_type,
            input_arg_offset,
            calling_class,
            calling_function,
            replace,
            add_lower_bound,
            bound_equality_classlike,
            depth,
            &original_atomic_types.len() == &1,
            &mut had_template,
        ))
    }

    if replace {
        if atomic_types.len() == 0 {
            return union_type.clone();
        }

        let mut new_union_type = TUnion::new(if atomic_types.len() > 1 {
            type_combiner::combine(atomic_types, Some(codebase), false)
        } else {
            atomic_types
        });

        new_union_type.ignore_falsable_issues = union_type.ignore_falsable_issues;

        if had_template {
            new_union_type.had_template = true;
        }

        return new_union_type;
    }

    union_type.clone()
}

fn handle_atomic_standin(
    atomic_type: &TAtomic,
    key: &String,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    input_type: &Option<TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&String>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    bound_equality_classlike: Option<&String>,
    depth: usize,
    was_single: bool,
    had_template: &mut bool,
) -> Vec<TAtomic> {
    let normalized_key = if let TAtomic::TNamedObject { name, .. } = atomic_type {
        name.clone()
    } else if let TAtomic::TTypeAlias { name, .. } = atomic_type {
        name.clone()
    } else {
        key.clone()
    };

    if let TAtomic::TTemplateParam {
        param_name,
        defining_entity,
        ..
    } = atomic_type
    {
        if let Some(template_type) = template_types_contains(
            &template_result.template_types.clone(),
            param_name,
            defining_entity,
        ) {
            return handle_template_param_standin(
                atomic_type,
                &normalized_key,
                template_type,
                template_result,
                codebase,
                input_type,
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                bound_equality_classlike,
                depth,
                had_template,
            );
        }
    }

    if let TAtomic::TTemplateParamClass {
        param_name,
        defining_entity,
        ..
    } = atomic_type
    {
        if let Some(_) = template_types_contains(
            &template_result.template_types.clone(),
            param_name,
            defining_entity,
        ) {
            if replace {
                return handle_template_param_class_standin(
                    atomic_type,
                    template_result,
                    codebase,
                    input_type,
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    true,
                    add_lower_bound,
                    bound_equality_classlike,
                    depth,
                    was_single,
                );
            }
        }
    }

    if let TAtomic::TTemplateParamType {
        param_name,
        defining_entity,
        ..
    } = atomic_type
    {
        if let Some(_) = template_types_contains(
            &template_result.template_types.clone(),
            param_name,
            defining_entity,
        ) {
            if replace {
                return handle_template_param_type_standin(
                    atomic_type,
                    template_result,
                    codebase,
                    input_type,
                    input_arg_offset,
                    calling_class,
                    depth,
                    was_single,
                );
            }
        }
    }

    let mut matching_atomic_types = Vec::new();

    if let Some(input_type) = input_type {
        if !input_type.is_mixed() {
            matching_atomic_types = find_matching_atomic_types_for_template(
                atomic_type,
                &normalized_key,
                codebase,
                input_type,
            );
        } else {
            matching_atomic_types.push(input_type.get_single().clone());
        }
    }

    if matching_atomic_types.is_empty() {
        let atomic_type = replace_atomic(
            atomic_type,
            template_result,
            codebase,
            None,
            input_arg_offset,
            calling_class,
            calling_function,
            replace,
            add_lower_bound,
            depth + 1,
        );

        return vec![atomic_type];
    }

    let mut atomic_types = Vec::new();

    for matching_atomic_type in matching_atomic_types {
        atomic_types.push(replace_atomic(
            atomic_type,
            template_result,
            codebase,
            Some(matching_atomic_type),
            input_arg_offset,
            calling_class,
            calling_function,
            replace,
            add_lower_bound,
            depth + 1,
        ))
    }

    atomic_types
}

fn replace_atomic(
    atomic_type: &TAtomic,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    input_type: Option<TAtomic>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&String>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    depth: usize,
) -> TAtomic {
    let mut atomic_type = atomic_type.clone();

    if let TAtomic::TDict {
        ref mut known_items,
        ref mut key_param,
        ref mut value_param,
        ..
    } = atomic_type
    {
        if let Some(ref mut known_items) = known_items {
            for (offset, (_, property)) in known_items {
                let input_type_param = if let Some(TAtomic::TDict {
                    known_items: Some(ref input_known_items),
                    ..
                }) = input_type
                {
                    if let Some((_, t)) = input_known_items.get(offset) {
                        Some((**t).clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                *property = Arc::new(self::replace(
                    property,
                    template_result,
                    codebase,
                    &input_type_param,
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    None,
                    depth,
                ));
            }
        } else {
            let input_params = if let Some(TAtomic::TDict { .. }) = &input_type {
                get_arrayish_params(&input_type.unwrap(), codebase)
            } else {
                None
            };

            *key_param = self::replace(
                &key_param,
                template_result,
                codebase,
                &if let Some(input_params) = &input_params {
                    Some(input_params.0.clone())
                } else {
                    None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                None,
                depth,
            );

            *value_param = self::replace(
                &value_param,
                template_result,
                codebase,
                &if let Some(input_params) = &input_params {
                    Some(input_params.1.clone())
                } else {
                    None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                None,
                depth,
            );
        }

        return atomic_type;
    }

    if let TAtomic::TVec {
        ref mut known_items,
        ref mut type_param,
        ..
    } = atomic_type
    {
        if let Some(known_items) = known_items {
            for (offset, (_, property)) in known_items {
                let input_type_param = if let Some(TAtomic::TVec {
                    known_items: Some(ref input_known_items),
                    ..
                }) = input_type
                {
                    if let Some((_, t)) = input_known_items.get(offset) {
                        Some(t)
                    } else {
                        None
                    }
                } else {
                    None
                };

                *property = self::replace(
                    property,
                    template_result,
                    codebase,
                    &input_type_param.cloned(),
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    None,
                    depth,
                );
            }
        } else {
            let input_param = if let Some(TAtomic::TVec { .. }) = &input_type {
                get_value_param(&input_type.unwrap(), codebase)
            } else {
                None
            };

            *type_param = self::replace(
                &type_param,
                template_result,
                codebase,
                &if let Some(input_param) = input_param {
                    Some(input_param)
                } else {
                    None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                None,
                depth,
            );
        }

        return atomic_type;
    }

    if let TAtomic::TKeyset {
        ref mut type_param, ..
    } = atomic_type
    {
        *type_param = self::replace(
            &type_param,
            template_result,
            codebase,
            &if let Some(TAtomic::TKeyset {
                type_param: input_param,
            }) = &input_type
            {
                Some(input_param.clone())
            } else {
                None
            },
            input_arg_offset,
            calling_class,
            calling_function,
            replace,
            add_lower_bound,
            None,
            depth,
        );

        return atomic_type;
    }

    if let TAtomic::TNamedObject {
        ref mut type_params,
        ref name,
        remapped_params,
        ..
    } = atomic_type
    {
        if let Some(ref mut type_params) = type_params {
            let mapped_type_params = if let Some(TAtomic::TNamedObject {
                type_params: Some(_),
                ..
            }) = &input_type
            {
                Some(get_mapped_generic_type_params(
                    codebase,
                    &input_type.clone().unwrap(),
                    name,
                    remapped_params,
                ))
            } else {
                None
            };

            let mut offset = 0;
            for type_param in type_params {
                let input_type_param = match &input_type {
                    Some(input_inner) => match input_inner {
                        TAtomic::TNamedObject {
                            type_params: Some(ref input_type_parts),
                            ..
                        } => input_type_parts.get(offset).cloned(),
                        TAtomic::TDict { .. } | TAtomic::TVec { .. } | TAtomic::TKeyset { .. } => {
                            let (key_param, value_param) =
                                get_arrayish_params(&input_inner, codebase).unwrap();
                            if name == "HH\\KeyedContainer" || name == "HH\\KeyedTraversable" {
                                if offset == 0 {
                                    Some(key_param)
                                } else {
                                    Some(value_param)
                                }
                            } else if name == "HH\\Container" || name == "HH\\Traversable" {
                                Some(value_param)
                            } else {
                                None
                            }
                        }
                        TAtomic::TMixedFromLoopIsset => Some(get_mixed_maybe_from_loop(true)),
                        TAtomic::TMixed | TAtomic::TNonnullMixed | TAtomic::TTruthyMixed => {
                            Some(get_mixed())
                        }
                        TAtomic::TMixedAny => Some(get_mixed_any()),
                        _ => None,
                    },
                    _ => None,
                };

                *type_param = self::replace(
                    type_param,
                    template_result,
                    codebase,
                    &if let Some(mapped_type_params) = &mapped_type_params {
                        mapped_type_params.get(offset).cloned()
                    } else {
                        input_type_param
                    },
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    None,
                    depth,
                );

                offset += 1;
            }
        }

        return atomic_type;
    }

    if let TAtomic::TTypeAlias {
        ref mut type_params,
        ref name,
        ..
    } = atomic_type
    {
        if let Some(ref mut type_params) = type_params {
            let mapped_type_params = if let Some(TAtomic::TTypeAlias {
                type_params: Some(input_type_params),
                name: input_name,
                ..
            }) = &input_type
            {
                if input_name == name {
                    Some(input_type_params)
                } else {
                    None
                }
            } else {
                None
            };

            let mut offset = 0;
            for type_param in type_params {
                *type_param = self::replace(
                    type_param,
                    template_result,
                    codebase,
                    &if let Some(mapped_type_params) = &mapped_type_params {
                        mapped_type_params.get(offset).cloned()
                    } else {
                        None
                    },
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    None,
                    depth,
                );

                offset += 1;
            }
        }

        return atomic_type;
    }

    if let TAtomic::TClosure {
        ref mut params,
        ref mut return_type,
        ..
    } = atomic_type
    {
        let mut offset = 0;
        for param in params {
            let input_type_param = if let Some(TAtomic::TClosure {
                params: input_params,
                ..
            }) = &input_type
            {
                if let Some(param) = input_params.get(offset) {
                    &param.signature_type
                } else {
                    &None
                }
            } else {
                &None
            };

            if let Some(ref mut param_type) = param.signature_type {
                *param_type = self::replace(
                    &param_type,
                    template_result,
                    codebase,
                    &input_type_param.clone(),
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    !add_lower_bound,
                    None,
                    depth,
                );
            }

            offset += 1;
        }

        if let Some(ref mut return_type) = return_type {
            *return_type = self::replace(
                &return_type,
                template_result,
                codebase,
                if let Some(TAtomic::TClosure {
                    return_type: input_return_type,
                    ..
                }) = &input_type
                {
                    &input_return_type
                } else {
                    &None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                None,
                depth - 1,
            );
        }

        return atomic_type;
    }

    atomic_type.clone()
}

fn handle_template_param_standin(
    atomic_type: &TAtomic,
    normalized_key: &String,
    template_type: &TUnion,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    input_type: &Option<TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&String>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    bound_equality_classlike: Option<&String>,
    depth: usize,
    had_template: &mut bool,
) -> Vec<TAtomic> {
    let (param_name, defining_entity, extra_types, as_type) = if let TAtomic::TTemplateParam {
        param_name,
        defining_entity,
        extra_types,
        as_type,
        ..
    } = atomic_type
    {
        (param_name, defining_entity, extra_types, as_type)
    } else {
        panic!()
    };

    if let Some(calling_class) = calling_class {
        if defining_entity == calling_class {
            return vec![atomic_type.clone()];
        }
    }

    if &template_type.get_id() == normalized_key {
        return template_type
            .clone()
            .types
            .into_iter()
            .map(|(_, v)| v)
            .collect();
    }

    let mut replacement_type = template_type.clone();

    let param_name_key = if normalized_key.contains("&") {
        normalized_key.clone()
    } else {
        param_name.clone()
    };

    let mut new_extra_types = FxHashMap::default();

    if let Some(extra_types) = extra_types {
        for (_, extra_type) in extra_types {
            let extra_type_union = self::replace(
                &TUnion::new(vec![extra_type.clone()]),
                template_result,
                codebase,
                input_type,
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                bound_equality_classlike,
                depth + 1,
            );

            if extra_type_union.is_single() {
                let extra_type = extra_type_union.get_single().clone();

                if let TAtomic::TNamedObject { .. } | TAtomic::TTemplateParam { .. } = extra_type {
                    new_extra_types.insert(extra_type.get_key(), extra_type);
                }
            }
        }
    }

    if replace {
        let mut atomic_types = Vec::new();

        if replacement_type.is_mixed() && !as_type.is_mixed() {
            for (_, as_atomic_type) in &as_type.types {
                atomic_types.push(as_atomic_type.clone());
            }
        } else {
            type_expander::expand_union(
                codebase,
                &mut replacement_type,
                &TypeExpansionOptions {
                    self_class: calling_class,
                    static_class_type: if let Some(c) = calling_class {
                        StaticClassType::Name(c)
                    } else {
                        StaticClassType::None
                    },

                    expand_templates: false,

                    ..Default::default()
                },
                &mut DataFlowGraph::new(GraphKind::Variable),
            );

            if depth < 10 && replacement_type.has_template_types() {
                replacement_type = self::replace(
                    &replacement_type,
                    template_result,
                    codebase,
                    input_type,
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    true,
                    add_lower_bound,
                    bound_equality_classlike,
                    depth + 1,
                );
            }

            for (_, replacement_atomic_type) in &replacement_type.types {
                let mut replacements_found = false;

                if let TAtomic::TTemplateParam {
                    defining_entity: replacement_defining_entity,
                    as_type: replacement_as_type,
                    ..
                } = replacement_atomic_type
                {
                    if replacement_defining_entity != calling_class.unwrap_or(&"".to_string())
                        && (calling_function.is_none()
                            || replacement_defining_entity
                                != &format!("fn-{}", calling_function.unwrap().to_string()))
                    {
                        for (_, nested_type_atomic) in &replacement_as_type.types {
                            replacements_found = true;
                            atomic_types.push(nested_type_atomic.clone());
                        }
                    }
                }

                if !replacements_found {
                    atomic_types.push(replacement_atomic_type.clone());
                }

                *had_template = true;
            }
        }

        let mut matching_input_keys = Vec::new();

        let mut as_type = as_type.clone();

        type_expander::expand_union(
            codebase,
            &mut as_type,
            &TypeExpansionOptions {
                self_class: calling_class,
                static_class_type: if let Some(c) = calling_class {
                    StaticClassType::Name(c)
                } else {
                    StaticClassType::None
                },

                expand_templates: false,

                ..Default::default()
            },
            &mut DataFlowGraph::new(GraphKind::Variable),
        );

        let as_type = self::replace(
            &as_type,
            template_result,
            codebase,
            input_type,
            input_arg_offset,
            calling_class,
            calling_function,
            true,
            add_lower_bound,
            bound_equality_classlike,
            depth + 1,
        );

        if let Some(input_type) = input_type {
            if !template_result.readonly
                && (as_type.is_mixed()
                    || union_type_comparator::can_be_contained_by(
                        codebase,
                        &input_type,
                        &as_type,
                        false,
                        false,
                        &mut matching_input_keys,
                    ))
            {
                let mut generic_param = input_type.clone();

                if !matching_input_keys.is_empty() {
                    for (atomic_key, _) in &generic_param.clone().types {
                        if !matching_input_keys.contains(atomic_key) {
                            generic_param.types.remove(atomic_key);
                        }
                    }
                }

                if add_lower_bound {
                    return generic_param.types.into_iter().map(|(_, v)| v).collect();
                }

                if let Some(existing_lower_bounds) =
                    if let Some(mapped_bounds) = template_result.lower_bounds.get(&param_name_key) {
                        mapped_bounds.get(defining_entity)
                    } else {
                        None
                    }
                {
                    let mut has_matching_lower_bound = false;

                    for existing_lower_bound in existing_lower_bounds {
                        let existing_depth = &existing_lower_bound.appearance_depth;
                        let existing_arg_offset = if let None = &existing_lower_bound.arg_offset {
                            &input_arg_offset
                        } else {
                            &existing_lower_bound.arg_offset
                        };

                        if existing_depth == &depth
                            && &input_arg_offset == existing_arg_offset
                            && existing_lower_bound.bound_type.get_id() == generic_param.get_id()
                            && existing_lower_bound.equality_bound_classlike.as_ref()
                                == bound_equality_classlike
                        {
                            has_matching_lower_bound = true;
                            break;
                        }
                    }

                    if !has_matching_lower_bound {
                        template_result
                            .lower_bounds
                            .entry(param_name_key)
                            .or_insert_with(FxHashMap::default)
                            .entry(defining_entity.clone())
                            .or_insert_with(Vec::new)
                            .push(TemplateBound {
                                bound_type: generic_param,
                                appearance_depth: depth,
                                arg_offset: input_arg_offset,
                                equality_bound_classlike: bound_equality_classlike.cloned(),
                            });
                    }
                } else {
                    template_result
                        .lower_bounds
                        .entry(param_name_key)
                        .or_insert_with(FxHashMap::default)
                        .entry(defining_entity.clone())
                        .or_insert(vec![TemplateBound {
                            bound_type: generic_param,
                            appearance_depth: depth,
                            arg_offset: input_arg_offset,
                            equality_bound_classlike: bound_equality_classlike.cloned(),
                        }]);
                }
            }
        }

        let mut new_atomic_types = Vec::new();

        for mut atomic_type in atomic_types {
            if let TAtomic::TNamedObject {
                extra_types: ref mut atomic_extra_types,
                ..
            }
            | TAtomic::TTemplateParam {
                extra_types: ref mut atomic_extra_types,
                ..
            } = atomic_type
            {
                *atomic_extra_types = if new_extra_types.is_empty() {
                    None
                } else {
                    Some(new_extra_types.clone())
                };
            }

            new_atomic_types.push(atomic_type);
        }

        return new_atomic_types;
    }

    if add_lower_bound && !template_result.readonly {
        if let Some(input_type) = input_type {
            let mut matching_input_keys = Vec::new();

            if union_type_comparator::can_be_contained_by(
                codebase,
                &input_type,
                &replacement_type,
                false,
                false,
                &mut matching_input_keys,
            ) {
                let mut generic_param = input_type.clone();

                if !matching_input_keys.is_empty() {
                    for (atomic_key, _) in &generic_param.clone().types {
                        if !matching_input_keys.contains(atomic_key) {
                            generic_param.types.remove(atomic_key);
                        }
                    }
                }

                let new_upper_bound = if let Some(upper_bound) =
                    if let Some(mapped_bounds) = template_result.upper_bounds.get(&param_name_key) {
                        mapped_bounds.get(defining_entity)
                    } else {
                        None
                    } {
                    let intersection_type = if !union_type_comparator::is_contained_by(
                        codebase,
                        &upper_bound.bound_type,
                        &generic_param,
                        false,
                        false,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) || !union_type_comparator::is_contained_by(
                        codebase,
                        &generic_param,
                        &upper_bound.bound_type,
                        false,
                        false,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) {
                        intersect_union_types(
                            &upper_bound.bound_type,
                            &generic_param,
                            Some(codebase),
                        )
                    } else {
                        Some(generic_param.clone())
                    };

                    let mut new_bound = upper_bound.clone();

                    if let Some(intersection_type) = intersection_type {
                        new_bound.bound_type = intersection_type;
                    } else {
                        template_result
                            .upper_bounds_unintersectable_types
                            .push(new_bound.bound_type.clone());
                        template_result
                            .upper_bounds_unintersectable_types
                            .push(generic_param.clone());

                        new_bound.bound_type = get_mixed_any();
                    }

                    new_bound
                } else {
                    TemplateBound {
                        bound_type: get_mixed_any(),
                        appearance_depth: 0,
                        arg_offset: None,
                        equality_bound_classlike: None,
                    }
                };

                template_result
                    .upper_bounds
                    .entry(param_name_key)
                    .or_insert_with(FxHashMap::default)
                    .insert(defining_entity.clone(), new_upper_bound);
            }
        }
    }

    vec![atomic_type.clone()]
}

fn handle_template_param_class_standin(
    atomic_type: &TAtomic,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    input_type: &Option<TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&String>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    bound_equality_classlike: Option<&String>,
    depth: usize,
    was_single: bool,
) -> Vec<TAtomic> {
    if let TAtomic::TTemplateParamClass {
        defining_entity,
        as_type,
        param_name,
        ..
    } = atomic_type
    {
        let mut atomic_type_as = *as_type.clone();
        if defining_entity == calling_class.unwrap_or(&"".to_string()) {
            return vec![atomic_type.clone()];
        }

        let mut atomic_types = vec![];

        if let Some(input_type) = if let Some(input_type) = input_type {
            if !template_result.readonly {
                Some(input_type)
            } else {
                None
            }
        } else {
            None
        } {
            let mut valid_input_atomic_types = vec![];

            for (_, input_atomic_type) in &input_type.types {
                if let TAtomic::TLiteralClassname { name } = input_atomic_type {
                    valid_input_atomic_types.push(TAtomic::TNamedObject {
                        name: name.clone(),
                        type_params: None,
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    });
                } else if let TAtomic::TTemplateParamClass {
                    param_name,
                    as_type,
                    defining_entity,
                    ..
                } = input_atomic_type
                {
                    valid_input_atomic_types.push(TAtomic::TTemplateParam {
                        param_name: param_name.clone(),
                        as_type: wrap_atomic(*as_type.clone()),
                        defining_entity: defining_entity.clone(),
                        from_class: false,
                        extra_types: None,
                    });
                } else if let TAtomic::TClassname { .. } = input_atomic_type {
                    valid_input_atomic_types.push(atomic_type_as.clone());
                }
            }

            let generic_param = if !valid_input_atomic_types.is_empty() {
                Some(TUnion::new(valid_input_atomic_types))
            } else if was_single {
                Some(get_mixed_any())
            } else {
                None
            };

            // sometimes templated class-strings can contain nested templates
            // in the as type that need to be resolved as well.

            let as_type_union = self::replace(
                &TUnion::new(vec![atomic_type_as.clone()]),
                template_result,
                codebase,
                &generic_param,
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                bound_equality_classlike,
                depth,
            );

            atomic_type_as = if as_type_union.is_single() {
                as_type_union.get_single().clone()
            } else {
                TAtomic::TObject
            };

            if let Some(generic_param) = generic_param {
                if let Some(template_bounds) = template_result
                    .lower_bounds
                    .get_mut(param_name)
                    .unwrap_or(&mut FxHashMap::default())
                    .get_mut(defining_entity)
                {
                    *template_bounds = vec![TemplateBound::new(
                        add_union_type(
                            generic_param,
                            &get_most_specific_type_from_bounds(&template_bounds, Some(codebase)),
                            Some(codebase),
                            false,
                        ),
                        depth,
                        input_arg_offset,
                        None,
                    )]
                } else {
                    template_result
                        .lower_bounds
                        .entry(param_name.clone())
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            defining_entity.clone(),
                            vec![TemplateBound::new(
                                generic_param,
                                depth,
                                input_arg_offset,
                                None,
                            )],
                        );
                }
            }
        } else {
            let template_type = template_result
                .template_types
                .get(param_name)
                .unwrap()
                .get(defining_entity)
                .unwrap();

            for (_, template_atomic_type) in &template_type.types {
                if let TAtomic::TNamedObject { .. } | TAtomic::TObject = &template_atomic_type {
                    atomic_types.push(TAtomic::TClassname {
                        as_type: Box::new(template_atomic_type.clone()),
                    });
                }
            }
        }

        if atomic_types.is_empty() {
            if let TAtomic::TTemplateParam {
                param_name,
                defining_entity,
                ..
            } = &atomic_type_as
            {
                atomic_types.push(TAtomic::TTemplateParamClass {
                    param_name: param_name.clone(),
                    as_type: Box::new(atomic_type_as.clone()),
                    defining_entity: defining_entity.clone(),
                });
            } else {
                atomic_types.push(TAtomic::TClassname {
                    as_type: Box::new(atomic_type_as),
                });
            }
        }

        atomic_types
    } else {
        panic!();
    }
}

fn handle_template_param_type_standin(
    atomic_type: &TAtomic,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    input_type: &Option<TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&String>,
    depth: usize,
    was_single: bool,
) -> Vec<TAtomic> {
    if let TAtomic::TTemplateParamType {
        defining_entity,
        param_name,
        ..
    } = atomic_type
    {
        if defining_entity == calling_class.unwrap_or(&"".to_string()) {
            return vec![atomic_type.clone()];
        }

        let mut atomic_types = vec![];

        if let Some(input_type) = if let Some(input_type) = input_type {
            if !template_result.readonly {
                Some(input_type)
            } else {
                None
            }
        } else {
            None
        } {
            let mut valid_input_atomic_types = vec![];

            for (_, input_atomic_type) in &input_type.types {
                if let TAtomic::TLiteralClassname { name } = input_atomic_type {
                    valid_input_atomic_types.push(TAtomic::TTypeAlias {
                        name: name.clone(),
                        type_params: None,
                    });
                } else if let TAtomic::TTemplateParamType {
                    param_name,
                    defining_entity,
                    ..
                } = input_atomic_type
                {
                    valid_input_atomic_types.push(TAtomic::TTemplateParam {
                        param_name: param_name.clone(),
                        as_type: get_mixed_any(),
                        defining_entity: defining_entity.clone(),
                        from_class: false,
                        extra_types: None,
                    });
                }
            }

            let generic_param = if !valid_input_atomic_types.is_empty() {
                Some(TUnion::new(valid_input_atomic_types))
            } else if was_single {
                Some(get_mixed_any())
            } else {
                None
            };

            if let Some(generic_param) = generic_param {
                if let Some(template_bounds) = template_result
                    .lower_bounds
                    .get_mut(param_name)
                    .unwrap_or(&mut FxHashMap::default())
                    .get_mut(defining_entity)
                {
                    *template_bounds = vec![TemplateBound::new(
                        add_union_type(
                            generic_param,
                            &get_most_specific_type_from_bounds(&template_bounds, Some(codebase)),
                            Some(codebase),
                            false,
                        ),
                        depth,
                        input_arg_offset,
                        None,
                    )]
                } else {
                    template_result
                        .lower_bounds
                        .entry(param_name.clone())
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            defining_entity.clone(),
                            vec![TemplateBound::new(
                                generic_param,
                                depth,
                                input_arg_offset,
                                None,
                            )],
                        );
                }
            }
        } else {
            let template_type = template_result
                .template_types
                .get(param_name)
                .unwrap()
                .get(defining_entity)
                .unwrap();

            for (_, template_atomic_type) in &template_type.types {
                if let TAtomic::TNamedObject { .. } | TAtomic::TObject = &template_atomic_type {
                    atomic_types.push(TAtomic::TClassname {
                        as_type: Box::new(template_atomic_type.clone()),
                    });
                }
            }
        }

        if atomic_types.is_empty() {
            atomic_types.push(TAtomic::TString);
        }

        atomic_types
    } else {
        panic!();
    }
}

fn template_types_contains<'a>(
    template_types: &'a IndexMap<String, FxHashMap<String, TUnion>>,
    param_name: &String,
    defining_entity: &String,
) -> Option<&'a TUnion> {
    if let Some(mapped_classes) = template_types.get(param_name) {
        return mapped_classes.get(defining_entity);
    }

    None
}

/**
   This method attempts to find bits of the input type (normally the argument type of a method call)
   that match the base type (normally the param type of the method). These matches are used to infer
   more template types

   Example: when passing `vec<string>` to a function that expects `array<T>`, a rule in this method
   identifies the matching atomic types for `T` as `string`
*/
fn find_matching_atomic_types_for_template(
    base_type: &TAtomic,
    normalized_key: &String,
    codebase: &CodebaseInfo,
    input_type: &TUnion,
) -> Vec<TAtomic> {
    let mut matching_atomic_types = Vec::new();

    for (input_key, atomic_input_type) in &input_type.types {
        let input_key = &if let TAtomic::TNamedObject { name, .. } = atomic_input_type {
            name.clone()
        } else if let TAtomic::TTypeAlias { name, .. } = atomic_input_type {
            name.clone()
        } else {
            input_key.clone()
        };

        if input_key == normalized_key {
            matching_atomic_types.push(atomic_input_type.clone());
            continue;
        }

        if matches!(atomic_input_type, TAtomic::TClosure { .. })
            && matches!(base_type, TAtomic::TClosure { .. })
        {
            matching_atomic_types.push(atomic_input_type.clone());
            continue;
        }

        if let TAtomic::TDict { .. } | TAtomic::TVec { .. } | TAtomic::TKeyset { .. } =
            atomic_input_type
        {
            if is_array_container(normalized_key) {
                matching_atomic_types.push(atomic_input_type.clone());
                continue;
            }
        }

        // todo handle intersections

        if let TAtomic::TLiteralClassname {
            name: atomic_class_name,
        } = atomic_input_type
        {
            if let TAtomic::TClassname {
                as_type: base_as_type,
                ..
            } = base_type
            {
                if let TAtomic::TNamedObject { name: as_value, .. } = &**base_as_type {
                    let classlike_info = codebase.classlike_infos.get(atomic_class_name);

                    if let Some(classlike_info) = classlike_info {
                        if let Some(extended_params) =
                            classlike_info.template_extended_params.get(as_value)
                        {
                            matching_atomic_types.push(TAtomic::TClassname {
                                as_type: Box::new(TAtomic::TNamedObject {
                                    name: as_value.clone(),
                                    type_params: Some(
                                        extended_params
                                            .clone()
                                            .into_iter()
                                            .map(|(_, v)| v)
                                            .collect::<Vec<TUnion>>(),
                                    ),
                                    is_this: false,
                                    extra_types: None,
                                    remapped_params: false,
                                }),
                            });
                            continue;
                        }
                    }
                }
            }
        }

        if let TAtomic::TNamedObject {
            name: input_name,
            type_params: input_type_params,
            ..
        } = atomic_input_type
        {
            if let TAtomic::TNamedObject {
                name: base_name, ..
            } = base_type
            {
                let classlike_info = if let Some(c) = codebase.classlike_infos.get(input_name) {
                    c
                } else {
                    println!("Cannot locate class {}", input_name);
                    matching_atomic_types.push(TAtomic::TObject);
                    continue;
                };

                if let Some(_) = input_type_params {
                    if let Some(_) = classlike_info.template_extended_params.get(base_name) {
                        matching_atomic_types.push(atomic_input_type.clone());
                        continue;
                    }
                }

                if let Some(extended_params) =
                    classlike_info.template_extended_params.get(base_name)
                {
                    matching_atomic_types.push(TAtomic::TNamedObject {
                        name: input_name.clone(),
                        type_params: Some(
                            extended_params
                                .clone()
                                .into_iter()
                                .map(|(_, v)| v)
                                .collect::<Vec<TUnion>>(),
                        ),
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    });
                    continue;
                }
            }
        }

        if let TAtomic::TTemplateParam { as_type, .. } = atomic_input_type {
            matching_atomic_types.extend(find_matching_atomic_types_for_template(
                base_type,
                normalized_key,
                codebase,
                as_type,
            ));
        }
    }
    matching_atomic_types
}

pub(crate) fn get_mapped_generic_type_params(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_name: &String,
    container_remapped_params: bool,
) -> Vec<TUnion> {
    let mut input_type_params = match input_type_part {
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params.clone(),
        _ => panic!(),
    };

    let input_name = match input_type_part {
        TAtomic::TNamedObject { name, .. } => name,
        _ => panic!(),
    };

    let input_class_storage = codebase.classlike_infos.get(input_name).unwrap();

    if input_name == container_name {
        return input_type_params.clone();
    }

    let input_template_types = &input_class_storage.template_types;

    let mut i = 0;

    let mut replacement_templates: IndexMap<String, FxHashMap<String, TUnion>> = IndexMap::new();

    if matches!(
        input_type_part,
        TAtomic::TNamedObject {
            remapped_params: false,
            ..
        }
    ) && !container_remapped_params
    {
        for (template_name, _) in input_template_types {
            if let Some(input_type) = input_type_params.get(i) {
                replacement_templates
                    .entry(template_name.clone())
                    .or_insert_with(FxHashMap::default)
                    .insert(input_name.clone(), input_type.clone());

                i += 1;
            } else {
                break;
            }
        }
    }

    let template_extends = &input_class_storage.template_extended_params;

    if let Some(params) = template_extends.get(container_name) {
        let mut new_input_params = Vec::new();

        for (_, extended_input_param) in params {
            let mut new_input_param = None;

            for (_, et) in &extended_input_param.types {
                let ets = get_extended_templated_types(&et, template_extends);

                let mut candidate_param_type: Option<TUnion> = None;

                if let Some(TAtomic::TTemplateParam {
                    param_name,
                    defining_entity,
                    ..
                }) = ets.get(0)
                {
                    if let Some(defining_classes) =
                        input_class_storage.template_types.get(param_name)
                    {
                        if defining_classes.contains_key(defining_entity) {
                            let old_params_offset = input_class_storage
                                .template_types
                                .keys()
                                .position(|x| x == param_name)
                                .unwrap();

                            candidate_param_type = Some(
                                input_type_params
                                    .get(old_params_offset)
                                    .unwrap_or(&get_mixed_any())
                                    .clone(),
                            );
                        }
                    }
                }

                let mut candidate_param_type =
                    candidate_param_type.unwrap_or(wrap_atomic(et.clone()));

                candidate_param_type.from_template_default = true;

                new_input_param = if let Some(new_input_param) = new_input_param {
                    Some(add_union_type(
                        new_input_param,
                        &candidate_param_type,
                        None,
                        true,
                    ))
                } else {
                    Some(candidate_param_type.clone())
                };
            }

            new_input_params.push(inferred_type_replacer::replace(
                &new_input_param.unwrap(),
                &TemplateResult::new(IndexMap::new(), replacement_templates.clone()),
                Some(codebase),
            ));
        }

        input_type_params = new_input_params;
    }

    input_type_params
}

pub fn get_extended_templated_types<'a>(
    atomic_type: &'a TAtomic,
    extends: &'a FxHashMap<String, IndexMap<String, TUnion>>,
) -> Vec<&'a TAtomic> {
    let mut extra_added_types = Vec::new();

    if let TAtomic::TTemplateParam {
        defining_entity,
        param_name,
        ..
    } = atomic_type
    {
        if let Some(defining_params) = extends.get(defining_entity) {
            if let Some(extended_param) = defining_params.get(param_name) {
                for (_, extended_atomic_type) in &extended_param.types {
                    if let TAtomic::TTemplateParam { .. } = extended_atomic_type {
                        extra_added_types
                            .extend(get_extended_templated_types(&extended_atomic_type, extends));
                    } else {
                        extra_added_types.push(&extended_atomic_type);
                    }
                }
            } else {
                extra_added_types.push(atomic_type);
            }
        } else {
            extra_added_types.push(atomic_type);
        }
    }

    extra_added_types
}

pub(crate) fn get_root_template_type(
    lower_bounds: &IndexMap<String, FxHashMap<String, Vec<TemplateBound>>>,
    param_name: &String,
    defining_entity: &String,
    mut visited_entities: FxHashSet<String>,
    codebase: Option<&CodebaseInfo>,
) -> Option<TUnion> {
    if visited_entities.contains(defining_entity) {
        return None;
    }

    if let Some(mapped) = lower_bounds.get(param_name) {
        if let Some(bounds) = mapped.get(defining_entity) {
            let mapped_type = get_most_specific_type_from_bounds(bounds, codebase);

            if !mapped_type.is_single() {
                return Some(mapped_type);
            }

            let first_template = &mapped_type.get_single();

            if let TAtomic::TTemplateParam {
                param_name,
                defining_entity,
                ..
            } = first_template
            {
                visited_entities.insert(defining_entity.clone());
                return Some(
                    get_root_template_type(
                        lower_bounds,
                        param_name,
                        defining_entity,
                        visited_entities,
                        codebase,
                    )
                    .unwrap_or(mapped_type),
                );
            }

            return Some(mapped_type.clone());
        }
    }

    None
}

pub fn get_most_specific_type_from_bounds(
    lower_bounds: &Vec<TemplateBound>,
    codebase: Option<&CodebaseInfo>,
) -> TUnion {
    if lower_bounds.len() == 1 {
        return lower_bounds.get(0).unwrap().bound_type.clone();
    }

    let mut lower_bounds = lower_bounds.into_iter().collect::<Vec<_>>();
    lower_bounds.sort_by(|a, b| a.appearance_depth.partial_cmp(&b.appearance_depth).unwrap());

    let mut current_depth = None;
    let mut current_type: Option<TUnion> = None;
    let mut had_invariant = false;
    let mut last_arg_offset = None;

    for template_bound in lower_bounds {
        if let Some(inner) = current_depth {
            if inner != template_bound.appearance_depth {
                if let Some(current_type) = &current_type {
                    if !current_type.is_nothing()
                        && (!had_invariant || last_arg_offset == template_bound.arg_offset)
                    {
                        // escape switches when matching on invariant generic params
                        // and when matching
                        break;
                    }

                    current_depth = Some(template_bound.appearance_depth);
                }
            }
        } else {
            current_depth = Some(template_bound.appearance_depth);
        }

        had_invariant = if had_invariant {
            true
        } else {
            template_bound.equality_bound_classlike.is_some()
        };

        current_type = Some(combine_optional_union_types(
            current_type.as_ref(),
            Some(&template_bound.bound_type),
            codebase,
        ));

        last_arg_offset = template_bound.arg_offset.clone();
    }

    current_type.unwrap_or(get_mixed_any())
}
