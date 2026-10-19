use hakana_analyzer::config::Config;
use hakana_code_info::{
    analysis_result::{AnalysisResult, Replacement},
    codebase_info::CodebaseInfo,
    functionlike_identifier::FunctionLikeIdentifier,
    functionlike_info::FunctionLikeInfo,
    issue::{Issue, IssueKind},
    method_identifier::MethodIdentifier,
    symbol_references::CallUsage,
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

type Symbol = (StrId, StrId);

pub(crate) fn check(
    result: &mut AnalysisResult,
    config: &Config,
    codebase: &CodebaseInfo,
    interner: &Interner,
    ignored_paths: &Option<FxHashSet<String>>,
) {
    let mut usage: FxHashMap<Symbol, CallUsage> = FxHashMap::default();
    let mut test_usage: FxHashMap<Symbol, CallUsage> = FxHashMap::default();
    for calls in result.symbol_references.non_production_call_usage.values() {
        for (callee, call) in calls {
            test_usage.entry(*callee).or_default().extend(call);
        }
    }
    let mut fix_sources = FxHashMap::default();
    let mut pending_fixes = FxHashMap::default();
    for calls in result.symbol_references.production_call_usage.values() {
        for (callee, call) in calls {
            usage.entry(*callee).or_default().extend(call);
        }
    }

    // A call through a parent/interface can dispatch to an override, and a call
    // on a child can consume a value promised by its parent's signature. Treat
    // the entire override family conservatively as one contract.
    let mut families: FxHashMap<Symbol, FxHashSet<Symbol>> = FxHashMap::default();
    for class in codebase.classlike_infos.values() {
        for (method, parents) in &class.overridden_method_ids {
            let child = codebase.get_declaring_method_id(&MethodIdentifier(class.name, *method));
            let child = (child.0, child.1);
            for parent in parents {
                let parent = codebase.get_declaring_method_id(&MethodIdentifier(*parent, *method));
                let parent = (parent.0, parent.1);
                if child != parent {
                    families.entry(child).or_default().insert(parent);
                    families.entry(parent).or_default().insert(child);
                }
            }
        }
    }
    let mut visited = FxHashSet::default();
    for member in families.keys() {
        if !visited.insert(*member) {
            continue;
        }
        let mut pending = vec![*member];
        let mut members = vec![];
        let mut family_usage = CallUsage::default();
        let mut called = false;
        while let Some(member) = pending.pop() {
            if let Some(call) = usage.get(&member) {
                called = true;
                family_usage.extend(call);
            }
            if codebase
                .functionlike_infos
                .get(&member)
                .is_some_and(|info| {
                    !info.user_defined || info.generated || info.dynamically_callable
                })
            {
                // External/generated contracts can have callers we cannot see.
                family_usage.return_value_used = true;
                family_usage.unknown_args = true;
            }
            members.push(member);
            if let Some(neighbors) = families.get(&member) {
                for neighbor in neighbors {
                    if visited.insert(*neighbor) {
                        pending.push(*neighbor);
                    }
                }
            }
        }
        if called {
            for member in members {
                usage.insert(member, family_usage.clone());
            }
        }
    }

    for (symbol, info) in &codebase.functionlike_infos {
        let Some(call) = usage.get(symbol) else {
            // Never-called and test-only definitions have their own diagnostics.
            continue;
        };
        if !info.user_defined
            || !info.is_production_code
            || info.generated
            || info.dynamically_callable
            || info.is_closure
            || info
                .method_info
                .as_ref()
                .is_some_and(|method| method.is_abstract)
        {
            continue;
        }
        if symbol.1 != StrId::EMPTY
            && ((interner.lookup(&symbol.1).starts_with("__") && symbol.1 != StrId::CONSTRUCT)
                || codebase
                    .classlike_infos
                    .get(&symbol.0)
                    .is_some_and(|class| !class.is_production_code || class.generated))
        {
            continue;
        }
        let Some(pos) = info.name_location else {
            continue;
        };
        let path = interner.lookup(&pos.file_path.0);
        if config.test_files.iter().any(|p| p.matches(path))
            || ignored_paths
                .as_ref()
                .is_some_and(|paths| paths.iter().any(|p| path.contains(p)))
        {
            continue;
        }
        let id = if symbol.1 == StrId::EMPTY {
            FunctionLikeIdentifier::Function(symbol.0)
        } else {
            FunctionLikeIdentifier::Method(symbol.0, symbol.1)
        };
        if !call.return_value_used
            && !info.must_use
            && !info.has_yield
            && info
                .return_type
                .as_ref()
                .is_some_and(|ty| returns_value(ty))
        {
            emit(
                result,
                config,
                info,
                path,
                Issue::new(
                    IssueKind::UnusedReturnValue,
                    format!(
                        "The return value of {} is never used by production callers",
                        id.to_string(interner)
                    ),
                    pos,
                    &Some(id),
                ),
            );
        }
        let mut parameters_to_fix = vec![];
        for (offset, param) in info.params.iter().enumerate() {
            if param.default_type.is_none()
                || !param.is_optional
                || param.is_variadic
                || param.is_splat
                || param.is_inout
                || call.supplies(offset)
                || param.suppressed_issues.as_ref().is_some_and(|issues| {
                    issues
                        .iter()
                        .any(|(kind, _)| *kind == IssueKind::UnusedOptionalParameter)
                })
            {
                continue;
            }
            let parameter_key = format!(
                "{}::{}",
                id.to_string(interner),
                interner.lookup(&param.name.0)
            );
            if config
                .issues_to_fix
                .contains(&IssueKind::UnusedOptionalParameter)
                && config
                    .parameter_fix_allowlist
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(&parameter_key))
            {
                if test_usage
                    .get(symbol)
                    .is_some_and(|usage| usage.supplies(offset))
                {
                    log::debug!(
                        "Optional parameter skipped: {parameter_key}: supplied by non-production callers"
                    );
                } else if info.overriding && !families.contains_key(symbol) {
                    log::debug!(
                        "Optional parameter skipped: {parameter_key}: inherited signature without a complete override family"
                    );
                }
            }
            if !config.add_fixmes
                && config
                    .issues_to_fix
                    .contains(&IssueKind::UnusedOptionalParameter)
                && (!info.overriding || families.contains_key(symbol))
                && !test_usage
                    .get(symbol)
                    .is_some_and(|usage| usage.supplies(offset))
                && !info
                    .suppressed_issues
                    .iter()
                    .any(|(kind, _)| *kind == IssueKind::UnusedOptionalParameter)
                && config.allow_issue_kind_in_file(&IssueKind::UnusedOptionalParameter, path)
                && config
                    .parameter_fix_allowlist
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(&parameter_key))
            {
                parameters_to_fix.push(offset);
            }
            emit(
                result,
                config,
                info,
                path,
                Issue::new(
                    IssueKind::UnusedOptionalParameter,
                    format!(
                        "Optional parameter {} of {} is never supplied by production callers; its default is always used",
                        interner.lookup(&param.name.0),
                        id.to_string(interner),
                    ),
                    param.name_location,
                    &Some(id),
                ),
            );
        }
        if !parameters_to_fix.is_empty() {
            pending_fixes.insert(*symbol, parameters_to_fix);
        }
    }
    // Change an override family atomically, and only at parameter positions that
    // every declaration can safely remove. This also enforces age allowlists
    // across contracts, rather than just at one implementation.
    let mut fixed = FxHashSet::default();
    for (symbol, offsets) in &pending_fixes {
        if !fixed.insert(*symbol) {
            continue;
        }
        let mut pending = vec![*symbol];
        let mut members = vec![];
        while let Some(member) = pending.pop() {
            members.push(member);
            if let Some(neighbors) = families.get(&member) {
                for neighbor in neighbors {
                    if fixed.insert(*neighbor) {
                        pending.push(*neighbor);
                    }
                }
            }
        }
        let offsets = offsets
            .iter()
            .copied()
            .filter(|offset| {
                members.iter().all(|member| {
                    if pending_fixes
                        .get(member)
                        .is_some_and(|fixes| fixes.contains(offset))
                    {
                        return true;
                    }
                    // Test implementations must follow a production contract's
                    // signature. Updating an unused mock parameter preserves its
                    // default locally without deleting any test scenarios.
                    let Some(info) = codebase.functionlike_infos.get(member) else {
                        return false;
                    };
                    // A parent or sibling that never declared this trailing
                    // optional position already has the desired signature.
                    if *offset >= info.params.len() {
                        return !info
                            .params
                            .iter()
                            .any(|param| param.is_variadic || param.is_splat);
                    }
                    let Some(pos) = info.name_location else {
                        return false;
                    };
                    let path = interner.lookup(&pos.file_path.0);
                    let non_production = !info.is_production_code
                        || config
                            .test_files
                            .iter()
                            .any(|pattern| pattern.matches(path))
                        || codebase
                            .classlike_infos
                            .get(&member.0)
                            .is_some_and(|class| !class.is_production_code);
                    non_production
                        && info.user_defined
                        && !info.generated
                        && !info.dynamically_callable
                        && config
                            .allow_issue_kind_in_file(&IssueKind::UnusedOptionalParameter, path)
                        && !codebase
                            .classlike_infos
                            .get(&member.0)
                            .is_some_and(|class| class.generated)
                        && !ignored_paths
                            .as_ref()
                            .is_some_and(|paths| paths.iter().any(|p| path.contains(p)))
                        && !info
                            .suppressed_issues
                            .iter()
                            .any(|(kind, _)| *kind == IssueKind::UnusedOptionalParameter)
                        && !test_usage
                            .get(member)
                            .is_some_and(|usage| usage.supplies(*offset))
                        && !usage
                            .get(member)
                            .is_some_and(|usage| usage.supplies(*offset))
                        && info.params.get(*offset).is_some_and(|param| {
                            param.is_optional
                                && param.default_value_location.is_some()
                                && !param.is_inout
                                && !param.is_variadic
                                && !param.is_splat
                                && !param.suppressed_issues.as_ref().is_some_and(|issues| {
                                    issues.iter().any(|(kind, _)| {
                                        *kind == IssueKind::UnusedOptionalParameter
                                    })
                                })
                        })
                })
            })
            .collect::<Vec<_>>();
        if offsets.is_empty() {
            log::debug!(
                "Optional parameter family skipped: {:?}: not every declaration is eligible",
                members
                    .iter()
                    .map(|m| (interner.lookup(&m.0), interner.lookup(&m.1)))
                    .collect::<Vec<_>>()
            );
            continue;
        }
        let plans = members
            .iter()
            .map(|member| {
                let info = codebase.functionlike_infos.get(member)?;
                let file = info.name_location?.file_path;
                let source = fix_sources
                    .entry(file)
                    .or_insert_with(|| std::fs::read_to_string(interner.lookup(&file.0)).ok());
                let offsets = offsets
                    .iter()
                    .copied()
                    .filter(|offset| *offset < info.params.len())
                    .collect::<Vec<_>>();
                if offsets.is_empty() {
                    return Some((file, vec![]));
                }
                Some((
                    file,
                    crate::optional_parameter_fixer::plan(
                        info,
                        interner,
                        source.as_ref()?,
                        &offsets,
                    )?,
                ))
            })
            .collect::<Option<Vec<_>>>();
        if let Some(plans) = plans {
            for (file, replacements) in plans {
                result
                    .replacements
                    .entry(file)
                    .or_default()
                    .extend(replacements);
            }
        } else {
            log::debug!(
                "Optional parameter family skipped: {:?}: unsupported declaration syntax",
                members
                    .iter()
                    .map(|m| (interner.lookup(&m.0), interner.lookup(&m.1)))
                    .collect::<Vec<_>>()
            );
        }
    }
}

fn returns_value(ty: &TUnion) -> bool {
    !ty.is_mixed()
        && !ty.is_nothing()
        && !ty.types.is_empty()
        && ty.types.iter().all(|atomic| match atomic {
            TAtomic::TVoid => false,
            TAtomic::TAwaitable { value, .. } => returns_value(value),
            _ => true,
        })
}

fn emit(
    result: &mut AnalysisResult,
    config: &Config,
    info: &FunctionLikeInfo,
    path: &str,
    issue: Issue,
) {
    if info
        .suppressed_issues
        .iter()
        .any(|(kind, _)| *kind == issue.kind)
        || !config.allow_issue_kind_in_file(&issue.kind, path)
        || !config.can_add_issue(&issue)
    {
        return;
    }
    if config.add_fixmes && config.issues_to_fix.contains(&issue.kind) {
        // Keep the documentation first, then place the fixme on the declaration.
        // One function-level fixme covers all of its optional parameters.
        let definition_start = info.def_location.start_offset;
        let source = std::fs::read_to_string(path).ok();
        let line_prefix = source
            .as_ref()
            .and_then(|source| source.get(..definition_start as usize))
            .map(|source| source.rsplit('\n').next().unwrap_or(""));
        let indentation_prefix =
            if line_prefix.is_some_and(|prefix| prefix.trim_start().starts_with("*/")) {
                source
                    .as_ref()
                    .and_then(|source| source.get(..definition_start as usize))
                    .and_then(|prefix| {
                        prefix
                            .rfind("/*")
                            .map(|start| prefix[..start].rsplit('\n').next().unwrap_or(""))
                    })
                    .or(line_prefix)
            } else {
                line_prefix
            };
        let indentation = indentation_prefix
            .map(|prefix| {
                prefix
                    .chars()
                    .take_while(|c| matches!(c, ' ' | '\t'))
                    .collect::<String>()
            })
            .unwrap_or_else(|| {
                "\t".repeat(info.def_location.start_column.saturating_sub(1) as usize)
            });
        // A docblock can end on the same line as the declaration. Split that
        // line without treating its comment text as declaration indentation.
        let needs_newline = line_prefix.is_some_and(|prefix| !prefix.trim().is_empty());
        let offset = if needs_newline {
            definition_start
                - line_prefix
                    .map(|prefix| prefix.len() - prefix.trim_end_matches([' ', '\t']).len())
                    .unwrap_or(0) as u32
        } else {
            definition_start
        };
        let marker = format!("HAKANA_FIXME[{}]", issue.kind.to_string());
        let text = format!("/* {} {} */\n{}", marker, issue.description, indentation);
        let replacement = result
            .replacements
            .entry(issue.pos.file_path)
            .or_default()
            .entry((offset, definition_start))
            .or_insert_with(|| Replacement::Substitute(String::new()));
        if let Replacement::Substitute(existing) = replacement
            && !existing.contains(&marker)
        {
            if existing.is_empty() && needs_newline {
                existing.push('\n');
                existing.push_str(&indentation);
            }
            existing.push_str(&text);
        }
    } else {
        *result.issue_counts.entry(issue.kind.clone()).or_default() += 1;
        result
            .emitted_definition_issues
            .entry(issue.pos.file_path)
            .or_default()
            .push(issue);
    }
}
