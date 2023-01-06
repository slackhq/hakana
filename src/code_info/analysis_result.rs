use std::collections::BTreeMap;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;

use crate::{
    data_flow::graph::{DataFlowGraph, GraphKind},
    issue::{Issue, IssueKind},
    symbol_references::SymbolReferences,
};

#[derive(Clone, Debug)]
pub enum Replacement {
    Remove,
    TrimPrecedingWhitespace(u64),
    Substitute(String),
}

#[derive(Clone, Debug)]
pub struct AnalysisResult {
    pub emitted_issues: BTreeMap<String, Vec<Issue>>,
    pub replacements: FxHashMap<String, BTreeMap<(usize, usize), Replacement>>,
    pub mixed_source_counts: FxHashMap<String, FxHashSet<String>>,
    pub program_dataflow_graph: DataFlowGraph,
    pub symbol_references: SymbolReferences,
    pub issue_counts: FxHashMap<IssueKind, usize>,
}

impl AnalysisResult {
    pub fn new(
        program_dataflow_graph_kind: GraphKind,
        symbol_references: SymbolReferences,
    ) -> Self {
        Self {
            emitted_issues: BTreeMap::new(),
            replacements: FxHashMap::default(),
            mixed_source_counts: FxHashMap::default(),
            program_dataflow_graph: DataFlowGraph::new(program_dataflow_graph_kind),
            issue_counts: FxHashMap::default(),
            symbol_references,
        }
    }

    pub fn extend(&mut self, other: Self) {
        for (file_path, issues) in other.emitted_issues {
            self.emitted_issues
                .entry(file_path)
                .or_insert_with(Vec::new)
                .extend(issues);
        }
        self.replacements.extend(other.replacements);
        for (id, c) in other.mixed_source_counts {
            self.mixed_source_counts
                .entry(id)
                .or_insert_with(FxHashSet::default)
                .extend(c);
        }
        self.program_dataflow_graph
            .add_graph(other.program_dataflow_graph);
        self.symbol_references.extend(other.symbol_references);
        for (kind, count) in other.issue_counts {
            *self.issue_counts.entry(kind).or_insert(0) += count;
        }
    }
}

#[derive(Serialize)]
pub struct CheckPointEntry {
    pub case: String,
    pub level: String,
    pub filename: String,
    pub line: usize,
    pub output: String,
}

impl CheckPointEntry {
    pub fn from_issue(issue: &Issue, path: &String) -> Self {
        Self {
            output: issue.description.clone(),
            level: "failure".to_string(),
            filename: path.clone(),
            line: issue.pos.start_line,
            case: issue.kind.to_string(),
        }
    }
}
