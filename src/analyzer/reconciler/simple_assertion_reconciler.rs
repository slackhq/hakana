use std::{collections::BTreeMap, sync::Arc};

use super::{simple_negated_assertion_reconciler::subtract_null, trigger_issue_for_impossible};
use crate::{
    function_analysis_data::FunctionAnalysisData, statements_analyzer::StatementsAnalyzer,
};
use hakana_code_info::{
    assertion::Assertion,
    codebase_info::CodebaseInfo,
    functionlike_identifier::FunctionLikeIdentifier,
    t_atomic::{DictKey, TAtomic, TDict, TVec},
    t_union::TUnion,
};
use hakana_code_info::{
    ttype::{
        comparison::union_type_comparator, get_mixed_any, get_mixed_maybe_from_loop, get_nothing,
        intersect_union_types_simple, wrap_atomic,
    },
    var_name::VarName,
};
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;

// This performs type intersections and more general reconciliations
pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&VarName>,
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    negated: bool,
    inside_loop: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> Option<TUnion> {
    let assertion_type = assertion.get_type();

    if let Some(assertion_type) = assertion_type {
        match assertion_type {
            TAtomic::TMixedWithFlags(_, _, _, true) => {
                return Some(subtract_null(
                    assertion,
                    existing_var_type,
                    key,
                    !negated,
                    analysis_data,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    suppressed_issues,
                ));
            }

            _ => {}
        }
    }

    match assertion {
        Assertion::Truthy => Some(reconcile_truthy(
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
        Assertion::IsEqualIsset | Assertion::IsIsset => Some(reconcile_isset(
            assertion,
            existing_var_type,
            possibly_undefined,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
            inside_loop,
        )),
        Assertion::HasStringArrayAccess => Some(reconcile_array_access(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
            false,
        )),
        Assertion::HasIntOrStringArrayAccess => Some(reconcile_array_access(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
            true,
        )),
        Assertion::ArrayKeyExists => {
            let mut existing_var_type = existing_var_type.clone();
            if existing_var_type.is_nothing() {
                existing_var_type = get_mixed_maybe_from_loop(inside_loop);
            }
            Some(existing_var_type)
        }
        Assertion::InArray(typed_value) => Some(reconcile_in_array(
            codebase,
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
        Assertion::HasArrayKey(key_name) => Some(reconcile_has_array_key(
            assertion,
            existing_var_type,
            key,
            key_name,
            negated,
            possibly_undefined,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
        )),
        Assertion::HasNonnullEntryForKey(key_name) => Some(reconcile_has_nonnull_entry_for_key(
            assertion,
            existing_var_type,
            key,
            key_name,
            negated,
            possibly_undefined,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
        )),
        Assertion::NonEmptyCountable(_) => Some(reconcile_non_empty_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
            false,
        )),
        Assertion::HasExactCount(count) => Some(reconcile_exactly_countable(
            assertion,
            existing_var_type,
            key,
            negated,
            analysis_data,
            statements_analyzer,
            pos,
            calling_functionlike_id,
            suppressed_issues,
            false,
            count,
        )),
        _ => None,
    }
}

fn reconcile_truthy(
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
        if atomic.is_falsy() {
            did_remove_type = true;
        } else if !atomic.is_truthy() || new_var_type.possibly_undefined_from_try {
            did_remove_type = true;

            match atomic {
                TAtomic::TGenericParam { ref as_type, .. } => {
                    if !as_type.is_mixed() {
                        let atomic = atomic.replace_template_extends(reconcile_truthy(
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
                    } else {
                        acceptable_types.push(atomic);
                    }
                }
                TAtomic::TTypeVariable { .. } => {
                    did_remove_type = true;
                    acceptable_types.push(atomic);
                }
                TAtomic::TBool { .. } => {
                    acceptable_types.push(TAtomic::TTrue);
                }
                TAtomic::TVec(TVec { .. }) => {
                    acceptable_types.push(atomic.get_non_empty_vec(None));
                }
                TAtomic::TDict(TDict { .. }) => {
                    acceptable_types.push(atomic.clone().make_non_empty_dict());
                }
                TAtomic::TMixed => {
                    acceptable_types.push(TAtomic::TMixedWithFlags(false, true, false, false));
                }
                TAtomic::TMixedWithFlags(is_any, false, false, _) => {
                    acceptable_types.push(TAtomic::TMixedWithFlags(is_any, true, false, false));
                }
                TAtomic::TMixedFromLoopIsset => {
                    acceptable_types.push(TAtomic::TMixedWithFlags(false, true, false, true));
                }
                TAtomic::TString => {
                    acceptable_types.push(TAtomic::TStringWithFlags(true, false, false));
                }
                TAtomic::TStringWithFlags(_, _, is_nonspecific_literal) => {
                    acceptable_types.push(TAtomic::TStringWithFlags(
                        true,
                        false,
                        is_nonspecific_literal,
                    ));
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

fn reconcile_isset(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
    inside_loop: bool,
) -> TUnion {
    let mut did_remove_type = possibly_undefined || existing_var_type.possibly_undefined_from_try;

    if possibly_undefined {
        did_remove_type = true;
    }

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TNull { .. } = atomic {
            did_remove_type = true;
        } else if let TAtomic::TMixed = atomic {
            acceptable_types.push(TAtomic::TMixedWithFlags(false, false, false, true));
            did_remove_type = true;
        } else if let TAtomic::TMixedWithFlags(is_any, false, _, false) = atomic {
            acceptable_types.push(TAtomic::TMixedWithFlags(is_any, false, false, true));
            did_remove_type = true;
        } else {
            acceptable_types.push(atomic);
        }
    }

    if !did_remove_type || acceptable_types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                let old_var_type_string =
                    existing_var_type.get_id(Some(statements_analyzer.interner));

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

        if acceptable_types.is_empty() {
            return get_nothing();
        }
    }

    new_var_type.possibly_undefined_from_try = false;
    new_var_type.types = acceptable_types;

    if new_var_type.is_nothing() {
        new_var_type.remove_type(&TAtomic::TNothing);
        new_var_type.types.push(if !inside_loop {
            TAtomic::TMixed
        } else {
            TAtomic::TMixedFromLoopIsset
        });
    }

    new_var_type
}

fn reconcile_non_empty_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
    recursive_check: bool,
) -> TUnion {
    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    for atomic in existing_var_types {
        if let TAtomic::TVec(TVec {
            non_empty,
            type_param,
            ..
        }) = &atomic
        {
            if !non_empty {
                if !type_param.is_nothing() {
                    let non_empty_vec = atomic.get_non_empty_vec(None);

                    acceptable_types.push(non_empty_vec);
                } else {
                    acceptable_types.push(atomic);
                }

                did_remove_type = true;
            } else {
                acceptable_types.push(atomic);
            }
        } else if let TAtomic::TKeyset {
            non_empty,
            type_param,
            ..
        } = &atomic
        {
            if !non_empty {
                if !type_param.is_nothing() {
                    let non_empty_keyset = TAtomic::TKeyset {
                        type_param: type_param.clone(),
                        non_empty: true,
                    };

                    acceptable_types.push(non_empty_keyset);
                } else {
                    acceptable_types.push(atomic);
                }

                did_remove_type = true;
            } else {
                acceptable_types.push(atomic);
            }
        } else if let TAtomic::TDict(TDict {
            non_empty,
            params,
            known_items,
            ..
        }) = &atomic
        {
            if !non_empty {
                did_remove_type = true;
                if !params.is_none() {
                    let non_empty_dict = atomic.clone().make_non_empty_dict();

                    acceptable_types.push(non_empty_dict);
                } else {
                    acceptable_types.push(atomic);
                }
            } else {
                if let Some(known_items) = known_items {
                    for (u, _) in known_items.values() {
                        if *u {
                            did_remove_type = true;
                        }
                    }
                }

                acceptable_types.push(atomic);
            }
        } else {
            did_remove_type = true;
            acceptable_types.push(atomic);
        }
    }

    if !did_remove_type || acceptable_types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                if !recursive_check {
                    let old_var_type_string =
                        existing_var_type.get_id(Some(statements_analyzer.interner));

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

        if acceptable_types.is_empty() {
            return get_nothing();
        }
    }

    new_var_type.types = acceptable_types;

    new_var_type
}

fn reconcile_exactly_countable(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
    recursive_check: bool,
    count: &usize,
) -> TUnion {
    let old_var_type_string = existing_var_type.get_id(Some(statements_analyzer.interner));

    let mut did_remove_type = false;

    let existing_var_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    for atomic in existing_var_types {
        if let TAtomic::TVec(TVec {
            non_empty,
            known_count,
            type_param,
            ..
        }) = atomic
        {
            let min_under_count = if let Some(known_count) = known_count {
                known_count < count
            } else {
                false
            };
            if !non_empty || min_under_count {
                if type_param.is_nothing() {
                    existing_var_type.remove_type(atomic);
                } else {
                    let non_empty_vec = atomic.get_non_empty_vec(Some(*count));

                    existing_var_type.types.push(non_empty_vec);
                }

                did_remove_type = true;
            }
        } else if let TAtomic::TDict(TDict {
            non_empty,
            params,
            known_items,
            ..
        }) = atomic
        {
            if !non_empty {
                if params.is_none() {
                    existing_var_type.remove_type(atomic);
                } else {
                    let non_empty_dict = atomic.clone().make_non_empty_dict();

                    existing_var_type.types.push(non_empty_dict);
                }

                did_remove_type = true;
            } else if let Some(known_items) = known_items {
                for (u, _) in known_items.values() {
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
    }

    existing_var_type
}

fn reconcile_array_access(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    negated: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
    allow_int_key: bool,
) -> TUnion {
    let mut new_var_type = existing_var_type.clone();

    if new_var_type.is_mixed() || new_var_type.has_template() {
        // maybe return something more specific in the future
        // dict<arraykey, mixed>|keyset<arraykey>
        return new_var_type;
    }

    new_var_type.types.retain(|atomic| {
        (allow_int_key
            && atomic.is_array_accessible_with_int_or_string_key(statements_analyzer.interner))
            || (!allow_int_key
                && atomic.is_array_accessible_with_string_key(statements_analyzer.interner))
    });

    if new_var_type.types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                let old_var_type_string =
                    existing_var_type.get_id(Some(statements_analyzer.interner));

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

        if new_var_type.types.is_empty() {
            return get_nothing();
        }
    }

    new_var_type
}

fn reconcile_in_array(
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
    let intersection = intersect_union_types_simple(typed_value, existing_var_type, codebase);

    if let Some(intersection) = intersection {
        return intersection;
    }

    if let Some(key) = key {
        if let Some(pos) = pos {
            trigger_issue_for_impossible(
                analysis_data,
                statements_analyzer,
                &existing_var_type.get_id(Some(statements_analyzer.interner)),
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

fn reconcile_has_array_key(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    key_name: &DictKey,
    negated: bool,
    possibly_undefined: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = possibly_undefined;

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
                    if let Some(known_item) = known_items.get_mut(key_name) {
                        if known_item.0 {
                            *known_item = (false, known_item.1.clone());
                            did_remove_type = true;
                        }
                    } else if let Some((_, value_param)) = params {
                        known_items
                            .insert(key_name.clone(), (false, Arc::new((**value_param).clone())));
                        did_remove_type = true;
                    } else {
                        did_remove_type = true;
                        continue;
                    }
                } else if let Some((key_param, value_param)) = params {
                    did_remove_type = true;

                    if union_type_comparator::can_expression_types_be_identical(
                        statements_analyzer.codebase,
                        statements_analyzer.get_file_path(),
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
                        *known_items = Some(BTreeMap::from([(
                            key_name.clone(),
                            (false, Arc::new((**value_param).clone())),
                        )]));
                    } else {
                        continue;
                    }
                } else {
                    did_remove_type = true;
                    continue;
                }

                acceptable_types.push(atomic);
            }
            TAtomic::TVec(TVec {
                ref mut known_items,
                ref mut type_param,
                ..
            }) => {
                if let DictKey::Int(i) = key_name {
                    if let Some(known_items) = known_items {
                        if let Some(known_item) = known_items.get_mut(&(*i as usize)) {
                            if known_item.0 {
                                *known_item = (false, known_item.1.clone());
                                did_remove_type = true;
                            }
                        } else if !type_param.is_nothing() {
                            known_items.insert(*i as usize, (false, (**type_param).clone()));
                            did_remove_type = true;
                        } else {
                            did_remove_type = true;
                            continue;
                        }
                    } else if !type_param.is_nothing() {
                        *known_items = Some(BTreeMap::from([(
                            *i as usize,
                            (false, (**type_param).clone()),
                        )]));
                        did_remove_type = true;
                    }

                    acceptable_types.push(atomic);
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TGenericParam { ref as_type, .. }
            | TAtomic::TClassTypeConstant { ref as_type, .. } => {
                if as_type.is_mixed() {
                    acceptable_types.push(atomic);
                } else {
                    let atomic = atomic.replace_template_extends(reconcile_has_array_key(
                        assertion,
                        as_type,
                        None,
                        key_name,
                        negated,
                        possibly_undefined,
                        analysis_data,
                        statements_analyzer,
                        None,
                        calling_functionlike_id,
                        suppressed_issues,
                    ));

                    acceptable_types.push(atomic);
                }
                did_remove_type = true;
            }
            TAtomic::TTypeVariable { .. } => {
                did_remove_type = true;
                acceptable_types.push(atomic);
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

    if !did_remove_type || acceptable_types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                let old_var_type_string =
                    existing_var_type.get_id(Some(statements_analyzer.interner));

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

        if acceptable_types.is_empty() {
            return get_nothing();
        }
    }

    new_var_type.types = acceptable_types;

    new_var_type
}

fn reconcile_has_nonnull_entry_for_key(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&VarName>,
    key_name: &DictKey,
    negated: bool,
    possibly_undefined: bool,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let mut did_remove_type = possibly_undefined;

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
                    if let Some(known_item) = known_items.get_mut(key_name) {
                        let nonnull = subtract_null(
                            assertion,
                            &known_item.1,
                            None,
                            negated,
                            analysis_data,
                            statements_analyzer,
                            None,
                            calling_functionlike_id,
                            suppressed_issues,
                        );

                        if known_item.0 {
                            *known_item = (false, Arc::new(nonnull));
                            did_remove_type = true;
                        } else if *known_item.1 != nonnull {
                            known_item.1 = Arc::new(nonnull);
                            did_remove_type = true;
                        }
                    } else if let Some((_, value_param)) = params {
                        let nonnull = subtract_null(
                            assertion,
                            value_param,
                            None,
                            negated,
                            analysis_data,
                            statements_analyzer,
                            None,
                            calling_functionlike_id,
                            suppressed_issues,
                        );
                        known_items.insert(key_name.clone(), (false, Arc::new(nonnull)));
                        did_remove_type = true;
                    } else {
                        did_remove_type = true;
                        continue;
                    }
                } else if let Some((key_param, value_param)) = params {
                    did_remove_type = true;

                    if union_type_comparator::can_expression_types_be_identical(
                        statements_analyzer.codebase,
                        statements_analyzer.get_file_path(),
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
                        let nonnull = subtract_null(
                            assertion,
                            value_param,
                            None,
                            negated,
                            analysis_data,
                            statements_analyzer,
                            None,
                            calling_functionlike_id,
                            suppressed_issues,
                        );
                        *known_items = Some(BTreeMap::from([(
                            key_name.clone(),
                            (false, Arc::new(nonnull)),
                        )]));
                    } else {
                        continue;
                    }
                } else {
                    did_remove_type = true;
                    continue;
                }

                acceptable_types.push(atomic);
            }
            TAtomic::TVec(TVec {
                ref mut known_items,
                ref mut type_param,
                ..
            }) => {
                if let DictKey::Int(i) = key_name {
                    if let Some(known_items) = known_items {
                        if let Some(known_item) = known_items.get_mut(&(*i as usize)) {
                            let nonnull = subtract_null(
                                assertion,
                                &known_item.1,
                                None,
                                negated,
                                analysis_data,
                                statements_analyzer,
                                None,
                                calling_functionlike_id,
                                suppressed_issues,
                            );

                            if known_item.0 {
                                *known_item = (false, nonnull);
                                did_remove_type = true;
                            } else if known_item.1 != nonnull {
                                known_item.1 = nonnull;
                                did_remove_type = true;
                            }
                        } else if !type_param.is_nothing() {
                            let nonnull = subtract_null(
                                assertion,
                                type_param,
                                None,
                                negated,
                                analysis_data,
                                statements_analyzer,
                                None,
                                calling_functionlike_id,
                                suppressed_issues,
                            );
                            known_items.insert(*i as usize, (false, nonnull));
                            did_remove_type = true;
                        } else {
                            did_remove_type = true;
                            continue;
                        }
                    } else if !type_param.is_nothing() {
                        let nonnull = subtract_null(
                            assertion,
                            type_param,
                            None,
                            negated,
                            analysis_data,
                            statements_analyzer,
                            None,
                            calling_functionlike_id,
                            suppressed_issues,
                        );
                        *known_items = Some(BTreeMap::from([(*i as usize, (false, nonnull))]));
                        did_remove_type = true;
                    }

                    acceptable_types.push(atomic);
                } else {
                    did_remove_type = true;
                }
            }
            TAtomic::TGenericParam { ref as_type, .. }
            | TAtomic::TClassTypeConstant { ref as_type, .. } => {
                if as_type.is_mixed() {
                    acceptable_types.push(atomic);
                } else {
                    let atomic =
                        atomic.replace_template_extends(reconcile_has_nonnull_entry_for_key(
                            assertion,
                            as_type,
                            None,
                            key_name,
                            negated,
                            possibly_undefined,
                            analysis_data,
                            statements_analyzer,
                            None,
                            calling_functionlike_id,
                            suppressed_issues,
                        ));

                    acceptable_types.push(atomic);
                }
                did_remove_type = true;
            }
            TAtomic::TTypeVariable { .. } => {
                did_remove_type = true;
                acceptable_types.push(atomic);
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
            TAtomic::TString | TAtomic::TStringWithFlags(..) => {
                if let DictKey::Int(_) = key_name {
                    acceptable_types.push(atomic);
                }
                did_remove_type = true;
            }
            _ => {
                did_remove_type = true;
            }
        }
    }

    if !did_remove_type || acceptable_types.is_empty() {
        // every type was removed, this is an impossible assertion
        if let Some(key) = key {
            if let Some(pos) = pos {
                let old_var_type_string =
                    existing_var_type.get_id(Some(statements_analyzer.interner));

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

        if acceptable_types.is_empty() {
            return get_nothing();
        }
    }

    new_var_type.types = acceptable_types;

    new_var_type
}

pub(crate) fn get_acceptable_type(
    acceptable_types: Vec<TAtomic>,
    did_remove_type: bool,
    key: Option<&VarName>,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    existing_var_type: &TUnion,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    assertion: &Assertion,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
    mut new_var_type: TUnion,
) -> TUnion {
    if acceptable_types.is_empty() || !did_remove_type {
        if let Some(key) = key {
            if let Some(pos) = pos {
                let old_var_type_string =
                    existing_var_type.get_id(Some(statements_analyzer.interner));

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

    if acceptable_types.is_empty() {
        return get_nothing();
    }

    new_var_type.types = acceptable_types;
    new_var_type
}
