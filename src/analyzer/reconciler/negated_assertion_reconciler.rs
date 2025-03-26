use super::{
    assertion_reconciler::intersect_atomic_with_atomic, simple_negated_assertion_reconciler,
    trigger_issue_for_impossible,
};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_code_info::t_atomic::TDict;
use hakana_code_info::ttype::{
    comparison::{
        atomic_type_comparator, type_comparison_result::TypeComparisonResult, union_type_comparator,
    },
    type_combiner,
};
use hakana_code_info::ttype::{get_nothing, get_placeholder, wrap_atomic};
use hakana_code_info::{
    assertion::Assertion, codebase_info::CodebaseInfo,
    functionlike_identifier::FunctionLikeIdentifier, t_atomic::TAtomic, t_union::TUnion,
};
use hakana_str::StrId;
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<&String>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    old_var_type_string: String,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let is_equality = assertion.has_equality();

    if is_equality && assertion.has_literal_string_or_int() {
        if existing_var_type.is_mixed() {
            return existing_var_type.clone();
        }

        return handle_literal_negated_equality(
            assertion,
            existing_var_type,
            key,
            statements_analyzer,
            analysis_data,
            old_var_type_string,
            pos,
            calling_functionlike_id,
            negated,
            suppressed_issues,
        );
    }

    let simple_negated_type = simple_negated_assertion_reconciler::reconcile(
        assertion,
        existing_var_type,
        possibly_undefined,
        key,
        statements_analyzer,
        analysis_data,
        pos,
        calling_functionlike_id,
        negated,
        suppressed_issues,
    );

    if let Some(simple_negated_type) = simple_negated_type {
        return simple_negated_type;
    }

    let mut existing_var_type = existing_var_type.clone();

    let codebase = statements_analyzer.codebase;

    if let Some(assertion_type) = assertion.get_type() {
        if !is_equality {
            if let Some(assertion_type) = assertion.get_type() {
                let mut has_changes = false;
                subtract_complex_type(
                    assertion_type,
                    codebase,
                    &mut existing_var_type,
                    &mut has_changes,
                );

                if !has_changes || existing_var_type.is_nothing() {
                    if let Some(key) = &key {
                        if let Some(pos) = pos {
                            trigger_issue_for_impossible(
                                analysis_data,
                                statements_analyzer,
                                &old_var_type_string,
                                key,
                                assertion,
                                !has_changes,
                                negated,
                                pos,
                                calling_functionlike_id,
                                suppressed_issues,
                            );
                        }
                    }
                }
            }
        } else if let Some(key) = &key {
            if let Some(pos) = pos {
                if !union_type_comparator::can_expression_types_be_identical(
                    codebase,
                    &existing_var_type,
                    &wrap_atomic(assertion_type.clone()),
                    true,
                ) {
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
            }
        }
    }

    if existing_var_type.types.is_empty() {
        // todo prevent complaining about $this assertions in traits

        if !is_equality {
            if let Some(key) = &key {
                if let Some(pos) = pos {
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

            return get_nothing();
        }
    }

    existing_var_type
}

fn subtract_complex_type(
    assertion_type: &TAtomic,
    codebase: &CodebaseInfo,
    existing_var_type: &mut TUnion,
    can_be_disjunct: &mut bool,
) {
    let mut acceptable_types = vec![];

    let existing_atomic_types = existing_var_type.types.drain(..).collect::<Vec<_>>();

    for existing_atomic in existing_atomic_types {
        if &existing_atomic == assertion_type {
            *can_be_disjunct = true;

            continue;
        }

        if atomic_type_comparator::is_contained_by(
            codebase,
            &existing_atomic,
            assertion_type,
            true,
            &mut TypeComparisonResult::new(),
        ) {
            *can_be_disjunct = true;

            // don't add as acceptable
            continue;
        }

        if atomic_type_comparator::is_contained_by(
            codebase,
            assertion_type,
            &existing_atomic,
            true,
            &mut TypeComparisonResult::new(),
        ) {
            *can_be_disjunct = true;
        }

        match (&existing_atomic, assertion_type) {
            (
                TAtomic::TNamedObject {
                    name: existing_classlike_name,
                    ..
                },
                TAtomic::TNamedObject {
                    name: assertion_classlike_name,
                    ..
                },
            ) => {
                if let Some(classlike_storage) =
                    codebase.classlike_infos.get(existing_classlike_name)
                {
                    // handle __Sealed classes, negating where possible
                    if let Some(child_classlikes) = &classlike_storage.child_classlikes {
                        if child_classlikes.contains(assertion_classlike_name) {
                            handle_negated_class(
                                child_classlikes,
                                &existing_atomic,
                                assertion_classlike_name,
                                codebase,
                                &mut acceptable_types,
                            );

                            *can_be_disjunct = true;

                            continue;
                        }
                    }
                }

                if (codebase.interface_exists(assertion_classlike_name)
                    || codebase.interface_exists(existing_classlike_name))
                    && assertion_classlike_name != existing_classlike_name
                {
                    *can_be_disjunct = true;
                }

                acceptable_types.push(existing_atomic);
            }
            (TAtomic::TDict(TDict { .. }), TAtomic::TDict(TDict { .. })) => {
                *can_be_disjunct = true;
                // todo subtract assertion dict from existing
                acceptable_types.push(existing_atomic);
            }
            (TAtomic::TString | TAtomic::TStringWithFlags(..), TAtomic::TEnum { .. })
            | (TAtomic::TEnum { .. }, TAtomic::TString | TAtomic::TStringWithFlags(..)) => {
                *can_be_disjunct = true;
                acceptable_types.push(existing_atomic);
            }
            (TAtomic::TEnum { .. }, TAtomic::TEnum { .. }) => {
                *can_be_disjunct = true;
                acceptable_types.push(existing_atomic);
            }
            (
                TAtomic::TTypeAlias {
                    as_type: Some(_), ..
                },
                _,
            )
            | (
                _,
                TAtomic::TTypeAlias {
                    as_type: Some(_), ..
                },
            ) => {
                *can_be_disjunct = true;
                acceptable_types.push(existing_atomic);
            }
            _ => {
                acceptable_types.push(existing_atomic);
            }
        }
    }

    if acceptable_types.is_empty() {
        acceptable_types.push(TAtomic::TNothing);
    } else if acceptable_types.len() > 1 && *can_be_disjunct {
        acceptable_types = type_combiner::combine(acceptable_types, codebase, false);
    }

    existing_var_type.types = acceptable_types;
}

fn handle_negated_class(
    child_classlikes: &FxHashSet<StrId>,
    existing_atomic: &TAtomic,
    assertion_classlike_name: &StrId,
    codebase: &CodebaseInfo,
    acceptable_types: &mut Vec<TAtomic>,
) {
    for child_classlike in child_classlikes {
        if child_classlike != assertion_classlike_name {
            let alternate_class = TAtomic::TNamedObject {
                name: *child_classlike,
                type_params: if let Some(child_classlike_info) =
                    codebase.classlike_infos.get(child_classlike)
                {
                    let placeholder_params = child_classlike_info
                        .template_types
                        .iter()
                        .map(|_| get_placeholder())
                        .collect::<Vec<_>>();

                    if placeholder_params.is_empty() {
                        None
                    } else {
                        Some(placeholder_params)
                    }
                } else {
                    None
                },
                extra_types: None,
                is_this: false,
                remapped_params: false,
            };

            if let Some(acceptable_alternate_class) =
                intersect_atomic_with_atomic(existing_atomic, &alternate_class, codebase)
            {
                acceptable_types.push(acceptable_alternate_class);
            }
        }
    }
}

fn handle_literal_negated_equality(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: Option<&String>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    old_var_type_string: String,
    pos: Option<&Pos>,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let assertion_type = assertion.get_type().unwrap();

    let mut did_remove_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    let codebase = statements_analyzer.codebase;

    for existing_atomic_type in existing_var_types {
        match existing_atomic_type {
            TAtomic::TInt { .. } | TAtomic::TNum => {
                if let TAtomic::TLiteralInt { .. } | TAtomic::TEnumLiteralCase { .. } =
                    assertion_type
                {
                    did_remove_type = true;
                }

                acceptable_types.push(existing_atomic_type);
            }
            TAtomic::TLiteralInt {
                value: existing_value,
                ..
            } => {
                if let TAtomic::TLiteralInt { value, .. } = assertion_type {
                    if value == &existing_value {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                }
            }
            TAtomic::TArraykey { .. } => {
                if let TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TEnumLiteralCase { .. } = assertion_type
                {
                    did_remove_type = true;
                }

                acceptable_types.push(existing_atomic_type);
            }
            TAtomic::TString => match assertion_type {
                TAtomic::TLiteralString { value, .. } => {
                    did_remove_type = true;

                    if value.is_empty() {
                        acceptable_types.push(TAtomic::TStringWithFlags(false, true, false));
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                }
                TAtomic::TEnumLiteralCase { .. } => {
                    did_remove_type = true;

                    acceptable_types.push(existing_atomic_type);
                }
                _ => {
                    acceptable_types.push(existing_atomic_type);
                }
            },
            TAtomic::TStringWithFlags(_, _, is_nonspecific_literal) => match assertion_type {
                TAtomic::TLiteralString { value, .. } => {
                    did_remove_type = true;

                    if value.is_empty() {
                        acceptable_types.push(TAtomic::TStringWithFlags(
                            false,
                            true,
                            is_nonspecific_literal,
                        ));
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                }
                TAtomic::TEnumLiteralCase { .. } => {
                    did_remove_type = true;
                    acceptable_types.push(existing_atomic_type);
                }
                _ => {
                    acceptable_types.push(existing_atomic_type);
                }
            },
            TAtomic::TLiteralString {
                value: ref existing_value,
                ..
            } => match assertion_type {
                TAtomic::TLiteralString { value, .. } => {
                    if value == existing_value {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                }
                TAtomic::TEnumLiteralCase { .. } => {
                    did_remove_type = true;
                    acceptable_types.push(existing_atomic_type);
                }
                _ => {
                    acceptable_types.push(existing_atomic_type);
                }
            },
            TAtomic::TEnum {
                name: existing_name,
                ..
            } => match assertion_type {
                TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                    as_type,
                    underlying_type,
                } => {
                    did_remove_type = true;

                    if enum_name == &existing_name {
                        let enum_storage = codebase.classlike_infos.get(enum_name).unwrap();

                        for (cname, _) in &enum_storage.constants {
                            if cname != member_name {
                                acceptable_types.push(TAtomic::TEnumLiteralCase {
                                    enum_name: *enum_name,
                                    member_name: *cname,
                                    as_type: as_type.clone(),
                                    underlying_type: underlying_type.clone(),
                                });
                            }
                        }
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                }
                TAtomic::TLiteralString {
                    value: assertion_value,
                    ..
                } => {
                    let enum_storage = if let Some(s) = codebase.classlike_infos.get(&existing_name)
                    {
                        s
                    } else {
                        return get_nothing();
                    };
                    let mut matched_string = false;

                    let mut member_enum_literals = vec![];
                    for (cname, const_info) in &enum_storage.constants {
                        if let Some(inferred_type) = &const_info.inferred_type {
                            if let TAtomic::TLiteralString {
                                value: const_inferred_value,
                            } = inferred_type
                            {
                                if const_inferred_value != assertion_value {
                                    if let Some(constant_type) = codebase.get_class_constant_type(
                                        &existing_name,
                                        false,
                                        cname,
                                        FxHashSet::default(),
                                    ) {
                                        member_enum_literals.push(constant_type.get_single_owned());
                                    } else {
                                        panic!("unrecognised constant type");
                                    }
                                } else {
                                    matched_string = true;
                                }
                            }
                        }
                    }

                    if !matched_string {
                        acceptable_types.push(existing_atomic_type);
                    } else {
                        acceptable_types.extend(member_enum_literals);
                        did_remove_type = true;
                    }
                }
                TAtomic::TLiteralInt {
                    value: assertion_value,
                    ..
                } => {
                    let enum_storage = codebase.classlike_infos.get(&existing_name).unwrap();
                    let mut matched_string = false;

                    let mut member_enum_literals = vec![];
                    for (cname, const_info) in &enum_storage.constants {
                        if let Some(inferred_type) = &const_info.inferred_type {
                            if let TAtomic::TLiteralInt {
                                value: const_inferred_value,
                            } = inferred_type
                            {
                                if const_inferred_value != assertion_value {
                                    if let Some(constant_type) = codebase.get_class_constant_type(
                                        &existing_name,
                                        false,
                                        cname,
                                        FxHashSet::default(),
                                    ) {
                                        member_enum_literals.push(constant_type.get_single_owned());
                                    } else {
                                        panic!("unrecognised constant type");
                                    }
                                } else {
                                    matched_string = true;
                                }
                            }
                        }
                    }

                    if !matched_string {
                        acceptable_types.push(existing_atomic_type);
                    } else {
                        acceptable_types.extend(member_enum_literals);
                        did_remove_type = true;
                    }
                }
                _ => {
                    acceptable_types.push(existing_atomic_type);
                }
            },
            TAtomic::TEnumLiteralCase {
                enum_name: existing_name,
                member_name: existing_member_name,
                ..
            } => {
                if let TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                    ..
                } = assertion_type
                {
                    if enum_name == &existing_name && member_name == &existing_member_name {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                } else if let TAtomic::TLiteralString { value, .. } = &assertion_type {
                    let enum_storage = codebase.classlike_infos.get(&existing_name).unwrap();

                    let mut matched_string = false;

                    if let Some(const_info) = enum_storage.constants.get(&existing_member_name) {
                        if let Some(const_inferred_type) = &const_info.inferred_type {
                            if let TAtomic::TLiteralString {
                                value: const_inferred_value,
                            } = const_inferred_type
                            {
                                if const_inferred_value == value {
                                    matched_string = true;
                                }
                            }
                        }
                    }

                    if !matched_string {
                        acceptable_types.push(existing_atomic_type);
                    } else {
                        did_remove_type = true;
                    }
                } else {
                    acceptable_types.push(existing_atomic_type);
                }
            }
            TAtomic::TTypeAlias { .. } => {
                did_remove_type = true;
                acceptable_types.push(existing_atomic_type);
            }
            TAtomic::TNamedObject { name, .. } => {
                if name == StrId::XHP_CHILD {
                    did_remove_type = true;
                }

                acceptable_types.push(existing_atomic_type);
            }
            TAtomic::TAwaitable { .. } => {
                acceptable_types.push(existing_atomic_type);
            }
            TAtomic::TTypeVariable { .. } => {
                did_remove_type = true;
                acceptable_types.push(existing_atomic_type);
            }
            _ => {
                acceptable_types.push(existing_atomic_type);
            }
        }
    }

    if let Some(key) = &key {
        if let Some(pos) = pos {
            if !did_remove_type || acceptable_types.is_empty() {
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

    new_var_type.types = acceptable_types;

    new_var_type
}
