use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};
use crate::{
    get_mixed_any, template,
    type_expander::{self, TypeExpansionOptions},
};
use hakana_reflection_info::{
    classlike_info::Variance,
    codebase_info::CodebaseInfo,
    data_flow::graph::{DataFlowGraph, GraphKind},
    t_atomic::TAtomic,
    t_union::TUnion,
    StrId, STR_AWAITABLE, STR_KEYED_CONTAINER,
};

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    inside_assertion: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut all_types_contain = true;

    let input_name = match input_type_part {
        TAtomic::TNamedObject {
            name: input_name, ..
        } => input_name,
        _ => {
            return false;
        }
    };

    let (container_name, container_remapped_params) = match container_type_part {
        TAtomic::TNamedObject {
            name: container_name,
            remapped_params: container_remapped_params,
            ..
        } => (container_name, container_remapped_params),
        _ => panic!(),
    };

    if !codebase.class_or_interface_or_enum_or_trait_exists(input_name) {
        return false;
    }

    if !codebase.class_or_interface_or_enum_or_trait_exists(container_name) {
        return false;
    }

    let container_type_params = match container_type_part {
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params,
        _ => panic!(),
    };

    // handle case where input named object has no generic params
    if let TAtomic::TNamedObject {
        type_params: None, ..
    } = input_type_part
    {
        if codebase.class_exists(input_name) {
            let class_storage = codebase.classlike_infos.get(input_name).unwrap();

            let mut input_type_part = input_type_part.clone();

            if let Some(extended_params) =
                class_storage.template_extended_params.get(container_name)
            {
                if let TAtomic::TNamedObject {
                    ref mut type_params,
                    ..
                } = input_type_part
                {
                    *type_params = Some(
                        extended_params
                            .values()
                            .cloned()
                            .map(|v| {
                                let mut v = (*v).clone();
                                type_expander::expand_union(
                                    codebase,
                                    &None,
                                    &mut v,
                                    &TypeExpansionOptions {
                                        ..Default::default()
                                    },
                                    &mut DataFlowGraph::new(GraphKind::FunctionBody),
                                );
                                v
                            })
                            .collect(),
                    );
                }
            } else if let TAtomic::TNamedObject {
                ref mut type_params,
                ..
            } = input_type_part
            {
                *type_params = Some(vec![get_mixed_any(); container_type_params.len()]);
            }

            return self::is_contained_by(
                codebase,
                &input_type_part,
                container_type_part,
                inside_assertion,
                atomic_comparison_result,
            );
        }

        return false;
    }

    let input_type_params = template::standin_type_replacer::get_mapped_generic_type_params(
        codebase,
        &None,
        input_type_part,
        container_name,
        *container_remapped_params,
    );

    let container_type_params = match container_type_part {
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params,
        _ => panic!(),
    };

    let awaitable_id = STR_AWAITABLE;

    if input_name == &awaitable_id && container_name == &awaitable_id {
        if let (Some(input_param), Some(container_param)) =
            (input_type_params.first(), container_type_params.first())
        {
            // this is a hack to match behaviour in the official typechecker
            if input_param.1.is_null() && container_param.is_void() {
                return true;
            }
        }
    }

    for (i, input_param) in input_type_params.iter().enumerate() {
        if let Some(container_param) = container_type_params.get(i) {
            compare_generic_params(
                codebase,
                input_type_part,
                input_name,
                &input_param.1,
                container_name,
                container_param,
                input_param.0,
                i,
                inside_assertion,
                &mut all_types_contain,
                atomic_comparison_result,
            );
        } else {
            break;
        }
    }

    if all_types_contain {
        return true;
    }

    false
}

pub(crate) fn compare_generic_params(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    input_name: &StrId,
    input_param: &TUnion,
    container_name: &StrId,
    container_param: &TUnion,
    input_param_offset: Option<usize>,
    container_param_offset: usize,
    inside_assertion: bool,
    all_types_contain: &mut bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) {
    if input_param.is_nothing() || input_param.is_placeholder() {
        if atomic_comparison_result.replacement_atomic_type.is_none() {
            atomic_comparison_result.replacement_atomic_type = Some(input_type_part.clone());
        }

        if let Some(TAtomic::TNamedObject {
            type_params: Some(ref mut type_params),
            ..
        }) = atomic_comparison_result.replacement_atomic_type
        {
            if let Some(input_param_offset) = input_param_offset {
                if let Some(existing_param) = type_params.get_mut(input_param_offset) {
                    *existing_param = container_param.clone();
                }
            }
        }

        return;
    }

    let mut param_comparison_result = TypeComparisonResult::new();

    let container_type_param_variance = if let Some(container_classlike_storage) =
        codebase.classlike_infos.get(container_name)
    {
        container_classlike_storage
            .generic_variance
            .get(&container_param_offset)
    } else if let Some(container_typealias_storage) = codebase.type_definitions.get(container_name)
    {
        container_typealias_storage
            .generic_variance
            .get(&container_param_offset)
    } else {
        None
    };

    if !union_type_comparator::is_contained_by(
        codebase,
        input_param,
        container_param,
        false,
        input_param.ignore_falsable_issues,
        inside_assertion,
        &mut param_comparison_result,
    ) {
        if let Some(Variance::Contravariant) = container_type_param_variance {
            if union_type_comparator::is_contained_by(
                codebase,
                container_param,
                input_param,
                false,
                container_param.ignore_falsable_issues,
                inside_assertion,
                &mut param_comparison_result,
            ) {
                return;
            }
        }

        if input_name == &STR_KEYED_CONTAINER && container_param_offset == 0 {
            param_comparison_result.type_coerced_from_nested_mixed = Some(true);
        }

        update_failed_result_from_nested(atomic_comparison_result, param_comparison_result);

        *all_types_contain = false;
    } else if !container_param.has_template() && !input_param.has_template() {
        if input_param.is_literal_of(container_param) {
            if atomic_comparison_result.replacement_atomic_type.is_none() {
                atomic_comparison_result.replacement_atomic_type = Some(input_type_part.clone());
            }

            if let Some(TAtomic::TNamedObject {
                type_params: Some(ref mut type_params),
                ..
            }) = atomic_comparison_result.replacement_atomic_type
            {
                type_params.insert(container_param_offset, container_param.clone());
            }
        } else if !matches!(container_type_param_variance, Some(Variance::Covariant))
            && !container_param.had_template
        {
            atomic_comparison_result
                .type_variable_lower_bounds
                .extend(param_comparison_result.type_variable_lower_bounds);

            atomic_comparison_result.type_variable_lower_bounds.extend(
                param_comparison_result
                    .type_variable_upper_bounds
                    .clone()
                    .into_iter()
                    .map(|(name, mut b)| {
                        b.equality_bound_classlike = Some(*container_name);
                        (name, b)
                    }),
            );

            atomic_comparison_result
                .type_variable_upper_bounds
                .extend(param_comparison_result.type_variable_upper_bounds);

            let mut param_comparison_result = TypeComparisonResult::new();

            if (!union_type_comparator::is_contained_by(
                codebase,
                container_param,
                input_param,
                false,
                input_param.ignore_falsable_issues,
                inside_assertion,
                &mut param_comparison_result,
            ) || param_comparison_result.type_coerced.unwrap_or(false)) && (!container_param.has_static_object() || !input_param.is_static_object()) {
                *all_types_contain = false;

                atomic_comparison_result.type_coerced = Some(false);
            }
        }
    }
}

pub(crate) fn update_failed_result_from_nested(
    atomic_comparison_result: &mut TypeComparisonResult,
    param_comparison_result: TypeComparisonResult,
) {
    atomic_comparison_result.type_coerced =
        Some(if let Some(val) = atomic_comparison_result.type_coerced {
            val
        } else {
            param_comparison_result.type_coerced.unwrap_or(false)
        });
    atomic_comparison_result.type_coerced_from_nested_mixed = Some(
        if let Some(val) = atomic_comparison_result.type_coerced_from_nested_mixed {
            val
        } else {
            param_comparison_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
        },
    );
    atomic_comparison_result.type_coerced_from_nested_any = Some(
        if let Some(val) = atomic_comparison_result.type_coerced_from_nested_any {
            val
        } else {
            param_comparison_result
                .type_coerced_from_nested_any
                .unwrap_or(false)
        },
    );
    atomic_comparison_result.type_coerced_from_as_mixed = Some(
        if let Some(val) = atomic_comparison_result.type_coerced_from_as_mixed {
            val
        } else {
            param_comparison_result
                .type_coerced_from_as_mixed
                .unwrap_or(false)
        },
    );
    atomic_comparison_result.type_coerced_to_literal = Some(
        if let Some(val) = atomic_comparison_result.type_coerced_to_literal {
            val
        } else {
            param_comparison_result
                .type_coerced_to_literal
                .unwrap_or(false)
        },
    );
}
