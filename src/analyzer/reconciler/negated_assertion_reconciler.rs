use super::{
    reconciler::{trigger_issue_for_impossible, ReconciliationStatus},
    simple_negated_assertion_reconciler,
};
use crate::typed_ast::TastInfo;
use crate::{scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::{
    assertion::Assertion, codebase_info::CodebaseInfo, t_atomic::TAtomic, t_union::TUnion,
};
use hakana_type::{get_nothing, wrap_atomic};
use hakana_type::{
    type_combiner,
    type_comparator::{
        atomic_type_comparator, type_comparison_result::TypeComparisonResult, union_type_comparator,
    },
};
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: Option<String>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    old_var_type_string: String,
    pos: Option<&Pos>,
    failed_reconciliation: &mut ReconciliationStatus,
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
            &key,
            statements_analyzer,
            tast_info,
            old_var_type_string,
            pos,
            negated,
            suppressed_issues,
        );
    }

    let simple_negated_type = simple_negated_assertion_reconciler::reconcile(
        assertion,
        existing_var_type,
        possibly_undefined,
        key.clone(),
        statements_analyzer,
        tast_info,
        pos,
        failed_reconciliation,
        negated,
        suppressed_issues,
    );

    if let Some(simple_negated_type) = simple_negated_type {
        return simple_negated_type;
    }

    let mut existing_var_type = existing_var_type.clone();

    let codebase = statements_analyzer.get_codebase();

    if !is_equality {
        if let Some(assertion_type) = assertion.get_type() {
            subtract_complex_type(assertion_type, codebase, &mut existing_var_type);
        }
    } else if let Some(assertion_type) = assertion.get_type() {
        // todo prevent complaining about $this assertions in traits

        if let Some(key) = &key {
            if let Some(pos) = pos {
                if !union_type_comparator::can_expression_types_be_identical(
                    codebase,
                    &existing_var_type,
                    &wrap_atomic(assertion_type.clone()),
                    true,
                ) {
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

    if existing_var_type.types.is_empty() {
        // todo prevent complaining about $this assertions in traits

        if !is_equality {
            if let Some(key) = &key {
                if let Some(pos) = pos {
                    trigger_issue_for_impossible(
                        tast_info,
                        statements_analyzer,
                        &old_var_type_string,
                        key,
                        assertion,
                        false,
                        negated,
                        pos,
                        suppressed_issues,
                    );
                }
            }

            *failed_reconciliation = ReconciliationStatus::Empty;

            return get_nothing();
        }
    }

    existing_var_type
}

fn subtract_complex_type(
    assertion_type: &TAtomic,
    codebase: &CodebaseInfo,
    existing_var_type: &mut TUnion,
) {
    let mut acceptable_types = vec![];

    let existing_atomic_types = existing_var_type.types.drain(..).collect::<Vec<_>>();

    for existing_atomic in existing_atomic_types {
        if atomic_type_comparator::is_contained_by(
            codebase,
            &existing_atomic,
            assertion_type,
            false,
            &mut TypeComparisonResult::new(),
        ) {
            // don't add as acceptable
            continue;
        }

        if atomic_type_comparator::is_contained_by(
            codebase,
            assertion_type,
            &existing_atomic,
            false,
            &mut TypeComparisonResult::new(),
        ) {
            // todo set is_different property
        }

        match (&existing_atomic, assertion_type) {
            (
                TAtomic::TNamedObject {
                    name: existing_classlike_name,
                    type_params: existing_type_params,
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
                            for child_classlike in child_classlikes {
                                if child_classlike != assertion_classlike_name {
                                    let alternate_class = TAtomic::TNamedObject {
                                        name: child_classlike.clone(),
                                        type_params: if let Some(existing_type_params) =
                                            existing_type_params
                                        {
                                            if let Some(child_classlike_info) =
                                                codebase.classlike_infos.get(child_classlike)
                                            {
                                                // this is hack — ideally we'd map between the two
                                                if child_classlike_info.template_types.len()
                                                    == existing_type_params.len()
                                                {
                                                    Some(existing_type_params.clone())
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        },
                                        extra_types: None,
                                        is_this: false,
                                        remapped_params: false,
                                    };
                                    acceptable_types.push(alternate_class);
                                }
                            }

                            continue;
                        }
                    }
                }

                acceptable_types.push(existing_atomic);
            }
            (TAtomic::TDict { .. }, TAtomic::TDict { .. }) => {
                // todo subtract assertion dict from existing
                acceptable_types.push(existing_atomic);
            }
            _ => {
                acceptable_types.push(existing_atomic);
            }
        }
    }

    if acceptable_types.len() > 1 {
        acceptable_types = type_combiner::combine(acceptable_types, codebase, false);
    }

    existing_var_type.types = acceptable_types;
}

fn handle_literal_negated_equality(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    key: &Option<String>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    old_var_type_string: String,
    pos: Option<&Pos>,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) -> TUnion {
    let assertion_type = assertion.get_type().unwrap();

    let existing_var_type_single = existing_var_type.is_single();

    let mut did_remove_type = false;
    let mut did_match_literal_type = false;

    let mut new_var_type = existing_var_type.clone();

    let existing_var_types = new_var_type.types.drain(..).collect::<Vec<_>>();

    let mut acceptable_types = vec![];

    let codebase = statements_analyzer.get_codebase();

    for existing_atomic_type in existing_var_types {
        match existing_atomic_type {
            TAtomic::TInt { .. } => {
                if let TAtomic::TLiteralInt { .. } = assertion_type {
                    did_remove_type = true;
                }

                acceptable_types.push(existing_atomic_type);
            }
            TAtomic::TLiteralInt {
                value: existing_value,
                ..
            } => {
                if let TAtomic::TLiteralInt { value, .. } = assertion_type {
                    did_match_literal_type = true;
                    if value == &existing_value {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                }
            }
            TAtomic::TString => {
                did_remove_type = true;

                if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    if value == "" {
                        acceptable_types.push(TAtomic::TStringWithFlags(false, true, false));
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                } else {
                    acceptable_types.push(existing_atomic_type);
                }
            }
            TAtomic::TStringWithFlags(_, _, is_nonspecific_literal) => {
                did_remove_type = true;

                if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    if value == "" {
                        acceptable_types.push(TAtomic::TStringWithFlags(
                            false,
                            true,
                            is_nonspecific_literal,
                        ));
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                } else {
                    acceptable_types.push(existing_atomic_type);
                }
            }
            TAtomic::TLiteralString {
                value: ref existing_value,
                ..
            } => {
                if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    did_match_literal_type = true;
                    if &value == &existing_value {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                } else {
                    acceptable_types.push(existing_atomic_type);
                }
            }
            TAtomic::TEnum {
                name: existing_name,
                ..
            } => {
                if let TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                    constraint_type,
                } = assertion_type
                {
                    if enum_name == &existing_name {
                        let enum_storage = codebase.classlike_infos.get(enum_name).unwrap();

                        did_remove_type = true;

                        for (cname, _) in &enum_storage.constants {
                            if cname != member_name {
                                acceptable_types.push(TAtomic::TEnumLiteralCase {
                                    enum_name: enum_name.clone(),
                                    member_name: cname.clone(),
                                    constraint_type: constraint_type.clone(),
                                });
                            }
                        }
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                } else if let TAtomic::TLiteralString {
                    value: assertion_value,
                    ..
                } = assertion_type
                {
                    let enum_storage = codebase.classlike_infos.get(&existing_name).unwrap();
                    let mut matched_string = false;

                    let mut member_enum_literals = vec![];
                    for (cname, const_info) in &enum_storage.constants {
                        if let Some(inferred_type) = &const_info.inferred_type {
                            if let Some(const_inferred_value) =
                                inferred_type.get_single_literal_string_value(&codebase.interner)
                            {
                                if &const_inferred_value != assertion_value {
                                    if let Some(constant_type) = codebase.get_class_constant_type(
                                        &existing_name,
                                        cname,
                                        FxHashSet::default(),
                                    ) {
                                        member_enum_literals.push(constant_type.get_single_owned());
                                    } else {
                                        panic!("unrecognised constant type");
                                    }

                                    did_match_literal_type = true;

                                    did_remove_type = true;
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
                    }
                } else {
                    acceptable_types.push(existing_atomic_type);
                }
            }
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
                    did_match_literal_type = true;

                    if enum_name == &existing_name && member_name == &existing_member_name {
                        did_remove_type = true;
                    } else {
                        acceptable_types.push(existing_atomic_type);
                    }
                } else if let TAtomic::TLiteralString { value, .. } = &assertion_type {
                    let enum_storage = codebase.classlike_infos.get(&existing_name).unwrap();
                    did_match_literal_type = true;

                    let mut matched_string = false;

                    if let Some(const_info) = enum_storage.constants.get(&existing_member_name) {
                        if let Some(const_inferred_type) = &const_info.inferred_type {
                            if let Some(const_inferred_value) = const_inferred_type
                                .get_single_literal_string_value(&codebase.interner)
                            {
                                if &const_inferred_value == value {
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
            _ => {
                acceptable_types.push(existing_atomic_type);
            }
        }
    }

    if let Some(key) = &key {
        if let Some(pos) = pos {
            if did_match_literal_type && (!did_remove_type || existing_var_type_single) {
                trigger_issue_for_impossible(
                    tast_info,
                    statements_analyzer,
                    &old_var_type_string,
                    key,
                    assertion,
                    !did_remove_type,
                    negated,
                    pos,
                    suppressed_issues,
                );
            }
        }
    }

    new_var_type.types = acceptable_types;

    new_var_type
}
