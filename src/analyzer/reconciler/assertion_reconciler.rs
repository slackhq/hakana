use std::{collections::BTreeMap, sync::Arc};

use super::{
    negated_assertion_reconciler, reconciler::trigger_issue_for_impossible,
    simple_assertion_reconciler,
};
use crate::{
    function_analysis_data::FunctionAnalysisData, scope_analyzer::ScopeAnalyzer,
    statements_analyzer::StatementsAnalyzer,
};
use hakana_reflection_info::{
    assertion::Assertion,
    codebase_info::CodebaseInfo,
    functionlike_identifier::FunctionLikeIdentifier,
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
    StrId, STR_ANY_ARRAY, STR_CONTAINER, STR_KEYED_CONTAINER, STR_KEYED_TRAVERSABLE,
    STR_TRAVERSABLE, STR_XHP_CHILD,
};
use hakana_type::{
    get_arraykey, get_mixed_any, get_mixed_maybe_from_loop, get_nothing, type_combiner,
    type_comparator::{atomic_type_comparator, type_comparison_result::TypeComparisonResult},
    type_expander::{self, TypeExpansionOptions},
    wrap_atomic,
};
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;

pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: Option<&TUnion>,
    possibly_undefined: bool,
    key: Option<&String>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    inside_loop: bool,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    can_report_issues: bool,
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

    let old_var_type_string = existing_var_type.get_id(Some(statements_analyzer.get_interner()));

    if is_negation {
        return negated_assertion_reconciler::reconcile(
            assertion,
            existing_var_type,
            possibly_undefined,
            key,
            statements_analyzer,
            analysis_data,
            old_var_type_string,
            if can_report_issues { pos } else { None },
            calling_functionlike_id,
            negated,
            suppressed_issues,
        );
    }

    let simple_asserted_type = simple_assertion_reconciler::reconcile(
        assertion,
        &existing_var_type,
        possibly_undefined,
        key.clone(),
        codebase,
        analysis_data,
        statements_analyzer,
        if can_report_issues { pos } else { None },
        calling_functionlike_id,
        negated,
        inside_loop,
        suppressed_issues,
    );

    if let Some(simple_asserted_type) = simple_asserted_type {
        return simple_asserted_type;
    }

    if let Some(assertion_type) = assertion.get_type() {
        let mut refined_type =
            refine_atomic_with_union(statements_analyzer, assertion_type, existing_var_type);

        if let Some(key) = key {
            if let Some(pos) = pos {
                if can_report_issues {
                    if &existing_var_type.types == &refined_type.types {
                        if !assertion.has_equality() && !assertion_type.is_mixed() {
                            trigger_issue_for_impossible(
                                analysis_data,
                                statements_analyzer,
                                &old_var_type_string,
                                key,
                                assertion,
                                true,
                                negated,
                                pos,
                                calling_functionlike_id,
                                suppressed_issues,
                            );
                        }
                    } else if refined_type.is_nothing() {
                        trigger_issue_for_impossible(
                            analysis_data,
                            statements_analyzer,
                            &old_var_type_string,
                            key,
                            assertion,
                            false,
                            negated,
                            pos,
                            calling_functionlike_id,
                            suppressed_issues,
                        );
                    }
                }
            }
        }

        type_expander::expand_union(
            codebase,
            &Some(statements_analyzer.get_interner()),
            &mut refined_type,
            &TypeExpansionOptions {
                expand_generic: true,
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
        );

        return refined_type;
    }

    get_mixed_any()
}

pub(crate) fn refine_atomic_with_union(
    statements_analyzer: &StatementsAnalyzer,
    new_type: &TAtomic,
    existing_var_type: &TUnion,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let intersection_type = intersect_union_with_atomic(codebase, existing_var_type, &new_type);

    if let Some(mut intersection_type) = intersection_type {
        for intersection_atomic_type in intersection_type.types.iter_mut() {
            intersection_atomic_type.remove_placeholders();
        }

        return intersection_type;
    }

    return get_nothing();
}

fn intersect_union_with_atomic(
    codebase: &CodebaseInfo,
    existing_var_type: &TUnion,
    new_type: &TAtomic,
) -> Option<TUnion> {
    let mut acceptable_types = Vec::new();

    for existing_atomic in &existing_var_type.types {
        let intersected_atomic_type =
            intersect_atomic_with_atomic(existing_atomic, new_type, codebase);

        if let Some(intersected_atomic_type) = intersected_atomic_type {
            acceptable_types.push(intersected_atomic_type);
        }
    }

    if !acceptable_types.is_empty() {
        if acceptable_types.len() > 1 {
            acceptable_types = type_combiner::combine(acceptable_types, codebase, false);
        }
        return Some(TUnion::new(acceptable_types));
    }

    None
}

pub(crate) fn intersect_atomic_with_atomic(
    type_1_atomic: &TAtomic,
    type_2_atomic: &TAtomic,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let mut atomic_comparison_results = TypeComparisonResult::new();

    if atomic_type_comparator::is_contained_by(
        codebase,
        type_2_atomic,
        type_1_atomic,
        true,
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

    atomic_comparison_results = TypeComparisonResult::new();

    if atomic_type_comparator::is_contained_by(
        codebase,
        type_1_atomic,
        type_2_atomic,
        false,
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

    if let TAtomic::TClassTypeConstant { .. } = type_1_atomic {
        return Some(type_2_atomic.clone());
    }

    if let TAtomic::TClassTypeConstant { .. } = type_2_atomic {
        return Some(type_1_atomic.clone());
    }

    if let TAtomic::TTypeVariable { .. } = type_1_atomic {
        return Some(type_1_atomic.clone());
    }

    if let TAtomic::TTypeVariable { .. } = type_2_atomic {
        return Some(type_2_atomic.clone());
    }

    match (type_1_atomic, type_2_atomic) {
        (
            TAtomic::TEnum {
                name: type_1_name, ..
            },
            TAtomic::TEnum {
                name: type_2_name, ..
            },
        ) => {
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
            TAtomic::TEnum {
                name: type_2_name, ..
            },
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
        (
            TAtomic::TEnumLiteralCase {
                enum_name: type_1_name,
                member_name: type_1_member_name,
                ..
            },
            TAtomic::TString | TAtomic::TStringWithFlags(..),
        ) => {
            return intersect_enumcase_with_string(
                codebase,
                type_1_name,
                type_1_member_name,
                type_2_atomic,
            );
        }
        (
            TAtomic::TString | TAtomic::TStringWithFlags(..),
            TAtomic::TEnumLiteralCase {
                enum_name: type_2_name,
                member_name: type_2_member_name,
                ..
            },
        ) => {
            return intersect_enumcase_with_string(
                codebase,
                type_2_name,
                type_2_member_name,
                type_1_atomic,
            );
        }
        (
            TAtomic::TEnumLiteralCase {
                enum_name: type_1_name,
                member_name: type_1_member_name,
                ..
            },
            TAtomic::TInt,
        ) => {
            return intersect_enum_case_with_int(
                codebase,
                type_1_name,
                type_1_member_name,
                type_2_atomic,
            )
        }
        (
            TAtomic::TInt,
            TAtomic::TEnumLiteralCase {
                enum_name: type_2_name,
                member_name: type_2_member_name,
                ..
            },
        ) => {
            return intersect_enum_case_with_int(
                codebase,
                type_2_name,
                type_2_member_name,
                type_1_atomic,
            )
        }
        (
            TAtomic::TEnum {
                name: type_1_name, ..
            },
            TAtomic::TLiteralInt { .. } | TAtomic::TLiteralString { .. },
        ) => return intersect_enum_with_literal(codebase, type_1_name, type_2_atomic),
        (
            TAtomic::TTypeAlias {
                as_type: Some(type_1_as),
                name,
                type_params,
            },
            _,
        ) => {
            return if let Some(intersected) =
                intersect_union_with_atomic(codebase, type_1_as, type_2_atomic)
            {
                Some(TAtomic::TTypeAlias {
                    name: *name,
                    type_params: type_params.clone(),
                    as_type: Some(Box::new(intersected)),
                })
            } else {
                None
            }
        }
        (
            _,
            TAtomic::TTypeAlias {
                as_type: Some(type_2_as),
                name,
                type_params,
            },
        ) => {
            return if let Some(intersected) =
                intersect_union_with_atomic(codebase, type_2_as, type_1_atomic)
            {
                Some(TAtomic::TTypeAlias {
                    name: *name,
                    type_params: type_params.clone(),
                    as_type: Some(Box::new(intersected)),
                })
            } else {
                None
            }
        }
        (
            TAtomic::TNamedObject {
                name: type_1_name, ..
            },
            TAtomic::TNamedObject {
                name: type_2_name,
                type_params: type_2_params,
                ..
            },
        ) => {
            if type_1_name == &STR_XHP_CHILD {
                if type_2_name == &STR_KEYED_CONTAINER || type_2_name == &STR_KEYED_TRAVERSABLE {
                    let mut atomic = TAtomic::TNamedObject {
                        name: STR_ANY_ARRAY,
                        type_params: type_2_params.clone(),
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    };
                    atomic.remove_placeholders();
                    return Some(atomic);
                } else if type_2_name == &STR_CONTAINER || type_2_name == &STR_TRAVERSABLE {
                    let type_2_params = if let Some(type_2_params) = type_2_params {
                        Some(vec![get_arraykey(true), type_2_params[0].clone()])
                    } else {
                        None
                    };

                    let mut atomic = TAtomic::TNamedObject {
                        name: STR_ANY_ARRAY,
                        type_params: type_2_params,
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    };
                    atomic.remove_placeholders();
                    return Some(atomic);
                }
            }
            if (codebase.interface_exists(type_1_name)
                && codebase.can_intersect_interface(type_2_name))
                || (codebase.interface_exists(type_2_name)
                    && codebase.can_intersect_interface(type_1_name))
            {
                let mut type_1_atomic = type_1_atomic.clone();
                type_1_atomic.add_intersection_type(type_2_atomic.clone());

                return Some(type_1_atomic);
            }
        }
        (
            TAtomic::TDict {
                known_items: type_1_known_items,
                params: type_1_params,
                ..
            },
            TAtomic::TDict {
                known_items: type_2_known_items,
                params: type_2_params,
                ..
            },
        ) => {
            return intersect_dicts(
                type_1_params,
                type_2_params,
                type_1_known_items,
                type_2_known_items,
                codebase,
            )
        }
        (
            TAtomic::TVec {
                known_items: type_1_known_items,
                type_param: type_1_param,
                ..
            },
            TAtomic::TVec {
                known_items: type_2_known_items,
                type_param: type_2_param,
                ..
            },
        ) => {
            return intersect_vecs(
                type_1_param,
                type_2_param,
                type_1_known_items,
                type_2_known_items,
                codebase,
            );
        }
        (
            TAtomic::TNamedObject {
                name: type_1_name,
                type_params: Some(type_1_params),
                ..
            },
            TAtomic::TDict {
                known_items: Some(type_2_known_items),
                params: type_2_params,
                ..
            },
        ) => {
            let (type_1_key_param, type_1_value_param) = if type_1_name == &STR_CONTAINER {
                (get_arraykey(true), &type_1_params[0])
            } else if type_1_name == &STR_KEYED_CONTAINER {
                (type_1_params[0].clone(), &type_1_params[1])
            } else {
                return None;
            };

            let mut type_2_known_items = type_2_known_items.clone();

            for (_, type_2_value) in type_2_known_items.iter_mut() {
                if type_1_value_param.is_nothing() {
                    // if the type_2 key is always defined, the intersection is impossible
                    if !type_2_value.0 {
                        return None;
                    }
                }
            }

            let mut params = type_2_params.clone();

            if let Some(ref mut params) = params {
                if params.0.is_arraykey() {
                    params.0 = Box::new(type_1_key_param);
                }
            }

            return Some(TAtomic::TDict {
                known_items: Some(type_2_known_items),
                params,
                non_empty: true,
                shape_name: None,
            });
        }
        (TAtomic::TGenericParam { as_type, .. }, TAtomic::TNamedObject { .. }) => {
            let new_as = intersect_union_with_atomic(codebase, as_type, type_2_atomic);

            if let Some(new_as) = new_as {
                let mut type_1_atomic = type_1_atomic.clone();

                if let TAtomic::TGenericParam {
                    ref mut as_type, ..
                } = type_1_atomic
                {
                    *as_type = Box::new(new_as);
                }

                return Some(type_1_atomic);
            }
        }
        (TAtomic::TNamedObject { .. }, TAtomic::TGenericParam { as_type, .. }) => {
            let new_as = intersect_union_with_atomic(codebase, as_type, type_1_atomic);

            if let Some(new_as) = new_as {
                let mut type_2_atomic = type_2_atomic.clone();

                if let TAtomic::TGenericParam {
                    ref mut as_type, ..
                } = type_2_atomic
                {
                    *as_type = Box::new(new_as);
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

fn intersect_vecs(
    type_1_param: &TUnion,
    type_2_param: &TUnion,
    type_1_known_items: &Option<BTreeMap<usize, (bool, TUnion)>>,
    type_2_known_items: &Option<BTreeMap<usize, (bool, TUnion)>>,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let type_param = intersect_union_with_union(type_1_param, type_2_param, codebase);

    match (type_1_known_items, type_2_known_items) {
        (Some(type_1_known_items), Some(type_2_known_items)) => {
            let mut type_2_known_items = type_2_known_items.clone();

            for (type_2_key, type_2_value) in type_2_known_items.iter_mut() {
                if let Some(type_1_value) = type_1_known_items.get(type_2_key) {
                    type_2_value.0 = type_2_value.0 && type_1_value.0;
                    type_2_value.1 = if let Some(t) =
                        intersect_union_with_union(&type_1_value.1, &type_2_value.1, codebase)
                    {
                        t
                    } else {
                        return None;
                    }
                } else if !type_1_param.is_nothing() {
                    type_2_value.0 = type_2_value.0;
                    type_2_value.1 = if let Some(t) =
                        intersect_union_with_union(&type_1_param, &type_2_value.1, codebase)
                    {
                        t
                    } else {
                        return None;
                    }
                } else {
                    // if the type_2 key is always defined, the intersection is impossible
                    if !type_2_value.0 {
                        return None;
                    }
                }
            }

            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec {
                    known_items: Some(type_2_known_items),
                    type_param: Box::new(type_param),
                    non_empty: true,
                    known_count: None,
                });
            }

            return None;
        }
        (None, Some(type_2_known_items)) => {
            let mut type_2_known_items = type_2_known_items.clone();

            for (_, type_2_value) in type_2_known_items.iter_mut() {
                type_2_value.1 = if let Some(t) =
                    intersect_union_with_union(&type_2_value.1, &type_1_param, codebase)
                {
                    t
                } else {
                    return None;
                }
            }

            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec {
                    known_items: Some(type_2_known_items),
                    type_param: Box::new(type_param),
                    non_empty: false,
                    known_count: None,
                });
            }

            return None;
        }
        (Some(type_1_known_items), None) => {
            let mut type_1_known_items = type_1_known_items.clone();

            for (_, type_1_value) in type_1_known_items.iter_mut() {
                type_1_value.1 = if let Some(t) =
                    intersect_union_with_union(&type_1_value.1, &type_2_param, codebase)
                {
                    t
                } else {
                    return None;
                }
            }

            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec {
                    known_items: Some(type_1_known_items),
                    type_param: Box::new(type_param),
                    non_empty: false,
                    known_count: None,
                });
            }

            return None;
        }
        _ => {
            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec {
                    known_items: None,
                    type_param: Box::new(type_param),
                    non_empty: false,
                    known_count: None,
                });
            }

            return None;
        }
    }
}

fn intersect_dicts(
    type_1_params: &Option<(Box<TUnion>, Box<TUnion>)>,
    type_2_params: &Option<(Box<TUnion>, Box<TUnion>)>,
    type_1_known_items: &Option<BTreeMap<DictKey, (bool, Arc<TUnion>)>>,
    type_2_known_items: &Option<BTreeMap<DictKey, (bool, Arc<TUnion>)>>,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let params = match (type_1_params, type_2_params) {
        (Some(type_1_params), Some(type_2_params)) => {
            let key = intersect_union_with_union(&type_1_params.0, &type_2_params.0, codebase);
            let value = intersect_union_with_union(&type_1_params.1, &type_2_params.1, codebase);

            if let (Some(key), Some(value)) = (key, value) {
                Some((Box::new(key), Box::new(value)))
            } else {
                return None;
            }
        }
        _ => None,
    };

    match (type_1_known_items, type_2_known_items) {
        (Some(type_1_known_items), Some(type_2_known_items)) => {
            let mut intersected_items = BTreeMap::new();

            for (type_2_key, type_2_value) in type_2_known_items {
                if let Some(type_1_value) = type_1_known_items.get(type_2_key) {
                    intersected_items.insert(
                        type_2_key.clone(),
                        (
                            type_2_value.0 && type_1_value.0,
                            if let Some(t) = intersect_union_with_union(
                                &type_1_value.1,
                                &type_2_value.1,
                                codebase,
                            ) {
                                Arc::new(t)
                            } else {
                                return None;
                            },
                        ),
                    );
                } else if let Some(type_1_params) = type_1_params {
                    intersected_items.insert(
                        type_2_key.clone(),
                        (
                            type_2_value.0,
                            if let Some(t) = intersect_union_with_union(
                                &type_1_params.1,
                                &type_2_value.1,
                                codebase,
                            ) {
                                Arc::new(t)
                            } else {
                                return None;
                            },
                        ),
                    );
                } else {
                    if !type_2_value.0 {
                        return None;
                    }
                }
            }

            Some(TAtomic::TDict {
                known_items: Some(intersected_items),
                params,
                non_empty: true,
                shape_name: None,
            })
        }
        (None, Some(type_2_known_items)) => {
            let mut type_2_known_items = type_2_known_items.clone();

            for (_, type_2_value) in type_2_known_items.iter_mut() {
                if let Some(type_1_params) = type_1_params {
                    type_2_value.1 = if let Some(t) =
                        intersect_union_with_union(&type_2_value.1, &type_1_params.1, codebase)
                    {
                        Arc::new(t)
                    } else {
                        return None;
                    }
                } else if let None = type_2_params {
                    if !type_2_value.0 {
                        return None;
                    }
                }
            }

            Some(TAtomic::TDict {
                known_items: Some(type_2_known_items),
                params,
                non_empty: true,
                shape_name: None,
            })
        }
        (Some(type_1_known_items), None) => {
            let mut type_1_known_items = type_1_known_items.clone();

            for (_, type_1_value) in type_1_known_items.iter_mut() {
                if let Some(type_2_params) = type_2_params {
                    type_1_value.1 = if let Some(t) =
                        intersect_union_with_union(&type_1_value.1, &type_2_params.1, codebase)
                    {
                        Arc::new(t)
                    } else {
                        return None;
                    }
                } else if let None = type_1_params {
                    if !type_1_value.0 {
                        return None;
                    }
                }
            }

            Some(TAtomic::TDict {
                known_items: Some(type_1_known_items),
                params,
                non_empty: true,
                shape_name: None,
            })
        }
        _ => Some(TAtomic::TDict {
            known_items: None,
            params,
            non_empty: true,
            shape_name: None,
        }),
    }
}

pub(crate) fn intersect_union_with_union(
    type_1_param: &TUnion,
    type_2_param: &TUnion,
    codebase: &CodebaseInfo,
) -> Option<TUnion> {
    let type_param = match (type_1_param.is_single(), type_2_param.is_single()) {
        (true, true) => {
            if let Some(t) = intersect_atomic_with_atomic(
                type_1_param.get_single(),
                type_2_param.get_single(),
                codebase,
            ) {
                Some(wrap_atomic(t))
            } else {
                None
            }
        }
        (false, true) => {
            intersect_union_with_atomic(codebase, type_1_param, type_2_param.get_single())
        }
        (true, false) => {
            intersect_union_with_atomic(codebase, type_2_param, type_1_param.get_single())
        }
        (false, false) => {
            if type_1_param == type_2_param {
                Some(type_1_param.clone())
            } else {
                let new_types = type_2_param
                    .types
                    .iter()
                    .map(|t| {
                        intersect_union_with_atomic(codebase, type_1_param, t)
                            .unwrap_or(get_nothing())
                            .types
                    })
                    .flatten()
                    .collect::<Vec<_>>();

                let combined_union =
                    TUnion::new(type_combiner::combine(new_types, codebase, false));

                if combined_union.is_nothing() {
                    None
                } else {
                    Some(combined_union)
                }
            }
        }
    };
    type_param
}

fn intersect_enumcase_with_string(
    codebase: &CodebaseInfo,
    type_1_name: &StrId,
    type_1_member_name: &StrId,
    type_2_atomic: &TAtomic,
) -> Option<TAtomic> {
    let enum_storage = codebase.classlike_infos.get(type_1_name).unwrap();
    if let Some(member_storage) = enum_storage.constants.get(type_1_member_name) {
        if let Some(inferred_type) = &member_storage.inferred_type {
            if let Some(inferred_value) = inferred_type.get_single_literal_string_value() {
                return Some(TAtomic::TLiteralString {
                    value: inferred_value,
                });
            }
        }
    }
    return Some(type_2_atomic.clone());
}

fn intersect_enum_case_with_int(
    codebase: &CodebaseInfo,
    type_1_name: &StrId,
    type_1_member_name: &StrId,
    type_2_atomic: &TAtomic,
) -> Option<TAtomic> {
    let enum_storage = codebase.classlike_infos.get(type_1_name).unwrap();
    if let Some(member_storage) = enum_storage.constants.get(type_1_member_name) {
        if let Some(inferred_type) = &member_storage.inferred_type {
            if let Some(inferred_value) = inferred_type.get_single_literal_int_value() {
                return Some(TAtomic::TLiteralInt {
                    value: inferred_value,
                });
            }
        }
    }
    Some(type_2_atomic.clone())
}

fn intersect_enum_with_literal(
    codebase: &CodebaseInfo,
    type_1_name: &StrId,
    type_2_atomic: &TAtomic,
) -> Option<TAtomic> {
    let enum_storage = codebase.classlike_infos.get(type_1_name).unwrap();

    let mut all_inferred = true;

    for (case_name, enum_case) in &enum_storage.constants {
        if let Some(inferred_type) = &enum_case.inferred_type {
            if inferred_type.get_single() == type_2_atomic {
                return Some(TAtomic::TEnumLiteralCase {
                    enum_name: *type_1_name,
                    member_name: *case_name,
                    constraint_type: enum_storage.enum_constraint.clone(),
                });
            }
        } else {
            all_inferred = false;
        }
    }

    if all_inferred {
        return None;
    }

    Some(type_2_atomic.clone())
}

fn intersect_contained_atomic_with_another(
    super_atomic: &TAtomic,
    sub_atomic: &TAtomic,
    codebase: &CodebaseInfo,
    generic_coercion: bool,
) -> Option<TAtomic> {
    if generic_coercion {
        if let TAtomic::TNamedObject {
            name: sub_atomic_name,
            type_params: None,
            ..
        } = sub_atomic
        {
            if let TAtomic::TNamedObject {
                name: super_atomic_name,
                type_params: Some(super_params),
                ..
            } = super_atomic
            {
                if super_atomic_name == sub_atomic_name {
                    return Some(TAtomic::TNamedObject {
                        name: sub_atomic_name.clone(),
                        type_params: Some(super_params.clone()),
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    });
                }
            }
        }
    }

    if let TAtomic::TNamedObject { .. } = sub_atomic {
        let mut type_1_atomic = super_atomic.clone();
        if let TAtomic::TGenericParam {
            as_type: ref mut type_1_as_type,
            ..
        } = type_1_atomic
        {
            if type_1_as_type.has_object_type() {
                let type_1_as = intersect_union_with_atomic(codebase, &type_1_as_type, sub_atomic);

                if let Some(type_1_as) = type_1_as {
                    *type_1_as_type = Box::new(type_1_as);
                } else {
                    return None;
                }

                return Some(type_1_atomic);
            }
        }
    }

    Some(sub_atomic.clone())
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
        let mut atomic = atomic.clone();
        atomic.remove_placeholders();
        return wrap_atomic(atomic.clone());
    }

    get_mixed_any()
}
