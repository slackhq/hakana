use std::{collections::BTreeMap, time::Duration};

use hakana_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;

use crate::{
    code_location::FilePath,
    data_flow::{
        graph::{DataFlowGraph, GraphKind},
        node::DataFlowNodeId,
    },
    edit::Edit,
    function_context::FunctionLikeIdentifier,
    issue::{Issue, IssueKind},
    symbol_references::SymbolReferences,
};

// Re-export Edit types for convenience
pub use crate::edit::{Edit as CodeEdit, EditKind, EditSet};

/// Legacy Replacement enum for backward compatibility.
/// New code should use Edit directly.
#[derive(Clone, Debug)]
pub enum Replacement {
    Remove,
    TrimPrecedingWhitespace(u32),
    TrimPrecedingWhitespaceAndTrailingComma(u32),
    TrimTrailingWhitespace(u32),
    Substitute(String),
}

impl Replacement {
    /// Convert a Replacement to an Edit at the given offsets
    pub fn to_edit(self, start: u32, end: u32) -> Edit {
        match self {
            Replacement::Remove => Edit::delete(start, end),
            Replacement::TrimPrecedingWhitespace(line_start) => {
                Edit::delete_with_preceding_whitespace(start, end, line_start)
            }
            Replacement::TrimPrecedingWhitespaceAndTrailingComma(line_start) => {
                Edit::delete_with_preceding_whitespace_and_trailing_comma(start, end, line_start)
            }
            Replacement::TrimTrailingWhitespace(line_end) => {
                Edit::delete_with_trailing_whitespace(start, end, line_end)
            }
            Replacement::Substitute(s) => Edit::new(start, end, s),
        }
    }
}

impl From<(u32, u32, Replacement)> for Edit {
    fn from((start, end, replacement): (u32, u32, Replacement)) -> Self {
        replacement.to_edit(start, end)
    }
}

#[derive(Clone, Debug)]
pub struct AnalysisResult {
    pub emitted_issues: FxHashMap<FilePath, Vec<Issue>>,
    pub emitted_definition_issues: FxHashMap<FilePath, Vec<Issue>>,
    pub replacements: FxHashMap<FilePath, BTreeMap<(u32, u32), Replacement>>,
    pub insertions: FxHashMap<FilePath, BTreeMap<u32, Vec<String>>>,
    pub codegen: Vec<(String, Result<String, String>)>,
    pub mixed_source_counts: FxHashMap<DataFlowNodeId, FxHashSet<String>>,
    pub program_dataflow_graph: DataFlowGraph,
    pub symbol_references: SymbolReferences,
    pub issue_counts: FxHashMap<IssueKind, usize>,
    pub time_in_analysis: Duration,
    pub functions_to_migrate: FxHashMap<FunctionLikeIdentifier, bool>,
    pub has_invalid_hack_files: bool,
    pub changed_during_analysis_files: FxHashSet<FilePath>,
    pub definition_locations: FxHashMap<FilePath, FxHashMap<(u32, u32), (StrId, StrId)>>,
}

impl AnalysisResult {
    pub fn new(
        program_dataflow_graph_kind: GraphKind,
        symbol_references: SymbolReferences,
    ) -> Self {
        Self {
            emitted_issues: FxHashMap::default(),
            emitted_definition_issues: FxHashMap::default(),
            replacements: FxHashMap::default(),
            insertions: FxHashMap::default(),
            mixed_source_counts: FxHashMap::default(),
            program_dataflow_graph: DataFlowGraph::new(program_dataflow_graph_kind),
            issue_counts: FxHashMap::default(),
            symbol_references,
            time_in_analysis: Duration::default(),
            functions_to_migrate: FxHashMap::default(),
            codegen: Vec::new(),
            has_invalid_hack_files: false,
            changed_during_analysis_files: FxHashSet::default(),
            definition_locations: FxHashMap::default(),
        }
    }

    pub fn extend(&mut self, other: Self) {
        for (file_path, issues) in other.emitted_issues {
            self.emitted_issues
                .entry(file_path)
                .or_default()
                .extend(issues);
        }
        self.replacements.extend(other.replacements);
        self.insertions.extend(other.insertions);
        for (id, c) in other.mixed_source_counts {
            self.mixed_source_counts.entry(id).or_default().extend(c);
        }
        self.program_dataflow_graph
            .add_graph(other.program_dataflow_graph);
        self.symbol_references.extend(other.symbol_references);
        for (kind, count) in other.issue_counts {
            *self.issue_counts.entry(kind).or_insert(0) += count;
        }
        self.functions_to_migrate.extend(other.functions_to_migrate);
        self.codegen.extend(other.codegen);
        self.changed_during_analysis_files
            .extend(other.changed_during_analysis_files);
        self.has_invalid_hack_files = self.has_invalid_hack_files || other.has_invalid_hack_files;

        // Extend definition locations
        for (file_path, symbol_locations) in other.definition_locations {
            self.definition_locations
                .entry(file_path)
                .or_default()
                .extend(symbol_locations);
        }
    }

    pub fn get_all_issues(
        &self,
        interner: &Interner,
        root_dir: &str,
        use_relative_path: bool,
    ) -> BTreeMap<String, Vec<&Issue>> {
        let mut issues = self
            .emitted_issues
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| {
                (
                    if use_relative_path {
                        k.get_relative_path(interner, root_dir)
                    } else {
                        interner.lookup(&k.0).to_string()
                    },
                    {
                        let mut file_issues = v.iter().collect::<Vec<_>>();
                        file_issues.sort_by(|a, b| a.pos.start_offset.cmp(&b.pos.start_offset));
                        file_issues
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        for (file_path, file_definition_issues) in &self.emitted_definition_issues {
            let file_path = if use_relative_path {
                file_path.get_relative_path(interner, root_dir)
            } else {
                interner.lookup(&file_path.0).to_string()
            };

            if let Some(file_issues) = issues.get_mut(&file_path) {
                file_issues.extend(file_definition_issues);
                file_issues.sort_by(|a, b| a.pos.start_offset.cmp(&b.pos.start_offset));
            } else {
                let mut file_issues: Vec<_> = file_definition_issues.iter().collect::<Vec<_>>();
                file_issues.sort_by(|a, b| a.pos.start_offset.cmp(&b.pos.start_offset));
                issues.insert(file_path, file_issues);
            }
        }

        issues
    }

    /// Get an EditSet for a file by converting replacements and insertions.
    /// This consumes the replacements and insertions for the given file.
    pub fn take_edits_for_file(&mut self, file_path: &FilePath) -> EditSet {
        let mut edit_set = EditSet::new();

        // Convert replacements to edits
        if let Some(replacements) = self.replacements.remove(file_path) {
            for ((start, end), replacement) in replacements {
                edit_set.add(replacement.to_edit(start, end));
            }
        }

        // Merge insertions
        if let Some(insertions) = self.insertions.remove(file_path) {
            edit_set.merge_insertions(insertions);
        }

        edit_set
    }

    /// Check if there are any edits (replacements or insertions) for any file
    pub fn has_edits(&self) -> bool {
        !self.replacements.is_empty() || !self.insertions.is_empty()
    }

    /// Get all file paths that have edits
    pub fn files_with_edits(&self) -> FxHashSet<FilePath> {
        let mut files = self.replacements.keys().copied().collect::<FxHashSet<_>>();
        files.extend(self.insertions.keys().copied());
        files
    }
}

#[derive(Serialize)]
pub struct FullEntry {
    pub kind: String,
    pub description: String,
    pub file_path: String,
    pub start_offset: u32,
    pub start_line: u32,
    pub start_column: u16,
    pub end_offset: u32,
    pub end_line: u32,
    pub end_column: u16,
}

impl FullEntry {
    pub fn from_issue(issue: &Issue, path: &str) -> Self {
        Self {
            kind: issue.kind.to_string(),
            description: issue.description.clone(),
            file_path: path.to_string(),
            start_offset: issue.pos.start_offset,
            start_line: issue.pos.start_line,
            start_column: issue.pos.start_column,
            end_offset: issue.pos.end_offset,
            end_line: issue.pos.end_line,
            end_column: issue.pos.end_column,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckPointEntryLevel {
    Failure,
}

#[derive(Serialize)]
pub struct CheckPointEntry {
    pub case: String,
    pub level: CheckPointEntryLevel,
    pub filename: String,
    pub line: u32,
    pub output: String,
}

impl CheckPointEntry {
    pub fn from_issue(issue: &Issue, path: &str) -> Self {
        Self {
            output: issue.description.clone(),
            level: CheckPointEntryLevel::Failure,
            filename: path.to_string(),
            line: issue.pos.start_line,
            case: issue.kind.to_string(),
        }
    }
}

#[derive(Serialize)]
pub struct HhClientEntry {
    pub descr: String,
    pub path: String,
    pub line: u32,
    pub start: u32,
    pub end: u32,
    pub code: String,
}

impl HhClientEntry {
    pub fn from_issue(issue: &Issue, path: &str) -> Self {
        Self {
            descr: issue.description.clone(),
            path: path.to_string(),
            line: issue.pos.start_line,
            start: issue.pos.start_column as u32,
            end: (issue.pos.end_offset - issue.pos.start_offset) + (issue.pos.start_column as u32),
            code: issue.kind.to_string(),
        }
    }
}
