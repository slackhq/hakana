use crate::{config::Config, scope_context::CaseScope};
use hakana_file_info::FileSource;
use hakana_reflection_info::{
    assertion::Assertion,
    data_flow::graph::{DataFlowGraph, GraphKind},
    functionlike_info::FunctionLikeInfo,
    issue::{Issue, IssueKind},
    symbol_references::SymbolReferences,
    t_union::TUnion,
};
use oxidized::ast_defs::Pos;
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc};

pub struct TastInfo {
    pub expr_types: FxHashMap<(usize, usize), Rc<TUnion>>,
    pub if_true_assertions: FxHashMap<(usize, usize), FxHashMap<String, Vec<Assertion>>>,
    pub if_false_assertions: FxHashMap<(usize, usize), FxHashMap<String, Vec<Assertion>>>,
    pub data_flow_graph: DataFlowGraph,
    pub case_scopes: Vec<CaseScope>,
    pub issues_to_emit: Vec<Issue>,
    pub pipe_expr_type: Option<TUnion>,
    pub inferred_return_types: Vec<TUnion>,
    pub fully_matched_switch_offsets: FxHashSet<usize>,
    pub closures: FxHashMap<Pos, FunctionLikeInfo>,
    pub replacements: BTreeMap<(usize, usize), String>,
    pub symbol_references: SymbolReferences,
    pub issue_filter: Option<FxHashSet<IssueKind>>,
    pub pure_exprs: FxHashSet<(usize, usize)>,
    recording_level: usize,
    recorded_issues: Vec<Vec<Issue>>,
    fixmes: BTreeMap<isize, BTreeMap<isize, Pos>>,
}

impl TastInfo {
    pub(crate) fn new(data_flow_graph: DataFlowGraph, file_source: &FileSource) -> Self {
        Self {
            expr_types: FxHashMap::default(),
            data_flow_graph,
            case_scopes: Vec::new(),
            issues_to_emit: Vec::new(),
            pipe_expr_type: None,
            inferred_return_types: Vec::new(),
            fully_matched_switch_offsets: FxHashSet::default(),
            recording_level: 0,
            recorded_issues: vec![],
            closures: FxHashMap::default(),
            if_true_assertions: FxHashMap::default(),
            if_false_assertions: FxHashMap::default(),
            replacements: BTreeMap::new(),
            fixmes: file_source.fixmes.clone(),
            symbol_references: SymbolReferences::new(),
            issue_filter: None,
            pure_exprs: FxHashSet::default(),
        }
    }

    pub fn add_issue(&mut self, issue: Issue) {
        self.issues_to_emit.push(issue);
    }

    pub fn maybe_add_issue(&mut self, issue: Issue, config: &Config) {
        if !config.allow_issue_kind_in_file(&issue.kind, &issue.pos.file_path) {
            return;
        }

        if !self.can_add_issue(&issue) {
            return;
        }

        self.add_issue(issue);
    }

    pub fn can_add_issue(&mut self, issue: &Issue) -> bool {
        if self.data_flow_graph.kind == GraphKind::Taint {
            return matches!(issue.kind, IssueKind::TaintedData(_));
        }

        if let Some(issue_filter) = &self.issue_filter {
            if !issue_filter.contains(&issue.kind) {
                return false;
            }
        }

        if let Some(fixmes) = self.fixmes.get(&(issue.pos.start_line as isize)) {
            for (hack_error, _) in fixmes {
                match *hack_error {
                    4110 => match &issue.kind {
                        IssueKind::FalsableReturnStatement
                        | IssueKind::FalseArgument
                        | IssueKind::ImpossibleAssignment
                        | IssueKind::InvalidArgument
                        | IssueKind::InvalidReturnStatement
                        | IssueKind::InvalidReturnType
                        | IssueKind::InvalidReturnValue
                        | IssueKind::LessSpecificArgument
                        | IssueKind::LessSpecificNestedArgumentType
                        | IssueKind::LessSpecificNestedReturnStatement
                        | IssueKind::LessSpecificReturnStatement
                        | IssueKind::MixedArgument
                        | IssueKind::MixedArrayAccess
                        | IssueKind::MixedArrayAssignment
                        | IssueKind::MixedAnyAssignment
                        | IssueKind::MixedMethodCall
                        | IssueKind::MixedReturnStatement
                        | IssueKind::MixedPropertyAssignment
                        | IssueKind::MixedPropertyTypeCoercion
                        | IssueKind::PropertyTypeCoercion
                        | IssueKind::NonNullableReturnType
                        | IssueKind::NullArgument
                        | IssueKind::NullablePropertyAssignment
                        | IssueKind::NullableReturnStatement
                        | IssueKind::NullableReturnValue
                        | IssueKind::PossiblyFalseArgument
                        | IssueKind::PossiblyInvalidArgument
                        | IssueKind::PossiblyNullArgument => {
                            return false;
                        }
                        _ => {}
                    },
                    4063 => match &issue.kind {
                        IssueKind::MixedArrayAccess => {
                            return false;
                        }
                        _ => {}
                    },
                    4005 => match &issue.kind {
                        IssueKind::MixedArrayAccess => {
                            return false;
                        }
                        _ => {}
                    },
                    2049 => match &issue.kind {
                        IssueKind::NonExistentMethod => return false,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        if let Some(recorded_issues) = self.recorded_issues.last_mut() {
            recorded_issues.push(issue.clone());
            return false;
        }

        return true;
    }

    pub fn start_recording_issues(&mut self) {
        self.recording_level += 1;
        self.recorded_issues.push(vec![]);
    }

    pub fn stop_recording_issues(&mut self) {
        self.recording_level -= 1;
        self.recorded_issues.pop();
    }

    pub fn clear_currently_recorded_issues(&mut self) -> Vec<Issue> {
        let issues = self.recorded_issues.pop().unwrap();
        self.recorded_issues.push(vec![]);
        issues
    }

    pub fn bubble_up_issue(&mut self, issue: Issue) {
        if self.recording_level == 0 {
            self.add_issue(issue);
            return;
        }

        if let Some(issues) = self.recorded_issues.last_mut() {
            issues.push(issue);
        }
    }

    #[inline]
    pub fn set_expr_type(&mut self, pos: &Pos, t: TUnion) {
        self.expr_types
            .insert((pos.start_offset(), pos.end_offset()), Rc::new(t));
    }

    #[inline]
    pub fn get_expr_type(&self, pos: &Pos) -> Option<&TUnion> {
        if let Some(t) = self.expr_types.get(&(pos.start_offset(), pos.end_offset())) {
            Some(&**t)
        } else {
            None
        }
    }

    #[inline]
    pub fn set_rc_expr_type(&mut self, pos: &Pos, t: Rc<TUnion>) {
        self.expr_types
            .insert((pos.start_offset(), pos.end_offset()), t);
    }

    #[inline]
    pub fn get_rc_expr_type(&self, pos: &Pos) -> Option<&Rc<TUnion>> {
        if let Some(t) = self.expr_types.get(&(pos.start_offset(), pos.end_offset())) {
            Some(t)
        } else {
            None
        }
    }
}
