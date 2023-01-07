use hakana_analyzer::config::Config;
use hakana_reflection_info::analysis_result::{AnalysisResult, Replacement};
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::member_visibility::MemberVisibility;
use hakana_reflection_info::StrId;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) fn find_unused_definitions(
    analysis_result: &mut AnalysisResult,
    config: &Arc<Config>,
    codebase: &CodebaseInfo,
    ignored_paths: &Option<FxHashSet<String>>,
) {
    let referenced_symbols = analysis_result.symbol_references.get_referenced_symbols();
    let referenced_class_members = analysis_result
        .symbol_references
        .get_referenced_class_members();
    let referenced_overridden_class_members = analysis_result
        .symbol_references
        .get_referenced_overridden_class_members();

    'outer1: for (function_name, functionlike_info) in &codebase.functionlike_infos {
        if functionlike_info.user_defined
            && !functionlike_info.dynamically_callable
            && !functionlike_info.generated
        {
            let pos = functionlike_info.name_location.as_ref().unwrap();
            let file_path = codebase.interner.lookup(pos.file_path);

            if let Some(ignored_paths) = ignored_paths {
                for ignored_path in ignored_paths {
                    if file_path.matches(ignored_path.as_str()).count() > 0 {
                        continue 'outer1;
                    }
                }
            }

            if !referenced_symbols.contains(function_name) {
                if let Some(suppressed_issues) = &functionlike_info.suppressed_issues {
                    if suppressed_issues.contains_key(&IssueKind::UnusedFunction) {
                        continue;
                    }
                }

                if !config.allow_issue_kind_in_file(&IssueKind::UnusedFunction, &file_path) {
                    continue;
                }

                if config.migration_symbols.contains(&(
                    "unused_symbol".to_string(),
                    codebase.interner.lookup(*function_name).to_string(),
                )) {
                    let def_pos = &functionlike_info.def_location;
                    analysis_result
                        .replacements
                        .entry(codebase.interner.lookup(pos.file_path).to_string())
                        .or_insert_with(BTreeMap::new)
                        .insert(
                            (def_pos.start_offset, def_pos.end_offset),
                            Replacement::TrimPrecedingWhitespace(
                                (def_pos.start_offset - (def_pos.start_column - 1)) as u64,
                            ),
                        );
                }

                let issue = Issue::new(
                    IssueKind::UnusedFunction,
                    format!(
                        "Unused function {}",
                        codebase.interner.lookup(*function_name)
                    ),
                    pos.clone(),
                );

                if config.can_add_issue(&issue) {
                    analysis_result
                        .emitted_issues
                        .entry(file_path.to_string())
                        .or_insert_with(Vec::new)
                        .push(issue);
                }
            }
        }
    }

    'outer2: for (classlike_name, classlike_info) in &codebase.classlike_infos {
        if classlike_info.user_defined && !classlike_info.generated {
            let pos = &classlike_info.name_location;
            let file_path = codebase.interner.lookup(pos.file_path);

            if let Some(ignored_paths) = ignored_paths {
                for ignored_path in ignored_paths {
                    if file_path.matches(ignored_path.as_str()).count() > 0 {
                        continue 'outer2;
                    }
                }
            }

            if !config.allow_issue_kind_in_file(&IssueKind::UnusedClass, &file_path) {
                continue;
            }

            for parent_class in &classlike_info.all_parent_classes {
                if let Some(parent_classlike_info) = codebase.classlike_infos.get(parent_class) {
                    if !parent_classlike_info.user_defined {
                        continue 'outer2;
                    }
                }
            }

            if !referenced_symbols.contains(classlike_name) {
                let issue = Issue::new(
                    IssueKind::UnusedClass,
                    format!(
                        "Unused class, interface or enum {}",
                        codebase.interner.lookup(*classlike_name)
                    ),
                    pos.clone(),
                );

                if config.migration_symbols.contains(&(
                    "unused_symbol".to_string(),
                    codebase.interner.lookup(*classlike_name).to_string(),
                )) {
                    let def_pos = &classlike_info.def_location;
                    analysis_result
                        .replacements
                        .entry(codebase.interner.lookup(pos.file_path).to_string())
                        .or_insert_with(BTreeMap::new)
                        .insert(
                            (def_pos.start_offset, def_pos.end_offset),
                            Replacement::TrimPrecedingWhitespace(
                                (def_pos.start_offset - (def_pos.start_column - 1)) as u64,
                            ),
                        );
                }

                if config.can_add_issue(&issue) {
                    analysis_result
                        .emitted_issues
                        .entry(file_path.to_string())
                        .or_insert_with(Vec::new)
                        .push(issue);
                }
            } else {
                'inner: for (method_name_ptr, functionlike_storage) in &classlike_info.methods {
                    if *method_name_ptr != StrId::construct() {
                        let method_name = codebase.interner.lookup(*method_name_ptr);

                        if method_name.starts_with("__") {
                            continue;
                        }
                    }

                    let pair = (classlike_name.clone(), *method_name_ptr);

                    if !referenced_class_members.contains(&pair)
                        && !referenced_overridden_class_members.contains(&pair)
                    {
                        if let Some(parent_elements) =
                            classlike_info.overridden_method_ids.get(method_name_ptr)
                        {
                            for parent_element in parent_elements {
                                if referenced_class_members
                                    .contains(&(*parent_element, *method_name_ptr))
                                {
                                    continue 'inner;
                                }
                            }
                        }

                        let method_storage = functionlike_storage.method_info.as_ref().unwrap();

                        if let Some(suppressed_issues) = &functionlike_storage.suppressed_issues {
                            if suppressed_issues.contains_key(&IssueKind::UnusedPrivateMethod) {
                                continue;
                            }
                        }

                        // allow one-liner private construct statements that prevent instantiation
                        if *method_name_ptr == StrId::construct()
                            && matches!(method_storage.visibility, MemberVisibility::Private)
                        {
                            let stmt_pos = &functionlike_storage.def_location;
                            if let Some(name_pos) = &functionlike_storage.name_location {
                                if stmt_pos.end_line - name_pos.start_line <= 1 {
                                    continue;
                                }
                            }
                        }

                        let issue =
                            if matches!(method_storage.visibility, MemberVisibility::Private) {
                                Issue::new(
                                    IssueKind::UnusedPrivateMethod,
                                    format!(
                                        "Unused method {}::{}",
                                        codebase.interner.lookup(*classlike_name),
                                        codebase.interner.lookup(*method_name_ptr)
                                    ),
                                    functionlike_storage.name_location.clone().unwrap(),
                                )
                            } else {
                                Issue::new(
                                    IssueKind::UnusedPublicOrProtectedMethod,
                                    format!(
                                        "Possibly-unused method {}::{}",
                                        codebase.interner.lookup(*classlike_name),
                                        codebase.interner.lookup(*method_name_ptr)
                                    ),
                                    functionlike_storage.name_location.clone().unwrap(),
                                )
                            };

                        let file_path = codebase.interner.lookup(pos.file_path);

                        if !config.allow_issue_kind_in_file(&issue.kind, &file_path) {
                            continue;
                        }

                        if config.can_add_issue(&issue) {
                            analysis_result
                                .emitted_issues
                                .entry(file_path.to_string())
                                .or_insert_with(Vec::new)
                                .push(issue);
                        }
                    }
                }
            }
        }
    }
}
