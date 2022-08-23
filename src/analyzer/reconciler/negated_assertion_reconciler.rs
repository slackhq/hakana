use super::{
    reconciler::{trigger_issue_for_impossible, ReconciliationStatus},
    simple_negated_assertion_reconciler,
};
use crate::typed_ast::TastInfo;
use crate::{scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::{
    assertion::Assertion, codebase_info::CodebaseInfo, t_atomic::TAtomic, t_union::TUnion,
};
use hakana_type::type_comparator::{
    atomic_type_comparator, type_comparison_result::TypeComparisonResult, union_type_comparator,
};
use hakana_type::{get_nothing, wrap_atomic};
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub(crate) fn reconcile(
    assertion: &Assertion,
    existing_var_type: &TUnion,
    possibly_undefined: bool,
    key: &Option<String>,
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
            key,
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

    let existing_atomic_types = &existing_var_type.types;
    let mut existing_var_type = existing_var_type.clone();

    let codebase = statements_analyzer.get_codebase();

    if !is_equality {
        if let Some(assertion_type) = assertion.get_type() {
            let assertion_type_key = assertion_type.get_key();
            if !existing_var_type.is_single()
                || !existing_var_type.has_named_object()
                || !existing_var_type.types.contains_key(&assertion_type_key)
            {
                if let None = existing_var_type.types.remove(&assertion_type_key) {
                    handle_named_object_subtraction(
                        assertion_type,
                        existing_atomic_types,
                        codebase,
                        &mut existing_var_type,
                    );
                }
            }
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

fn handle_named_object_subtraction(
    assertion_type: &TAtomic,
    existing_atomic_types: &std::collections::BTreeMap<String, TAtomic>,
    codebase: &CodebaseInfo,
    existing_var_type: &mut TUnion,
) {
    if let TAtomic::TNamedObject {
        name: assertion_object_name,
        ..
    } = assertion_type
    {
        for (type_key, atomic) in existing_atomic_types {
            if !atomic.is_object_type() {
                continue;
            }

            if let TAtomic::TNamedObject {
                name: existing_name,
                ..
            } = atomic
            {
                if let Some(classlike_storage) = codebase.classlike_infos.get(existing_name) {
                    if let Some(child_classlikes) = &classlike_storage.child_classlikes {
                        if child_classlikes.contains(assertion_object_name) {
                            existing_var_type.types.remove(type_key);

                            for child_classlike in child_classlikes {
                                if child_classlike != assertion_object_name {
                                    let result_error = TAtomic::TNamedObject {
                                        name: child_classlike.clone(),
                                        type_params: None,
                                        extra_types: None,
                                        is_this: false,
                                        remapped_params: false,
                                    };
                                    existing_var_type
                                        .types
                                        .insert(result_error.get_key(), result_error);
                                }
                            }

                            continue;
                        }
                    }
                }
            }

            if atomic_type_comparator::is_contained_by(
                codebase,
                atomic,
                assertion_type,
                false,
                &mut TypeComparisonResult::new(),
            ) {
                existing_var_type.types.remove(type_key);
            } else if atomic_type_comparator::is_contained_by(
                codebase,
                assertion_type,
                atomic,
                false,
                &mut TypeComparisonResult::new(),
            ) {
                // todo set is_different property
            }
        }
    }
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

    let mut existing_var_type = existing_var_type.clone();

    let codebase = statements_analyzer.get_codebase();

    for (k, existing_atomic_type) in &existing_var_type.types.clone() {
        match existing_atomic_type {
            TAtomic::TInt { .. } => {
                if let TAtomic::TLiteralInt { .. } = assertion_type {
                    did_remove_type = true;
                }
            }
            TAtomic::TLiteralInt {
                value: existing_value,
                ..
            } => {
                if let TAtomic::TLiteralInt { value, .. } = assertion_type {
                    did_match_literal_type = true;
                    if value == existing_value {
                        did_remove_type = true;
                        existing_var_type.types.remove(k);
                    }
                }
            }
            TAtomic::TString => {
                did_remove_type = true;

                if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    if value == "" {
                        existing_var_type.types.remove(k);
                        existing_var_type.add_type(TAtomic::TStringWithFlags(false, true, false));
                    }
                }
            }
            TAtomic::TStringWithFlags(_, _, is_nonspecific_literal) => {
                did_remove_type = true;

                if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    if value == "" {
                        existing_var_type.types.remove(k);
                        existing_var_type.add_type(TAtomic::TStringWithFlags(
                            false,
                            true,
                            *is_nonspecific_literal,
                        ));
                    }
                }
            }
            TAtomic::TLiteralString {
                value: existing_value,
                ..
            } => {
                if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    did_match_literal_type = true;
                    if value == existing_value {
                        did_remove_type = true;
                        existing_var_type.types.remove(k);
                    }
                }
            }
            TAtomic::TEnum {
                name: existing_name,
                ..
            } => {
                if let TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                } = assertion_type
                {
                    if enum_name == existing_name {
                        let enum_storage = codebase.classlike_infos.get(enum_name).unwrap();

                        did_remove_type = true;

                        existing_var_type.types.remove(k);

                        for (cname, _) in &enum_storage.constants {
                            if cname != member_name {
                                if let Some(constant_type) = codebase.get_class_constant_type(
                                    existing_name,
                                    cname,
                                    FxHashSet::default(),
                                ) {
                                    existing_var_type.add_type(constant_type.get_single_owned());
                                }
                            }
                        }
                    }
                } else if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    let enum_storage = codebase.classlike_infos.get(existing_name).unwrap();
                    for (cname, const_info) in &enum_storage.constants {
                        if let Some(inferred_type) = &const_info.inferred_type {
                            if let Some(inferred_value) =
                                inferred_type.get_single_literal_string_value()
                            {
                                if &inferred_value != value {
                                    if let Some(constant_type) = codebase.get_class_constant_type(
                                        existing_name,
                                        cname,
                                        FxHashSet::default(),
                                    ) {
                                        existing_var_type
                                            .add_type(constant_type.get_single_owned());
                                    }

                                    did_match_literal_type = true;

                                    did_remove_type = true;

                                    existing_var_type.types.remove(k);
                                }
                            }
                        }
                    }
                }
            }
            TAtomic::TEnumLiteralCase {
                enum_name: existing_name,
                member_name: existing_member_name,
            } => {
                if let TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                } = assertion_type
                {
                    did_match_literal_type = true;

                    if enum_name == existing_name && member_name == existing_member_name {
                        did_remove_type = true;
                        existing_var_type.types.remove(k);
                    }
                } else if let TAtomic::TLiteralString { value, .. } = assertion_type {
                    let enum_storage = codebase.classlike_infos.get(existing_name).unwrap();
                    did_match_literal_type = true;

                    for (cname, const_info) in &enum_storage.constants {
                        if let Some(inferred_type) = &const_info.inferred_type {
                            if let Some(inferred_value) =
                                inferred_type.get_single_literal_string_value()
                            {
                                if &inferred_value != value {
                                    if let Some(constant_type) = codebase.get_class_constant_type(
                                        existing_name,
                                        cname,
                                        FxHashSet::default(),
                                    ) {
                                        existing_var_type
                                            .add_type(constant_type.get_single_owned());
                                    }

                                    did_remove_type = true;

                                    existing_var_type.types.remove(k);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(key) = key {
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

    existing_var_type
}
