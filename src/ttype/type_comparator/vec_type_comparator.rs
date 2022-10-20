use crate::get_arrayish_params;
use hakana_reflection_info::{codebase_info::CodebaseInfo, t_atomic::TAtomic};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    inside_assertion: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut all_types_contain = true;

    let mut obviously_bad = false;

    if let TAtomic::TVec {
        known_items: Some(container_known_items),
        ..
    } = container_type_part
    {
        if let TAtomic::TVec {
            known_items: Some(input_known_items),
            ..
        } = input_type_part
        {
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
                        obviously_bad = true;
                    }
                } else {
                    if !c_u {
                        all_types_contain = false;
                        obviously_bad = true;
                    }
                }
            }
        } else {
            all_types_contain = false;
        }
    }

    if !obviously_bad {
        let tuples_only = if let TAtomic::TVec {
            type_param: container_type_param,
            ..
        } = container_type_part
        {
            if let TAtomic::TVec {
                type_param: input_type_param,
                ..
            } = input_type_part
            {
                container_type_param.is_nothing() && input_type_param.is_nothing()
            } else {
                false
            }
        } else {
            false
        };

        if !tuples_only {
            let input_params = get_arrayish_params(input_type_part, codebase).unwrap();
            let container_params = get_arrayish_params(container_type_part, codebase).unwrap();

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

    all_types_contain
}
