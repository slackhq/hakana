use super::{
    negated_assertion_reconciler,
    reconciler::{trigger_issue_for_impossible, ReconciliationStatus},
    simple_assertion_reconciler,
};
use crate::{
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo,
};
use hakana_reflection_info::{
    assertion::Assertion, codebase_info::CodebaseInfo, t_atomic::TAtomic, t_union::TUnion,
};
use hakana_type::{
    get_mixed_any, get_mixed_maybe_from_loop, get_nothing,
    type_comparator::{
        atomic_type_comparator, type_comparison_result::TypeComparisonResult, union_type_comparator,
    },
    type_expander::{self, TypeExpansionOptions},
    wrap_atomic,
};
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;

pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: Option<&TUnion>,
    possibly_undefined: bool,
    key: &Option<String>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    inside_loop: bool,
    pos: Option<&Pos>,
    can_report_issues: bool,
    failed_reconciliation: &mut ReconciliationStatus,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let is_negation = assertion.has_negation();

    let existing_var_type = if let Some(existing_var_type) = existing_var_type {
        existing_var_type
    } else {
        return get_missing_type(assertion, inside_loop);
    };

    let old_var_type_string = existing_var_type.get_id();

    *failed_reconciliation = ReconciliationStatus::Ok;

    if is_negation {
        return negated_assertion_reconciler::reconcile(
            assertion,
            existing_var_type,
            possibly_undefined,
            key,
            statements_analyzer,
            tast_info,
            old_var_type_string,
            if can_report_issues { pos } else { None },
            failed_reconciliation,
            negated,
            suppressed_issues,
        );
    }

    let simple_asserted_type = simple_assertion_reconciler::reconcile(
        assertion,
        &existing_var_type,
        possibly_undefined,
        key,
        codebase,
        tast_info,
        statements_analyzer,
        if can_report_issues { pos } else { None },
        failed_reconciliation,
        negated,
        inside_loop,
        suppressed_issues,
    );

    if let Some(simple_asserted_type) = simple_asserted_type {
        return simple_asserted_type;
    }

    if let Some(assertion_type) = assertion.get_type() {
        let mut refined_type = refine_atomic_with_union(
            statements_analyzer,
            tast_info,
            assertion_type,
            assertion,
            existing_var_type,
            &key,
            negated,
            if can_report_issues { pos } else { None },
            suppressed_issues,
            failed_reconciliation,
        );

        type_expander::expand_union(
            codebase,
            &mut refined_type,
            &TypeExpansionOptions {
                expand_generic: true,
                ..Default::default()
            },
            &mut tast_info.data_flow_graph,
        );

        return refined_type;
    }

    get_mixed_any()
}

pub(crate) fn refine_atomic_with_union(
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    new_type: &TAtomic,
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    pos: Option<&Pos>,
    suppressed_issues: &FxHashMap<String, usize>,
    failed_reconciliation: &mut ReconciliationStatus,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let old_var_type_string = existing_var_type.get_id();

    if !new_type.is_mixed() {
        if let Some(key) = key {
            if let Some(pos) = pos {
                if let TAtomic::TNamedObject { name, .. } = &new_type {
                    if !codebase.interface_exists(name)
                        && !assertion.has_equality()
                        && union_type_comparator::is_contained_by(
                            codebase,
                            existing_var_type,
                            &wrap_atomic(new_type.clone()),
                            false,
                            false,
                            false,
                            &mut TypeComparisonResult::new(),
                        )
                    {
                        trigger_issue_for_impossible(
                            tast_info,
                            statements_analyzer,
                            &old_var_type_string,
                            key,
                            assertion,
                            true,
                            negated,
                            pos,
                            suppressed_issues,
                        );
                    }
                }
            }
        }

        let intersection_type = intersect_union_with_atomic(codebase, existing_var_type, &new_type);

        if let Some(intersection_type) = intersection_type {
            return intersection_type;
        }

        *failed_reconciliation = ReconciliationStatus::Empty;

        return get_nothing();
    }

    return wrap_atomic(new_type.clone());
}

fn intersect_union_with_atomic(
    codebase: &CodebaseInfo,
    existing_var_type: &TUnion,
    new_type: &TAtomic,
) -> Option<TUnion> {
    let mut matching_atomic_types = Vec::new();

    for (_, existing_atomic) in &existing_var_type.types {
        let intersected_atomic_type =
            intersect_atomic_with_atomic(existing_atomic, new_type, codebase);

        if let Some(intersected_atomic_type) = intersected_atomic_type {
            matching_atomic_types.push(intersected_atomic_type);
        }
    }

    if !matching_atomic_types.is_empty() {
        return Some(TUnion::new(matching_atomic_types));
    }

    None
}

fn intersect_atomic_with_atomic(
    type_1_atomic: &TAtomic,
    type_2_atomic: &TAtomic,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let mut atomic_comparison_results = TypeComparisonResult::new();

    if atomic_type_comparator::is_contained_by(
        codebase,
        type_2_atomic,
        type_1_atomic,
        !matches!(type_1_atomic, TAtomic::TNamedObject { .. })
            && !matches!(type_2_atomic, TAtomic::TNamedObject { .. }),
        &mut atomic_comparison_results,
    ) {
        if let TAtomic::TTypeAlias {
            as_type: Some(_), ..
        } = type_2_atomic
        {
            return Some(type_2_atomic.clone());
        }

        let type_2_atomic =
            if let Some(replacement) = atomic_comparison_results.replacement_atomic_type {
                replacement
            } else {
                type_2_atomic.clone()
            };

        return intersect_contained_atomic_with_another(
            type_1_atomic,
            &type_2_atomic,
            codebase,
            atomic_comparison_results.type_coerced.unwrap_or(false),
        );
    }

    if atomic_type_comparator::is_contained_by(
        codebase,
        type_1_atomic,
        type_2_atomic,
        !matches!(type_1_atomic, TAtomic::TNamedObject { .. })
            && !matches!(type_2_atomic, TAtomic::TNamedObject { .. }),
        &mut atomic_comparison_results,
    ) {
        let type_1_atomic =
            if let Some(replacement) = atomic_comparison_results.replacement_atomic_type {
                replacement
            } else {
                type_1_atomic.clone()
            };

        return intersect_contained_atomic_with_another(
            type_2_atomic,
            &type_1_atomic,
            codebase,
            atomic_comparison_results.type_coerced.unwrap_or(false),
        );
    }

    match (type_1_atomic, type_2_atomic) {
        (TAtomic::TEnum { name: type_1_name }, TAtomic::TEnum { name: type_2_name }) => {
            if let (Some(storage_1), Some(storage_2)) = (
                codebase.classlike_infos.get(type_1_name),
                codebase.classlike_infos.get(type_2_name),
            ) {
                for (_, c1) in &storage_1.constants {
                    for (_, c2) in &storage_2.constants {
                        if let (Some(c1_type), Some(c2_type)) =
                            (&c1.inferred_type, &c2.inferred_type)
                        {
                            if c1_type == c2_type {
                                return Some(type_2_atomic.clone());
                            }
                        } else {
                            return Some(type_2_atomic.clone());
                        }
                    }
                }
            }
        }
        (
            TAtomic::TEnumLiteralCase {
                enum_name: type_1_name,
                member_name,
                ..
            },
            TAtomic::TEnum { name: type_2_name },
        ) => {
            if let (Some(storage_1), Some(storage_2)) = (
                codebase.classlike_infos.get(type_1_name),
                codebase.classlike_infos.get(type_2_name),
            ) {
                if let Some(c1) = &storage_1.constants.get(member_name) {
                    for (_, c2) in &storage_2.constants {
                        if let (Some(c1_type), Some(c2_type)) =
                            (&c1.inferred_type, &c2.inferred_type)
                        {
                            if c1_type == c2_type {
                                return Some(type_2_atomic.clone());
                            }
                        } else {
                            return Some(type_2_atomic.clone());
                        }
                    }
                }
            }
        }
        (TAtomic::TEnumLiteralCase { .. }, TAtomic::TString) => {
            return Some(type_1_atomic.clone());
        }
        (TAtomic::TString, TAtomic::TEnumLiteralCase { .. }) => {
            return Some(type_2_atomic.clone());
        }
        (
            TAtomic::TNamedObject {
                name: type_1_name, ..
            },
            TAtomic::TNamedObject {
                name: type_2_name, ..
            },
        ) => {
            if codebase.interface_exists(type_1_name) || codebase.interface_exists(type_2_name) {
                let mut type_1_atomic = type_1_atomic.clone();
                type_1_atomic.add_intersection_type(type_2_atomic.clone());

                return Some(type_1_atomic);
            }
        }
        (
            TAtomic::TDict {
                known_items: Some(type_1_known_items),
                params: type_1_params,
                ..
            },
            TAtomic::TDict {
                known_items: Some(type_2_known_items),
                params: type_2_params,
                ..
            },
        ) => {
            let params = match (type_1_params, type_2_params) {
                (Some(_type_1_params), Some(type_2_params)) => Some(type_2_params.clone()),
                _ => None,
            };

            let mut type_2_known_items = type_2_known_items.clone();

            for (type_2_key, type_2_value) in type_2_known_items.iter_mut() {
                if let Some(type_1_value) = type_1_known_items.get(type_2_key) {
                    type_2_value.0 = type_2_value.0 && type_1_value.0;
                    // todo check intersected type is valie
                } else if let None = type_1_params {
                    // if the type_2 key is always defined, the intersection is impossible
                    if !type_2_value.0 {
                        return None;
                    }
                }
            }

            return Some(TAtomic::TDict {
                known_items: Some(type_2_known_items),
                params,
                non_empty: true,
                shape_name: None,
            });
        }
        (TAtomic::TTemplateParam { as_type, .. }, TAtomic::TNamedObject { .. }) => {
            let new_as = intersect_union_with_atomic(codebase, as_type, type_2_atomic);

            if let Some(new_as) = new_as {
                let mut type_1_atomic = type_1_atomic.clone();

                if let TAtomic::TTemplateParam {
                    ref mut as_type, ..
                } = type_1_atomic
                {
                    *as_type = new_as;
                }

                return Some(type_1_atomic);
            }
        }
        (TAtomic::TNamedObject { .. }, TAtomic::TTemplateParam { as_type, .. }) => {
            let new_as = intersect_union_with_atomic(codebase, as_type, type_1_atomic);

            if let Some(new_as) = new_as {
                let mut type_2_atomic = type_2_atomic.clone();

                if let TAtomic::TTemplateParam {
                    ref mut as_type, ..
                } = type_2_atomic
                {
                    *as_type = new_as;
                }

                return Some(type_2_atomic);
            }
        }
        _ => (),
    }

    // todo intersect T1 as object && T2 as object

    // todo intersect Foo<int> and Foo<arraykey> in a way that's not broken

    None
}

fn intersect_contained_atomic_with_another(
    type_1_atomic: &TAtomic,
    type_2_atomic: &TAtomic,
    codebase: &CodebaseInfo,
    generic_coercion: bool,
) -> Option<TAtomic> {
    if generic_coercion {
        if let TAtomic::TNamedObject {
            name: type_2_name,
            type_params: None,
            ..
        } = type_2_atomic
        {
            if let TAtomic::TNamedObject {
                type_params: Some(type_1_params),
                ..
            } = type_1_atomic
            {
                // this is a hack - it's not actually rigorous, as the params may be different
                return Some(TAtomic::TNamedObject {
                    name: type_2_name.clone(),
                    type_params: Some(type_1_params.clone()),
                    is_this: false,
                    extra_types: None,
                    remapped_params: false,
                });
            }
        }
    }

    if let TAtomic::TNamedObject { .. } = type_2_atomic {
        let mut type_1_atomic = type_1_atomic.clone();
        if let TAtomic::TTemplateParam {
            as_type: ref mut type_1_as_type,
            ..
        } = type_1_atomic
        {
            if type_1_as_type.has_object_type() {
                let type_1_as =
                    intersect_union_with_atomic(codebase, &type_1_as_type, type_2_atomic);

                if let Some(type_1_as) = type_1_as {
                    *type_1_as_type = type_1_as;
                } else {
                    return None;
                }

                return Some(type_1_atomic);
            }
        }
    }

    let mut type_2_atomic = type_2_atomic.clone();
    type_2_atomic.remove_placeholders();

    Some(type_2_atomic)
}

fn get_missing_type(assertion: &Assertion, inside_loop: bool) -> TUnion {
    if matches!(assertion, Assertion::IsIsset | Assertion::IsEqualIsset) {
        return get_mixed_maybe_from_loop(inside_loop);
    }

    if matches!(
        assertion,
        Assertion::ArrayKeyExists | Assertion::NonEmptyCountable(_) | Assertion::HasExactCount(_)
    ) {
        return get_mixed_any();
    }

    if let Assertion::IsEqual(atomic) | Assertion::IsType(atomic) = assertion {
        return wrap_atomic(atomic.clone());
    }

    get_mixed_any()
}
