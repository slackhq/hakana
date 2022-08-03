use crate::get_arrayish_params;
use hakana_reflection_info::{codebase_info::CodebaseInfo, t_atomic::TAtomic, t_union::TUnion};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut all_types_contain = true;

    if let TAtomic::TDict {
        known_items: container_known_items,
        key_param: container_key_param,
        value_param: container_value_param,
        ..
    } = container_type_part
    {
        if let TAtomic::TDict {
            known_items: input_known_items,
            key_param: input_key_param,
            value_param: input_value_param,
            ..
        } = input_type_part
        {
            if let Some(container_known_items) = container_known_items {
                if let Some(input_known_items) = input_known_items {
                    for (key, (c_u, container_property_type)) in container_known_items {
                        if let Some((i_u, input_property_type)) = input_known_items.get(key) {
                            if *i_u && !c_u {
                                all_types_contain = false;
                            }

                            if !type_coercing_is_contained_by(
                                codebase,
                                input_property_type,
                                container_property_type,
                                allow_interface_equality,
                                atomic_comparison_result,
                            ) {
                                all_types_contain = false;

                                let mut mixed_with_any = false;
                                if input_property_type.is_mixed_with_any(&mut mixed_with_any) {
                                    atomic_comparison_result.type_coerced_from_nested_mixed =
                                        Some(true);
                                    if mixed_with_any {
                                        atomic_comparison_result.type_coerced_from_nested_any =
                                            Some(true);
                                    }
                                }
                            }
                        } else {
                            if !c_u {
                                all_types_contain = false;
                            }
                        }
                    }

                    if all_types_contain {
                        if !input_value_param.is_nothing() {
                            if !type_coercing_is_contained_by(
                                codebase,
                                &input_key_param,
                                &container_key_param,
                                allow_interface_equality,
                                atomic_comparison_result,
                            ) {
                                all_types_contain = false;
                            }

                            if !type_coercing_is_contained_by(
                                codebase,
                                &input_value_param,
                                &container_value_param,
                                allow_interface_equality,
                                atomic_comparison_result,
                            ) {
                                all_types_contain = false;
                            }
                        }
                    }

                    return all_types_contain;
                } else {
                    let mut all_possibly_undefined = true;
                    for (_, (c_u, _)) in container_known_items {
                        if !c_u {
                            all_possibly_undefined = false;
                        }
                    }

                    all_types_contain = all_possibly_undefined && input_value_param.is_nothing();
                }
            } else {
                let input_params = get_arrayish_params(input_type_part, codebase).unwrap();
                let container_params = get_arrayish_params(container_type_part, codebase).unwrap();

                if !type_coercing_is_contained_by(
                    codebase,
                    &input_params.0,
                    &container_params.0,
                    allow_interface_equality,
                    atomic_comparison_result,
                ) {
                    if container_params.1.is_arraykey() {
                        atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
                    }
                }

                if !type_coercing_is_contained_by(
                    codebase,
                    &input_params.1,
                    &container_params.1,
                    allow_interface_equality,
                    atomic_comparison_result,
                ) {
                    all_types_contain = false;

                    let mut mixed_with_any = false;
                    if container_params.1.is_mixed_with_any(&mut mixed_with_any) {
                        atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);

                        if mixed_with_any {
                            atomic_comparison_result.type_coerced_from_nested_any = Some(true);
                        }
                    }
                }
            }
        } else {
            panic!()
        }
    } else {
        panic!()
    }

    all_types_contain
}

pub(crate) fn type_coercing_is_contained_by(
    codebase: &CodebaseInfo,
    input_type: &TUnion,
    container_type: &TUnion,
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut property_type_comparison = TypeComparisonResult::new();

    if union_type_comparator::is_contained_by(
        codebase,
        input_type,
        container_type,
        false,
        false,
        allow_interface_equality,
        &mut property_type_comparison,
    ) || property_type_comparison
        .type_coerced_to_literal
        .unwrap_or(false)
    {
        return true;
    }

    let mut inverse_property_type_comparison = TypeComparisonResult::new();

    // it only counts as coercion if the type is cleanly contained
    if union_type_comparator::is_contained_by(
        codebase,
        container_type,
        input_type,
        false,
        false,
        allow_interface_equality,
        &mut inverse_property_type_comparison,
    ) || inverse_property_type_comparison
        .type_coerced_to_literal
        .unwrap_or(false)
    {
        atomic_comparison_result.type_coerced = Some(true);
    }

    return false;
}
