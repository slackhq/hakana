use super::{
    reconciler::ReconciliationStatus,
    simple_assertion_reconciler::{get_acceptable_type, intersect_null},
};
use crate::{
    reconciler::reconciler::trigger_issue_for_impossible, scope_analyzer::ScopeAnalyzer,
    statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo,
};
use hakana_reflection_info::{
    assertion::Assertion,
    codebase_info::CodebaseInfo,
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_type::{get_mixed_any, get_nothing, get_null, intersect_union_types};
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;

// This performs type subtractions and more general reconciliations
pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&String>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> Option<TUnion> {
    let assertion_type = assertion.get_type();

    if let Some(assertion_type) = assertion_type {
        match assertion_type {
            TAtomic::TObject => {
                return Some(subtract_object(
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
            TAtomic::TBool { .. } => {
                return Some(subtract_bool(
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
            TAtomic::TNum { .. } => {
                return Some(subtract_num(
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
            TAtomic::TFloat { .. } => {
                return Some(subtract_float(
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
            TAtomic::TInt { .. } => {
                return Some(subtract_int(
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
            TAtomic::TString { .. } => {
                return Some(subtract_string(
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
            TAtomic::TArraykey { .. } => {
                return Some(subtract_arraykey(
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
            TAtomic::TVec { .. } => {
                return Some(subtract_vec(
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
            TAtomic::TDict {
                known_items: None,
                params: Some(params),
                ..
            } => {
                if params.0.is_placeholder() && params.1.is_placeholder() {
                    return Some(subtract_dict(
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
            TAtomic::TKeyset { .. } => {
                return Some(subtract_keyset(
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
            TAtomic::TNull { .. } => {
                return Some(subtract_null(
                    assertion,
                    existing_var_type,
                    key,
                    negated,
                    tast_info,
                    statements_analyzer,
                    pos,
                    failed_reconciliation,
                    suppressed_issues,
                ));
            }
            TAtomic::TMixedWithFlags(_, _, _, true) => {
                return Some(intersect_null(
                    assertion,
                    existing_var_type,
                    key,
                    negated,
                    tast_info,
                    statements_analyzer,
                    pos,
                    failed_reconciliation,
                    suppressed_issues,
                ));
            }
            TAtomic::TFalse { .. } => {
                return Some(subtract_false(
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
            TAtomic::TTrue { .. } => {
                return Some(subtract_true(
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
            _ => (),
        }
    }

    return match assertion {
        Assertion::Falsy => Some(reconcile_falsy(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
        )),
        Assertion::IsNotIsset => Some(reconcile_not_isset(
            existing_var_type,
            statements_analyzer,
            possibly_undefined,
            key,
            pos,
            suppressed_issues,
        )),
        Assertion::ArrayKeyDoesNotExist => {
            return Some(get_nothing());
        }
        Assertion::DoesNotHaveArrayKey(key_name) => {
            Some(reconcile_no_array_key(existing_var_type, key_name))
        }
        Assertion::NotInArray(typed_value) => Some(reconcile_not_in_array(
            statements_analyzer.get_codebase(),
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
        Assertion::EmptyCountable => Some(reconcile_empty_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
        )),
        Assertion::DoesNotHaveExactCount(count) => Some(reconcile_not_exactly_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            tast_info,
            statements_analyzer,
            pos,
            failed_reconciliation,
            suppressed_issues,
            count,
        )),
        _ => None,
    };
}

fn subtract_object(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let new_atomic = atomic.replace_template_extends(subtract_object(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if atomic.is_object_type() {
            did_remove_type = true;

            if is_equality {
                acceptable_types.push(atomic);
            }
        } else {
            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

// TODO: in the future subtract from Container and KeyedContainer
fn subtract_vec(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let new_atomic = atomic.replace_template_extends(subtract_vec(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TVec { .. } = atomic {
            did_remove_type = true;

            if is_equality {
                acceptable_types.push(atomic);
            }
        } else {
            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn subtract_keyset(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let new_atomic = atomic.replace_template_extends(subtract_keyset(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TKeyset { .. } = atomic {
            did_remove_type = true;

            if is_equality {
                acceptable_types.push(atomic);
            }
        } else {
            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn subtract_dict(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let new_atomic = atomic.replace_template_extends(subtract_dict(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TDict { .. } = atomic {
            did_remove_type = true;

            if is_equality {
                acceptable_types.push(atomic);
            }
        } else {
            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn subtract_string(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let new_atomic = atomic.replace_template_extends(subtract_string(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TArraykey { .. } = atomic {
            did_remove_type = true;

            if !is_equality {
                acceptable_types.push(TAtomic::TInt);
            } else {
                acceptable_types.push(atomic);
            }
        } else if let TAtomic::TScalar = atomic {
            did_remove_type = true;

            if !is_equality {
                new_var_type.types.push(TAtomic::TNum);
                new_var_type.types.push(TAtomic::TBool);
            } else {
                acceptable_types.push(atomic);
            }
        } else if atomic.is_string() {
            did_remove_type = true;

            if is_equality {
                acceptable_types.push(atomic);
            }
        } else {
            if let TAtomic::TTypeAlias {
                as_type: Some(_), ..
            } = atomic
            {
                did_remove_type = true;
            }

            if let TAtomic::TEnum {
                base_type: Some(_), ..
            } = &atomic
            {
                did_remove_type = true;
            }

            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn subtract_int(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let new_atomic = atomic.replace_template_extends(subtract_int(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TArraykey { .. } = atomic {
            did_remove_type = true;

            if !is_equality {
                acceptable_types.push(TAtomic::TString);
            } else {
                acceptable_types.push(atomic);
            }
        } else if let TAtomic::TScalar = atomic {
            did_remove_type = true;

            if !is_equality {
                acceptable_types.push(TAtomic::TString);
                acceptable_types.push(TAtomic::TFloat);
                acceptable_types.push(TAtomic::TBool);
            } else {
                acceptable_types.push(atomic);
            }
        } else if let TAtomic::TNum = atomic {
            did_remove_type = true;

            if !is_equality {
                acceptable_types.push(TAtomic::TFloat);
            } else {
                acceptable_types.push(atomic);
            }
        } else if atomic.is_int() {
            did_remove_type = true;

            if is_equality {
                acceptable_types.push(atomic);
            }
        } else {
            if let TAtomic::TTypeAlias {
                as_type: Some(_), ..
            } = &atomic
            {
                did_remove_type = true;
            }

            if let TAtomic::TEnum {
                base_type: Some(_), ..
            } = &atomic
            {
                did_remove_type = true;
            }

            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn subtract_float(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let new_atomic = atomic.replace_template_extends(subtract_float(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TScalar = atomic {
            if !is_equality {
                acceptable_types.push(TAtomic::TString);
                acceptable_types.push(TAtomic::TInt);
                acceptable_types.push(TAtomic::TBool);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TNum = atomic {
            if !is_equality {
                acceptable_types.push(TAtomic::TInt);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TFloat { .. } = atomic {
            did_remove_type = true;

            if is_equality {
                acceptable_types.push(atomic);
            }
        } else {
            if let TAtomic::TTypeAlias {
                as_type: Some(_), ..
            } = atomic
            {
                did_remove_type = true;
            }

            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn subtract_num(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string =
        existing_var_type.get_id(Some(&statements_analyzer.get_codebase().interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let atomic = atomic.replace_template_extends(subtract_num(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                if template_failed_reconciliation == ReconciliationStatus::Ok {
                    existing_var_type.remove_type(&atomic);
                    existing_var_type.types.push(atomic);
                }
            }

            did_remove_type = true;
        } else if let TAtomic::TScalar = atomic {
            if !is_equality {
                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(TAtomic::TString);
                existing_var_type.types.push(TAtomic::TBool);
            }

            did_remove_type = true;
        } else if let TAtomic::TArraykey { .. } = atomic {
            if !is_equality {
                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(TAtomic::TString);
            }

            did_remove_type = true;
        } else if let TAtomic::TFloat { .. } | TAtomic::TInt { .. } | TAtomic::TNum { .. } = atomic
        {
            did_remove_type = true;

            if !is_equality {
                existing_var_type.remove_type(&atomic);
            }
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(ref key) = key {
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

        if !did_remove_type {
            *failed_reconciliation = ReconciliationStatus::Redundant;
        }
    }

    if existing_var_type.types.is_empty() {
        *failed_reconciliation = ReconciliationStatus::Empty;
        return get_nothing();
    }

    existing_var_type
}

fn subtract_arraykey(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string =
        existing_var_type.get_id(Some(&statements_analyzer.get_codebase().interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let atomic = atomic.replace_template_extends(subtract_arraykey(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                if template_failed_reconciliation == ReconciliationStatus::Ok {
                    existing_var_type.remove_type(&atomic);
                    existing_var_type.types.push(atomic);
                }
            } else {
                did_remove_type = true;
            }
        } else if let TAtomic::TScalar = atomic {
            if !is_equality {
                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(TAtomic::TFloat);
                existing_var_type.types.push(TAtomic::TBool);
            }

            did_remove_type = true;
        } else if let TAtomic::TNum = atomic {
            if !is_equality {
                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(TAtomic::TFloat);
            }

            did_remove_type = true;
        } else if atomic.is_int()
            || atomic.is_string()
            || matches!(atomic, TAtomic::TArraykey { .. })
        {
            did_remove_type = true;

            if !is_equality {
                existing_var_type.remove_type(&atomic);
            }
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(ref key) = key {
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

        if !did_remove_type {
            *failed_reconciliation = ReconciliationStatus::Redundant;
        }
    }

    if existing_var_type.types.is_empty() {
        *failed_reconciliation = ReconciliationStatus::Empty;
        return get_nothing();
    }

    existing_var_type
}

fn subtract_bool(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string =
        existing_var_type.get_id(Some(&statements_analyzer.get_codebase().interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let atomic = atomic.replace_template_extends(subtract_bool(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                if template_failed_reconciliation == ReconciliationStatus::Ok {
                    existing_var_type.remove_type(&atomic);
                    existing_var_type.types.push(atomic);
                }
            } else {
                did_remove_type = true;
            }
        } else if let TAtomic::TScalar = atomic {
            if !is_equality {
                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(TAtomic::TString);
                existing_var_type.types.push(TAtomic::TInt);
                existing_var_type.types.push(TAtomic::TFloat);
            }

            did_remove_type = true;
        } else if atomic.is_bool() {
            did_remove_type = true;

            if !is_equality {
                existing_var_type.remove_type(&atomic);
            }
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(ref key) = key {
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

        if !did_remove_type {
            *failed_reconciliation = ReconciliationStatus::Redundant;
        }
    }

    if existing_var_type.types.is_empty() {
        *failed_reconciliation = ReconciliationStatus::Empty;
        return get_nothing();
    }

    existing_var_type
}

pub(crate) fn subtract_null(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
            let mut template_failed_reconciliation = ReconciliationStatus::Ok;
            let new_atomic = atomic.replace_template_extends(subtract_null(
                assertion,
                &as_type,
                None,
                false,
                tast_info,
                statements_analyzer,
                None,
                &mut template_failed_reconciliation,
                suppressed_issues,
            ));

            acceptable_types.push(new_atomic);

            did_remove_type = true;
        } else if let TAtomic::TMixed = atomic {
            did_remove_type = true;
            acceptable_types.push(TAtomic::TMixedWithFlags(false, false, false, true));
        } else if let TAtomic::TMixedWithFlags(is_any, false, _, false) = atomic {
            did_remove_type = true;
            acceptable_types.push(TAtomic::TMixedWithFlags(is_any, false, false, true));
        } else if let TAtomic::TNull = atomic {
            did_remove_type = true;
        } else if let TAtomic::TNamedObject {
            name,
            type_params: None,
            ..
        } = atomic
        {
            match statements_analyzer.get_codebase().interner.lookup(name) {
                "XHPChild" => {
                    did_remove_type = true;
                    acceptable_types.push(atomic);
                }
                _ => {
                    acceptable_types.push(atomic);
                }
            }
        } else {
            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn subtract_false(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string =
        existing_var_type.get_id(Some(&statements_analyzer.get_codebase().interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let atomic = atomic.replace_template_extends(subtract_false(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                if template_failed_reconciliation == ReconciliationStatus::Ok {
                    existing_var_type.remove_type(&atomic);
                    existing_var_type.types.push(atomic);
                }
            } else {
                did_remove_type = true;
            }
        } else if let TAtomic::TBool = atomic {
            existing_var_type.remove_type(&atomic);
            existing_var_type.types.push(TAtomic::TTrue);
            did_remove_type = true;
        } else if let TAtomic::TFalse { .. } = atomic {
            did_remove_type = true;

            existing_var_type.remove_type(&atomic);
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(ref key) = key {
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

        if !did_remove_type {
            *failed_reconciliation = ReconciliationStatus::Redundant;
        }
    }

    if existing_var_type.types.is_empty() {
        *failed_reconciliation = ReconciliationStatus::Empty;
        return get_nothing();
    }

    existing_var_type
}

fn subtract_true(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string =
        existing_var_type.get_id(Some(&statements_analyzer.get_codebase().interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TTemplateParam { as_type, .. } = atomic {
            if !is_equality && !as_type.is_mixed() {
                let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                let atomic = atomic.replace_template_extends(subtract_true(
                    assertion,
                    as_type,
                    None,
                    false,
                    tast_info,
                    statements_analyzer,
                    None,
                    &mut template_failed_reconciliation,
                    is_equality,
                    suppressed_issues,
                ));

                if template_failed_reconciliation == ReconciliationStatus::Ok {
                    existing_var_type.remove_type(&atomic);
                    existing_var_type.types.push(atomic);
                }
            } else {
                did_remove_type = true;
            }
        } else if let TAtomic::TBool = atomic {
            existing_var_type.remove_type(&atomic);
            existing_var_type.types.push(TAtomic::TFalse);
            did_remove_type = true;
        } else if let TAtomic::TTrue { .. } = atomic {
            did_remove_type = true;

            existing_var_type.remove_type(&atomic);
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(ref key) = key {
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

        if !did_remove_type {
            *failed_reconciliation = ReconciliationStatus::Redundant;
        }
    }

    if existing_var_type.types.is_empty() {
        *failed_reconciliation = ReconciliationStatus::Empty;
        return get_nothing();
    }

    existing_var_type
}

fn reconcile_falsy(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = existing_var_type.possibly_undefined_from_try;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        // if any atomic in the union is either always falsy, we remove it.
        // If not always truthy, we mark the check as not redundant.
        if atomic.is_truthy(&statements_analyzer.get_codebase().interner)
            && !new_var_type.possibly_undefined_from_try
        {
            did_remove_type = true;
        } else if !atomic.is_falsy() {
            did_remove_type = true;

            if let TAtomic::TTemplateParam { as_type, .. } = &atomic {
                if !as_type.is_mixed() {
                    let mut template_failed_reconciliation = ReconciliationStatus::Ok;
                    let atomic = atomic.replace_template_extends(reconcile_falsy(
                        assertion,
                        as_type,
                        None,
                        false,
                        tast_info,
                        statements_analyzer,
                        None,
                        &mut template_failed_reconciliation,
                        suppressed_issues,
                    ));

                    acceptable_types.push(atomic);
                }
            } else if let TAtomic::TBool { .. } = atomic {
                acceptable_types.push(TAtomic::TFalse);
            } else if let TAtomic::TVec { .. } = atomic {
                let new_atomic = TAtomic::TVec {
                    type_param: get_nothing(),
                    known_items: None,
                    non_empty: false,
                    known_count: None,
                };
                acceptable_types.push(new_atomic);
            } else if let TAtomic::TDict { .. } = atomic {
                let new_atomic = TAtomic::TDict {
                    params: None,
                    known_items: None,
                    non_empty: false,
                    shape_name: None,
                };
                acceptable_types.push(new_atomic);
            } else if let TAtomic::TMixed = atomic {
                acceptable_types.push(TAtomic::TMixedWithFlags(false, false, true, false));
            } else if let TAtomic::TMixedWithFlags(is_any, false, false, _) = atomic {
                acceptable_types.push(TAtomic::TMixedWithFlags(is_any, false, true, false));
            } else if let TAtomic::TMixedFromLoopIsset = atomic {
                acceptable_types.push(TAtomic::TMixedWithFlags(false, false, true, false));
            } else if let TAtomic::TString { .. } = atomic {
                let empty_string = TAtomic::TLiteralString {
                    value: "".to_string(),
                };
                let falsy_string = TAtomic::TLiteralString {
                    value: "0".to_string(),
                };
                acceptable_types.push(empty_string);
                acceptable_types.push(falsy_string);
            } else if let TAtomic::TInt { .. } = atomic {
                let zero = TAtomic::TLiteralInt { value: 0 };
                acceptable_types.push(zero);
            } else {
                acceptable_types.push(atomic);
            }
        } else {
            acceptable_types.push(atomic);
        }
    }

    new_var_type.possibly_undefined_from_try = false;

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn reconcile_not_isset(
    existing_var_type: &TUnion,
    statements_analyzer: &StatementsAnalyzer,
    possibly_undefined: bool,
    key: Option<&String>,
    pos: Option<&Pos>,
    _suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if possibly_undefined {
        return get_nothing();
    }

    if !existing_var_type.is_nullable() {
        if let Some(key) = key {
            if !key.contains("[")
                && (!existing_var_type.is_mixed()
                    || existing_var_type
                        .is_always_truthy(&statements_analyzer.get_codebase().interner))
            {
                if let Some(_pos) = pos {
                    // todo do stuff
                }

                return get_nothing();
            }
        }
    }

    get_null()
}

fn reconcile_empty_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = existing_var_type.possibly_undefined_from_try;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    new_var_type.possibly_undefined_from_try = false;

    for atomic in existing_var_types {
        if let TAtomic::TVec { .. } = atomic {
            did_remove_type = true;

            if atomic.is_truthy(&statements_analyzer.get_codebase().interner) {
                // don't keep
            } else {
                let new_atomic = TAtomic::TVec {
                    type_param: get_nothing(),
                    known_items: None,
                    non_empty: false,
                    known_count: None,
                };
                acceptable_types.push(new_atomic);
            }
        } else if let TAtomic::TDict { .. } = atomic {
            did_remove_type = true;

            if atomic.is_truthy(&statements_analyzer.get_codebase().interner) {
                // don't keep
            } else {
                let new_atomic = TAtomic::TDict {
                    params: None,
                    known_items: None,
                    non_empty: false,
                    shape_name: None,
                };
                acceptable_types.push(new_atomic);
            }
        } else {
            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn reconcile_not_exactly_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    count: &usize,
) -> TUnion {
    let mut did_remove_type = existing_var_type.possibly_undefined_from_try;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    new_var_type.possibly_undefined_from_try = false;

    for atomic in existing_var_types {
        if let TAtomic::TVec { known_count, .. } = atomic {
            if let Some(known_count) = &known_count {
                if known_count == count {
                    did_remove_type = true;
                    continue;
                }
            } else if !atomic.is_falsy() {
                did_remove_type = true;
            }
        } else if let TAtomic::TDict { .. } = atomic {
            if !atomic.is_falsy() {
                did_remove_type = true;
            }
        }

        acceptable_types.push(atomic);
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        &existing_var_type,
        statements_analyzer,
        tast_info,
        assertion,
        negated,
        suppressed_issues,
        failed_reconciliation,
        new_var_type,
    )
}

fn reconcile_not_in_array(
    codebase: &CodebaseInfo,
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    negated: bool,
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
    suppressed_issues: &FxHashMap<String, usize>,
    typed_value: &TUnion,
) -> TUnion {
    let intersection = intersect_union_types(typed_value, existing_var_type, codebase);

    if let Some(_) = intersection {
        return existing_var_type.clone();
    }

    if let Some(key) = key {
        if let Some(pos) = pos {
            trigger_issue_for_impossible(
                tast_info,
                statements_analyzer,
                &existing_var_type.get_id(Some(&statements_analyzer.get_codebase().interner)),
                &key,
                assertion,
                true,
                negated,
                pos,
                suppressed_issues,
            );
        }

        *failed_reconciliation = ReconciliationStatus::Empty;
    }

    get_mixed_any()
}

fn reconcile_no_array_key(existing_var_type: &TUnion, key_name: &DictKey) -> TUnion {
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_type.types.iter_mut() {
        if let TAtomic::TDict { known_items, .. } = atomic {
            let mut all_known_items_removed = false;
            if let Some(known_items_inner) = known_items {
                if let Some(known_item) = known_items_inner.remove(key_name) {
                    if !known_item.0 {
                        // impossible to not have this key
                        // todo emit issue
                    }

                    if known_items_inner.len() == 0 {
                        all_known_items_removed = true;
                    }
                } else {
                    // todo emit issue
                }
            } else {
                // do nothing
            }

            if all_known_items_removed {
                *known_items = None;
            }
        }
    }

    existing_var_type
}
