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
use hakana_type::template::TemplateBound;
use oxidized::{ast_defs::Pos, prim_defs::Comment};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc};

pub struct FunctionAnalysisData {
    pub expr_types: FxHashMap<(u32, u32), Rc<TUnion>>,
    pub if_true_assertions: FxHashMap<(u32, u32), FxHashMap<String, Vec<Assertion>>>,
    pub if_false_assertions: FxHashMap<(u32, u32), FxHashMap<String, Vec<Assertion>>>,
    pub data_flow_graph: DataFlowGraph,
    pub case_scopes: Vec<CaseScope>,
    pub issues_to_emit: Vec<Issue>,
    pub inferred_return_types: Vec<TUnion>,
    pub fully_matched_switch_offsets: FxHashSet<usize>,
    pub closures: FxHashMap<Pos, FunctionLikeInfo>,
    pub closure_spans: Vec<(u32, u32)>,
    pub replacements: BTreeMap<(u32, u32), Replacement>,
    pub insertions: BTreeMap<u32, Vec<String>>,
    pub current_stmt_offset: Option<StmtStart>,
    pub expr_fixme_positions: FxHashMap<(u32, u32), StmtStart>,
    pub symbol_references: SymbolReferences,
    pub issue_filter: Option<FxHashSet<IssueKind>>,
    pub expr_effects: FxHashMap<(u32, u32), u8>,
    pub issue_counts: FxHashMap<IssueKind, usize>,
    recording_level: usize,
    recorded_issues: Vec<Vec<Issue>>,
    hh_fixmes: BTreeMap<isize, BTreeMap<isize, Pos>>,
    pub hakana_fixme_or_ignores: BTreeMap<u32, Vec<(IssueKind, (u32, u32, u32, u32, bool))>>,
    pub matched_ignore_positions: FxHashSet<(u32, u32)>,
    pub type_variable_bounds: FxHashMap<String, (Vec<TemplateBound>, Vec<TemplateBound>)>,
    pub migrate_function: Option<bool>,
    pub after_expr_hook_called: FxHashSet<(u32, u32)>,
    pub after_arg_hook_called: FxHashSet<(u32, u32)>,
}

impl FunctionAnalysisData {
    pub(crate) fn new(
        data_flow_graph: DataFlowGraph,
        file_source: &FileSource,
        comments: &Vec<&(Pos, Comment)>,
        all_custom_issues: &FxHashSet<String>,
        current_stmt_offset: Option<StmtStart>,
        hakana_fixme_or_ignores: Option<
            BTreeMap<u32, Vec<(IssueKind, (u32, u32, u32, u32, bool))>>,
        >,
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
            insertions: BTreeMap::new(),
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
            type_variable_bounds: FxHashMap::default(),
            migrate_function: None,
            after_arg_hook_called: FxHashSet::default(),
            after_expr_hook_called: FxHashSet::default(),
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

        if issue.can_fix && !issue.fixme_added {
            issue.fixme_added = self.add_issue_fixme(&issue);
        }

        self.add_issue(issue);
    }

    fn add_issue_fixme(&mut self, issue: &Issue) -> bool {
        if let Some(insertion_start) = &issue.pos.insertion_start {
            self.add_replacement(
                (insertion_start.offset, insertion_start.offset),
                Replacement::Substitute(
                    format!(
                        "/* HAKANA_FIXME[{}]{} */{}",
                        issue.kind.to_string(),
                        if let IssueKind::UnusedParameter
                        | IssueKind::UnusedAssignment
                        | IssueKind::UnusedAssignmentInClosure
                        | IssueKind::UnusedAssignmentStatement
                        | IssueKind::UnusedStatement
                        | IssueKind::UnusedFunction
                        | IssueKind::UnusedPrivateMethod = issue.kind
                        {
                            "".to_string()
                        } else {
                            " ".to_string() + &issue.description
                        },
                        if insertion_start.add_newline {
                            "\n".to_string() + &"\t".repeat(insertion_start.column as usize)
                        } else {
                            " ".to_string()
                        }
                    )
                    .to_string(),
                ),
            );

            true
        } else {
            false
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

        if let Some(start_end) = self.get_matching_hakana_fixme(issue) {
            self.matched_ignore_positions.insert(start_end);

            if self.recorded_issues.is_empty() {
                *self.issue_counts.entry(issue.kind.clone()).or_insert(0) += 1;
            }

            return false;
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

        true
    }

    pub(crate) fn get_matching_hakana_fixme(&self, issue: &Issue) -> Option<(u32, u32)> {
        for hakana_fixme_or_ignores in &self.hakana_fixme_or_ignores {
            if hakana_fixme_or_ignores.0 == &issue.pos.start_line
                || hakana_fixme_or_ignores.0 == &(issue.pos.start_line - 1)
                || hakana_fixme_or_ignores.0 == &(issue.pos.end_line - 1)
            {
                for line_issue in hakana_fixme_or_ignores.1 {
                    if line_issue.0 == issue.kind
                        || (line_issue.0 == IssueKind::UnusedAssignment
                            && issue.kind == IssueKind::UnusedAssignmentStatement)
                    {
                        return Some((line_issue.1 .0, line_issue.1 .1));
                    }
                }
            }
        }

        None
    }

    fn covered_by_hh_fixme(&mut self, issue_kind: &IssueKind, start_line: u32) -> bool {
        if let Some(fixmes) = self.hh_fixmes.get(&(start_line as isize)) {
            for hack_error in fixmes.keys() {
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
                        | IssueKind::NullablePropertyAssignment
                        | IssueKind::NullableReturnStatement
                        | IssueKind::NullableReturnValue
                        | IssueKind::PossiblyFalseArgument
                        | IssueKind::PossiblyInvalidArgument
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
                    4063 => match &issue_kind {
                        IssueKind::MixedArrayAccess | IssueKind::PossiblyNullArrayAccess => {
                            return true;
                        }
                        _ => {}
                    },
                    4064 => {
                        if let IssueKind::PossiblyNullPropertyFetch = &issue_kind {
                            return true;
                        }
                    }
                    4005 => {
                        if let IssueKind::MixedArrayAccess = &issue_kind {
                            return true;
                        }
                    }
                    2049 => match &issue_kind {
                        IssueKind::NonExistentMethod => return true,
                        IssueKind::NonExistentClass => return true,
                        _ => {}
                    },
                    // missing member
                    4053 => match &issue_kind {
                        IssueKind::NonExistentMethod | IssueKind::NonExistentXhpAttribute => {
                            return true
                        }
                        _ => {}
                    },
                    // missing shape field or shape field unknown
                    4057 | 4138 => match &issue_kind {
                        IssueKind::LessSpecificArgument
                        | IssueKind::LessSpecificReturnStatement
                        | IssueKind::InvalidReturnStatement => return true,
                        _ => {}
                    },
                    4062 => {
                        if let IssueKind::MixedMethodCall = &issue_kind {
                            return true;
                        }
                    }
                    4321 | 4108 => match &issue_kind {
                        IssueKind::UndefinedStringArrayOffset
                        | IssueKind::UndefinedIntArrayOffset
                        | IssueKind::ImpossibleNonnullEntryCheck => return true,
                        _ => {}
                    },
                    4165 => match &issue_kind {
                        IssueKind::PossiblyUndefinedStringArrayOffset
                        | IssueKind::PossiblyUndefinedIntArrayOffset => return true,
                        _ => {}
                    },
                    4249 | 4250 => match &issue_kind {
                        IssueKind::RedundantKeyCheck | IssueKind::ImpossibleKeyCheck => {
                            return true
                        }
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
                self.add_issue_fixme(&issue);
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
            (
                destination_pos.start_offset() as u32,
                destination_pos.end_offset() as u32,
            ),
            *self
                .expr_effects
                .get(&(
                    source_pos_1.start_offset() as u32,
                    source_pos_1.end_offset() as u32,
                ))
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
            (
                destination_pos.start_offset() as u32,
                destination_pos.end_offset() as u32,
            ),
            self.expr_effects
                .get(&(
                    source_pos_1.start_offset() as u32,
                    source_pos_1.end_offset() as u32,
                ))
                .unwrap_or(&0)
                | self
                    .expr_effects
                    .get(&(
                        source_pos_2.start_offset() as u32,
                        source_pos_2.end_offset() as u32,
                    ))
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
            (
                destination_pos.start_offset() as u32,
                destination_pos.end_offset() as u32,
            ),
            self.expr_effects
                .get(&(
                    source_pos_1.start_offset() as u32,
                    source_pos_1.end_offset() as u32,
                ))
                .unwrap_or(&0)
                | self
                    .expr_effects
                    .get(&(
                        source_pos_2.start_offset() as u32,
                        source_pos_2.end_offset() as u32,
                    ))
                    .unwrap_or(&0)
                | effect,
        );
    }

    pub(crate) fn is_pure(&self, source_pos: &Pos) -> bool {
        if let Some(expr_effect) = self.expr_effects.get(&(
            source_pos.start_offset() as u32,
            source_pos.end_offset() as u32,
        )) {
            expr_effect == &0
        } else {
            true
        }
    }

    #[inline]
    pub fn set_expr_type(&mut self, pos: &Pos, t: TUnion) {
        self.expr_types.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            Rc::new(t),
        );
    }

    #[inline]
    pub fn get_expr_type(&self, pos: &Pos) -> Option<&TUnion> {
        if let Some(t) = self
            .expr_types
            .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
        {
            Some(&**t)
        } else {
            None
        }
    }

    #[inline]
    pub fn set_rc_expr_type(&mut self, pos: &Pos, t: Rc<TUnion>) {
        self.expr_types
            .insert((pos.start_offset() as u32, pos.end_offset() as u32), t);
    }

    #[inline]
    pub fn get_rc_expr_type(&self, pos: &Pos) -> Option<&Rc<TUnion>> {
        if let Some(t) = self
            .expr_types
            .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
        {
            Some(t)
        } else {
            None
        }
    }

    pub(crate) fn get_unused_hakana_fixme_positions(&self) -> Vec<(u32, u32, u32, u32, bool)> {
        let mut unused_fixme_positions = vec![];

        for hakana_fixme_or_ignores in &self.hakana_fixme_or_ignores {
            for line_issue in hakana_fixme_or_ignores.1 {
                if line_issue.0 == IssueKind::NoJoinInAsyncFunction {
                    continue;
                }
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

    pub fn add_replacement(&mut self, offsets: (u32, u32), replacement: Replacement) -> bool {
        let offsets = (offsets.0, offsets.1);
        for (start, end) in self.replacements.keys() {
            if (offsets.0 >= *start && offsets.0 <= *end)
                || (offsets.1 >= *start && offsets.1 <= *end)
            {
                return false;
            }

            if (*start >= offsets.0 && *start <= offsets.1)
                || (*end >= offsets.0 && *end <= offsets.1)
            {
                return false;
            }
        }

        self.replacements.insert(offsets, replacement);
        true
    }

    pub fn insert_at(&mut self, insertion_point: u32, replacement: String) {
        self.insertions
            .entry(insertion_point)
            .or_default()
            .push(replacement);
    }
}

fn get_hakana_fixmes_and_ignores(
    comments: &Vec<&(Pos, Comment)>,
    all_custom_issues: &FxHashSet<String>,
) -> BTreeMap<u32, Vec<(IssueKind, (u32, u32, u32, u32, bool))>> {
    let mut hakana_fixme_or_ignores = BTreeMap::new();
    for (pos, comment) in comments {
        if let Comment::CmtBlock(text) = comment {
            let trimmed_text = if let Some(trimmed_text) = text.strip_prefix('*') {
                trimmed_text.trim()
            } else {
                text.trim()
            };

            if let Some(Ok(issue_kind)) = get_issue_from_comment(trimmed_text, all_custom_issues) {
                hakana_fixme_or_ignores
                    .entry(pos.line() as u32)
                    .or_insert_with(Vec::new)
                    .push((
                        issue_kind,
                        (
                            pos.start_offset() as u32,
                            pos.end_offset() as u32,
                            pos.to_raw_span().start.beg_of_line() as u32,
                            pos.end_offset() as u32,
                            false,
                        ),
                    ));
            }
        }
    }
    hakana_fixme_or_ignores
}
