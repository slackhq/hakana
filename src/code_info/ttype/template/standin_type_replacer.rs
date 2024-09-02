use crate::{
    codebase_info::CodebaseInfo,
    data_flow::graph::{DataFlowGraph, GraphKind},
    t_atomic::TAtomic,
    t_union::TUnion,
};
use crate::{function_context::FunctionLikeIdentifier, GenericParent};
use crate::{
    t_atomic::TDict,
    ttype::{
        add_union_type,
        comparison::{type_comparison_result::TypeComparisonResult, union_type_comparator},
        get_arrayish_params, get_arraykey, get_mixed, get_mixed_any, get_mixed_maybe_from_loop,
        get_value_param, intersect_union_types, type_combiner,
        type_expander::{self, StaticClassType, TypeExpansionOptions},
        wrap_atomic,
    },
};
use hakana_str::{Interner, StrId};
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

use super::{inferred_type_replacer, TemplateBound, TemplateResult};

pub fn replace(
    union_type: &TUnion,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    input_type: &Option<&TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&StrId>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,         // true
    add_lower_bound: bool, // false
    depth: usize,          // 1
) -> TUnion {
    let mut atomic_types = Vec::new();

    let original_atomic_types = union_type.types.clone();

    let mut input_type = input_type.cloned();

    if let Some(ref mut input_type_inner) = input_type {
        if !input_type_inner.is_single() {
            // here we want to subtract atomic types from the input type
            // when they're also in the union type, so those shared atomic
            // types will never be inferred as part of the generic type
            for original_atomic_type in &original_atomic_types {
                input_type_inner.remove_type(original_atomic_type);
            }

            if input_type_inner.types.is_empty() {
                return union_type.clone();
            }
        }
    }

    let mut had_template = false;

    for atomic_type in original_atomic_types.iter() {
        atomic_types.extend(handle_atomic_standin(
            atomic_type,
            template_result,
            codebase,
            interner,
            &input_type.as_ref(),
            input_arg_offset,
            calling_class,
            calling_function,
            replace,
            add_lower_bound,
            depth,
            original_atomic_types.len() == 1,
            &mut had_template,
        ))
    }

    if replace {
        if atomic_types.is_empty() {
            return union_type.clone();
        }

        let mut new_union_type = TUnion::new(if atomic_types.len() > 1 {
            type_combiner::combine(atomic_types, codebase, false)
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
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    input_type: &Option<&TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&StrId>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    depth: usize,
    was_single: bool,
    had_template: &mut bool,
) -> Vec<TAtomic> {
    let normalized_key = if let TAtomic::TNamedObject { name, .. } = atomic_type {
        name.0.to_string()
    } else if let TAtomic::TTypeAlias { name, .. } = atomic_type {
        name.0.to_string()
    } else {
        atomic_type.get_key()
    };

    if let TAtomic::TGenericParam {
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
                interner,
                input_type,
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                depth,
                had_template,
            );
        }
    }

    if let TAtomic::TGenericClassname {
        param_name,
        defining_entity,
        ..
    } = atomic_type
    {
        if template_types_contains(
            &template_result.template_types.clone(),
            param_name,
            defining_entity,
        )
        .is_some()
            && replace
        {
            return handle_template_param_class_standin(
                atomic_type,
                template_result,
                codebase,
                interner,
                input_type,
                input_arg_offset,
                calling_class,
                calling_function,
                true,
                add_lower_bound,
                depth,
                was_single,
            );
        }
    }

    if let TAtomic::TGenericTypename {
        param_name,
        defining_entity,
        ..
    } = atomic_type
    {
        if template_types_contains(
            &template_result.template_types.clone(),
            param_name,
            defining_entity,
        )
        .is_some()
            && replace
        {
            return handle_template_param_type_standin(
                atomic_type,
                template_result,
                codebase,
                interner,
                input_type,
                input_arg_offset,
                calling_class,
                calling_function,
                true,
                add_lower_bound,
                depth,
                was_single,
            );
        }
    }

    let mut matching_input_types = Vec::new();

    if let Some(input_type) = input_type {
        if !input_type.is_mixed() {
            matching_input_types = find_matching_atomic_types_for_template(
                atomic_type,
                &normalized_key,
                codebase,
                input_type,
            );
        } else {
            matching_input_types.push(input_type.get_single().clone());
        }
    }

    if matching_input_types.is_empty() {
        let atomic_type = replace_atomic(
            atomic_type,
            template_result,
            codebase,
            interner,
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

    for matching_input_type in matching_input_types {
        atomic_types.push(replace_atomic(
            atomic_type,
            template_result,
            codebase,
            interner,
            Some(matching_input_type),
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
    interner: &Option<&Interner>,
    input_type: Option<TAtomic>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&StrId>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    depth: usize,
) -> TAtomic {
    let mut atomic_type = atomic_type.clone();

    match atomic_type {
        TAtomic::TDict(TDict {
            ref mut known_items,
            ref mut params,
            ..
        }) => {
            if let Some(ref mut known_items) = known_items {
                for (offset, (_, property)) in known_items {
                    let input_type_param = if let Some(TAtomic::TDict(TDict {
                        known_items: Some(ref input_known_items),
                        ..
                    })) = input_type
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
                        interner,
                        &input_type_param.as_ref(),
                        input_arg_offset,
                        calling_class,
                        calling_function,
                        replace,
                        add_lower_bound,
                        depth,
                    ));
                }
            } else if let Some(params) = params {
                let input_params = if let Some(TAtomic::TDict(TDict { .. })) = &input_type {
                    if !params.0.is_arraykey() || !params.1.is_mixed() {
                        get_arrayish_params(&input_type.unwrap(), codebase)
                    } else {
                        Some((get_arraykey(false), get_mixed()))
                    }
                } else {
                    None
                };

                params.0 = Box::new(self::replace(
                    &params.0,
                    template_result,
                    codebase,
                    interner,
                    &if let Some(input_params) = &input_params {
                        Some(&input_params.0)
                    } else {
                        None
                    },
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    depth,
                ));

                params.1 = Box::new(self::replace(
                    &params.1,
                    template_result,
                    codebase,
                    interner,
                    &if let Some(input_params) = &input_params {
                        Some(&input_params.1)
                    } else {
                        None
                    },
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    depth,
                ));
            }

            return atomic_type;
        }
        TAtomic::TVec {
            ref mut known_items,
            ref mut type_param,
            ..
        } => {
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
                        interner,
                        &input_type_param,
                        input_arg_offset,
                        calling_class,
                        calling_function,
                        replace,
                        add_lower_bound,
                        depth,
                    );
                }
            } else {
                let input_param = if let Some(TAtomic::TVec { .. }) = &input_type {
                    get_value_param(&input_type.unwrap(), codebase)
                } else {
                    None
                };

                *type_param = Box::new(self::replace(
                    type_param,
                    template_result,
                    codebase,
                    interner,
                    &if let Some(input_param) = &input_param {
                        Some(input_param)
                    } else {
                        None
                    },
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    depth,
                ));
            }

            return atomic_type;
        }
        TAtomic::TKeyset {
            ref mut type_param, ..
        } => {
            *type_param = Box::new(self::replace(
                type_param,
                template_result,
                codebase,
                interner,
                &if let Some(TAtomic::TKeyset {
                    type_param: input_param,
                }) = &input_type
                {
                    Some(input_param)
                } else {
                    None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                depth,
            ));

            return atomic_type;
        }
        TAtomic::TAwaitable { ref mut value, .. } => {
            *value = Box::new(self::replace(
                value,
                template_result,
                codebase,
                interner,
                &if let Some(TAtomic::TAwaitable {
                    value: input_param, ..
                }) = &input_type
                {
                    Some(input_param)
                } else {
                    None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                depth,
            ));

            return atomic_type;
        }
        TAtomic::TNamedObject {
            ref mut type_params,
            ref name,
            remapped_params,
            ..
        } => {
            if let Some(ref mut type_params) = type_params {
                let mapped_type_params = if let Some(TAtomic::TNamedObject {
                    type_params: Some(_),
                    ..
                }) = &input_type
                {
                    Some(get_mapped_generic_type_params(
                        codebase,
                        interner,
                        &input_type.clone().unwrap(),
                        name,
                        remapped_params,
                    ))
                } else {
                    None
                };

                for (offset, type_param) in type_params.iter_mut().enumerate() {
                    let input_type_param = match &input_type {
                        Some(input_inner) => match input_inner {
                            TAtomic::TNamedObject {
                                type_params: Some(ref input_type_parts),
                                ..
                            } => input_type_parts.get(offset).cloned(),
                            TAtomic::TDict(TDict { .. })
                            | TAtomic::TVec { .. }
                            | TAtomic::TKeyset { .. } => {
                                let (key_param, value_param) =
                                    get_arrayish_params(input_inner, codebase).unwrap();

                                match name {
                                    &StrId::KEYED_CONTAINER | &StrId::KEYED_TRAVERSABLE => {
                                        if offset == 0 {
                                            Some(key_param)
                                        } else {
                                            Some(value_param)
                                        }
                                    }
                                    &crate::StrId::CONTAINER | &StrId::TRAVERSABLE => {
                                        Some(value_param)
                                    }
                                    _ => None,
                                }
                            }
                            TAtomic::TMixedFromLoopIsset => Some(get_mixed_maybe_from_loop(true)),
                            TAtomic::TMixed | TAtomic::TMixedWithFlags(..) => Some(get_mixed_any()),
                            _ => None,
                        },
                        _ => None,
                    };

                    *type_param = self::replace(
                        type_param,
                        template_result,
                        codebase,
                        interner,
                        &if let Some(mapped_type_params) = &mapped_type_params {
                            if let Some(matched) = mapped_type_params.get(offset) {
                                Some(&matched.1)
                            } else {
                                None
                            }
                        } else {
                            input_type_param.as_ref()
                        },
                        input_arg_offset,
                        calling_class,
                        calling_function,
                        replace,
                        add_lower_bound,
                        depth,
                    );
                }
            }

            return atomic_type;
        }
        TAtomic::TTypeAlias {
            ref mut type_params,
            ref name,
            ..
        } => {
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

                for (offset, type_param) in type_params.iter_mut().enumerate() {
                    *type_param = self::replace(
                        type_param,
                        template_result,
                        codebase,
                        interner,
                        &if let Some(mapped_type_params) = &mapped_type_params {
                            mapped_type_params.get(offset)
                        } else {
                            None
                        },
                        input_arg_offset,
                        calling_class,
                        calling_function,
                        replace,
                        add_lower_bound,
                        depth,
                    );
                }
            }

            return atomic_type;
        }
        TAtomic::TClosure {
            ref mut params,
            ref mut return_type,
            ..
        } => {
            for (offset, param) in params.iter_mut().enumerate() {
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
                    *param_type = Box::new(self::replace(
                        param_type,
                        template_result,
                        codebase,
                        interner,
                        &if let Some(input_type_param) = input_type_param {
                            Some(input_type_param)
                        } else {
                            None
                        },
                        input_arg_offset,
                        calling_class,
                        calling_function,
                        replace,
                        !add_lower_bound,
                        depth,
                    ));
                }
            }

            if let Some(ref mut return_type) = return_type {
                *return_type = Box::new(self::replace(
                    return_type,
                    template_result,
                    codebase,
                    interner,
                    &if let Some(TAtomic::TClosure {
                        return_type: Some(input_return_type),
                        ..
                    }) = &input_type
                    {
                        Some(input_return_type)
                    } else {
                        None
                    },
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    replace,
                    add_lower_bound,
                    depth - 1,
                ));
            }

            return atomic_type;
        }
        TAtomic::TClassname { ref mut as_type } => {
            *as_type = Box::new(replace_atomic(
                as_type,
                template_result,
                codebase,
                interner,
                if let Some(TAtomic::TClassname {
                    as_type: input_as_type,
                }) = input_type
                {
                    Some(*input_as_type)
                } else {
                    None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                depth,
            ));

            return atomic_type;
        }
        TAtomic::TTypename { ref mut as_type } => {
            *as_type = Box::new(replace_atomic(
                as_type,
                template_result,
                codebase,
                interner,
                if let Some(TAtomic::TTypename {
                    as_type: input_as_type,
                }) = input_type
                {
                    Some(*input_as_type)
                } else {
                    None
                },
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                depth,
            ));

            return atomic_type;
        }
        _ => (),
    }

    atomic_type.clone()
}

fn handle_template_param_standin(
    atomic_type: &TAtomic,
    normalized_key: &String,
    template_type: &TUnion,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    input_type: &Option<&TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&StrId>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    depth: usize,
    had_template: &mut bool,
) -> Vec<TAtomic> {
    let (param_name, defining_entity, extra_types, as_type) = if let TAtomic::TGenericParam {
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
        if defining_entity == &GenericParent::ClassLike(*calling_class) {
            return vec![atomic_type.clone()];
        }
    }

    if &template_type.get_id(None) == normalized_key {
        return template_type.clone().types;
    }

    let mut replacement_type = template_type.clone();

    let param_name_key = *param_name;

    let mut new_extra_types = vec![];

    if let Some(extra_types) = extra_types {
        for extra_type in extra_types {
            let extra_type_union = self::replace(
                &TUnion::new(vec![extra_type.clone()]),
                template_result,
                codebase,
                interner,
                input_type,
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
                depth + 1,
            );

            if extra_type_union.is_single() {
                let extra_type = extra_type_union.get_single().clone();

                if let TAtomic::TNamedObject { .. } | TAtomic::TGenericParam { .. } = extra_type {
                    new_extra_types.push(extra_type);
                }
            }
        }
    }

    if replace {
        let mut atomic_types = Vec::new();

        if replacement_type.is_mixed() && !as_type.is_mixed() {
            for as_atomic_type in &as_type.types {
                if let TAtomic::TArraykey { from_any: false } = as_atomic_type {
                    atomic_types.push(TAtomic::TArraykey { from_any: true });
                } else {
                    atomic_types.push(as_atomic_type.clone());
                }
            }
        } else {
            type_expander::expand_union(
                codebase,
                interner,
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
                &mut DataFlowGraph::new(GraphKind::FunctionBody),
            );

            if depth < 10 && replacement_type.has_template_types() {
                replacement_type = self::replace(
                    &replacement_type,
                    template_result,
                    codebase,
                    interner,
                    input_type,
                    input_arg_offset,
                    calling_class,
                    calling_function,
                    true,
                    add_lower_bound,
                    depth + 1,
                );
            }

            for replacement_atomic_type in &replacement_type.types {
                let mut replacements_found = false;

                if let TAtomic::TGenericParam {
                    defining_entity: replacement_defining_entity,
                    as_type: replacement_as_type,
                    ..
                } = replacement_atomic_type
                {
                    if (calling_class.is_none()
                        || replacement_defining_entity
                            != &GenericParent::ClassLike(*calling_class.unwrap()))
                        && (calling_function.is_none()
                            || match calling_function.unwrap() {
                                FunctionLikeIdentifier::Function(calling_function) => {
                                    replacement_defining_entity
                                        != &GenericParent::FunctionLike(*calling_function)
                                }
                                FunctionLikeIdentifier::Method(_, _) => true,
                                _ => {
                                    panic!()
                                }
                            })
                    {
                        for nested_type_atomic in &replacement_as_type.types {
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
            interner,
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
            &mut DataFlowGraph::new(GraphKind::FunctionBody),
        );

        let as_type = self::replace(
            &as_type,
            template_result,
            codebase,
            interner,
            input_type,
            input_arg_offset,
            calling_class,
            calling_function,
            true,
            add_lower_bound,
            depth + 1,
        );

        if let Some(input_type) = input_type {
            if !template_result.readonly
                && (as_type.is_mixed()
                    || union_type_comparator::can_be_contained_by(
                        codebase,
                        input_type,
                        &as_type,
                        false,
                        false,
                        &mut matching_input_keys,
                    ))
            {
                let mut generic_param = (*input_type).clone();

                if !matching_input_keys.is_empty() {
                    for atomic in &generic_param.clone().types {
                        if !matching_input_keys.contains(&atomic.get_key()) {
                            generic_param.remove_type(atomic);
                        }
                    }
                }

                if add_lower_bound {
                    return generic_param.types.clone();
                }

                template_result
                    .lower_bounds
                    .entry(param_name_key)
                    .or_insert_with(FxHashMap::default)
                    .entry(*defining_entity)
                    .or_insert(vec![TemplateBound {
                        bound_type: generic_param.clone(),
                        appearance_depth: depth,
                        arg_offset: input_arg_offset,
                        equality_bound_classlike: None,
                        pos: None,
                    }]);
            }
        }

        let mut new_atomic_types = Vec::new();

        for mut atomic_type in atomic_types {
            if let TAtomic::TNamedObject {
                extra_types: ref mut atomic_extra_types,
                ..
            }
            | TAtomic::TGenericParam {
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
                input_type,
                &replacement_type,
                false,
                false,
                &mut matching_input_keys,
            ) {
                let mut generic_param = (*input_type).clone();

                if !matching_input_keys.is_empty() {
                    for atomic in &generic_param.clone().types {
                        if !matching_input_keys.contains(&atomic.get_key()) {
                            generic_param.remove_type(atomic);
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
                        intersect_union_types(&upper_bound.bound_type, &generic_param, codebase)
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
                        pos: None,
                    }
                };

                template_result
                    .upper_bounds
                    .entry(param_name_key)
                    .or_insert_with(FxHashMap::default)
                    .insert(*defining_entity, new_upper_bound);
            }
        }
    }

    vec![atomic_type.clone()]
}

fn handle_template_param_class_standin(
    atomic_type: &TAtomic,
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    input_type: &Option<&TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&StrId>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    depth: usize,
    was_single: bool,
) -> Vec<TAtomic> {
    if let TAtomic::TGenericClassname {
        defining_entity,
        as_type,
        param_name,
        ..
    } = atomic_type
    {
        let mut atomic_type_as = *as_type.clone();
        if let Some(calling_class) = calling_class {
            if defining_entity == &GenericParent::ClassLike(*calling_class) {
                return vec![atomic_type.clone()];
            }
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

            for input_atomic_type in &input_type.types {
                if let TAtomic::TLiteralClassname { name } = input_atomic_type {
                    valid_input_atomic_types.push(TAtomic::TNamedObject {
                        name: *name,
                        type_params: None,
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    });
                } else if let TAtomic::TGenericClassname {
                    param_name,
                    as_type,
                    defining_entity,
                    ..
                } = input_atomic_type
                {
                    valid_input_atomic_types.push(TAtomic::TGenericParam {
                        param_name: *param_name,
                        as_type: Box::new(wrap_atomic(*as_type.clone())),
                        defining_entity: *defining_entity,
                        extra_types: None,
                    });
                } else if let TAtomic::TClassname {
                    as_type: atomic_type_as,
                } = input_atomic_type
                {
                    valid_input_atomic_types.push((**atomic_type_as).clone());
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
                interner,
                &generic_param.as_ref(),
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
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
                            &get_most_specific_type_from_bounds(template_bounds, codebase),
                            codebase,
                            false,
                        ),
                        depth,
                        input_arg_offset,
                        None,
                    )]
                } else {
                    template_result
                        .lower_bounds
                        .entry(*param_name)
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            *defining_entity,
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
                .iter()
                .filter(|(e, _)| e == defining_entity)
                .map(|(_, v)| v)
                .next()
                .unwrap();

            for template_atomic_type in &template_type.types {
                if let TAtomic::TNamedObject { .. } | TAtomic::TObject = &template_atomic_type {
                    atomic_types.push(TAtomic::TClassname {
                        as_type: Box::new(template_atomic_type.clone()),
                    });
                }
            }
        }

        if atomic_types.is_empty() {
            if let TAtomic::TGenericParam {
                param_name,
                defining_entity,
                ..
            } = &atomic_type_as
            {
                atomic_types.push(TAtomic::TGenericClassname {
                    param_name: *param_name,
                    as_type: Box::new(atomic_type_as.clone()),
                    defining_entity: *defining_entity,
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
    interner: &Option<&Interner>,
    input_type: &Option<&TUnion>,
    input_arg_offset: Option<usize>,
    calling_class: Option<&StrId>,
    calling_function: Option<&FunctionLikeIdentifier>,
    replace: bool,
    add_lower_bound: bool,
    depth: usize,
    was_single: bool,
) -> Vec<TAtomic> {
    if let TAtomic::TGenericTypename {
        defining_entity,
        as_type,
        param_name,
        ..
    } = atomic_type
    {
        let mut atomic_type_as = *as_type.clone();
        if let Some(calling_class) = calling_class {
            if defining_entity == &GenericParent::ClassLike(*calling_class) {
                return vec![atomic_type.clone()];
            }
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

            for input_atomic_type in &input_type.types {
                if let TAtomic::TLiteralClassname { name } = input_atomic_type {
                    valid_input_atomic_types.extend(get_actual_type_from_literal(name, codebase));
                } else if let TAtomic::TGenericTypename {
                    param_name,
                    as_type,
                    defining_entity,
                    ..
                } = input_atomic_type
                {
                    valid_input_atomic_types.push(TAtomic::TGenericParam {
                        param_name: *param_name,
                        as_type: Box::new(wrap_atomic(*as_type.clone())),
                        defining_entity: *defining_entity,
                        extra_types: None,
                    });
                } else if let TAtomic::TTypename { .. } = input_atomic_type {
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
                interner,
                &generic_param.as_ref(),
                input_arg_offset,
                calling_class,
                calling_function,
                replace,
                add_lower_bound,
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
                            &get_most_specific_type_from_bounds(template_bounds, codebase),
                            codebase,
                            false,
                        ),
                        depth,
                        input_arg_offset,
                        None,
                    )]
                } else {
                    template_result
                        .lower_bounds
                        .entry(*param_name)
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            *defining_entity,
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
                .iter()
                .filter(|(e, _)| e == defining_entity)
                .map(|(_, v)| v)
                .next()
                .unwrap();

            for template_atomic_type in &template_type.types {
                atomic_types.push(TAtomic::TClassname {
                    as_type: Box::new(template_atomic_type.clone()),
                });
            }
        }

        if atomic_types.is_empty() {
            if let TAtomic::TGenericParam {
                param_name,
                defining_entity,
                ..
            } = &atomic_type_as
            {
                atomic_types.push(TAtomic::TGenericTypename {
                    param_name: *param_name,
                    as_type: Box::new(atomic_type_as.clone()),
                    defining_entity: *defining_entity,
                });
            } else {
                atomic_types.push(TAtomic::TTypename {
                    as_type: Box::new(atomic_type_as),
                });
            }
        }

        atomic_types
    } else {
        panic!();
    }
}

pub fn get_actual_type_from_literal(name: &StrId, codebase: &CodebaseInfo) -> Vec<TAtomic> {
    if let Some(typedefinition_info) = codebase.type_definitions.get(name) {
        if typedefinition_info.newtype_file.is_some() {
            vec![TAtomic::TTypeAlias {
                name: *name,
                type_params: None,
                as_type: typedefinition_info
                    .as_type
                    .as_ref()
                    .map(|t| Box::new(t.clone())),
            }]
        } else {
            typedefinition_info
                .actual_type
                .clone()
                .types
                .into_iter()
                .map(|mut t| match t {
                    TAtomic::TDict(TDict {
                        known_items: Some(_),
                        ref mut shape_name,
                        ..
                    }) => {
                        *shape_name = Some((*name, None));
                        t
                    }
                    _ => t,
                })
                .collect()
        }
    } else if codebase.classlike_infos.contains_key(name) {
        vec![TAtomic::TNamedObject {
            name: *name,
            type_params: None,
            is_this: false,
            extra_types: None,
            remapped_params: false,
        }]
    } else {
        vec![]
    }
}

fn template_types_contains<'a>(
    template_types: &'a IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
    param_name: &StrId,
    defining_entity: &GenericParent,
) -> Option<&'a Arc<TUnion>> {
    if let Some(mapped_classes) = template_types.get(param_name) {
        return mapped_classes
            .iter()
            .filter(|(e, _)| e == defining_entity)
            .map(|(_, v)| v)
            .next();
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

    for atomic_input_type in &input_type.types {
        let input_key = &if let TAtomic::TNamedObject { name, .. } = atomic_input_type {
            name.0.to_string()
        } else if let TAtomic::TTypeAlias { name, .. } = atomic_input_type {
            name.0.to_string()
        } else {
            atomic_input_type.get_key()
        };

        if input_key == normalized_key {
            matching_atomic_types.push(atomic_input_type.clone());
            continue;
        }

        match atomic_input_type {
            TAtomic::TClosure { .. } => {
                if matches!(base_type, TAtomic::TClosure { .. }) {
                    matching_atomic_types.push(atomic_input_type.clone());
                    continue;
                }
            }
            TAtomic::TDict(TDict { .. }) | TAtomic::TVec { .. } | TAtomic::TKeyset { .. } => {
                if let TAtomic::TNamedObject { name, .. } = base_type {
                    if is_array_container(name) {
                        matching_atomic_types.push(atomic_input_type.clone());
                        continue;
                    }
                }
            }
            TAtomic::TLiteralClassname {
                name: atomic_class_name,
            } => {
                if let TAtomic::TClassname {
                    as_type: base_as_type,
                    ..
                } = base_type
                {
                    if let TAtomic::TNamedObject {
                        name: base_as_value,
                        ..
                    } = &**base_as_type
                    {
                        let classlike_info = codebase.classlike_infos.get(atomic_class_name);

                        if let Some(classlike_info) = classlike_info {
                            if let Some(extended_params) =
                                classlike_info.template_extended_params.get(base_as_value)
                            {
                                matching_atomic_types.push(TAtomic::TClassname {
                                    as_type: Box::new(TAtomic::TNamedObject {
                                        name: *base_as_value,
                                        type_params: Some(
                                            extended_params
                                                .clone()
                                                .into_iter()
                                                .map(|(_, v)| (*v).clone())
                                                .collect::<Vec<_>>(),
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
            TAtomic::TNamedObject {
                name: input_name,
                type_params: input_type_params,
                ..
            } => {
                if let TAtomic::TNamedObject {
                    name: base_name, ..
                } = base_type
                {
                    let classlike_info = if let Some(c) = codebase.classlike_infos.get(input_name) {
                        c
                    } else {
                        matching_atomic_types.push(TAtomic::TObject);
                        continue;
                    };

                    if input_type_params.is_some()
                        && classlike_info
                            .template_extended_params
                            .contains_key(base_name)
                    {
                        matching_atomic_types.push(atomic_input_type.clone());
                        continue;
                    }

                    if let Some(extended_params) =
                        classlike_info.template_extended_params.get(base_name)
                    {
                        matching_atomic_types.push(TAtomic::TNamedObject {
                            name: *input_name,
                            type_params: Some(
                                extended_params
                                    .clone()
                                    .into_iter()
                                    .map(|(_, v)| (*v).clone())
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
            TAtomic::TGenericParam { as_type, .. } => {
                matching_atomic_types.extend(find_matching_atomic_types_for_template(
                    base_type,
                    normalized_key,
                    codebase,
                    as_type,
                ));
            }
            TAtomic::TTypeAlias {
                as_type: Some(as_type),
                ..
            } => {
                matching_atomic_types.extend(find_matching_atomic_types_for_template(
                    base_type,
                    normalized_key,
                    codebase,
                    as_type,
                ));
            }
            TAtomic::TEnumClassLabel {
                class_name,
                member_name,
            } => {
                if let TAtomic::TTypeAlias {
                    name: base_name,
                    type_params: Some(base_type_params),
                    ..
                } = base_type
                {
                    if *base_name == StrId::ENUM_CLASS_LABEL {
                        let enum_type = if let Some(class_name) = class_name {
                            TAtomic::TNamedObject {
                                name: *class_name,
                                type_params: None,
                                is_this: false,
                                extra_types: None,
                                remapped_params: false,
                            }
                        } else {
                            base_type_params[0].get_single().clone()
                        };

                        if let TAtomic::TNamedObject {
                            name: enum_name, ..
                        } = &enum_type
                        {
                            if let Some(classlike_info) = codebase.classlike_infos.get(enum_name) {
                                if let Some(constant_info) =
                                    classlike_info.constants.get(member_name)
                                {
                                    let provided_type =
                                        constant_info.provided_type.as_ref().unwrap().get_single();

                                    if let TAtomic::TTypeAlias {
                                        type_params: Some(type_params),
                                        ..
                                    } = provided_type
                                    {
                                        matching_atomic_types.push(TAtomic::TTypeAlias {
                                            name: *base_name,
                                            type_params: Some(vec![
                                                wrap_atomic(enum_type),
                                                type_params[1].clone(),
                                            ]),
                                            as_type: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            TAtomic::TTypeVariable { .. } => {
                // todo we can probably do better here
                matching_atomic_types.push(TAtomic::TMixedWithFlags(true, false, false, false));
            }
            _ => (),
        }
    }
    matching_atomic_types
}

fn is_array_container(name: &StrId) -> bool {
    name == &StrId::TRAVERSABLE
        || name == &StrId::KEYED_TRAVERSABLE
        || name == &StrId::CONTAINER
        || name == &StrId::KEYED_CONTAINER
        || name == &StrId::ANY_ARRAY
}

pub fn get_mapped_generic_type_params(
    codebase: &CodebaseInfo,
    interner: &Option<&Interner>,
    input_type_part: &TAtomic,
    container_name: &StrId,
    container_remapped_params: bool,
) -> Vec<(Option<usize>, TUnion)> {
    let mut input_type_params = match input_type_part {
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params
            .iter()
            .enumerate()
            .map(|(k, v)| (Some(k), v.clone()))
            .collect::<Vec<_>>(),
        _ => panic!(),
    };

    let input_name = match input_type_part {
        TAtomic::TNamedObject { name, .. } => name,
        _ => panic!(),
    };

    let input_class_storage = if let Some(storage) = codebase.classlike_infos.get(input_name) {
        storage
    } else {
        return vec![];
    };

    if input_name == container_name {
        return input_type_params;
    }

    let input_template_types = &input_class_storage.template_types;

    let mut i = 0;

    let mut replacement_templates = IndexMap::new();

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
                    .entry(*template_name)
                    .or_insert_with(FxHashMap::default)
                    .insert(GenericParent::ClassLike(*input_name), input_type.clone().1);

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
            let mut mapped_input_offset = None;

            let mut new_input_param = None;

            for et in &extended_input_param.types {
                let ets = get_extended_templated_types(et, template_extends);

                let mut candidate_param_type: Option<_> = None;

                if let Some(TAtomic::TGenericParam {
                    param_name,
                    defining_entity,
                    ..
                }) = ets.first()
                {
                    if let Some((old_params_offset, (_, defining_classes))) = input_class_storage
                        .template_types
                        .iter()
                        .enumerate()
                        .find(|(_, (n, _))| n == param_name)
                    {
                        if defining_classes.iter().any(|(e, _)| defining_entity == e) {
                            let candidate_param_type_inner = input_type_params
                                .get(old_params_offset)
                                .unwrap_or(&(None, get_mixed_any()))
                                .clone()
                                .1;

                            mapped_input_offset = Some(old_params_offset);

                            candidate_param_type = Some(candidate_param_type_inner);
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
                        codebase,
                        true,
                    ))
                } else {
                    Some(candidate_param_type.clone())
                };
            }

            new_input_params.push((
                mapped_input_offset,
                inferred_type_replacer::replace(
                    &new_input_param.unwrap(),
                    &TemplateResult::new(IndexMap::new(), replacement_templates.clone()),
                    codebase,
                ),
            ));
        }

        input_type_params = new_input_params
            .into_iter()
            .map(|mut v| {
                type_expander::expand_union(
                    codebase,
                    interner,
                    &mut v.1,
                    &TypeExpansionOptions {
                        ..Default::default()
                    },
                    &mut DataFlowGraph::new(GraphKind::FunctionBody),
                );
                v
            })
            .collect::<Vec<_>>();
    }

    input_type_params
}

pub fn get_extended_templated_types<'a>(
    atomic_type: &'a TAtomic,
    extends: &'a FxHashMap<StrId, IndexMap<StrId, Arc<TUnion>>>,
) -> Vec<&'a TAtomic> {
    let mut extra_added_types = Vec::new();

    if let TAtomic::TGenericParam {
        defining_entity: GenericParent::ClassLike(defining_entity),
        param_name,
        ..
    } = atomic_type
    {
        if let Some(defining_params) = extends.get(defining_entity) {
            if let Some(extended_param) = defining_params.get(param_name) {
                for extended_atomic_type in &extended_param.types {
                    if let TAtomic::TGenericParam { .. } = extended_atomic_type {
                        extra_added_types
                            .extend(get_extended_templated_types(extended_atomic_type, extends));
                    } else {
                        extra_added_types.push(extended_atomic_type);
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
    lower_bounds: &IndexMap<StrId, FxHashMap<GenericParent, Vec<TemplateBound>>>,
    param_name: &StrId,
    defining_entity: &GenericParent,
    mut visited_entities: FxHashSet<GenericParent>,
    codebase: &CodebaseInfo,
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

            if let TAtomic::TGenericParam {
                param_name,
                defining_entity,
                ..
            } = first_template
            {
                visited_entities.insert(*defining_entity);
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
    lower_bounds: &[TemplateBound],
    codebase: &CodebaseInfo,
) -> TUnion {
    let relevant_bounds = get_relevant_bounds(lower_bounds);

    if relevant_bounds.is_empty() {
        return get_mixed_any();
    }

    if relevant_bounds.len() == 1 {
        return relevant_bounds[0].bound_type.clone();
    }

    let mut specific_type = relevant_bounds[0].bound_type.clone();

    for bound in relevant_bounds {
        specific_type = add_union_type(specific_type, &bound.bound_type, codebase, false);
    }

    specific_type
}

pub fn get_relevant_bounds(lower_bounds: &[TemplateBound]) -> Vec<&TemplateBound> {
    if lower_bounds.len() == 1 {
        return vec![&lower_bounds[0]];
    }

    let mut lower_bounds = lower_bounds.iter().collect::<Vec<_>>();
    lower_bounds.sort_by(|a, b| a.appearance_depth.partial_cmp(&b.appearance_depth).unwrap());

    let mut current_depth = None;
    let mut had_invariant = false;
    let mut last_arg_offset = None;

    let mut applicable_bounds = vec![];

    for template_bound in lower_bounds {
        if let Some(inner) = current_depth {
            if inner != template_bound.appearance_depth && !applicable_bounds.is_empty() {
                if !had_invariant || last_arg_offset == template_bound.arg_offset {
                    // escape switches when matching on invariant generic params
                    // and when matching
                    break;
                }

                current_depth = Some(template_bound.appearance_depth);
            }
        } else {
            current_depth = Some(template_bound.appearance_depth);
        }

        had_invariant = if had_invariant {
            true
        } else {
            template_bound.equality_bound_classlike.is_some()
        };

        applicable_bounds.push(template_bound);

        last_arg_offset = template_bound.arg_offset;
    }

    applicable_bounds
}
