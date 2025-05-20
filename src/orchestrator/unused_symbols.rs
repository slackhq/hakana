use chrono::{Datelike, Utc};
use hakana_analyzer::config::Config;
use hakana_code_info::analysis_result::{AnalysisResult, Replacement};
use hakana_code_info::classlike_info::ClassLikeInfo;
use hakana_code_info::code_location::{HPos, StmtStart};
use hakana_code_info::codebase_info::symbols::SymbolKind;
use hakana_code_info::codebase_info::{CodebaseInfo, Symbols};
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::member_visibility::MemberVisibility;
use hakana_code_info::property_info::PropertyKind;
use hakana_str::{ReflectionInterner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

use std::sync::Arc;

use crate::file::VirtualFileSystem;

pub(crate) fn find_unused_definitions(
    analysis_result: &mut AnalysisResult,
    config: &Arc<Config>,
    codebase: &mut CodebaseInfo,
    interner: &ReflectionInterner,
    ignored_paths: &Option<FxHashSet<String>>,
    file_system: &mut VirtualFileSystem,
) {
    // don’t show unused definitions if we have any invalid Hack files
    if analysis_result.has_invalid_hack_files {
        for file_path in &analysis_result.changed_during_analysis_files {
            if let Some(file_system_info) = file_system.file_hashes_and_times.get_mut(file_path) {
                // reset the file info so the AST gets recomputed
                *file_system_info = (0, 0);
            }

            if let Some(file_info) = codebase.files.get_mut(file_path) {
                for node in file_info.ast_nodes.iter_mut() {
                    if node.children.is_empty() {
                        node.body_hash = None;
                    } else {
                        for node_child in node.children.iter_mut() {
                            node_child.body_hash = None;
                        }
                    }
                }
            }
        }
        return;
    }

    let has_undefined_symbols = analysis_result
        .issue_counts
        .get(&IssueKind::NonExistentClass)
        .unwrap_or(&0)
        > &0
        || analysis_result
            .issue_counts
            .get(&IssueKind::NonExistentFunction)
            .unwrap_or(&0)
            > &0;

    // don’t show unused definitions if there are undefined symbols
    if has_undefined_symbols {
        return;
    }

    let referenced_symbols_and_members = analysis_result.symbol_references.back_references();
    let mut test_symbols = codebase
        .classlike_infos
        .iter()
        .filter(|(_, c)| c.user_defined && !c.is_production_code)
        .map(|(k, _)| (*k, StrId::EMPTY))
        .collect::<FxHashSet<_>>();
    test_symbols.extend(
        codebase
            .functionlike_infos
            .iter()
            .filter(|(_, c)| c.user_defined && !c.is_production_code)
            .map(|(k, _)| *k),
    );

    let mut referenced_symbols_and_members_in_production = FxHashSet::default();

    for (k, v) in referenced_symbols_and_members.clone().into_iter() {
        if !v.is_subset(&test_symbols) {
            referenced_symbols_and_members_in_production.insert(k);
        }
    }

    if config
        .issues_to_fix
        .contains(&IssueKind::MissingCallsDbAsioJoinAttribute)
        && !config.add_fixmes
    {
        add_calls_db_asio_attributes(analysis_result, codebase, &referenced_symbols_and_members);
    }

    let referenced_symbols_and_members = referenced_symbols_and_members
        .into_keys()
        .collect::<FxHashSet<_>>();

    let referenced_overridden_class_members = analysis_result
        .symbol_references
        .get_referenced_overridden_class_members();

    let mut referenced_overridden_class_members_in_production = FxHashSet::default();

    for (k, v) in referenced_overridden_class_members.clone().into_iter() {
        if !v.is_subset(&test_symbols) {
            referenced_overridden_class_members_in_production.insert(k);
        }
    }

    let referenced_overridden_class_members = referenced_overridden_class_members
        .into_keys()
        .collect::<FxHashSet<_>>();

    'outer1: for (functionlike_name, functionlike_info) in &codebase.functionlike_infos {
        if functionlike_name.1 == StrId::EMPTY
            && functionlike_info.user_defined
            && !functionlike_info.dynamically_callable
            && !functionlike_info.generated
        {
            let pos = functionlike_info.name_location.as_ref().unwrap();
            let file_path = interner.lookup(&pos.file_path.0);

            if let Some(ignored_paths) = ignored_paths {
                for ignored_path in ignored_paths {
                    if file_path.matches(ignored_path.as_str()).count() > 0 {
                        continue 'outer1;
                    }
                }
            }

            if !referenced_symbols_and_members.contains(functionlike_name) {
                if functionlike_info
                    .suppressed_issues
                    .iter()
                    .any(|(i, _)| i == &IssueKind::UnusedFunction)
                {
                    continue;
                }

                if !config.allow_issue_kind_in_file(&IssueKind::UnusedFunction, file_path) {
                    continue;
                }

                let issue = Issue::new(
                    IssueKind::UnusedFunction,
                    format!("Unused function {}", interner.lookup(&functionlike_name.0)),
                    *pos,
                    &Some(FunctionLikeIdentifier::Function(functionlike_name.0)),
                );

                if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
                    let meta_start = &functionlike_info.meta_start;
                    let def_pos = &functionlike_info.def_location;
                    analysis_result
                        .replacements
                        .entry(pos.file_path)
                        .or_default()
                        .insert(
                            (meta_start.start_offset, def_pos.end_offset),
                            Replacement::TrimPrecedingWhitespace(
                                meta_start.start_offset + 1 - meta_start.start_column as u32,
                            ),
                        );
                }

                if config.can_add_issue(&issue) {
                    *analysis_result
                        .issue_counts
                        .entry(issue.kind.clone())
                        .or_insert(0) += 1;
                    analysis_result
                        .emitted_definition_issues
                        .entry(pos.file_path)
                        .or_default()
                        .push(issue);
                }
            } else if functionlike_info.is_production_code
                && !referenced_symbols_and_members_in_production.contains(functionlike_name)
                && config.allow_issue_kind_in_file(&IssueKind::OnlyUsedInTests, file_path)
            {
                let issue = Issue::new(
                    IssueKind::OnlyUsedInTests,
                    format!(
                        "Production-code function {} is only used in tests — if this is deliberate add the <<Hakana\\TestOnly>> attribute",
                        interner.lookup(&functionlike_name.0)
                    ),
                    *pos,
                    &Some(FunctionLikeIdentifier::Function(functionlike_name.0)),
                );

                add_testonly_issue(analysis_result, config, pos, functionlike_info, issue);
            }
        }
    }

    'outer2: for (classlike_name, classlike_info) in &codebase.classlike_infos {
        if classlike_info.user_defined && !classlike_info.generated {
            let pos = &classlike_info.name_location;
            let file_path = interner.lookup(&pos.file_path.0);

            if let Some(ignored_paths) = ignored_paths {
                for ignored_path in ignored_paths {
                    if file_path.matches(ignored_path.as_str()).count() > 0 {
                        continue 'outer2;
                    }
                }
            }

            if !referenced_symbols_and_members.contains(&(*classlike_name, StrId::EMPTY)) {
                if !config.allow_issue_kind_in_file(&IssueKind::UnusedClass, file_path)
                    || classlike_info
                        .suppressed_issues
                        .iter()
                        .any(|(i, _)| i == &IssueKind::UnusedClass)
                {
                    continue;
                }

                let mut issue = Issue::new(
                    IssueKind::UnusedClass,
                    format!(
                        "Unused class, interface or enum {}",
                        interner.lookup(classlike_name),
                    ),
                    *pos,
                    &Some(FunctionLikeIdentifier::Function(*classlike_name)),
                );

                if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
                    let meta_start = &classlike_info.meta_start;
                    let def_pos = &classlike_info.def_location;
                    analysis_result
                        .replacements
                        .entry(pos.file_path)
                        .or_default()
                        .insert(
                            (meta_start.start_offset, def_pos.end_offset),
                            Replacement::TrimPrecedingWhitespace(
                                meta_start.start_offset + 1 - meta_start.start_column as u32,
                            ),
                        );
                }

                issue.insertion_start = Some(StmtStart {
                    offset: classlike_info.def_location.start_offset,
                    line: classlike_info.def_location.start_line,
                    column: classlike_info.def_location.start_column,
                    add_newline: true,
                });

                if config.can_add_issue(&issue) {
                    if config.add_fixmes {
                        analysis_result
                            .replacements
                            .entry(pos.file_path)
                            .or_default()
                            .insert(
                                (
                                    classlike_info.def_location.start_offset,
                                    classlike_info.def_location.start_offset,
                                ),
                                Replacement::Substitute(format!(
                                    "/* HAKANA_FIXME[{}] gen:{} */\n",
                                    issue.kind.to_string(),
                                    Utc::now().format("%y%m%d")
                                )),
                            );
                    } else {
                        *analysis_result
                            .issue_counts
                            .entry(issue.kind.clone())
                            .or_insert(0) += 1;
                        analysis_result
                            .emitted_definition_issues
                            .entry(pos.file_path)
                            .or_default()
                            .push(issue);
                    }
                }
            } else {
                let mut classlike_only_used_in_tests = false;

                if classlike_info.is_production_code
                    && classlike_name != &StrId::HAKANA_TEST_ONLY
                    && !referenced_symbols_and_members_in_production
                        .contains(&(*classlike_name, StrId::EMPTY))
                {
                    classlike_only_used_in_tests = true;

                    if config.allow_issue_kind_in_file(&IssueKind::OnlyUsedInTests, file_path) {
                        let issue = Issue::new(
                            IssueKind::OnlyUsedInTests,
                            format!(
                                "Production-code class {} is only used in tests — if this is deliberate add the <<Hakana\\TestOnly>> attribute",
                                interner.lookup(classlike_name)
                            ),
                            *pos,
                            &Some(FunctionLikeIdentifier::Function(*classlike_name)),
                        );

                        if config.can_add_issue(&issue) {
                            *analysis_result
                                .issue_counts
                                .entry(issue.kind.clone())
                                .or_insert(0) += 1;
                            analysis_result
                                .emitted_definition_issues
                                .entry(pos.file_path)
                                .or_default()
                                .push(issue);
                        }
                    }
                }

                for method_name_ptr in &classlike_info.methods {
                    if *method_name_ptr != StrId::EMPTY {
                        let method_name = interner.lookup(method_name_ptr);

                        if method_name.starts_with("__") {
                            continue;
                        }
                    }

                    let pair = (*classlike_name, *method_name_ptr);

                    if !referenced_symbols_and_members.contains(&pair)
                        && !referenced_overridden_class_members.contains(&pair)
                    {
                        if is_method_referenced_somewhere_else(
                            classlike_name,
                            method_name_ptr,
                            codebase,
                            classlike_info,
                            &referenced_symbols_and_members,
                        ) {
                            continue;
                        }

                        let functionlike_storage = codebase
                            .functionlike_infos
                            .get(&(*classlike_name, *method_name_ptr))
                            .unwrap();

                        let method_storage = functionlike_storage.method_info.as_ref().unwrap();

                        // allow one-liner private construct statements that prevent instantiation
                        if *method_name_ptr == StrId::CONSTRUCT
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
                            if matches!(method_storage.visibility, MemberVisibility::Private)
                                || (matches!(
                                    method_storage.visibility,
                                    MemberVisibility::Protected
                                ) && method_storage.is_final
                                    && !functionlike_storage.overriding)
                            {
                                Issue::new(
                                    IssueKind::UnusedPrivateMethod,
                                    format!(
                                        "Unused method {}::{}",
                                        interner.lookup(classlike_name),
                                        interner.lookup(method_name_ptr)
                                    ),
                                    functionlike_storage.name_location.unwrap(),
                                    &Some(FunctionLikeIdentifier::Method(
                                        *classlike_name,
                                        *method_name_ptr,
                                    )),
                                )
                            } else if functionlike_storage.overriding {
                                Issue::new(
                                    IssueKind::UnusedInheritedMethod,
                                    format!(
                                        "Unused inherited method {}::{}",
                                        interner.lookup(classlike_name),
                                        interner.lookup(method_name_ptr)
                                    ),
                                    functionlike_storage.name_location.unwrap(),
                                    &Some(FunctionLikeIdentifier::Method(
                                        *classlike_name,
                                        *method_name_ptr,
                                    )),
                                )
                            } else {
                                Issue::new(
                                    IssueKind::UnusedPublicOrProtectedMethod,
                                    format!(
                                        "Unused public or protected method {}::{}",
                                        interner.lookup(classlike_name),
                                        interner.lookup(method_name_ptr)
                                    ),
                                    functionlike_storage.name_location.unwrap(),
                                    &Some(FunctionLikeIdentifier::Method(
                                        *classlike_name,
                                        *method_name_ptr,
                                    )),
                                )
                            };

                        if functionlike_storage
                            .suppressed_issues
                            .iter()
                            .any(|(i, _)| i == &issue.kind)
                        {
                            continue;
                        }

                        let file_path = interner.lookup(&pos.file_path.0);

                        if !config.allow_issue_kind_in_file(&issue.kind, file_path) {
                            continue;
                        }

                        if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
                            let meta_start = functionlike_storage.meta_start;
                            let def_pos = functionlike_storage.def_location;
                            analysis_result
                                .replacements
                                .entry(pos.file_path)
                                .or_default()
                                .insert(
                                    (meta_start.start_offset, def_pos.end_offset),
                                    Replacement::TrimPrecedingWhitespace(
                                        meta_start.start_offset + 1
                                            - meta_start.start_column as u32,
                                    ),
                                );
                        } else if config.can_add_issue(&issue) {
                            *analysis_result
                                .issue_counts
                                .entry(issue.kind.clone())
                                .or_insert(0) += 1;
                            analysis_result
                                .emitted_definition_issues
                                .entry(pos.file_path)
                                .or_default()
                                .push(issue);
                        }
                    } else if !classlike_only_used_in_tests
                        && classlike_info.is_production_code
                        && config.allow_issue_kind_in_file(&IssueKind::OnlyUsedInTests, file_path)
                        && !classlike_info
                            .suppressed_issues
                            .iter()
                            .any(|(issue, _)| matches!(issue, IssueKind::OnlyUsedInTests))
                        && !referenced_symbols_and_members_in_production
                            .contains(&(*classlike_name, *method_name_ptr))
                        && !referenced_overridden_class_members_in_production.contains(&pair)
                        && !is_method_referenced_somewhere_else(
                            classlike_name,
                            method_name_ptr,
                            codebase,
                            classlike_info,
                            &referenced_symbols_and_members_in_production,
                        )
                    {
                        let functionlike_storage = codebase
                            .functionlike_infos
                            .get(&(*classlike_name, *method_name_ptr))
                            .unwrap();

                        if functionlike_storage.is_production_code {
                            let issue = Issue::new(
                                IssueKind::OnlyUsedInTests,
                                format!(
                                    "Production-code method {}::{} is only used in tests — if this is deliberate add the <<Hakana\\TestOnly>> attribute",
                                    interner.lookup(classlike_name),
                                    interner.lookup(method_name_ptr)
                                ),
                                functionlike_storage.name_location.unwrap(),
                                &Some(FunctionLikeIdentifier::Method(*classlike_name, *method_name_ptr)),
                            );

                            add_testonly_issue(
                                analysis_result,
                                config,
                                pos,
                                functionlike_storage,
                                issue,
                            );
                        }
                    }
                }

                for (property_name_ptr, property_storage) in &classlike_info.properties {
                    let pair = (*classlike_name, *property_name_ptr);

                    if !referenced_symbols_and_members.contains(&pair)
                        && !referenced_overridden_class_members.contains(&pair)
                    {
                        if let Some(suppressed_issues) = &property_storage.suppressed_issues {
                            if suppressed_issues.contains_key(&IssueKind::UnusedPrivateProperty) {
                                continue;
                            }
                        }

                        let issue =
                            if matches!(property_storage.visibility, MemberVisibility::Private) {
                                Issue::new(
                                    IssueKind::UnusedPrivateProperty,
                                    format!(
                                        "Unused private property {}::${}",
                                        interner.lookup(classlike_name),
                                        interner.lookup(property_name_ptr)
                                    ),
                                    property_storage.pos.unwrap(),
                                    &Some(FunctionLikeIdentifier::Method(
                                        *classlike_name,
                                        *property_name_ptr,
                                    )),
                                )
                            } else if let PropertyKind::XhpAttribute { .. } = property_storage.kind
                            {
                                Issue::new(
                                    IssueKind::UnusedXhpAttribute,
                                    format!(
                                        "Unused XHP attribute {} in class {}",
                                        interner.lookup(property_name_ptr),
                                        interner.lookup(classlike_name),
                                    ),
                                    property_storage.pos.unwrap(),
                                    &Some(FunctionLikeIdentifier::Method(
                                        *classlike_name,
                                        *property_name_ptr,
                                    )),
                                )
                            } else {
                                Issue::new(
                                    IssueKind::UnusedPublicOrProtectedProperty,
                                    format!(
                                        "Unused public or protected property {}::${}",
                                        interner.lookup(classlike_name),
                                        interner.lookup(property_name_ptr)
                                    ),
                                    property_storage.pos.unwrap(),
                                    &Some(FunctionLikeIdentifier::Method(
                                        *classlike_name,
                                        *property_name_ptr,
                                    )),
                                )
                            };

                        let file_path = interner.lookup(&pos.file_path.0);

                        if !config.allow_issue_kind_in_file(&issue.kind, file_path) {
                            continue;
                        }

                        if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
                            if let Some(stmt_pos) = property_storage.stmt_pos {
                                analysis_result
                                    .replacements
                                    .entry(pos.file_path)
                                    .or_default()
                                    .insert(
                                        (stmt_pos.start_offset, stmt_pos.end_offset),
                                        Replacement::TrimPrecedingWhitespaceAndTrailingComma(
                                            stmt_pos.start_offset - stmt_pos.start_column as u32,
                                        ),
                                    );
                            }
                        } else if config.can_add_issue(&issue) {
                            *analysis_result
                                .issue_counts
                                .entry(issue.kind.clone())
                                .or_insert(0) += 1;
                            analysis_result
                                .emitted_definition_issues
                                .entry(pos.file_path)
                                .or_default()
                                .push(issue);
                        }
                    }
                }
            }
        }
    }

    'outer2: for (type_name, type_definition_info) in &codebase.type_definitions {
        if type_definition_info.user_defined && !type_definition_info.generated {
            let pos = &type_definition_info.location;
            let file_path = interner.lookup(&pos.file_path.0);

            if let Some(ignored_paths) = ignored_paths {
                for ignored_path in ignored_paths {
                    if file_path.matches(ignored_path.as_str()).count() > 0 {
                        continue 'outer2;
                    }
                }
            }

            if !config.allow_issue_kind_in_file(&IssueKind::UnusedTypeDefinition, file_path) {
                continue;
            }

            if !referenced_symbols_and_members.contains(&(*type_name, StrId::EMPTY)) {
                let issue = Issue::new(
                    IssueKind::UnusedTypeDefinition,
                    format!("Unused type definition {}", interner.lookup(type_name)),
                    *pos,
                    &Some(FunctionLikeIdentifier::Function(*type_name)),
                );

                if config
                    .issues_to_fix
                    .contains(&IssueKind::UnusedTypeDefinition)
                {
                    analysis_result
                        .replacements
                        .entry(pos.file_path)
                        .or_default()
                        .insert(
                            (pos.start_offset, pos.end_offset),
                            Replacement::TrimPrecedingWhitespace(
                                pos.start_offset - (pos.start_column as u32 - 1),
                            ),
                        );
                }

                if config.can_add_issue(&issue) {
                    *analysis_result
                        .issue_counts
                        .entry(issue.kind.clone())
                        .or_insert(0) += 1;
                    analysis_result
                        .emitted_definition_issues
                        .entry(pos.file_path)
                        .or_default()
                        .push(issue);
                }
            }
        }
    }
}

fn add_calls_db_asio_attributes(
    analysis_result: &mut AnalysisResult,
    codebase: &CodebaseInfo,
    referenced_symbols_and_members: &FxHashMap<(StrId, StrId), FxHashSet<(StrId, StrId)>>,
) {
    let calls_db_asio_join_fns = codebase
        .functionlike_infos
        .iter()
        .filter(|(_, c)| c.user_defined && c.is_production_code && c.calls_db_asio_join)
        .map(|(k, _)| *k)
        .collect::<FxHashSet<_>>();

    let request_handler_fns = codebase
        .functionlike_infos
        .iter()
        .filter(|(_, c)| c.is_request_handler)
        .map(|(k, _)| *k)
        .collect::<FxHashSet<_>>();

    let mut all_calls_db_asio_join_fns = calls_db_asio_join_fns.clone();
    let mut next_new_caller_ids = calls_db_asio_join_fns.into_iter().collect::<Vec<_>>();

    for _ in 0..4 {
        let mut new_caller_ids = next_new_caller_ids;
        next_new_caller_ids = vec![];
        while let Some(new_caller_id) = new_caller_ids.pop() {
            let Some(back_refs) = referenced_symbols_and_members.get(&new_caller_id) else {
                continue;
            };
            let back_refs = back_refs
                .iter()
                .filter(|k| {
                    !all_calls_db_asio_join_fns.contains(&k)
                        && match codebase.functionlike_infos.get(&k) {
                            Some(functionlike_info) => {
                                if functionlike_info.is_production_code
                                    && !functionlike_info.is_request_handler
                                    && !functionlike_info.generated
                                {
                                    if k.1 == StrId::EMPTY {
                                        true
                                    } else {
                                        match codebase.classlike_infos.get(&k.0) {
                                            Some(classlike_info) => {
                                                if let Some(parent_classes) =
                                                    classlike_info.overridden_method_ids.get(&k.1)
                                                {
                                                    !parent_classes.iter().any(|parent_class| {
                                                        request_handler_fns
                                                            .contains(&(*parent_class, k.1))
                                                    })
                                                } else {
                                                    true
                                                }
                                            }
                                            _ => false,
                                        }
                                    }
                                } else {
                                    false
                                }
                            }
                            None => false,
                        }
                })
                .map(|k| *k)
                .collect::<FxHashSet<_>>();
            next_new_caller_ids.extend(back_refs.clone());
            all_calls_db_asio_join_fns.extend(back_refs);
        }
    }

    for k in all_calls_db_asio_join_fns {
        if let Some(functionlike_info) = codebase.functionlike_infos.get(&k) {
            if !functionlike_info.calls_db_asio_join {
                let def_pos = functionlike_info.def_location;
                analysis_result
                    .replacements
                    .entry(def_pos.file_path)
                    .or_default()
                    .insert(
                        (def_pos.start_offset, def_pos.start_offset),
                        Replacement::Substitute(format!(
                            "<<\\Hakana\\CallsDbAsioJoin>>\n{}",
                            &"\t".repeat((def_pos.start_column as usize) - 1)
                        )),
                    );
            }
        }
    }
}

fn add_testonly_issue(
    analysis_result: &mut AnalysisResult,
    config: &Config,
    pos: &HPos,
    functionlike_storage: &FunctionLikeInfo,
    issue: Issue,
) {
    if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
        let now = Utc::now();
        let def_pos = functionlike_storage.def_location;
        analysis_result
            .replacements
            .entry(pos.file_path)
            .or_default()
            .insert(
                (def_pos.start_offset, def_pos.start_offset),
                Replacement::Substitute(format!(
                    "<<\\Hakana\\TestOnly{}>>\n{}",
                    if config.add_date_comments {
                        format!(
                            "('Added automatically on {}-{}-{}')",
                            now.year(),
                            now.month(),
                            now.day(),
                        )
                    } else {
                        "".to_string()
                    },
                    &"\t".repeat((def_pos.start_column as usize) - 1)
                )),
            );
    } else if config.can_add_issue(&issue) {
        *analysis_result
            .issue_counts
            .entry(issue.kind.clone())
            .or_insert(0) += 1;
        analysis_result
            .emitted_definition_issues
            .entry(pos.file_path)
            .or_default()
            .push(issue);
    }
}

fn is_method_referenced_somewhere_else(
    classlike_name: &StrId,
    method_name_ptr: &StrId,
    codebase: &CodebaseInfo,
    classlike_info: &ClassLikeInfo,
    referenced_symbols_and_members: &FxHashSet<(StrId, StrId)>,
) -> bool {
    if has_upstream_method_call(
        classlike_info,
        method_name_ptr,
        referenced_symbols_and_members,
        codebase,
    ) {
        return true;
    }
    for descendant_classlike in codebase.get_all_descendants(classlike_name) {
        if let Some(descendant_classlike_storage) =
            codebase.classlike_infos.get(&descendant_classlike)
        {
            for parent_interface in &descendant_classlike_storage.all_parent_interfaces {
                if referenced_symbols_and_members.contains(&(*parent_interface, *method_name_ptr)) {
                    return true;
                }
            }
        }
    }

    for trait_user in get_trait_users(
        classlike_name,
        &codebase.symbols,
        &codebase.all_classlike_descendants,
    ) {
        if let Some(trait_user_classlike_info) = codebase.classlike_infos.get(&trait_user) {
            if has_upstream_method_call(
                trait_user_classlike_info,
                method_name_ptr,
                referenced_symbols_and_members,
                codebase,
            ) {
                return true;
            }
        }
    }

    false
}

fn has_upstream_method_call(
    classlike_info: &ClassLikeInfo,
    method_name_ptr: &StrId,
    referenced_class_members: &FxHashSet<(StrId, StrId)>,
    codebase: &CodebaseInfo,
) -> bool {
    if let Some(parent_elements) = classlike_info.overridden_method_ids.get(method_name_ptr) {
        for parent_element in parent_elements {
            if let Some(ClassLikeInfo {
                user_defined: false,
                ..
            }) = codebase.classlike_infos.get(parent_element)
            {
                return true;
            }

            if referenced_class_members.contains(&(*parent_element, *method_name_ptr)) {
                return true;
            }
        }
    }

    false
}

fn get_trait_users(
    classlike_name: &StrId,
    symbols: &Symbols,
    all_classlike_descendants: &FxHashMap<StrId, FxHashSet<StrId>>,
) -> FxHashSet<StrId> {
    let mut base_set = FxHashSet::default();

    if let Some(SymbolKind::Trait) = symbols.all.get(classlike_name) {
        if let Some(trait_users) = all_classlike_descendants.get(classlike_name) {
            base_set.extend(trait_users);
            for classlike_descendant in trait_users {
                base_set.extend(get_trait_users(
                    classlike_descendant,
                    symbols,
                    all_classlike_descendants,
                ));
            }
        }
    }

    base_set
}
