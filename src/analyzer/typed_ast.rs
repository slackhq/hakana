use crate::{config::Config, scope_context::CaseScope};
use hakana_reflection_info::code_location::StmtStart;
use hakana_reflection_info::FileSource;
use hakana_reflection_info::{
    assertion::Assertion,
    data_flow::graph::{DataFlowGraph, GraphKind, WholeProgramKind},
    functionlike_info::FunctionLikeInfo,
    issue::{get_issue_from_comment, Issue, IssueKind},
    symbol_references::SymbolReferences,
    t_union::TUnion,
};
use oxidized::{ast_defs::Pos, prim_defs::Comment};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc};

pub(crate) const PURE: u8 = 0b00000000;
pub(crate) const READ_PROPS: u8 = 0b00000001;
pub(crate) const READ_GLOBALS: u8 = 0b00000010;
pub(crate) const WRITE_PROPS: u8 = 0b00000100;
pub(crate) const WRITE_GLOBALS: u8 = 0b0001000;
pub(crate) const IMPURE: u8 = READ_PROPS | READ_GLOBALS | WRITE_PROPS | WRITE_GLOBALS;

pub struct TastInfo {
    pub expr_types: FxHashMap<(usize, usize), Rc<TUnion>>,
    pub if_true_assertions: FxHashMap<(usize, usize), FxHashMap<String, Vec<Assertion>>>,
    pub if_false_assertions: FxHashMap<(usize, usize), FxHashMap<String, Vec<Assertion>>>,
    pub data_flow_graph: DataFlowGraph,
    pub case_scopes: Vec<CaseScope>,
    pub issues_to_emit: Vec<Issue>,
    pub inferred_return_types: Vec<TUnion>,
    pub fully_matched_switch_offsets: FxHashSet<usize>,
    pub closures: FxHashMap<Pos, FunctionLikeInfo>,
    pub closure_spans: Vec<(usize, usize)>,
    pub replacements: BTreeMap<(usize, usize), String>,
    pub current_stmt_offset: Option<StmtStart>,
    pub symbol_references: SymbolReferences,
    pub issue_filter: Option<FxHashSet<IssueKind>>,
    pub expr_effects: FxHashMap<(usize, usize), u8>,
    recording_level: usize,
    recorded_issues: Vec<Vec<Issue>>,
    hh_fixmes: BTreeMap<isize, BTreeMap<isize, Pos>>,
    hakana_ignores: BTreeMap<usize, Vec<IssueKind>>,
}

impl TastInfo {
    pub(crate) fn new(
        data_flow_graph: DataFlowGraph,
        file_source: &FileSource,
        comments: &Vec<&(Pos, Comment)>,
        all_custom_issues: &FxHashSet<String>,
    ) -> Self {
        let mut hakana_ignores = BTreeMap::new();
        for (pos, comment) in comments {
            match comment {
                Comment::CmtBlock(text) => {
                    let trimmed_text = if text.starts_with("*") {
                        text[1..].trim()
                    } else {
                        text.trim()
                    };

                    if let Some(issue_kind) =
                        get_issue_from_comment(trimmed_text, all_custom_issues)
                    {
                        hakana_ignores
                            .entry(pos.line())
                            .or_insert_with(Vec::new)
                            .push(issue_kind);
                    }
                }
                _ => {}
            }
        }

        Self {
            expr_types: FxHashMap::default(),
            data_flow_graph,
            case_scopes: Vec::new(),
            issues_to_emit: Vec::new(),
            inferred_return_types: Vec::new(),
            fully_matched_switch_offsets: FxHashSet::default(),
            recording_level: 0,
            recorded_issues: vec![],
            closures: FxHashMap::default(),
            closure_spans: vec![],
            if_true_assertions: FxHashMap::default(),
            if_false_assertions: FxHashMap::default(),
            replacements: BTreeMap::new(),
            current_stmt_offset: None,
            hh_fixmes: file_source.hh_fixmes.clone(),
            symbol_references: SymbolReferences::new(),
            issue_filter: None,
            expr_effects: FxHashMap::default(),
            hakana_ignores,
        }
    }

    pub fn add_issue(&mut self, issue: Issue) {
        if !self.issues_to_emit.contains(&issue) {
            self.issues_to_emit.push(issue);
        }
    }

    pub fn maybe_add_issue(&mut self, mut issue: Issue, config: &Config, file_path: &str) {
        if config.ignore_mixed_issues && issue.kind.is_mixed_issue() {
            return;
        }

        if !config.allow_issue_kind_in_file(&issue.kind, file_path) {
            return;
        }

        issue.pos.insertion_start = self.current_stmt_offset;

        issue.can_fix = config.add_fixmes && config.issues_to_fix.contains(&issue.kind);

        if !self.can_add_issue(&issue) {
            return;
        }

        if issue.can_fix {
            self.fix_issue(&issue);
        }

        self.add_issue(issue);
    }

    fn fix_issue(&mut self, issue: &Issue) {
        if let Some(insertion_start) = &issue.pos.insertion_start {
            self.replacements.insert(
                (insertion_start.0, insertion_start.0),
                format!(
                    "/* HAKANA_FIXME[{}] {} */\n{}",
                    issue.kind.to_string(),
                    issue.description,
                    "\t".repeat(insertion_start.2)
                )
                .to_string(),
            );
        }
    }

    pub fn can_add_issue(&mut self, issue: &Issue) -> bool {
        if matches!(
            &self.data_flow_graph.kind,
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        ) {
            return matches!(issue.kind, IssueKind::TaintedData(_));
        }

        if let Some(issue_filter) = &self.issue_filter {
            if !issue_filter.contains(&issue.kind) {
                return false;
            }
        }

        if let Some(fixmes) = self.hh_fixmes.get(&(issue.pos.start_line as isize)) {
            for (hack_error, _) in fixmes {
                match *hack_error {
                    // Unify error
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
                    // RequiredFieldIsOptional
                    4163 => match &issue.kind {
                        IssueKind::InvalidArgument
                        | IssueKind::InvalidReturnStatement
                        | IssueKind::InvalidReturnType
                        | IssueKind::InvalidReturnValue
                        | IssueKind::LessSpecificArgument
                        | IssueKind::LessSpecificNestedArgumentType
                        | IssueKind::LessSpecificNestedReturnStatement
                        | IssueKind::LessSpecificReturnStatement
                        | IssueKind::PropertyTypeCoercion
                        | IssueKind::PossiblyInvalidArgument => {
                            return false;
                        }
                        _ => {}
                    },
                    4323 => match &issue.kind {
                        IssueKind::PossiblyNullArgument => {
                            return false;
                        }
                        _ => {}
                    },
                    4063 => match &issue.kind {
                        IssueKind::MixedArrayAccess | IssueKind::PossiblyNullArrayAccess => {
                            return false;
                        }
                        _ => {}
                    },
                    4064 => match &issue.kind {
                        IssueKind::PossiblyNullArgument | IssueKind::PossiblyNullPropertyFetch => {
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
                    // missing shape field or shape field unknown
                    4057 | 4138 => match &issue.kind {
                        IssueKind::LessSpecificArgument
                        | IssueKind::LessSpecificReturnStatement
                        | IssueKind::InvalidReturnStatement => return false,
                        _ => {}
                    },
                    4062 => match &issue.kind {
                        IssueKind::MixedMethodCall => return false,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        for ignored_issues in &self.hakana_ignores {
            if ignored_issues.0 == &issue.pos.start_line
                || ignored_issues.0 == &(issue.pos.start_line - 1)
                || ignored_issues.0 == &(issue.pos.end_line - 1)
            {
                if ignored_issues.1.contains(&issue.kind) {
                    return false;
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
            if issue.can_fix {
                self.fix_issue(&issue);
            }

            self.add_issue(issue);
            return;
        }

        if let Some(issues) = self.recorded_issues.last_mut() {
            issues.push(issue);
        }
    }

    pub(crate) fn copy_effects(&mut self, source_pos_1: &Pos, destination_pos: &Pos) {
        self.expr_effects.insert(
            (destination_pos.start_offset(), destination_pos.end_offset()),
            *self
                .expr_effects
                .get(&(source_pos_1.start_offset(), source_pos_1.end_offset()))
                .unwrap_or(&0),
        );
    }

    pub(crate) fn combine_effects(
        &mut self,
        source_pos_1: &Pos,
        source_pos_2: &Pos,
        destination_pos: &Pos,
    ) {
        self.expr_effects.insert(
            (destination_pos.start_offset(), destination_pos.end_offset()),
            self.expr_effects
                .get(&(source_pos_1.start_offset(), source_pos_1.end_offset()))
                .unwrap_or(&0)
                | self
                    .expr_effects
                    .get(&(source_pos_2.start_offset(), source_pos_2.end_offset()))
                    .unwrap_or(&0),
        );
    }

    pub(crate) fn combine_effects_with(
        &mut self,
        source_pos_1: &Pos,
        source_pos_2: &Pos,
        destination_pos: &Pos,
        effect: u8,
    ) {
        self.expr_effects.insert(
            (destination_pos.start_offset(), destination_pos.end_offset()),
            self.expr_effects
                .get(&(source_pos_1.start_offset(), source_pos_1.end_offset()))
                .unwrap_or(&0)
                | self
                    .expr_effects
                    .get(&(source_pos_2.start_offset(), source_pos_2.end_offset()))
                    .unwrap_or(&0)
                | effect,
        );
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
