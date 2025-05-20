use super::simple_assertion_reconciler::{get_acceptable_type, intersect_null};
use crate::{
    function_analysis_data::FunctionAnalysisData, reconciler::trigger_issue_for_impossible,
    statements_analyzer::StatementsAnalyzer,
};
use hakana_code_info::ttype::{
    comparison::union_type_comparator, get_mixed_any, get_nothing, get_null, intersect_union_types,
    wrap_atomic,
};
use hakana_code_info::var_name::VarName;
use hakana_code_info::{
    assertion::Assertion,
    codebase_info::CodebaseInfo,
    functionlike_identifier::FunctionLikeIdentifier,
    t_atomic::{DictKey, TAtomic, TDict},
    t_union::TUnion,
};
use hakana_str::StrId;
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;

// This performs type subtractions and more general reconciliations
pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&VarName>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    assertion.has_equality(),
                    suppressed_issues,
                ));
            }
            TAtomic::TDict(TDict {
                known_items: None,
                params: Some(params),
                ..
            }) => {
                if params.0.is_placeholder() && params.1.is_placeholder() {
                    return Some(subtract_dict(
                        assertion,
                        existing_var_type,
                        key,
                        negated,
                        analysis_data,
                        statements_analyzer,
                        pos,
                        calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                ));
            }
            TAtomic::TMixedWithFlags(_, _, _, true) => {
                return Some(intersect_null(
                    assertion,
                    existing_var_type,
                    key,
                    negated,
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                ));
            }
            TAtomic::TFalse { .. } => {
                return Some(subtract_false(
                    assertion,
                    existing_var_type,
                    key,
                    negated,
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
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
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
        )),
        Assertion::IsNotIsset => Some(reconcile_not_isset(
            existing_var_type,
            possibly_undefined,
            key,
            pos,
            suppressed_issues,
        )),
        Assertion::ArrayKeyDoesNotExist => {
            return Some(get_nothing());
        }
        Assertion::DoesNotHaveArrayKey(key_name) => Some(reconcile_no_array_key(
            assertion,
            existing_var_type,
            key,
            pos,
            calling_functionlike_id,
            key_name,
            negated,
            analysis_data,
            statements_analyzer,
            suppressed_issues,
        )),
        Assertion::DoesNotHaveNonnullEntryForKey(key_name) => Some(
            reconcile_no_nonnull_entry_for_key(existing_var_type, key_name),
        ),
        Assertion::NotInArray(typed_value) => Some(reconcile_not_in_array(
            statements_analyzer.codebase,
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
            typed_value,
        )),
        Assertion::EmptyCountable => Some(reconcile_empty_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
        )),
        Assertion::DoesNotHaveExactCount(count) => Some(reconcile_not_exactly_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
            count,
        )),
        _ => None,
    };
}

fn subtract_object(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = &atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let new_atomic = atomic.replace_template_extends(subtract_object(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

// TODO: in the future subtract from Container and KeyedContainer
fn subtract_vec(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = &atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let new_atomic = atomic.replace_template_extends(subtract_vec(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn subtract_keyset(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = &atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let new_atomic = atomic.replace_template_extends(subtract_keyset(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn subtract_dict(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = &atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let new_atomic = atomic.replace_template_extends(subtract_dict(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
                    is_equality,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);
            } else {
                acceptable_types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TDict(TDict { .. }) = atomic {
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn subtract_string(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = &atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let new_atomic = atomic.replace_template_extends(subtract_string(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
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
            if is_equality {
                if let TAtomic::TTypeAlias { .. } | TAtomic::TEnum { .. } = &atomic {
                    did_remove_type = true;
                }
            }

            acceptable_types.push(atomic);
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn subtract_int(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = &atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let new_atomic = atomic.replace_template_extends(subtract_int(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
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
                as_type: Some(_), ..
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn subtract_float(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = &atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let new_atomic = atomic.replace_template_extends(subtract_float(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn subtract_num(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string = existing_var_type.get_id(Some(&statements_analyzer.interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let atomic = atomic.replace_template_extends(subtract_num(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
                    is_equality,
                    suppressed_issues,
                ));

                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(atomic);
            }

            did_remove_type = true;
        } else if let TAtomic::TScalar = atomic {
            if !is_equality {
                existing_var_type.remove_type(atomic);
                existing_var_type.types.push(TAtomic::TString);
                existing_var_type.types.push(TAtomic::TBool);
            }

            did_remove_type = true;
        } else if let TAtomic::TArraykey { .. } = atomic {
            if !is_equality {
                existing_var_type.remove_type(atomic);
                existing_var_type.types.push(TAtomic::TString);
            }

            did_remove_type = true;
        } else if let TAtomic::TFloat { .. } | TAtomic::TInt { .. } | TAtomic::TNum { .. } = atomic
        {
            did_remove_type = true;

            if !is_equality {
                existing_var_type.remove_type(atomic);
            }
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    analysis_data,
                    statements_analyzer,
                    &old_var_type_string,
                    key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                );
            }
        }
    }

    if existing_var_type.types.is_empty() {
        return get_nothing();
    }

    existing_var_type
}

fn subtract_arraykey(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string = existing_var_type.get_id(Some(&statements_analyzer.interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let atomic = atomic.replace_template_extends(subtract_arraykey(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
                    is_equality,
                    suppressed_issues,
                ));

                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(atomic);
            } else {
                did_remove_type = true;
            }
        } else if let TAtomic::TScalar = atomic {
            if !is_equality {
                existing_var_type.remove_type(atomic);
                existing_var_type.types.push(TAtomic::TFloat);
                existing_var_type.types.push(TAtomic::TBool);
            }

            did_remove_type = true;
        } else if let TAtomic::TNum = atomic {
            if !is_equality {
                existing_var_type.remove_type(atomic);
                existing_var_type.types.push(TAtomic::TFloat);
            }

            did_remove_type = true;
        } else if atomic.is_int()
            || atomic.is_string()
            || matches!(atomic, TAtomic::TArraykey { .. })
        {
            did_remove_type = true;

            if !is_equality {
                existing_var_type.remove_type(atomic);
            }
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    analysis_data,
                    statements_analyzer,
                    &old_var_type_string,
                    key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                );
            }
        }
    }

    if existing_var_type.types.is_empty() {
        return get_nothing();
    }

    existing_var_type
}

fn subtract_bool(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string = existing_var_type.get_id(Some(&statements_analyzer.interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let atomic = atomic.replace_template_extends(subtract_bool(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
                    is_equality,
                    suppressed_issues,
                ));

                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(atomic);
            } else {
                did_remove_type = true;
            }
        } else if let TAtomic::TScalar = atomic {
            if !is_equality {
                existing_var_type.remove_type(atomic);
                existing_var_type.types.push(TAtomic::TString);
                existing_var_type.types.push(TAtomic::TInt);
                existing_var_type.types.push(TAtomic::TFloat);
            }

            did_remove_type = true;
        } else if atomic.is_bool() {
            did_remove_type = true;

            if !is_equality {
                existing_var_type.remove_type(atomic);
            }
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    analysis_data,
                    statements_analyzer,
                    &old_var_type_string,
                    key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                );
            }
        }
    }

    if existing_var_type.types.is_empty() {
        return get_nothing();
    }

    existing_var_type
}

pub(crate) fn subtract_null(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        match atomic {
            TAtomic::TGenericParam { ref as_type, .. }
            | TAtomic::TClassTypeConstant { ref as_type, .. } => {
                let new_atomic = atomic.replace_template_extends(subtract_null(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
                    suppressed_issues,
                ));

                acceptable_types.push(new_atomic);

                did_remove_type = true;
            }
            TAtomic::TTypeVariable { .. } => {
                did_remove_type = true;
                acceptable_types.push(atomic);
            }
            TAtomic::TMixed => {
                did_remove_type = true;
                acceptable_types.push(TAtomic::TMixedWithFlags(false, false, false, true));
            }
            TAtomic::TMixedWithFlags(is_any, false, _, false) => {
                did_remove_type = true;
                acceptable_types.push(TAtomic::TMixedWithFlags(is_any, false, false, true));
            }
            TAtomic::TNull => {
                did_remove_type = true;
            }
            TAtomic::TNamedObject {
                name: StrId::XHP_CHILD,
                type_params: None,
                ..
            } => {
                did_remove_type = true;
                acceptable_types.push(atomic);
            }
            _ => {
                acceptable_types.push(atomic);
            }
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn subtract_false(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string = existing_var_type.get_id(Some(&statements_analyzer.interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = atomic
        {
            if !is_equality && !as_type.is_mixed() {
                let atomic = atomic.replace_template_extends(subtract_false(
                    assertion,
                    as_type,
                    None,
                    false,
                    analysis_data,
                    statements_analyzer,
                    None,
                    calling_functionlike_id,
                    is_equality,
                    suppressed_issues,
                ));

                existing_var_type.remove_type(&atomic);
                existing_var_type.types.push(atomic)
            } else {
                did_remove_type = true;
            }
        } else if let TAtomic::TBool = atomic {
            existing_var_type.remove_type(atomic);
            existing_var_type.types.push(TAtomic::TTrue);
            did_remove_type = true;
        } else if let TAtomic::TFalse { .. } = atomic {
            did_remove_type = true;

            existing_var_type.remove_type(atomic);
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    analysis_data,
                    statements_analyzer,
                    &old_var_type_string,
                    key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                );
            }
        }
    }

    if existing_var_type.types.is_empty() {
        return get_nothing();
    }

    existing_var_type
}

fn subtract_true(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    is_equality: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if existing_var_type.is_mixed() {
        return existing_var_type.clone();
    }

    let old_var_type_string = existing_var_type.get_id(Some(&statements_analyzer.interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        match atomic {
            TAtomic::TGenericParam { as_type, .. }
            | TAtomic::TClassTypeConstant { as_type, .. } => {
                if !is_equality && !as_type.is_mixed() {
                    let atomic = atomic.replace_template_extends(subtract_true(
                        assertion,
                        as_type,
                        None,
                        false,
                        analysis_data,
                        statements_analyzer,
                        None,
                        calling_functionlike_id,
                        is_equality,
                        suppressed_issues,
                    ));

                    existing_var_type.remove_type(&atomic);
                    existing_var_type.types.push(atomic);
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TTypeVariable { .. } => {
                did_remove_type = true;
            }
            TAtomic::TBool => {
                existing_var_type.remove_type(atomic);
                existing_var_type.types.push(TAtomic::TFalse);
                did_remove_type = true;
            }
            TAtomic::TTrue { .. } => {
                did_remove_type = true;

                existing_var_type.remove_type(atomic);
            }
            _ => (),
        }
    }

    if existing_var_type.types.is_empty() || !did_remove_type {
        if let Some(key) = key {
            if let Some(pos) = pos {
                trigger_issue_for_impossible(
                    analysis_data,
                    statements_analyzer,
                    &old_var_type_string,
                    key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                );
            }
        }
    }

    if existing_var_type.types.is_empty() {
        return get_nothing();
    }

    existing_var_type
}

fn reconcile_falsy(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = existing_var_type.possibly_undefined_from_try;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        // if any atomic in the union is either always falsy, we remove it.
        // If not always truthy, we mark the check as not redundant.
        if atomic.is_truthy() && !new_var_type.possibly_undefined_from_try {
            did_remove_type = true;
        } else if !atomic.is_falsy() {
            did_remove_type = true;

            match atomic {
                TAtomic::TGenericParam { ref as_type, .. } => {
                    if !as_type.is_mixed() {
                        let atomic = atomic.replace_template_extends(reconcile_falsy(
                            assertion,
                            as_type,
                            None,
                            false,
                            analysis_data,
                            statements_analyzer,
                            None,
                            calling_functionlike_id,
                            suppressed_issues,
                        ));

                        acceptable_types.push(atomic);
                    }
                }
                TAtomic::TTypeVariable { .. } => {
                    acceptable_types.push(atomic);
                }
                TAtomic::TBool { .. } => {
                    acceptable_types.push(TAtomic::TFalse);
                }
                TAtomic::TVec { .. } => {
                    let new_atomic = TAtomic::TVec {
                        type_param: Box::new(get_nothing()),
                        known_items: None,
                        non_empty: false,
                        known_count: None,
                    };
                    acceptable_types.push(new_atomic);
                }
                TAtomic::TDict(TDict { .. }) => {
                    let new_atomic = TAtomic::TDict(TDict {
                        params: None,
                        known_items: None,
                        non_empty: false,
                        shape_name: None,
                    });
                    acceptable_types.push(new_atomic);
                }
                TAtomic::TMixed => {
                    acceptable_types.push(TAtomic::TMixedWithFlags(false, false, true, false));
                }
                TAtomic::TMixedWithFlags(is_any, false, false, _) => {
                    acceptable_types.push(TAtomic::TMixedWithFlags(is_any, false, true, false));
                }
                TAtomic::TMixedFromLoopIsset => {
                    acceptable_types.push(TAtomic::TMixedWithFlags(false, false, true, false));
                }
                TAtomic::TString { .. } => {
                    let empty_string = TAtomic::TLiteralString {
                        value: "".to_string(),
                    };
                    let falsy_string = TAtomic::TLiteralString {
                        value: "0".to_string(),
                    };
                    acceptable_types.push(empty_string);
                    acceptable_types.push(falsy_string);
                }
                TAtomic::TInt { .. } => {
                    let zero = TAtomic::TLiteralInt { value: 0 };
                    acceptable_types.push(zero);
                }
                _ => {
                    acceptable_types.push(atomic);
                }
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn reconcile_not_isset(
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&VarName>,
    pos: Option<&Pos>,
    _suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    if possibly_undefined {
        return get_nothing();
    }

    if !existing_var_type.is_nullable() {
        if let Some(key) = key {
            if !key.contains('[')
                && (!existing_var_type.is_mixed() || existing_var_type.is_always_truthy())
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
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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

            if atomic.is_truthy() {
                // don't keep
            } else {
                let new_atomic = TAtomic::TVec {
                    type_param: Box::new(get_nothing()),
                    known_items: None,
                    non_empty: false,
                    known_count: None,
                };
                acceptable_types.push(new_atomic);
            }
        } else if let TAtomic::TDict(TDict { .. }) = atomic {
            did_remove_type = true;

            if atomic.is_truthy() {
                // don't keep
            } else {
                let new_atomic = TAtomic::TDict(TDict {
                    params: None,
                    known_items: None,
                    non_empty: false,
                    shape_name: None,
                });
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn reconcile_not_exactly_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
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
        } else if let TAtomic::TDict(TDict { .. }) = atomic {
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
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn reconcile_not_in_array(
    codebase: &CodebaseInfo,
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
    typed_value: &TUnion,
) -> TUnion {
    let intersection = intersect_union_types(typed_value, existing_var_type, codebase);

    if intersection.is_some() {
        return existing_var_type.clone();
    }

    if let Some(key) = key {
        if let Some(pos) = pos {
            trigger_issue_for_impossible(
                analysis_data,
                statements_analyzer,
                &existing_var_type.get_id(Some(&statements_analyzer.interner)),
                key,
                assertion,
                true,
                negated,
                pos,
                calling_functionlike_id,
                suppressed_issues,
            );
        }
    }

    get_mixed_any()
}

fn reconcile_no_array_key(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    key_name: &DictKey,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = existing_var_type.possibly_undefined_from_try;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for mut atomic in existing_var_types {
        match atomic {
            TAtomic::TDict(TDict {
                ref mut known_items,
                ref mut params,
                ..
            }) => {
                if let Some(known_items) = known_items {
                    if let Some(known_item) = known_items.get(key_name) {
                        if known_item.0 {
                            known_items.remove(key_name);
                            did_remove_type = true;
                        }
                    } else if let Some((key_param, _)) = params {
                        if union_type_comparator::can_expression_types_be_identical(
                            statements_analyzer.codebase,
                            &wrap_atomic(match key_name {
                                DictKey::Int(_) => TAtomic::TInt,
                                DictKey::String(_) => TAtomic::TString,
                                DictKey::Enum(a, b) => TAtomic::TEnumLiteralCase {
                                    enum_name: *a,
                                    member_name: *b,
                                    as_type: None,
                                    underlying_type: None,
                                },
                            }),
                            key_param,
                            false,
                        ) {
                            did_remove_type = true;
                        }
                    }
                } else if let Some((key_param, _)) = params {
                    if union_type_comparator::can_expression_types_be_identical(
                        statements_analyzer.codebase,
                        &wrap_atomic(match key_name {
                            DictKey::Int(_) => TAtomic::TInt,
                            DictKey::String(_) => TAtomic::TString,
                            DictKey::Enum(a, b) => TAtomic::TEnumLiteralCase {
                                enum_name: *a,
                                member_name: *b,
                                as_type: None,
                                underlying_type: None,
                            },
                        }),
                        key_param,
                        false,
                    ) {
                        did_remove_type = true;
                    }
                }

                acceptable_types.push(atomic);
            }
            TAtomic::TVec {
                ref mut known_items,
                ref mut type_param,
                ..
            } => {
                if let DictKey::Int(i) = key_name {
                    if let Some(known_items) = known_items {
                        if let Some(known_item) = known_items.get(&(*i as usize)) {
                            if known_item.0 {
                                known_items.remove(&(*i as usize));
                                did_remove_type = true;
                            }
                        } else if !type_param.is_nothing() {
                            did_remove_type = true;
                        }
                    } else if !type_param.is_nothing() {
                        did_remove_type = true;
                    }
                }

                acceptable_types.push(atomic);
            }
            TAtomic::TGenericParam { ref as_type, .. } => {
                if as_type.is_mixed() {
                    acceptable_types.push(atomic);
                } else {
                    let atomic = atomic.replace_template_extends(reconcile_no_array_key(
                        assertion,
                        as_type,
                        None,
                        None,
                        calling_functionlike_id,
                        key_name,
                        negated,
                        analysis_data,
                        statements_analyzer,
                        suppressed_issues,
                    ));

                    acceptable_types.push(atomic);
                }
                did_remove_type = true;
            }

            TAtomic::TMixed
            | TAtomic::TMixedWithFlags(..)
            | TAtomic::TMixedFromLoopIsset
            | TAtomic::TTypeAlias { .. } => {
                did_remove_type = true;
                acceptable_types.push(atomic);
            }
            TAtomic::TNamedObject { .. } => {
                did_remove_type = true;
                acceptable_types.push(atomic);
            }
            TAtomic::TKeyset { .. } => {
                did_remove_type = true;
                acceptable_types.push(atomic);
            }
            _ => {
                did_remove_type = true;
            }
        }
    }

    get_acceptable_type(
        acceptable_types,
        did_remove_type,
        key,
        pos,
        calling_functionlike_id,
        existing_var_type,
        statements_analyzer,
        analysis_data,
        assertion,
        negated,
        suppressed_issues,
        new_var_type,
    )
}

fn reconcile_no_nonnull_entry_for_key(existing_var_type: &TUnion, key_name: &DictKey) -> TUnion {
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_type.types.iter_mut() {
        if let TAtomic::TDict(TDict { known_items, .. }) = atomic {
            let mut all_known_items_removed = false;
            if let Some(known_items_inner) = known_items {
                if let Some(known_item) = known_items_inner.remove(key_name) {
                    if !known_item.0 {
                        // impossible to not have this key
                        // todo emit issue
                    }

                    if known_items_inner.is_empty() {
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
