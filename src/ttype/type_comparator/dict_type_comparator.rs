use crate::{get_arrayish_params, get_arraykey, get_mixed};
use hakana_reflection_info::{codebase_info::CodebaseInfo, t_atomic::TAtomic};

use super::{
    generic_type_comparator::update_failed_result_from_nested,
    type_comparison_result::TypeComparisonResult, union_type_comparator,
};

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    inside_assertion: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut all_types_contain = true;

    if let TAtomic::TDict {
        known_items: container_known_items,
        params: container_params,
        ..
    } = container_type_part
    {
        if let TAtomic::TDict {
            known_items: input_known_items,
            params: input_params,
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

                            if !union_type_comparator::is_contained_by(
                                codebase,
                                input_property_type,
                                container_property_type,
                                false,
                                input_property_type.ignore_falsable_issues,
                                inside_assertion,
                                atomic_comparison_result,
                            ) {
                                all_types_contain = false;
                            }
                        } else {
                            if !c_u {
                                all_types_contain = false;
                            }
                        }
                    }

                    if all_types_contain {
                        match (input_params, container_params) {
                            (None, None) => {}
                            (None, Some(_)) => {}
                            (Some(_), None) => {
                                all_types_contain = false;
                            }
                            (Some(input_params), Some(container_params)) => {
                                if !union_type_comparator::is_contained_by(
                                    codebase,
                                    &input_params.0,
                                    &container_params.0,
                                    false,
                                    input_params.0.ignore_falsable_issues,
                                    inside_assertion,
                                    atomic_comparison_result,
                                ) {
                                    all_types_contain = false;
                                }

                                if !union_type_comparator::is_contained_by(
                                    codebase,
                                    &input_params.1,
                                    &container_params.1,
                                    false,
                                    input_params.1.ignore_falsable_issues,
                                    inside_assertion,
                                    atomic_comparison_result,
                                ) {
                                    all_types_contain = false;
                                }
                            }
                        }
                    }

                    return all_types_contain;
                }

                let mut all_possibly_undefined = true;
                for (_, (c_u, _)) in container_known_items {
                    if !c_u {
                        all_possibly_undefined = false;
                    }
                }

                all_types_contain = all_possibly_undefined && input_params.is_none();

                if !all_types_contain {
                    atomic_comparison_result.type_coerced = Some(true);

                    if let Some(input_params) = input_params {
                        let mut has_any = false;
                        if input_params.1.is_mixed_with_any(&mut has_any) {
                            atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
                            if has_any {
                                atomic_comparison_result.type_coerced_from_nested_any = Some(true);
                            }
                        }
                    }
                }
            } else {
                let container_params = get_arrayish_params(container_type_part, codebase).unwrap();
                let input_params =
                    if !container_params.0.is_arraykey() || !container_params.1.is_mixed() {
                        get_arrayish_params(input_type_part, codebase).unwrap()
                    } else {
                        (get_arraykey(false), get_mixed())
                    };

                let mut nested_comparison_result = TypeComparisonResult::new();

                if !union_type_comparator::is_contained_by(
                    codebase,
                    &input_params.0,
                    &container_params.0,
                    false,
                    input_params.0.ignore_falsable_issues,
                    inside_assertion,
                    &mut nested_comparison_result,
                ) {
                    all_types_contain = false;

                    update_failed_result_from_nested(
                        atomic_comparison_result,
                        nested_comparison_result,
                    );
                }

                let mut nested_comparison_result = TypeComparisonResult::new();

                if !union_type_comparator::is_contained_by(
                    codebase,
                    &input_params.1,
                    &container_params.1,
                    false,
                    input_params.1.ignore_falsable_issues,
                    inside_assertion,
                    &mut nested_comparison_result,
                ) {
                    all_types_contain = false;

                    update_failed_result_from_nested(
                        atomic_comparison_result,
                        nested_comparison_result,
                    );
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
