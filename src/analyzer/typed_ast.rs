use crate::{config::Config, scope_context::CaseScope};
use hakana_reflection_info::analysis_result::Replacement;
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
    pub replacements: BTreeMap<(usize, usize), Replacement>,
    pub current_stmt_offset: Option<StmtStart>,
    pub expr_fixme_positions: FxHashMap<(usize, usize), StmtStart>,
    pub symbol_references: SymbolReferences,
    pub issue_filter: Option<FxHashSet<IssueKind>>,
    pub expr_effects: FxHashMap<(usize, usize), u8>,
    pub issue_counts: FxHashMap<IssueKind, usize>,
    recording_level: usize,
    recorded_issues: Vec<Vec<Issue>>,
    hh_fixmes: BTreeMap<isize, BTreeMap<isize, Pos>>,
    pub hakana_fixme_or_ignores: BTreeMap<usize, Vec<(IssueKind, (usize, usize, u64))>>,
    pub matched_ignore_positions: FxHashSet<(usize, usize)>,
}

impl TastInfo {
    pub(crate) fn new(
        data_flow_graph: DataFlowGraph,
        file_source: &FileSource,
        comments: &Vec<&(Pos, Comment)>,
        all_custom_issues: &FxHashSet<String>,
        current_stmt_offset: Option<StmtStart>,
        hakana_fixme_or_ignores: Option<BTreeMap<usize, Vec<(IssueKind, (usize, usize, u64))>>>,
    ) -> Self {
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
            current_stmt_offset,
            hh_fixmes: file_source.hh_fixmes.clone(),
            symbol_references: SymbolReferences::new(),
            issue_filter: None,
            expr_effects: FxHashMap::default(),
            hakana_fixme_or_ignores: hakana_fixme_or_ignores
                .unwrap_or(get_hakana_fixmes_and_ignores(comments, all_custom_issues)),
            expr_fixme_positions: FxHashMap::default(),
            matched_ignore_positions: FxHashSet::default(),
            issue_counts: FxHashMap::default(),
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

        issue.pos.insertion_start = if let Some(expr_fixme_position) = self
            .expr_fixme_positions
            .get(&(issue.pos.start_offset, issue.pos.end_offset))
        {
            Some(*expr_fixme_position)
        } else {
            self.current_stmt_offset
        };

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
                Replacement::Substitute(
                    format!(
                        "/* HAKANA_FIXME[{}] {} */\n{}",
                        issue.kind.to_string(),
                        issue.description,
                        "\t".repeat(insertion_start.2)
                    )
                    .to_string(),
                ),
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

        if self.covered_by_hh_fixme(&issue.kind, issue.pos.start_line)
            || self.covered_by_hh_fixme(&issue.kind, issue.pos.start_line - 1)
        {
            return false;
        }

        for hakana_fixme_or_ignores in &self.hakana_fixme_or_ignores {
            if hakana_fixme_or_ignores.0 == &issue.pos.start_line
                || hakana_fixme_or_ignores.0 == &(issue.pos.start_line - 1)
                || hakana_fixme_or_ignores.0 == &(issue.pos.end_line - 1)
            {
                for line_issue in hakana_fixme_or_ignores.1 {
                    if line_issue.0 == issue.kind {
                        self.matched_ignore_positions
                            .insert((line_issue.1 .0, line_issue.1 .1));

                        if self.recorded_issues.is_empty() {
                            *self.issue_counts.entry(issue.kind.clone()).or_insert(0) += 1;
                        }
                        return false;
                    }
                }
            }
        }

        if let Some(recorded_issues) = self.recorded_issues.last_mut() {
            recorded_issues.push(issue.clone());
            return false;
        }

        *self.issue_counts.entry(issue.kind.clone()).or_insert(0) += 1;

        if let Some(issue_filter) = &self.issue_filter {
            if !issue_filter.contains(&issue.kind) {
                return false;
            }
        }

        return true;
    }

    fn covered_by_hh_fixme(&mut self, issue_kind: &IssueKind, start_line: usize) -> bool {
        if let Some(fixmes) = self.hh_fixmes.get(&(start_line as isize)) {
            for (hack_error, _) in fixmes {
                match *hack_error {
                    // Unify error
                    4110 => match &issue_kind {
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
                        | IssueKind::PossiblyNullArgument
                        | IssueKind::InvalidPropertyAssignmentValue
                        | IssueKind::LessSpecificNestedAnyReturnStatement
                        | IssueKind::LessSpecificNestedAnyArgumentType => {
                            return true;
                        }
                        _ => {}
                    },
                    // type inference failed
                    4297 => match &issue_kind {
                        IssueKind::MixedAnyArgument
                        | IssueKind::MixedAnyArrayAccess
                        | IssueKind::MixedAnyArrayAssignment
                        | IssueKind::MixedAnyArrayOffset
                        | IssueKind::MixedAnyAssignment
                        | IssueKind::MixedAnyMethodCall
                        | IssueKind::MixedAnyPropertyAssignment
                        | IssueKind::MixedAnyPropertyTypeCoercion
                        | IssueKind::MixedAnyReturnStatement
                        | IssueKind::MixedArgument
                        | IssueKind::MixedArrayAccess
                        | IssueKind::MixedArrayAssignment
                        | IssueKind::MixedArrayOffset
                        | IssueKind::MixedMethodCall
                        | IssueKind::MixedPropertyAssignment
                        | IssueKind::MixedPropertyTypeCoercion
                        | IssueKind::MixedReturnStatement => {
                            return true;
                        }
                        _ => {}
                    },
                    // RequiredFieldIsOptional
                    4163 => match &issue_kind {
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
                            return true;
                        }
                        _ => {}
                    },
                    4323 => match &issue_kind {
                        IssueKind::PossiblyNullArgument => {
                            return true;
                        }
                        _ => {}
                    },
                    4063 => match &issue_kind {
                        IssueKind::MixedArrayAccess | IssueKind::PossiblyNullArrayAccess => {
                            return true;
                        }
                        _ => {}
                    },
                    4064 => match &issue_kind {
                        IssueKind::PossiblyNullArgument | IssueKind::PossiblyNullPropertyFetch => {
                            return true;
                        }
                        _ => {}
                    },
                    4005 => match &issue_kind {
                        IssueKind::MixedArrayAccess => {
                            return true;
                        }
                        _ => {}
                    },
                    2049 => match &issue_kind {
                        IssueKind::NonExistentMethod => return true,
                        _ => {}
                    },
                    // missing shape field or shape field unknown
                    4057 | 4138 => match &issue_kind {
                        IssueKind::LessSpecificArgument
                        | IssueKind::LessSpecificReturnStatement
                        | IssueKind::InvalidReturnStatement => return true,
                        _ => {}
                    },
                    4062 => match &issue_kind {
                        IssueKind::MixedMethodCall => return true,
                        _ => {}
                    },
                    4321 | 4108 => match &issue_kind {
                        IssueKind::UndefinedStringArrayOffset => return true,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        false
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

            *self.issue_counts.entry(issue.kind.clone()).or_insert(0) += 1;

            if let Some(issue_filter) = &self.issue_filter {
                if !issue_filter.contains(&issue.kind) {
                    return;
                }
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

    pub(crate) fn get_unused_hakana_fixme_positions(&self) -> Vec<(usize, usize, u64)> {
        let mut unused_fixme_positions = vec![];

        for hakana_fixme_or_ignores in &self.hakana_fixme_or_ignores {
            for line_issue in hakana_fixme_or_ignores.1 {
                if !self
                    .matched_ignore_positions
                    .contains(&(line_issue.1 .0, line_issue.1 .1))
                {
                    unused_fixme_positions.push(line_issue.1);
                }
            }
        }

        unused_fixme_positions
    }
}

fn get_hakana_fixmes_and_ignores(
    comments: &Vec<&(Pos, Comment)>,
    all_custom_issues: &FxHashSet<String>,
) -> BTreeMap<usize, Vec<(IssueKind, (usize, usize, u64))>> {
    let mut hakana_fixme_or_ignores = BTreeMap::new();
    for (pos, comment) in comments {
        match comment {
            Comment::CmtBlock(text) => {
                let trimmed_text = if text.starts_with("*") {
                    text[1..].trim()
                } else {
                    text.trim()
                };

                if let Some(issue_kind) = get_issue_from_comment(trimmed_text, all_custom_issues) {
                    hakana_fixme_or_ignores
                        .entry(pos.line())
                        .or_insert_with(Vec::new)
                        .push((
                            issue_kind,
                            (
                                pos.start_offset(),
                                pos.end_offset(),
                                pos.to_raw_span().start.beg_of_line(),
                            ),
                        ));
                }
            }
            _ => {}
        }
    }
    hakana_fixme_or_ignores
}
