use crate::code_location::FilePath;
use crate::t_atomic::TVec;
use crate::ttype::get_arrayish_params;
use crate::{codebase_info::CodebaseInfo, t_atomic::TAtomic};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(crate) fn is_contained_by(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    inside_assertion: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut all_types_contain = true;

    let mut obviously_bad = false;

    if let TAtomic::TVec(TVec {
        known_items: Some(container_known_items),
        variadic_type,
        ..
    }) = container_type_part
    {
        if let TAtomic::TVec(TVec {
            known_items: Some(input_known_items),
            ..
        }) = input_type_part
        {
            for (key, (c_u, container_property_type)) in container_known_items {
                if let Some((i_u, input_property_type)) = input_known_items.get(key) {
                    if *i_u && !c_u {
                        all_types_contain = false;
                    }

                    if !union_type_comparator::is_contained_by(
                        codebase,
                        file_path,
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
                } else if !c_u {
                    all_types_contain = false;
                    obviously_bad = true;
                }
            }

            // For open tuples, e.g. tuple(int, Foo...), ensure any extra items in the input type
            // match the type of the open part of the tuple.
            if let Some(container_variadic_type) = variadic_type {
                let num_known_items = container_known_items.len();

                for (_, input_property_type) in input_known_items.values().skip(num_known_items) {
                    if !union_type_comparator::is_contained_by(
                        codebase,
                        file_path,
                        input_property_type,
                        container_variadic_type,
                        false,
                        input_property_type.ignore_falsable_issues,
                        inside_assertion,
                        atomic_comparison_result,
                    ) {
                        all_types_contain = false;
                        obviously_bad = true;
                        break;
                    }
                }
            }
        } else if let TAtomic::TVec(TVec {
            type_param,
            variadic_type: input_variadic_type,
            ..
        }) = input_type_part
            && type_param.is_nothing()
            && input_variadic_type
                .as_ref()
                .is_none_or(|ty| ty.is_nothing())
        {
            // An empty vec has no known-item map. Missing optional tuple
            // entries are valid here just as they are for a shorter tuple.
            all_types_contain = container_known_items
                .values()
                .all(|(optional, _)| *optional);
        } else {
            all_types_contain = false;
        }
    }

    if !obviously_bad {
        let tuples_only = if let TAtomic::TVec(TVec {
            type_param: container_type_param,
            ..
        }) = container_type_part
        {
            if let TAtomic::TVec(TVec {
                type_param: input_type_param,
                ..
            }) = input_type_part
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
                file_path,
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
                file_path,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        t_union::TUnion,
        ttype::{get_int, get_nothing, get_string},
    };
    use hakana_str::StrId;

    fn vector(items: Option<Vec<(bool, TUnion)>>, tail: TUnion) -> TAtomic {
        TAtomic::TVec(TVec {
            known_items: items
                .map(|items| items.into_iter().enumerate().collect::<BTreeMap<_, _>>()),
            type_param: Box::new(tail),
            variadic_type: None,
            known_count: None,
            non_empty: false,
        })
    }

    fn contains(input: &TAtomic, container: &TAtomic) -> bool {
        is_contained_by(
            &CodebaseInfo::new(),
            &FilePath(StrId::EMPTY),
            input,
            container,
            false,
            &mut TypeComparisonResult::new(),
        )
    }

    #[test]
    fn empty_vector_satisfies_optional_tuple_entries() {
        let optional_tuple = vector(
            Some(vec![(true, get_int()), (true, get_string())]),
            get_nothing(),
        );
        assert!(contains(&vector(None, get_nothing()), &optional_tuple));
        assert!(contains(
            &vector(Some(vec![]), get_nothing()),
            &optional_tuple,
        ));
        assert!(contains(
            &vector(Some(vec![(false, get_int())]), get_nothing()),
            &optional_tuple,
        ));
    }

    #[test]
    fn empty_vector_cannot_omit_required_tuple_entries() {
        for entries in [
            vec![(false, get_int())],
            vec![(false, TUnion::new(vec![TAtomic::TNull, TAtomic::TInt]))],
            vec![(true, get_int()), (false, get_string())],
        ] {
            assert!(!contains(
                &vector(None, get_nothing()),
                &vector(Some(entries), get_nothing()),
            ));
        }
    }

    #[test]
    fn optional_tuple_still_checks_present_entries_and_unknown_tails() {
        let optional_tuple = vector(Some(vec![(true, get_int())]), get_nothing());
        assert!(!contains(
            &vector(Some(vec![(false, get_string())]), get_nothing()),
            &optional_tuple,
        ));
        assert!(!contains(&vector(None, get_int()), &optional_tuple));

        let mut variadic_input = vector(None, get_nothing());
        if let TAtomic::TVec(vec) = &mut variadic_input {
            vec.variadic_type = Some(get_int());
        }
        assert!(!contains(&variadic_input, &optional_tuple));
    }
}
