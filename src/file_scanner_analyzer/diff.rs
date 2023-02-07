use hakana_analyzer::config::{Config, Verbosity};
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::diff::CodebaseDiff;
use hakana_reflection_info::issue::Issue;
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::Interner;
use hakana_reflection_info::StrId;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;

use crate::cache::load_cached_existing_issues;
use crate::cache::load_cached_existing_references;
use crate::get_relative_path;

#[derive(Default)]
pub(crate) struct CachedAnalysis {
    pub safe_symbols: FxHashSet<StrId>,
    pub safe_symbol_members: FxHashSet<(StrId, StrId)>,
    pub existing_issues: BTreeMap<String, Vec<Issue>>,
    pub symbol_references: SymbolReferences,
}

pub(crate) fn mark_safe_symbols_from_diff(
    references_path: &Option<String>,
    verbosity: Verbosity,
    codebase_diff: CodebaseDiff,
    codebase: &CodebaseInfo,
    interner: &mut Interner,
    files_to_analyze: &mut Vec<String>,
    config: &Config,
    issues_path: &Option<String>,
) -> Option<CachedAnalysis> {
    if let Some(existing_references) =
        load_cached_existing_references(references_path.as_ref().unwrap(), true, verbosity)
    {
        let (invalid_symbols, invalid_symbol_members, partially_invalid_symbols) =
            existing_references.get_invalid_symbols(&codebase_diff);

        let mut cached_analysis = CachedAnalysis::default();

        cached_analysis.symbol_references = existing_references;

        for keep_symbol in &codebase_diff.keep {
            if !keep_symbol.1.is_empty() {
                if !invalid_symbols.contains(&keep_symbol.0)
                    && !invalid_symbol_members.contains(&(keep_symbol.0, keep_symbol.1))
                {
                    cached_analysis
                        .safe_symbol_members
                        .insert((keep_symbol.0, keep_symbol.1));
                }
            } else {
                if !invalid_symbols.contains(&keep_symbol.0) {
                    cached_analysis.safe_symbols.insert(keep_symbol.0);
                }
            }
        }

        cached_analysis
            .symbol_references
            .remove_references_from_invalid_symbols(&invalid_symbols, &invalid_symbol_members);

        let invalid_files = codebase
            .files
            .iter()
            .filter(|(_, file_info)| {
                file_info.ast_nodes.iter().any(|node| {
                    invalid_symbols.contains(&node.name)
                        || partially_invalid_symbols.contains(&node.name)
                })
            })
            .map(|(file_id, _)| interner.lookup(file_id).to_string())
            .collect::<FxHashSet<_>>();

        files_to_analyze.retain(|full_path| {
            invalid_files.contains(&get_relative_path(full_path, &config.root_dir))
        });

        if let Some(existing_issues_path) = issues_path {
            if let Some(mut existing_issues) =
                load_cached_existing_issues(existing_issues_path, true, verbosity)
            {
                update_issues_from_diff(
                    &mut existing_issues,
                    interner,
                    codebase_diff,
                    &invalid_symbols,
                    &invalid_symbol_members,
                );
                cached_analysis.existing_issues = existing_issues;
            }
        }

        Some(cached_analysis)
    } else {
        None
    }
}

fn update_issues_from_diff(
    existing_issues: &mut BTreeMap<String, Vec<Issue>>,
    interner: &mut Interner,
    codebase_diff: CodebaseDiff,
    invalid_symbols: &FxHashSet<StrId>,
    invalid_symbol_members: &FxHashSet<(StrId, StrId)>,
) {
    for (existing_file, file_issues) in existing_issues.iter_mut() {
        let file_id = &interner.intern(existing_file.clone());

        file_issues.retain(|issue| {
            !invalid_symbols.contains(&issue.symbol.0)
                && !invalid_symbol_members.contains(&issue.symbol)
                && &issue.symbol.0 != file_id
        });

        if file_issues.is_empty() {
            continue;
        }

        let diff_map = codebase_diff
            .diff_map
            .get(file_id)
            .cloned()
            .unwrap_or(vec![]);

        let deletion_ranges = codebase_diff
            .deletion_ranges_map
            .get(file_id)
            .cloned()
            .unwrap_or(vec![]);

        if !deletion_ranges.is_empty() {
            file_issues.retain(|issue| {
                for (from, to) in &deletion_ranges {
                    if &issue.pos.start_offset >= from && &issue.pos.start_offset <= to {
                        return false;
                    }
                }

                return true;
            });
        }

        if !diff_map.is_empty() {
            for issue in file_issues {
                for (from, to, file_offset, line_offset) in &diff_map {
                    if &issue.pos.start_offset >= from && &issue.pos.start_offset <= to {
                        issue.pos.start_offset =
                            ((issue.pos.start_offset as isize) + file_offset) as usize;
                        issue.pos.end_offset =
                            ((issue.pos.end_offset as isize) + file_offset) as usize;
                        issue.pos.start_line =
                            ((issue.pos.start_line as isize) + line_offset) as usize;
                        issue.pos.end_line = ((issue.pos.end_line as isize) + line_offset) as usize;
                        break;
                    }
                }
            }
        }
    }
}
