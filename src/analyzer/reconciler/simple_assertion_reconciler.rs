use super::reconciler::{trigger_issue_for_impossible, ReconciliationStatus};
use crate::{intersect_simple, statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo};
use hakana_reflection_info::{
    assertion::Assertion, codebase_info::CodebaseInfo, t_atomic::TAtomic, t_union::TUnion,
};
use hakana_type::{
    get_arraykey, get_bool, get_dict, get_false, get_float, get_int, get_keyset, get_mixed_any,
    get_mixed_closure, get_mixed_dict, get_mixed_maybe_from_loop, get_mixed_vec, get_nothing,
    get_null, get_num, get_object, get_scalar, get_string, get_true, get_vec,
    intersect_union_types,
    type_comparator::{atomic_type_comparator, type_comparison_result::TypeComparisonResult},
};
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;

// This performs type intersections and more general reconciliations
pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: &Option<String>,
    codebase: &CodebaseInfo,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    negated: bool,
    inside_loop: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> Option<TUnion> {
    let assertion_type = assertion.get_type();

    if let Some(assertion_type) = assertion_type {
        if assertion_type.is_mixed() && existing_var_type.is_mixed() {
            return Some(existing_var_type.clone());
        }

        if let TAtomic::TClosure { .. } = assertion_type {
            return intersect_simple!(
                TAtomic::TClosure { .. },
                TAtomic::TMixedAny
                    | TAtomic::TMixed
                    | TAtomic::TTruthyMixed
                    | TAtomic::TNonnullMixed
                    | TAtomic::TMixedFromLoopIsset,
                get_mixed_closure(),
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            );
        }

        if let TAtomic::TScalar { .. } = assertion_type {
            return intersect_simple!(
                TAtomic::TLiteralClassname { .. }
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TArraykey { .. }
                    | TAtomic::TBool { .. }
                    | TAtomic::TClassname { .. }
                    | TAtomic::TFalse
                    | TAtomic::TFloat
                    | TAtomic::TInt { .. }
                    | TAtomic::TStringWithFlags(..)
                    | TAtomic::TNum
                    | TAtomic::TString
                    | TAtomic::TTrue,
                TAtomic::TMixed
                    | TAtomic::TFalsyMixed
                    | TAtomic::TTruthyMixed
                    | TAtomic::TNonnullMixed
                    | TAtomic::TMixedAny
                    | TAtomic::TMixedFromLoopIsset,
                get_scalar(),
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            );
        }

        if let TAtomic::TBool { .. } = assertion_type {
            return intersect_simple!(
                TAtomic::TBool { .. } | TAtomic::TFalse | TAtomic::TTrue,
                TAtomic::TMixed
                    | TAtomic::TFalsyMixed
                    | TAtomic::TTruthyMixed
                    | TAtomic::TNonnullMixed
                    | TAtomic::TMixedAny
                    | TAtomic::TScalar
                    | TAtomic::TMixedFromLoopIsset,
                get_bool(),
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            );
        }

        if let TAtomic::TFalse { .. } = assertion_type {
            return intersect_simple!(
                TAtomic::TFalse { .. },
                TAtomic::TMixed
                    | TAtomic::TFalsyMixed
                    | TAtomic::TNonnullMixed
                    | TAtomic::TMixedAny
                    | TAtomic::TScalar
                    | TAtomic::TBool
                    | TAtomic::TMixedFromLoopIsset,
                get_false(),
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            );
        }

        if let TAtomic::TTrue { .. } = assertion_type {
            return intersect_simple!(
                TAtomic::TTrue { .. },
                TAtomic::TMixed
                    | TAtomic::TTruthyMixed
                    | TAtomic::TNonnullMixed
                    | TAtomic::TMixedAny
                    | TAtomic::TScalar
                    | TAtomic::TBool
                    | TAtomic::TMixedFromLoopIsset,
                get_true(),
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            );
        }

        if let TAtomic::TFloat { .. } = assertion_type {
            return intersect_simple!(
                TAtomic::TFloat { .. },
                TAtomic::TMixed
                    | TAtomic::TFalsyMixed
                    | TAtomic::TTruthyMixed
                    | TAtomic::TNonnullMixed
                    | TAtomic::TMixedAny
                    | TAtomic::TScalar
                    | TAtomic::TNum
                    | TAtomic::TMixedFromLoopIsset,
                get_float(),
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            );
        }

        if let TAtomic::TNull { .. } = assertion_type {
            return intersect_simple!(
                TAtomic::TNull { .. },
                TAtomic::TMixed | TAtomic::TFalsyMixed | TAtomic::TMixedAny,
                get_null(),
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            );
        }

        if let TAtomic::TObject = assertion_type {
            return Some(intersect_object(
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }

        if let TAtomic::TVec { .. } = assertion_type {
            return Some(intersect_vec(
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }

        if let TAtomic::TDict { .. } = assertion_type {
            return Some(intersect_dict(
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }

        if let TAtomic::TKeyset { .. } = assertion_type {
            return Some(intersect_keyset(
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }

        if let TAtomic::TArraykey { .. } = assertion_type {
            return Some(intersect_arraykey(
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }

        if let TAtomic::TNum { .. } = assertion_type {
            return Some(intersect_num(
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }

        if let TAtomic::TString = assertion_type {
            return Some(intersect_string(
                codebase,
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }

        if let TAtomic::TInt = assertion_type {
            return Some(intersect_int(
                codebase,
                assertion,
                existing_var_type,
                key,
                negated,
                tast_info,
                statements_analyzer,
                pos,
                failed_reconciliation,
                assertion.has_equality(),
                suppressed_issues,
            ));
        }
    }

    return match assertion {
        Assertion::Truthy => Some(reconcile_truthy(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            false,
        )),
        Assertion::IsEqualIsset | Assertion::IsIsset => Some(reconcile_isset(
            assertion,
            existing_var_type,
            possibly_undefined,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            inside_loop,
        )),
        Assertion::HasStringArrayAccess => Some(reconcile_array_access(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            false,
        )),
        Assertion::HasIntOrStringArrayAccess => Some(reconcile_array_access(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            true,
        )),
        Assertion::ArrayKeyExists => {
            let mut existing_var_type = existing_var_type.clone();
            if existing_var_type.is_nothing() {
                existing_var_type = get_mixed_maybe_from_loop(inside_loop);
            }
            return Some(existing_var_type);
        }
        Assertion::InArray(typed_value) => Some(reconcile_in_array(
            codebase,
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            typed_value,
        )),
        Assertion::HasArrayKey(key_name) => {
            Some(reconcile_has_array_key(existing_var_type, key_name))
        }
        Assertion::NonEmptyCountable(_) => Some(reconcile_non_empty_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            false,
        )),
        Assertion::HasExactCount(count) => Some(reconcile_exactly_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            false,
            count,
        )),
        _ => None,
    };
}

fn intersect_object(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return get_object();
    }

    let old_var_type_string = existing_var_type.get_id();

    let mut object_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        if atomic.is_object_type() {
            object_types.push(atomic.clone());
        } else if let TAtomic::TTemplateParam { as_type, .. } = atomic {
            if as_type.is_mixed() {
                let atomic = atomic.replace_template_extends(get_object());

                object_types.push(atomic);
            } else if as_type.types.contains_key("object") || as_type.is_mixed() {
                let atomic = atomic.replace_template_extends(intersect_object(
                    assertion,
                    as_type,
                    &None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                object_types.push(atomic);
            }

            did_remove_type = true;
        } else {
            did_remove_type = true;
        }
    }

    if (object_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &old_var_type_string,
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !object_types.is_empty() {
        return TUnion::new(object_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn intersect_vec(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return get_mixed_vec();
    }

    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        if matches!(atomic, TAtomic::TVec { .. }) {
            acceptable_types.push(atomic.clone());
        } else {
            if let TAtomic::TNamedObject {
                name,
                type_params: Some(typed_params),
                ..
            } = atomic
            {
                if name == "HH\\Container" {
                    return get_vec(typed_params.get(0).unwrap().clone());
                }

                if name == "HH\\KeyedContainer" {
                    return get_vec(typed_params.get(1).unwrap().clone());
                }
            }

            did_remove_type = true;
        }
    }

    if (acceptable_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &existing_var_type.get_id(),
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !acceptable_types.is_empty() {
        return TUnion::new(acceptable_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn intersect_keyset(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return get_keyset(get_arraykey());
    }

    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        if matches!(atomic, TAtomic::TKeyset { .. }) {
            acceptable_types.push(atomic.clone());
        } else {
            if let TAtomic::TNamedObject {
                name,
                type_params: Some(typed_params),
                ..
            } = atomic
            {
                if name == "HH\\Container" {
                    return get_keyset(get_arraykey());
                }

                if name == "HH\\KeyedContainer" {
                    return get_keyset(typed_params.get(0).unwrap().clone());
                }
            }

            did_remove_type = true;
        }
    }

    if (acceptable_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &existing_var_type.get_id(),
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !acceptable_types.is_empty() {
        return TUnion::new(acceptable_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn intersect_dict(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return get_mixed_dict();
    }

    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        if matches!(atomic, TAtomic::TDict { .. }) {
            acceptable_types.push(atomic.clone());
        } else {
            if let TAtomic::TNamedObject {
                name,
                type_params: Some(typed_params),
                ..
            } = atomic
            {
                if name == "HH\\Container" {
                    return get_dict(get_arraykey(), typed_params.get(0).unwrap().clone());
                }

                if name == "HH\\KeyedContainer" {
                    return get_dict(
                        typed_params.get(0).unwrap().clone(),
                        typed_params.get(1).unwrap().clone(),
                    );
                }
            }

            did_remove_type = true;
        }
    }

    if (acceptable_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &existing_var_type.get_id(),
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !acceptable_types.is_empty() {
        return TUnion::new(acceptable_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn intersect_arraykey(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return get_arraykey();
    }

    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        if atomic.is_int() || atomic.is_string() || matches!(atomic, TAtomic::TArraykey { .. }) {
            acceptable_types.push(atomic.clone());
        } else if matches!(atomic, TAtomic::TNum { .. }) {
            return get_int();
        } else {
            did_remove_type = true;
        }
    }

    if (acceptable_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &existing_var_type.get_id(),
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !acceptable_types.is_empty() {
        return TUnion::new(acceptable_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn intersect_num(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return get_num();
    }

    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        if atomic.is_int() || matches!(atomic, TAtomic::TFloat { .. }) {
            acceptable_types.push(atomic.clone());
        } else if matches!(atomic, TAtomic::TArraykey { .. }) {
            return get_int();
        } else {
            did_remove_type = true;
        }
    }

    if (acceptable_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &existing_var_type.get_id(),
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !acceptable_types.is_empty() {
        return TUnion::new(acceptable_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn intersect_string(
    codebase: &CodebaseInfo,
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        match atomic {
            TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TStringWithFlags(..)
            | TAtomic::TString { .. } => {
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TMixed
            | TAtomic::TFalsyMixed
            | TAtomic::TTruthyMixed
            | TAtomic::TNonnullMixed
            | TAtomic::TMixedFromLoopIsset
            | TAtomic::TMixedAny
            | TAtomic::TScalar
            | TAtomic::TArraykey { .. } => {
                return get_string();
            }
            TAtomic::TEnumLiteralCase {
                constraint_type, ..
            } => {
                if let Some(constraint_type) = constraint_type {
                    if atomic_type_comparator::is_contained_by(
                        codebase,
                        constraint_type,
                        &TAtomic::TString,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) {
                        acceptable_types.push(atomic.clone());
                    } else {
                        did_remove_type = true;
                    }
                } else {
                    return get_string();
                }
            }
            _ => {
                if atomic_type_comparator::is_contained_by(
                    codebase,
                    atomic,
                    &TAtomic::TString,
                    false,
                    &mut TypeComparisonResult::new(),
                ) {
                    acceptable_types.push(atomic.clone());
                } else {
                    did_remove_type = true;
                }
            }
        }
    }

    if (acceptable_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &existing_var_type.get_id(),
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !acceptable_types.is_empty() {
        return TUnion::new(acceptable_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn intersect_int(
    codebase: &CodebaseInfo,
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut acceptable_types = Vec::new();
    let mut did_remove_type = false;

    for (_, atomic) in &existing_var_type.types {
        match atomic {
            TAtomic::TLiteralInt { .. } | TAtomic::TInt => {
                acceptable_types.push(atomic.clone());
            }
            TAtomic::TMixed
            | TAtomic::TFalsyMixed
            | TAtomic::TTruthyMixed
            | TAtomic::TNonnullMixed
            | TAtomic::TMixedAny
            | TAtomic::TScalar
            | TAtomic::TNum
            | TAtomic::TArraykey { .. }
            | TAtomic::TMixedFromLoopIsset => {
                return get_int();
            }
            _ => {
                if atomic_type_comparator::is_contained_by(
                    codebase,
                    atomic,
                    &TAtomic::TInt,
                    false,
                    &mut TypeComparisonResult::new(),
                ) {
                    acceptable_types.push(atomic.clone());
                } else {
                    did_remove_type = true;
                }
            }
        }
    }

    if (acceptable_types.is_empty() || !did_remove_type) && !is_equality {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &existing_var_type.get_id(),
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    if !acceptable_types.is_empty() {
        return TUnion::new(acceptable_types);
    }

    *failed_reconciliation = ReconciliationStatus::Empty;

    get_nothing()
}

fn reconcile_truthy(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    recursive_check: bool,
) -> TUnion {
    let old_var_type_string = existing_var_type.get_id();

    let mut did_remove_type = existing_var_type.possibly_undefined_from_try;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    existing_var_type.possibly_undefined_from_try = false;

    for (type_key, atomic) in existing_var_types {
        // if any atomic in the union is either always falsy, we remove it.
        // If not always truthy, we mark the check as not redundant.
        if atomic.is_falsy() {
            did_remove_type = true;
            existing_var_type.types.remove(type_key);
        } else if !atomic.is_truthy() || existing_var_type.possibly_undefined_from_try {
            did_remove_type = true;

            if let TAtomic::TTemplateParam { as_type, .. } = atomic {
                if !as_type.is_mixed() {
                    let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                    let atomic = atomic.replace_template_extends(reconcile_truthy(
                        assertion,
                        as_type,
                        &None,
                        false,
                        tast_info,
                        statements_analyzer,
                        None,
                        &mut template_failed_reconciliation,
                        suppressed_issues,
                        true,
                    ));

                    if template_failed_reconciliation == ReconciliationStatus::Ok {
                        existing_var_type.types.remove(type_key);
                        existing_var_type.types.insert(atomic.get_key(), atomic);
                    }
                }
            } else if let TAtomic::TBool { .. } = atomic {
                existing_var_type.types.remove("bool");
                existing_var_type
                    .types
                    .insert("true".to_string(), TAtomic::TTrue);
            } else if let TAtomic::TVec { .. } = atomic {
                existing_var_type
                    .types
                    .insert("vec".to_string(), atomic.get_non_empty_vec(None));
            } else if let TAtomic::TDict { .. } = atomic {
                existing_var_type
                    .types
                    .insert("dict".to_string(), atomic.clone().make_non_empty_dict());
            } else if let TAtomic::TMixed | TAtomic::TMixedAny = atomic {
                existing_var_type
                    .types
                    .insert("mixed".to_string(), TAtomic::TTruthyMixed);
            } else if let TAtomic::TMixedFromLoopIsset = atomic {
                existing_var_type
                    .types
                    .insert("mixed".to_string(), TAtomic::TTruthyMixed);
            } else if let TAtomic::TString = atomic {
                existing_var_type.types.insert(
                    "string".to_string(),
                    TAtomic::TStringWithFlags(true, false, false),
                );
            } else if let TAtomic::TStringWithFlags(_, _, is_nonspecific_literal) = atomic {
                existing_var_type.types.insert(
                    "string".to_string(),
                    TAtomic::TStringWithFlags(true, false, *is_nonspecific_literal),
                );
            }
        }
    }

    if !did_remove_type || existing_var_type.types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                if !recursive_check {
                    trigger_issue_for_impossible(
                        tast_info,
                        statements_analyzer,
                        &old_var_type_string,
                        &key,
                        assertion,
                        !did_remove_type,
                        negated,
                        pos,
                        suppressed_issues,
                    );
                }
            }
        }

        if existing_var_type.types.is_empty() {
            *failed_reconciliation = ReconciliationStatus::Empty;
            return get_nothing();
        }

        *failed_reconciliation = ReconciliationStatus::Redundant;
    }

    existing_var_type
}

fn reconcile_isset(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    inside_loop: bool,
) -> TUnion {
    let old_var_type_string = existing_var_type.get_id();

    let mut did_remove_type = possibly_undefined || existing_var_type.possibly_undefined_from_try;

    if let Some(key) = key {
        if key.contains("[") {
            did_remove_type = true;
        }
    }

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for (type_key, atomic) in existing_var_types {
        if let TAtomic::TNull { .. } = atomic {
            existing_var_type.types.remove(type_key);
            did_remove_type = true;
        } else if let TAtomic::TMixed | TAtomic::TMixedAny | TAtomic::TFalsyMixed = atomic {
            existing_var_type.types.remove(type_key);
            existing_var_type
                .types
                .insert("nonnull".to_string(), TAtomic::TNonnullMixed);
            did_remove_type = true;
        }
    }

    existing_var_type.possibly_undefined_from_try = false;

    if !did_remove_type || existing_var_type.types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &old_var_type_string,
                    &key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }

        if existing_var_type.types.is_empty() {
            *failed_reconciliation = ReconciliationStatus::Empty;
            return get_nothing();
        }

        *failed_reconciliation = ReconciliationStatus::Redundant;
    }

    if existing_var_type.is_nothing() {
        existing_var_type.types.remove("nothing");
        existing_var_type.types.insert(
            "mixed".to_string(),
            if !inside_loop {
                TAtomic::TMixed
            } else {
                TAtomic::TMixedFromLoopIsset
            },
        );
    }

    existing_var_type
}

fn reconcile_non_empty_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    recursive_check: bool,
) -> TUnion {
    let old_var_type_string = existing_var_type.get_id();

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for (type_key, atomic) in existing_var_types {
        if let TAtomic::TVec {
            non_empty,
            type_param,
            ..
        } = atomic
        {
            if !non_empty {
                if type_param.is_nothing() {
                    existing_var_type.types.remove(type_key);
                } else {
                    let non_empty_vec = atomic.get_non_empty_vec(None);

                    existing_var_type
                        .types
                        .insert(non_empty_vec.get_key(), non_empty_vec);
                }

                did_remove_type = true;
            }
        } else if let TAtomic::TDict {
            non_empty,
            value_param,
            known_items,
            ..
        } = atomic
        {
            if !non_empty {
                if value_param.is_nothing() {
                    existing_var_type.types.remove(type_key);
                } else {
                    let non_empty_dict = atomic.clone().make_non_empty_dict();

                    existing_var_type
                        .types
                        .insert(non_empty_dict.get_key(), non_empty_dict);
                }

                did_remove_type = true;
            } else if let Some(known_items) = known_items {
                for (_, (u, _)) in known_items {
                    if *u {
                        did_remove_type = true;
                    }
                }
            }
        }
    }

    if !did_remove_type || existing_var_type.types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                if !recursive_check {
                    trigger_issue_for_impossible(
                        tast_info,
                        statements_analyzer,
                        &old_var_type_string,
                        &key,
                        assertion,
                        !did_remove_type,
                        negated,
                        pos,
                        suppressed_issues,
                    );
                }
            }
        }

        if existing_var_type.types.is_empty() {
            *failed_reconciliation = ReconciliationStatus::Empty;
            return get_nothing();
        }

        *failed_reconciliation = ReconciliationStatus::Redundant;
    }

    existing_var_type
}

fn reconcile_exactly_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    recursive_check: bool,
    count: &usize,
) -> TUnion {
    let old_var_type_string = existing_var_type.get_id();

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for (type_key, atomic) in existing_var_types {
        if let TAtomic::TVec {
            non_empty,
            known_count,
            type_param,
            ..
        } = atomic
        {
            let min_under_count = if let Some(known_count) = known_count {
                known_count < count
            } else {
                false
            };
            if !non_empty || min_under_count {
                if type_param.is_nothing() {
                    existing_var_type.types.remove(type_key);
                } else {
                    let non_empty_vec = atomic.get_non_empty_vec(Some(*count));

                    existing_var_type
                        .types
                        .insert(non_empty_vec.get_key(), non_empty_vec);
                }

                did_remove_type = true;
            }
        } else if let TAtomic::TDict {
            non_empty,
            value_param,
            known_items,
            ..
        } = atomic
        {
            if !non_empty {
                if value_param.is_nothing() {
                    existing_var_type.types.remove(type_key);
                } else {
                    let non_empty_dict = atomic.clone().make_non_empty_dict();

                    existing_var_type
                        .types
                        .insert(non_empty_dict.get_key(), non_empty_dict);
                }

                did_remove_type = true;
            } else if let Some(known_items) = known_items {
                for (_, (u, _)) in known_items {
                    if *u {
                        did_remove_type = true;
                    }
                }
            }
        }
    }

    if !did_remove_type || existing_var_type.types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                if !recursive_check {
                    trigger_issue_for_impossible(
                        tast_info,
                        statements_analyzer,
                        &old_var_type_string,
                        &key,
                        assertion,
                        !did_remove_type,
                        negated,
                        pos,
                        suppressed_issues,
                    );
                }
            }
        }

        if existing_var_type.types.is_empty() {
            *failed_reconciliation = ReconciliationStatus::Empty;
            return get_nothing();
        }

        *failed_reconciliation = ReconciliationStatus::Redundant;
    }

    existing_var_type
}

fn reconcile_array_access(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    allow_int_key: bool,
) -> TUnion {
    let old_var_type_string = existing_var_type.get_id();

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    if existing_var_type.is_mixed() || existing_var_type.has_template() {
        // maybe return something more specific in the future
        // dict<arraykey, mixed>|keyset<arraykey>
        return existing_var_type;
    }

    for (type_key, atomic) in existing_var_types {
        if (allow_int_key && atomic.is_array_accessible_with_int_or_string_key())
            || (!allow_int_key && atomic.is_array_accessible_with_string_key())
        {
            // do nothing
        } else {
            existing_var_type.types.remove(type_key);
        }
    }

    if existing_var_type.types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &old_var_type_string,
                    &key,
                    assertion,
                    false,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }

        if existing_var_type.types.is_empty() {
            *failed_reconciliation = ReconciliationStatus::Empty;
            return get_nothing();
        }

        *failed_reconciliation = ReconciliationStatus::Redundant;
    }

    existing_var_type
}

fn reconcile_in_array(
    codebase: &CodebaseInfo,
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    _failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    typed_value: &TUnion,
) -> TUnion {
    let intersection = intersect_union_types(typed_value, existing_var_type, Some(codebase));

    if let Some(intersection) = intersection {
        return intersection;
    }

    if let Some(key) = key {
        if let Some(pos) = pos {
            trigger_issue_for_impossible(
                tast_info,
                statements_analyzer,
                &existing_var_type.get_id(),
                &key,
                assertion,
                true,
                negated,
                pos,
                suppressed_issues,
            );
        }
    }

    get_mixed_any()
}

fn reconcile_has_array_key(existing_var_type: &TUnion, key_name: &String) -> TUnion {
    let mut existing_var_type = existing_var_type.clone();

    for (_, atomic) in existing_var_type.types.iter_mut() {
        if let TAtomic::TDict {
            known_items: Some(known_items),
            ..
        } = atomic
        {
            if let Some(known_item) = known_items.get_mut(key_name) {
                *known_item = (false, known_item.1.clone());

                break;
            }
        }
    }

    existing_var_type
}
