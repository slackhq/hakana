use std::{collections::BTreeMap, sync::Arc};

use super::{
    negated_assertion_reconciler, simple_assertion_reconciler, trigger_issue_for_impossible,
};
use crate::{
    function_analysis_data::FunctionAnalysisData, intersect_simple,
    statements_analyzer::StatementsAnalyzer,
};
use hakana_code_info::{
    assertion::Assertion,
    code_location::FilePath,
    codebase_info::CodebaseInfo,
    functionlike_identifier::FunctionLikeIdentifier,
    t_atomic::{TAtomic, TDict, TVec},
    t_union::TUnion,
    ttype::{
        get_bool, get_false, get_float, get_null, get_object, get_scalar, get_true,
        template::TemplateBound,
    },
};
use hakana_code_info::{
    ttype::{
        comparison::{
            atomic_type_comparator::{self, expand_constant_value},
            type_comparison_result::TypeComparisonResult,
        },
        get_arraykey, get_int, get_mixed_any, get_mixed_maybe_from_loop, get_nothing, get_string,
        type_combiner,
        type_expander::{self, TypeExpansionOptions},
        wrap_atomic,
    },
    var_name::VarName,
};
use hakana_str::StrId;
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;

pub fn reconcile(
    assertion: &Assertion,
    existing_var_type: Option<&TUnion>,
    possibly_undefined: bool,
    key: Option<&VarName>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    inside_loop: bool,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    can_report_issues: bool,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let codebase = statements_analyzer.codebase;

    let is_negation = assertion.has_negation();

    let existing_var_type = if let Some(existing_var_type) = existing_var_type {
        existing_var_type
    } else {
        return get_missing_type(assertion, inside_loop);
    };

    let old_var_type_string = existing_var_type.get_id(Some(statements_analyzer.interner));

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
        existing_var_type,
        possibly_undefined,
        key,
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
        match assertion_type {
            TAtomic::TScalar => {
                return intersect_simple!(
                    TAtomic::TLiteralClassname { .. }
                        | TAtomic::TLiteralInt { .. }
                        | TAtomic::TLiteralString { .. }
                        | TAtomic::TArraykey { .. }
                        | TAtomic::TBool
                        | TAtomic::TClassname { .. }
                        | TAtomic::TTypename { .. }
                        | TAtomic::TFalse
                        | TAtomic::TFloat
                        | TAtomic::TInt
                        | TAtomic::TStringWithFlags(..)
                        | TAtomic::TNum
                        | TAtomic::TString
                        | TAtomic::TTrue,
                    TAtomic::TMixed | TAtomic::TMixedWithFlags(..) | TAtomic::TMixedFromLoopIsset,
                    get_scalar(),
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
                );
            }
            TAtomic::TBool { .. } => {
                return intersect_simple!(
                    TAtomic::TBool { .. } | TAtomic::TFalse | TAtomic::TTrue,
                    TAtomic::TMixed
                        | TAtomic::TMixedWithFlags(..)
                        | TAtomic::TScalar
                        | TAtomic::TMixedFromLoopIsset,
                    get_bool(),
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
                );
            }
            TAtomic::TFalse { .. } => {
                return intersect_simple!(
                    TAtomic::TFalse { .. },
                    TAtomic::TMixed
                        | TAtomic::TMixedWithFlags(_, false, _, _)
                        | TAtomic::TScalar
                        | TAtomic::TBool
                        | TAtomic::TMixedFromLoopIsset,
                    get_false(),
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
                );
            }
            TAtomic::TTrue { .. } => {
                return intersect_simple!(
                    TAtomic::TTrue { .. },
                    TAtomic::TMixed
                        | TAtomic::TMixedWithFlags(_, _, false, _)
                        | TAtomic::TScalar
                        | TAtomic::TBool
                        | TAtomic::TMixedFromLoopIsset,
                    get_true(),
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
                );
            }
            TAtomic::TFloat { .. } => {
                return intersect_simple!(
                    TAtomic::TFloat { .. },
                    TAtomic::TMixed
                        | TAtomic::TMixedWithFlags(..)
                        | TAtomic::TScalar
                        | TAtomic::TNum
                        | TAtomic::TMixedFromLoopIsset,
                    get_float(),
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
                );
            }
            TAtomic::TMixed | TAtomic::TMixedFromLoopIsset => {
                if existing_var_type.is_mixed() {
                    return existing_var_type.clone();
                }
            }
            _ => {}
        }

        let mut did_remove_type = false;

        let mut refined_type = refine_atomic_with_union(
            statements_analyzer,
            analysis_data,
            assertion_type,
            existing_var_type,
            pos,
            &mut did_remove_type,
        );

        if let Some(key) = key {
            if let Some(pos) = pos {
                if can_report_issues {
                    if existing_var_type.types == refined_type.types {
                        if !assertion.has_equality()
                            && !assertion_type.is_mixed()
                            && !did_remove_type
                        {
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
            &Some(statements_analyzer.interner),
            statements_analyzer.get_file_path(),
            &mut refined_type,
            &TypeExpansionOptions {
                expand_generic: true,
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
            &mut 0,
        );

        return refined_type;
    }

    get_mixed_any()
}

fn refine_atomic_with_union(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    new_type: &TAtomic,
    existing_var_type: &TUnion,
    pos: Option<&Pos>,
    did_remove_type: &mut bool,
) -> TUnion {
    let intersection_type = intersect_union_with_atomic(
        statements_analyzer,
        analysis_data,
        existing_var_type,
        new_type,
        pos,
        did_remove_type,
    );

    if let Some(mut intersection_type) = intersection_type {
        for intersection_atomic_type in intersection_type.types.iter_mut() {
            intersection_atomic_type.remove_placeholders();
        }

        return intersection_type;
    }

    get_nothing()
}

pub(crate) fn intersect_union_with_atomic(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    existing_var_type: &TUnion,
    new_type: &TAtomic,
    pos: Option<&Pos>,
    did_remove_type: &mut bool,
) -> Option<TUnion> {
    let mut acceptable_types = Vec::new();

    for existing_atomic in &existing_var_type.types {
        let intersected_atomic_type = intersect_atomic_with_atomic(
            statements_analyzer,
            analysis_data,
            existing_atomic,
            new_type,
            pos,
            did_remove_type,
        );

        if let Some(intersected_atomic_type) = intersected_atomic_type {
            acceptable_types.push(intersected_atomic_type);
        }
    }

    if !acceptable_types.is_empty() {
        if acceptable_types.len() > 1 {
            acceptable_types =
                type_combiner::combine(acceptable_types, statements_analyzer.codebase, false);
        }
        return Some(TUnion::new(acceptable_types));
    }

    None
}

pub(crate) fn intersect_atomic_with_atomic(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    type_1_atomic: &TAtomic,
    type_2_atomic: &TAtomic,
    pos: Option<&Pos>,
    did_remove_type: &mut bool,
) -> Option<TAtomic> {
    let mut atomic_comparison_results = TypeComparisonResult::new();

    match (type_1_atomic, type_2_atomic) {
        (TAtomic::TNull, TAtomic::TNull) => {
            return Some(TAtomic::TNull);
        }
        (TAtomic::TMixed | TAtomic::TMixedWithFlags(_, false, _, false), TAtomic::TNull) => {
            *did_remove_type = true;
            return Some(TAtomic::TNull);
        }
        (
            TAtomic::TGenericParam { as_type, .. } | TAtomic::TClassTypeConstant { as_type, .. },
            TAtomic::TNull,
        ) => {
            *did_remove_type = true;

            if as_type.is_mixed() {
                let type_1_atomic = type_1_atomic.replace_template_extends(get_null());

                return Some(type_1_atomic);
            } else {
                let type_1_atomic = type_1_atomic.replace_template_extends(
                    intersect_union_with_atomic(
                        statements_analyzer,
                        analysis_data,
                        as_type,
                        type_2_atomic,
                        pos,
                        did_remove_type,
                    )
                    .unwrap_or(get_nothing()),
                );

                return Some(type_1_atomic);
            }
        }
        (TAtomic::TTypeVariable { name }, TAtomic::TNull) => {
            if let Some(pos) = pos {
                if let Some((lower_bounds, _)) = analysis_data.type_variable_bounds.get_mut(name) {
                    let mut bound = TemplateBound::new(get_null(), 0, None, None);
                    bound.pos = Some(statements_analyzer.get_hpos(pos));
                    lower_bounds.push(bound);
                }
            }

            *did_remove_type = true;

            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TNamedObject {
                name: StrId::XHP_CHILD,
                type_params: None,
                ..
            },
            TAtomic::TNull,
        ) => {
            *did_remove_type = true;
            return Some(TAtomic::TNull);
        }
        (
            TAtomic::TNamedObject {
                type_params: None, ..
            },
            TAtomic::TNull,
        ) => {
            return None;
        }
        (_, TAtomic::TNull) => {
            return None;
        }
        (
            TAtomic::TObject { .. }
            | TAtomic::TClosure(_)
            | TAtomic::TAwaitable { .. }
            | TAtomic::TNamedObject { .. },
            TAtomic::TObject,
        ) => {
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TGenericParam {
                as_type,
                extra_types: None,
                ..
            },
            TAtomic::TObject,
        ) => {
            return if as_type.is_objecty() {
                Some(type_1_atomic.clone())
            } else {
                None
            };
        }
        (TAtomic::TGenericParam { as_type, .. }, TAtomic::TObject) => {
            *did_remove_type = true;

            if as_type.is_mixed() {
                let type_1_atomic = type_1_atomic.replace_template_extends(get_object());

                return Some(type_1_atomic);
            } else if as_type.has_object_type() || as_type.is_mixed() {
                let type_1_atomic = type_1_atomic.replace_template_extends(
                    intersect_union_with_atomic(
                        statements_analyzer,
                        analysis_data,
                        as_type,
                        type_2_atomic,
                        pos,
                        did_remove_type,
                    )
                    .unwrap_or(get_nothing()),
                );

                return Some(type_1_atomic);
            } else {
                return None;
            }
        }
        (_, TAtomic::TObject) => {
            return None;
        }
        (type_1_atomic, TAtomic::TArraykey { .. }) => {
            if type_1_atomic.is_mixed() {
                return Some(TAtomic::TArraykey { from_any: false });
            } else if type_1_atomic.is_int()
                || type_1_atomic.is_string()
                || matches!(type_1_atomic, TAtomic::TArraykey { .. })
            {
                return Some(type_1_atomic.clone());
            } else if let TAtomic::TClassTypeConstant { .. } = type_1_atomic {
                *did_remove_type = true;
                return Some(TAtomic::TArraykey { from_any: true });
            } else if matches!(type_1_atomic, TAtomic::TNum) {
                return Some(TAtomic::TInt);
            } else {
                return None;
            }
        }
        (type_1_atomic, TAtomic::TNum) => {
            if type_1_atomic.is_mixed() {
                return Some(TAtomic::TNum);
            } else if type_1_atomic.is_int() || matches!(type_1_atomic, TAtomic::TFloat { .. }) {
                return Some(type_1_atomic.clone());
            } else if let TAtomic::TClassTypeConstant { .. } = type_1_atomic {
                *did_remove_type = true;
                return Some(TAtomic::TNum);
            } else if matches!(type_1_atomic, TAtomic::TArraykey { .. }) {
                return Some(TAtomic::TInt);
            } else {
                return None;
            }
        }
        (
            TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TTypename { .. }
            | TAtomic::TStringWithFlags(..)
            | TAtomic::TString { .. },
            TAtomic::TString,
        ) => {
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TMixed
            | TAtomic::TMixedWithFlags(..)
            | TAtomic::TMixedFromLoopIsset
            | TAtomic::TScalar
            | TAtomic::TArraykey { .. },
            TAtomic::TString,
        ) => {
            return Some(TAtomic::TString);
        }
        (
            TAtomic::TEnumLiteralCase {
                enum_name,
                as_type,
                underlying_type: Some(underlying_type),
                member_name,
                ..
            },
            TAtomic::TString,
        ) => {
            if let Some(as_type) = as_type {
                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    as_type,
                    &TAtomic::TString,
                    true,
                    &mut TypeComparisonResult::new(),
                ) {
                    return Some(TAtomic::TEnumLiteralCase {
                        enum_name: *enum_name,
                        member_name: *member_name,
                        as_type: Some(Arc::new(TAtomic::TString)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    return None;
                }
            } else {
                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    underlying_type,
                    &TAtomic::TString,
                    false,
                    &mut TypeComparisonResult::new(),
                ) {
                    *did_remove_type = true;
                    return Some(TAtomic::TEnumLiteralCase {
                        enum_name: *enum_name,
                        member_name: *member_name,
                        as_type: Some(Arc::new(TAtomic::TString)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    *did_remove_type = true;
                    return Some(TAtomic::TString);
                }
            }
        }
        (
            TAtomic::TEnum {
                name: enum_name,
                underlying_type: Some(underlying_type),
                as_type: enum_as_type,
                ..
            },
            TAtomic::TString,
        ) => {
            if let Some(enum_as_type) = enum_as_type {
                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    enum_as_type,
                    &TAtomic::TString,
                    true,
                    &mut TypeComparisonResult::new(),
                ) {
                    *did_remove_type = true;
                    return Some(TAtomic::TEnum {
                        name: *enum_name,
                        as_type: Some(Arc::new(TAtomic::TString)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    return None;
                }
            } else {
                *did_remove_type = true;
                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    underlying_type,
                    &TAtomic::TString,
                    false,
                    &mut TypeComparisonResult::new(),
                ) {
                    return Some(TAtomic::TEnum {
                        name: *enum_name,
                        as_type: Some(Arc::new(TAtomic::TString)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    return Some(TAtomic::TString);
                }
            }
        }
        (
            TAtomic::TGenericParam { as_type, .. } | TAtomic::TClassTypeConstant { as_type, .. },
            TAtomic::TString,
        ) => {
            *did_remove_type = true;
            return Some(if as_type.is_mixed() {
                type_1_atomic.replace_template_extends(get_string())
            } else {
                type_1_atomic.replace_template_extends(
                    intersect_union_with_atomic(
                        statements_analyzer,
                        analysis_data,
                        as_type,
                        type_2_atomic,
                        pos,
                        did_remove_type,
                    )
                    .unwrap_or(get_nothing()),
                )
            });
        }
        (TAtomic::TTypeVariable { name }, TAtomic::TString) => {
            if let Some(pos) = pos {
                if let Some((lower_bounds, _)) = analysis_data.type_variable_bounds.get_mut(name) {
                    let mut bound = TemplateBound::new(get_string(), 0, None, None);
                    bound.pos = Some(statements_analyzer.get_hpos(pos));
                    lower_bounds.push(bound);
                }
            }

            *did_remove_type = true;
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TNamedObject {
                name: StrId::XHP_CHILD,
                type_params: None,
                ..
            },
            TAtomic::TString,
        ) => {
            *did_remove_type = true;
            return Some(TAtomic::TString);
        }
        (
            TAtomic::TNamedObject {
                type_params: None, ..
            },
            TAtomic::TString,
        ) => return None,
        (type_1_atomic, TAtomic::TString) => {
            if atomic_type_comparator::is_contained_by(
                statements_analyzer.codebase,
                statements_analyzer.get_file_path(),
                type_1_atomic,
                &TAtomic::TString,
                false,
                &mut TypeComparisonResult::new(),
            ) {
                return Some(type_1_atomic.clone());
            } else {
                return None;
            }
        }
        (TAtomic::TLiteralInt { .. } | TAtomic::TInt, TAtomic::TInt) => {
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TMixed
            | TAtomic::TMixedWithFlags(..)
            | TAtomic::TScalar
            | TAtomic::TNum
            | TAtomic::TArraykey { .. }
            | TAtomic::TMixedFromLoopIsset,
            TAtomic::TInt,
        ) => {
            *did_remove_type = true;
            return Some(TAtomic::TInt);
        }
        (
            TAtomic::TGenericParam { as_type, .. } | TAtomic::TClassTypeConstant { as_type, .. },
            TAtomic::TInt,
        ) => {
            *did_remove_type = true;
            if as_type.is_mixed() {
                let type_1_atomic = type_1_atomic.replace_template_extends(get_int());
                return Some(type_1_atomic);
            } else {
                let type_1_atomic = type_1_atomic.replace_template_extends(
                    intersect_union_with_atomic(
                        statements_analyzer,
                        analysis_data,
                        as_type,
                        type_2_atomic,
                        pos,
                        did_remove_type,
                    )
                    .unwrap_or(get_nothing()),
                );
                return Some(type_1_atomic);
            }
        }
        (TAtomic::TTypeVariable { name }, TAtomic::TInt) => {
            *did_remove_type = true;

            if let Some(pos) = pos {
                if let Some((lower_bounds, _)) = analysis_data.type_variable_bounds.get_mut(name) {
                    let mut bound = TemplateBound::new(get_int(), 0, None, None);
                    bound.pos = Some(statements_analyzer.get_hpos(pos));
                    lower_bounds.push(bound);
                }
            }
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TEnumLiteralCase {
                enum_name,
                as_type,
                underlying_type: Some(underlying_type),
                member_name,
                ..
            },
            TAtomic::TInt,
        ) => {
            if let Some(as_type) = as_type {
                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    as_type,
                    &TAtomic::TInt,
                    true,
                    &mut TypeComparisonResult::new(),
                ) {
                    return Some(TAtomic::TEnumLiteralCase {
                        enum_name: *enum_name,
                        member_name: *member_name,
                        as_type: Some(Arc::new(TAtomic::TInt)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    return None;
                }
            } else {
                *did_remove_type = true;
                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    underlying_type,
                    &TAtomic::TInt,
                    false,
                    &mut TypeComparisonResult::new(),
                ) {
                    return Some(TAtomic::TEnumLiteralCase {
                        enum_name: *enum_name,
                        member_name: *member_name,
                        as_type: Some(Arc::new(TAtomic::TInt)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    return Some(TAtomic::TInt);
                }
            }
        }
        (
            TAtomic::TEnum {
                name: enum_name,
                underlying_type: Some(underlying_type),
                as_type: enum_as_type,
                ..
            },
            TAtomic::TInt,
        ) => {
            if let Some(enum_as_type) = enum_as_type {
                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    enum_as_type,
                    &TAtomic::TInt,
                    true,
                    &mut TypeComparisonResult::new(),
                ) {
                    *did_remove_type = true;
                    return Some(TAtomic::TEnum {
                        name: *enum_name,
                        as_type: Some(Arc::new(TAtomic::TInt)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    return None;
                }
            } else {
                *did_remove_type = true;

                if atomic_type_comparator::is_contained_by(
                    statements_analyzer.codebase,
                    statements_analyzer.get_file_path(),
                    underlying_type,
                    &TAtomic::TInt,
                    false,
                    &mut TypeComparisonResult::new(),
                ) {
                    return Some(TAtomic::TEnum {
                        name: *enum_name,
                        as_type: Some(Arc::new(TAtomic::TInt)),
                        underlying_type: Some(underlying_type.clone()),
                    });
                } else {
                    return Some(TAtomic::TInt);
                }
            }
        }
        (type_1_atomic, TAtomic::TInt) => {
            if atomic_type_comparator::is_contained_by(
                statements_analyzer.codebase,
                statements_analyzer.get_file_path(),
                type_1_atomic,
                &TAtomic::TInt,
                false,
                &mut TypeComparisonResult::new(),
            ) {
                return Some(type_1_atomic.clone());
            } else {
                return None;
            }
        }
        (
            TAtomic::TGenericParam { as_type, .. } | TAtomic::TClassTypeConstant { as_type, .. },
            TAtomic::TMixedWithFlags(_, _, _, true),
        ) => {
            let type_1_atomic = type_1_atomic.replace_template_extends(
                intersect_union_with_atomic(
                    statements_analyzer,
                    analysis_data,
                    as_type,
                    type_2_atomic,
                    pos,
                    did_remove_type,
                )
                .unwrap_or(get_nothing()),
            );

            *did_remove_type = true;

            return Some(type_1_atomic);
        }
        (TAtomic::TTypeVariable { .. }, TAtomic::TMixedWithFlags(_, _, _, true)) => {
            *did_remove_type = true;
            return Some(type_1_atomic.clone());
        }
        (TAtomic::TMixed, TAtomic::TMixedWithFlags(_, _, _, true)) => {
            *did_remove_type = true;
            return Some(TAtomic::TMixedWithFlags(false, false, false, true));
        }
        (
            TAtomic::TMixedWithFlags(is_any, false, _, false),
            TAtomic::TMixedWithFlags(_, _, _, true),
        ) => {
            *did_remove_type = true;
            return Some(TAtomic::TMixedWithFlags(*is_any, false, false, true));
        }
        (TAtomic::TNull, TAtomic::TMixedWithFlags(_, _, _, true)) => return None,
        (
            TAtomic::TNamedObject {
                name: StrId::XHP_CHILD,
                type_params: None,
                ..
            },
            TAtomic::TMixedWithFlags(_, _, _, true),
        ) => {
            *did_remove_type = true;
            return Some(type_1_atomic.clone());
        }
        (_, TAtomic::TMixedWithFlags(_, _, _, true)) => {
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TNamedObject {
                name: type_1_name,
                type_params: Some(type_1_params),
                ..
            },
            TAtomic::TDict(TDict {
                known_items: None,
                params: Some(type_2_params),
                ..
            }),
        ) => {
            if type_2_params.0.is_placeholder() && type_2_params.1.is_placeholder() {
                if type_1_name == &StrId::CONTAINER {
                    return Some(TAtomic::TDict(TDict {
                        params: Some((
                            Box::new(get_arraykey(true)),
                            Box::new(type_1_params[0].clone()),
                        )),
                        ..TDict::default()
                    }));
                } else if type_1_name == &StrId::KEYED_CONTAINER || type_1_name == &StrId::ANY_ARRAY
                {
                    return Some(TAtomic::TDict(TDict {
                        params: Some((
                            Box::new(type_1_params[0].clone()),
                            Box::new(type_1_params[1].clone()),
                        )),
                        ..TDict::default()
                    }));
                } else {
                    return None;
                };
            }
        }
        (
            TAtomic::TNamedObject {
                name: type_1_name,
                type_params: Some(type_1_params),
                ..
            },
            TAtomic::TKeyset {
                type_param: type_2_param,
                ..
            },
        ) => {
            if type_2_param.is_placeholder() {
                if type_1_name == &StrId::CONTAINER {
                    return intersect_union_with_atomic(
                        statements_analyzer,
                        analysis_data,
                        &type_1_params[0],
                        &TAtomic::TArraykey { from_any: true },
                        pos,
                        did_remove_type,
                    )
                    .map(|intersected| TAtomic::TKeyset {
                        type_param: Box::new(intersected),
                        non_empty: false,
                    });
                } else if type_1_name == &StrId::KEYED_CONTAINER || type_1_name == &StrId::ANY_ARRAY
                {
                    return intersect_union_with_atomic(
                        statements_analyzer,
                        analysis_data,
                        &type_1_params[1],
                        &TAtomic::TArraykey { from_any: true },
                        pos,
                        did_remove_type,
                    )
                    .map(|intersected| TAtomic::TKeyset {
                        type_param: Box::new(intersected),
                        non_empty: false,
                    });
                } else {
                    return None;
                };
            }
        }
        (
            TAtomic::TNamedObject {
                name: StrId::XHP_CHILD,
                ..
            }
            | TAtomic::TMixed
            | TAtomic::TMixedWithFlags(..)
            | TAtomic::TMixedFromLoopIsset,
            TAtomic::TDict(..),
        ) => {
            let mut type_2_atomic = type_2_atomic.clone();
            type_2_atomic.remove_placeholders();
            return Some(type_2_atomic);
        }
        _ => (),
    }

    if atomic_type_comparator::is_contained_by(
        statements_analyzer.codebase,
        statements_analyzer.get_file_path(),
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
            statements_analyzer,
            analysis_data,
            type_1_atomic,
            &type_2_atomic,
            atomic_comparison_results.type_coerced.unwrap_or(false),
            pos,
            did_remove_type,
        );
    }

    atomic_comparison_results = TypeComparisonResult::new();

    if atomic_type_comparator::is_contained_by(
        statements_analyzer.codebase,
        statements_analyzer.get_file_path(),
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
            statements_analyzer,
            analysis_data,
            type_2_atomic,
            &type_1_atomic,
            atomic_comparison_results.type_coerced.unwrap_or(false),
            pos,
            did_remove_type,
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

    let codebase = statements_analyzer.codebase;

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
                        let c1_value = expand_constant_value(c1, codebase);
                        let c2_value = expand_constant_value(c2, codebase);
                        if c1_value == c2_value {
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
                        let c1_value = expand_constant_value(c1, codebase);
                        let c2_value = expand_constant_value(c2, codebase);

                        if c1_value == c2_value {
                            return Some(type_2_atomic.clone());
                        }
                    }
                }
            }
        }
        (
            TAtomic::TEnum {
                name: type_1_name, ..
            },
            TAtomic::TEnumLiteralCase {
                enum_name: type_2_name,
                member_name,
                ..
            },
        ) => {
            if let (Some(storage_1), Some(storage_2)) = (
                codebase.classlike_infos.get(type_1_name),
                codebase.classlike_infos.get(type_2_name),
            ) {
                if let Some(c2) = &storage_2.constants.get(member_name) {
                    for (_, c1) in &storage_1.constants {
                        let c1_value = expand_constant_value(c1, codebase);
                        let c2_value = expand_constant_value(c2, codebase);

                        if c1_value == c2_value {
                            return Some(type_1_atomic.clone());
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
                statements_analyzer.codebase,
                type_1_name,
                type_1_member_name,
                type_2_atomic,
            );
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
                statements_analyzer.codebase,
                type_2_name,
                type_2_member_name,
                type_1_atomic,
            );
        }
        (
            TAtomic::TEnum {
                name: enum_name,
                underlying_type: Some(enum_underlying_type),
                as_type: enum_as_type,
                ..
            },
            TAtomic::TLiteralInt { .. } | TAtomic::TLiteralString { .. },
        ) => {
            return intersect_enum_with_literal(
                statements_analyzer.codebase,
                enum_name,
                enum_underlying_type,
                enum_as_type,
                type_2_atomic,
            );
        }
        (
            TAtomic::TEnum {
                name: type_1_name,
                underlying_type: Some(underlying_type),
                ..
            },
            TAtomic::TString | TAtomic::TStringWithFlags(..) | TAtomic::TInt,
        ) => {
            return intersect_enum_with_int_or_string(
                statements_analyzer.codebase,
                statements_analyzer.get_file_path(),
                type_1_name,
                &underlying_type,
                type_2_atomic.clone(),
            );
        }
        (
            TAtomic::TString | TAtomic::TStringWithFlags(..) | TAtomic::TInt,
            TAtomic::TEnum {
                name: type_2_name,
                underlying_type: Some(underlying_type),
                ..
            },
        ) => {
            return intersect_enum_with_int_or_string(
                statements_analyzer.codebase,
                statements_analyzer.get_file_path(),
                type_2_name,
                &underlying_type,
                type_1_atomic.clone(),
            );
        }
        (
            TAtomic::TTypeAlias {
                name: StrId::MEMBER_OF,
                type_params: Some(type_params),
                newtype: true,
                as_type,
            },
            _,
        ) => {
            return intersect_union_with_atomic(
                statements_analyzer,
                analysis_data,
                &type_params[1],
                type_2_atomic,
                pos,
                did_remove_type,
            )
            .map(|intersected| TAtomic::TTypeAlias {
                name: StrId::MEMBER_OF,
                type_params: Some(vec![type_params[0].clone(), intersected]),
                as_type: as_type.clone(),
                newtype: true,
            });
        }
        (
            _,
            TAtomic::TTypeAlias {
                name: StrId::MEMBER_OF,
                type_params: Some(type_params),
                newtype: true,
                as_type,
            },
        ) => {
            return intersect_union_with_atomic(
                statements_analyzer,
                analysis_data,
                &type_params[1],
                type_1_atomic,
                pos,
                did_remove_type,
            )
            .map(|intersected| TAtomic::TTypeAlias {
                name: StrId::MEMBER_OF,
                type_params: Some(vec![type_params[0].clone(), intersected]),
                as_type: as_type.clone(),
                newtype: true,
            });
        }
        (
            TAtomic::TTypeAlias {
                as_type: Some(type_1_as),
                name,
                type_params,
                newtype,
            },
            _,
        ) => {
            return intersect_union_with_atomic(
                statements_analyzer,
                analysis_data,
                type_1_as,
                type_2_atomic,
                pos,
                did_remove_type,
            )
            .map(|intersected| TAtomic::TTypeAlias {
                name: *name,
                type_params: type_params.clone(),
                as_type: Some(Box::new(intersected)),
                newtype: *newtype,
            });
        }
        (
            _,
            TAtomic::TTypeAlias {
                as_type: Some(type_2_as),
                name,
                type_params,
                newtype,
            },
        ) => {
            return intersect_union_with_atomic(
                statements_analyzer,
                analysis_data,
                type_2_as,
                type_1_atomic,
                pos,
                did_remove_type,
            )
            .map(|intersected| TAtomic::TTypeAlias {
                name: *name,
                type_params: type_params.clone(),
                as_type: Some(Box::new(intersected)),
                newtype: *newtype,
            });
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
            if type_1_name == &StrId::XHP_CHILD {
                if type_2_name == &StrId::KEYED_CONTAINER
                    || type_2_name == &StrId::KEYED_TRAVERSABLE
                {
                    let mut atomic = TAtomic::TNamedObject {
                        name: StrId::ANY_ARRAY,
                        type_params: type_2_params.clone(),
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    };
                    atomic.remove_placeholders();
                    return Some(atomic);
                } else if type_2_name == &StrId::CONTAINER || type_2_name == &StrId::TRAVERSABLE {
                    let type_2_params = type_2_params
                        .as_ref()
                        .map(|type_2_params| vec![get_arraykey(true), type_2_params[0].clone()]);

                    let mut atomic = TAtomic::TNamedObject {
                        name: StrId::ANY_ARRAY,
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
        (TAtomic::TDict(type_1_dict), TAtomic::TDict(type_2_dict)) => {
            return intersect_dicts(
                statements_analyzer,
                analysis_data,
                type_1_dict,
                type_2_dict,
                pos,
                did_remove_type,
            );
        }
        (
            TAtomic::TVec(TVec {
                known_items: type_1_known_items,
                type_param: type_1_param,
                ..
            }),
            TAtomic::TVec(TVec {
                known_items: type_2_known_items,
                type_param: type_2_param,
                ..
            }),
        ) => {
            return intersect_vecs(
                statements_analyzer,
                analysis_data,
                type_1_param,
                type_2_param,
                type_1_known_items,
                type_2_known_items,
                pos,
                did_remove_type,
            );
        }
        (
            TAtomic::TKeyset {
                type_param: type_1_param,
                ..
            },
            TAtomic::TKeyset {
                type_param: type_2_param,
                ..
            },
        ) => {
            return intersect_union_with_union(
                statements_analyzer,
                analysis_data,
                &type_1_param,
                &type_2_param,
                pos,
                did_remove_type,
            )
            .map(|intersected| TAtomic::TKeyset {
                type_param: Box::new(intersected),
                non_empty: false,
            });
        }
        (
            TAtomic::TNamedObject {
                name: type_1_name,
                type_params: Some(type_1_params),
                ..
            },
            TAtomic::TDict(TDict {
                known_items: Some(type_2_known_items),
                params: type_2_params,
                ..
            }),
        ) => {
            let (type_1_key_param, type_1_value_param) = if type_1_name == &StrId::CONTAINER {
                (get_arraykey(true), &type_1_params[0])
            } else if type_1_name == &StrId::KEYED_CONTAINER || type_1_name == &StrId::ANY_ARRAY {
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

            return Some(TAtomic::TDict(TDict {
                known_items: Some(type_2_known_items),
                params,
                non_empty: true,
                shape_name: None,
            }));
        }
        (TAtomic::TGenericParam { as_type, .. }, TAtomic::TNamedObject { .. }) => {
            let new_as = intersect_union_with_atomic(
                statements_analyzer,
                analysis_data,
                as_type,
                type_2_atomic,
                pos,
                did_remove_type,
            );

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
            let new_as = intersect_union_with_atomic(
                statements_analyzer,
                analysis_data,
                as_type,
                type_1_atomic,
                pos,
                did_remove_type,
            );

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
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    type_1_param: &TUnion,
    type_2_param: &TUnion,
    type_1_known_items: &Option<BTreeMap<usize, (bool, TUnion)>>,
    type_2_known_items: &Option<BTreeMap<usize, (bool, TUnion)>>,
    pos: Option<&Pos>,
    did_remove_type: &mut bool,
) -> Option<TAtomic> {
    let type_param = intersect_union_with_union(
        statements_analyzer,
        analysis_data,
        type_1_param,
        type_2_param,
        pos,
        did_remove_type,
    );

    match (type_1_known_items, type_2_known_items) {
        (Some(type_1_known_items), Some(type_2_known_items)) => {
            let mut type_2_known_items = type_2_known_items.clone();

            for (type_2_key, type_2_value) in type_2_known_items.iter_mut() {
                if let Some(type_1_value) = type_1_known_items.get(type_2_key) {
                    type_2_value.0 = type_2_value.0 && type_1_value.0;
                    type_2_value.1 = intersect_union_with_union(
                        statements_analyzer,
                        analysis_data,
                        &type_1_value.1,
                        &type_2_value.1,
                        pos,
                        did_remove_type,
                    )?
                } else if !type_1_param.is_nothing() {
                    type_2_value.1 = intersect_union_with_union(
                        statements_analyzer,
                        analysis_data,
                        type_1_param,
                        &type_2_value.1,
                        pos,
                        did_remove_type,
                    )?
                } else {
                    // if the type_2 key is always defined, the intersection is impossible
                    if !type_2_value.0 {
                        return None;
                    }
                }
            }

            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec(TVec {
                    known_items: Some(type_2_known_items),
                    type_param: Box::new(type_param),
                    non_empty: true,
                    known_count: None,
                }));
            }

            None
        }
        (None, Some(type_2_known_items)) => {
            let mut type_2_known_items = type_2_known_items.clone();

            for (_, type_2_value) in type_2_known_items.iter_mut() {
                type_2_value.1 = intersect_union_with_union(
                    statements_analyzer,
                    analysis_data,
                    &type_2_value.1,
                    type_1_param,
                    pos,
                    did_remove_type,
                )?
            }

            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec(TVec {
                    known_items: Some(type_2_known_items),
                    type_param: Box::new(type_param),
                    non_empty: false,
                    known_count: None,
                }));
            }

            None
        }
        (Some(type_1_known_items), None) => {
            let mut type_1_known_items = type_1_known_items.clone();

            for (_, type_1_value) in type_1_known_items.iter_mut() {
                type_1_value.1 = intersect_union_with_union(
                    statements_analyzer,
                    analysis_data,
                    &type_1_value.1,
                    type_2_param,
                    pos,
                    did_remove_type,
                )?
            }

            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec(TVec {
                    known_items: Some(type_1_known_items),
                    type_param: Box::new(type_param),
                    non_empty: false,
                    known_count: None,
                }));
            }

            None
        }
        _ => {
            if let Some(type_param) = type_param {
                return Some(TAtomic::TVec(TVec {
                    known_items: None,
                    type_param: Box::new(type_param),
                    non_empty: false,
                    known_count: None,
                }));
            }

            None
        }
    }
}

fn intersect_dicts(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    type_1_dict: &TDict,
    type_2_dict: &TDict,
    pos: Option<&Pos>,
    did_remove_type: &mut bool,
) -> Option<TAtomic> {
    let params = match (&type_1_dict.params, &type_2_dict.params) {
        (Some(type_1_params), Some(type_2_params)) => {
            let key = intersect_union_with_union(
                statements_analyzer,
                analysis_data,
                &type_1_params.0,
                &type_2_params.0,
                pos,
                did_remove_type,
            );
            let value = intersect_union_with_union(
                statements_analyzer,
                analysis_data,
                &type_1_params.1,
                &type_2_params.1,
                pos,
                did_remove_type,
            );

            if let (Some(key), Some(value)) = (key, value) {
                Some((Box::new(key), Box::new(value)))
            } else {
                return None;
            }
        }
        _ => None,
    };

    match (&type_1_dict.known_items, &type_2_dict.known_items) {
        (Some(type_1_known_items), Some(type_2_known_items)) => {
            let mut intersected_items = BTreeMap::new();

            for (type_2_key, type_2_value) in type_2_known_items {
                if let Some(type_1_value) = type_1_known_items.get(type_2_key) {
                    intersected_items.insert(
                        type_2_key.clone(),
                        (
                            type_2_value.0 && type_1_value.0,
                            if let Some(t) = intersect_union_with_union(
                                statements_analyzer,
                                analysis_data,
                                &type_1_value.1,
                                &type_2_value.1,
                                pos,
                                did_remove_type,
                            ) {
                                Arc::new(t)
                            } else {
                                return None;
                            },
                        ),
                    );
                } else if let Some(type_1_params) = &type_1_dict.params {
                    intersected_items.insert(
                        type_2_key.clone(),
                        (
                            type_2_value.0,
                            if let Some(t) = intersect_union_with_union(
                                statements_analyzer,
                                analysis_data,
                                &type_1_params.1,
                                &type_2_value.1,
                                pos,
                                did_remove_type,
                            ) {
                                Arc::new(t)
                            } else {
                                return None;
                            },
                        ),
                    );
                } else if !type_2_value.0 {
                    return None;
                }
            }

            Some(TAtomic::TDict(TDict {
                known_items: Some(intersected_items),
                params,
                non_empty: true,
                shape_name: None,
            }))
        }
        (None, Some(type_2_known_items)) => {
            let mut type_2_known_items = type_2_known_items.clone();

            for (_, type_2_value) in type_2_known_items.iter_mut() {
                if let Some(type_1_params) = &type_1_dict.params {
                    type_2_value.1 = if let Some(t) = intersect_union_with_union(
                        statements_analyzer,
                        analysis_data,
                        &type_2_value.1,
                        &type_1_params.1,
                        pos,
                        did_remove_type,
                    ) {
                        Arc::new(t)
                    } else {
                        return None;
                    }
                } else if type_2_dict.params.is_none() && !type_2_value.0 {
                    return None;
                }
            }

            Some(TAtomic::TDict(TDict {
                known_items: Some(type_2_known_items),
                params,
                non_empty: true,
                shape_name: None,
            }))
        }
        (Some(type_1_known_items), None) => {
            let mut type_1_known_items = type_1_known_items.clone();

            for (_, type_1_value) in type_1_known_items.iter_mut() {
                if let Some(type_2_params) = &type_2_dict.params {
                    type_1_value.1 = if let Some(t) = intersect_union_with_union(
                        statements_analyzer,
                        analysis_data,
                        &type_1_value.1,
                        &type_2_params.1,
                        pos,
                        did_remove_type,
                    ) {
                        Arc::new(t)
                    } else {
                        return None;
                    }
                } else if type_1_dict.params.is_none() && !type_1_value.0 {
                    return None;
                }
            }

            Some(TAtomic::TDict(TDict {
                known_items: Some(type_1_known_items),
                params,
                non_empty: true,
                shape_name: None,
            }))
        }
        _ => Some(TAtomic::TDict(TDict {
            known_items: None,
            params,
            non_empty: true,
            shape_name: None,
        })),
    }
}

pub(crate) fn intersect_union_with_union(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    type_1_param: &TUnion,
    type_2_param: &TUnion,
    pos: Option<&Pos>,
    did_remove_type: &mut bool,
) -> Option<TUnion> {
    if type_1_param == type_2_param {
        return Some(type_1_param.clone());
    }

    let type_param = match (type_1_param.is_single(), type_2_param.is_single()) {
        (true, true) => intersect_atomic_with_atomic(
            statements_analyzer,
            analysis_data,
            type_1_param.get_single(),
            type_2_param.get_single(),
            pos,
            &mut false,
        )
        .map(wrap_atomic),
        (false, true) => intersect_union_with_atomic(
            statements_analyzer,
            analysis_data,
            type_1_param,
            type_2_param.get_single(),
            pos,
            did_remove_type,
        ),
        (true, false) => intersect_union_with_atomic(
            statements_analyzer,
            analysis_data,
            type_2_param,
            type_1_param.get_single(),
            pos,
            did_remove_type,
        ),
        (false, false) => {
            let new_types = type_2_param
                .types
                .iter()
                .flat_map(|t| {
                    intersect_union_with_atomic(
                        statements_analyzer,
                        analysis_data,
                        type_1_param,
                        t,
                        pos,
                        did_remove_type,
                    )
                    .unwrap_or(get_nothing())
                    .types
                })
                .collect::<Vec<_>>();

            let combined_union = TUnion::new(type_combiner::combine(
                new_types,
                statements_analyzer.codebase,
                false,
            ));

            if combined_union.is_nothing() {
                None
            } else {
                Some(combined_union)
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
            if let TAtomic::TLiteralString {
                value: inferred_value,
            } = inferred_type
            {
                return Some(TAtomic::TLiteralString {
                    value: inferred_value.clone(),
                });
            }
        }
    }
    Some(type_2_atomic.clone())
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
            if let TAtomic::TLiteralInt {
                value: inferred_value,
            } = inferred_type
            {
                return Some(TAtomic::TLiteralInt {
                    value: *inferred_value,
                });
            }
        }
    }
    Some(type_2_atomic.clone())
}

fn intersect_enum_with_int_or_string(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    enum_name: &StrId,
    underlying_type: &Arc<TAtomic>,
    int_or_string: TAtomic,
) -> Option<TAtomic> {
    let mut atomic_comparison_results = TypeComparisonResult::new();

    if atomic_type_comparator::is_contained_by(
        codebase,
        file_path,
        &int_or_string,
        underlying_type,
        true,
        &mut atomic_comparison_results,
    ) {
        return Some(TAtomic::TEnum {
            name: *enum_name,
            as_type: Some(Arc::new(int_or_string)),
            underlying_type: Some(underlying_type.clone()),
        });
    }
    None
}

fn intersect_enum_with_literal(
    codebase: &CodebaseInfo,
    enum_name: &StrId,
    enum_underlying_type: &Arc<TAtomic>,
    enum_as_type: &Option<Arc<TAtomic>>,
    type_2_atomic: &TAtomic,
) -> Option<TAtomic> {
    let enum_storage = codebase.classlike_infos.get(enum_name)?;

    let mut all_inferred = true;

    for (case_name, enum_case) in &enum_storage.constants {
        if let Some(inferred_type) = &enum_case.inferred_type {
            if inferred_type == type_2_atomic {
                return Some(TAtomic::TEnumLiteralCase {
                    enum_name: *enum_name,
                    member_name: *case_name,
                    as_type: enum_as_type.clone(),
                    underlying_type: Some(enum_underlying_type.clone()),
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
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    super_atomic: &TAtomic,
    sub_atomic: &TAtomic,
    generic_coercion: bool,
    pos: Option<&Pos>,
    did_remove_type: &mut bool,
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
                        name: *sub_atomic_name,
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
                let type_1_as = intersect_union_with_atomic(
                    statements_analyzer,
                    analysis_data,
                    type_1_as_type,
                    sub_atomic,
                    pos,
                    did_remove_type,
                );

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
