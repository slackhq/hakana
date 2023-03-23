use crate::get_mixed_any;
use hakana_reflection_info::{codebase_info::CodebaseInfo, t_atomic::TAtomic};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if let TAtomic::TClosure {
        params: input_params,
        return_type: input_return_type,
        effects: input_effects,
        ..
    } = input_type_part
    {
        if let TAtomic::TClosure {
            params: container_params,
            return_type: container_return_type,
            effects: container_effects,
            ..
        } = container_type_part
        {
            if let Some(container_effects) = container_effects {
                if container_effects == &0 && input_effects.unwrap_or(0) > 0 {
                    atomic_comparison_result.type_coerced = Some(true);

                    return false;
                }
            }

            for (i, input_param) in input_params.iter().enumerate() {
                let mut container_param = None;

                if let Some(inner) = container_params.get(i) {
                    container_param = Some(inner);
                } else if let Some(last_param) = container_params.last() {
                    if last_param.is_variadic {
                        container_param = Some(last_param);
                    }
                }

                if let Some(container_param) = container_param {
                    if let Some(container_param_type) = &container_param.signature_type {
                        let mut param_comparison_result = TypeComparisonResult::new();

                        if !container_param_type.is_mixed()
                            && !union_type_comparator::is_contained_by(
                                codebase,
                                container_param_type,
                                &input_param
                                    .signature_type
                                    .clone()
                                    .unwrap_or(get_mixed_any()),
                                false,
                                false,
                                false,
                                &mut param_comparison_result,
                            )
                        {
                            return false;
                        }

                        atomic_comparison_result
                            .type_variable_lower_bounds
                            .extend(param_comparison_result.type_variable_upper_bounds);

                        atomic_comparison_result
                            .type_variable_upper_bounds
                            .extend(param_comparison_result.type_variable_lower_bounds);
                    }
                } else {
                    if input_param.is_optional {
                        break;
                    }

                    return false;
                }
            }

            if let Some(container_return_type) = container_return_type {
                if let Some(input_return_type) = input_return_type {
                    if input_return_type.is_void() && container_return_type.is_nullable() {
                        return true;
                    }

                    if !container_return_type.is_void()
                        && !union_type_comparator::is_contained_by(
                            codebase,
                            &input_return_type,
                            &container_return_type,
                            false,
                            input_return_type.ignore_falsable_issues,
                            false,
                            atomic_comparison_result,
                        )
                    {
                        return false;
                    }
                } else {
                    atomic_comparison_result.type_coerced = Some(true);
                    atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);

                    return false;
                }
            }

            return true;
        }
    }

    false
}
