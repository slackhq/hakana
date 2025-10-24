use crate::{config::Config, scope::CaseScope};
use hakana_code_info::FileSource;
use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::code_location::StmtStart;
use hakana_code_info::ttype::template::TemplateBound;
use hakana_code_info::{
    assertion::Assertion,
    data_flow::graph::{DataFlowGraph, GraphKind, WholeProgramKind},
    functionlike_info::FunctionLikeInfo,
    issue::{Issue, IssueKind, get_issue_from_comment},
    symbol_references::SymbolReferences,
    t_union::TUnion,
};
use hakana_str::StrId;
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
    pub inferred_yield_type: Option<TUnion>,
    pub fully_matched_switch_offsets: FxHashSet<usize>,
    pub closures: FxHashMap<Pos, FunctionLikeInfo>,
    pub closure_spans: Vec<(u32, u32)>,
    pub replacements: BTreeMap<(u32, u32), Replacement>,
    pub insertions: BTreeMap<u32, Vec<String>>,
    pub current_stmt_offset: Option<StmtStart>,
    pub current_stmt_end: Option<u32>,
    pub applicable_fixme_start: u32,
    pub expr_fixme_positions: FxHashMap<(u32, u32), StmtStart>,
    pub symbol_references: SymbolReferences,
    pub issue_filter: Option<FxHashSet<IssueKind>>,
    pub expr_effects: FxHashMap<(u32, u32), u8>,
    pub issue_counts: FxHashMap<IssueKind, usize>,
    pub actual_service_calls: FxHashSet<String>,
    recording_level: usize,
    recorded_issues: Vec<Vec<Issue>>,
    hh_fixmes: BTreeMap<isize, BTreeMap<isize, Pos>>,
    pub hakana_fixme_or_ignores: BTreeMap<u32, Vec<(IssueKind, (u32, u32, u32, u32, bool))>>,
    pub matched_ignore_positions: FxHashSet<(u32, u32)>,
    pub previously_used_fixme_positions: FxHashMap<(u32, u32), (u32, u32)>,
    pub type_variable_bounds: FxHashMap<String, (Vec<TemplateBound>, Vec<TemplateBound>)>,
    pub migrate_function: Option<bool>,
    pub after_expr_hook_called: FxHashSet<(u32, u32)>,
    pub after_arg_hook_called: FxHashSet<(u32, u32)>,
    pub has_await: bool,
    pub await_calls_count: usize,
    pub if_block_boundaries: Vec<(u32, u32)>,
    pub loop_boundaries: Vec<(u32, u32, u32)>,
    pub for_loop_init_boundaries: Vec<(u32, u32)>,
    pub concurrent_block_boundaries: Vec<(u32, u32)>,
    pub definition_locations: FxHashMap<(u32, u32), (StrId, StrId)>,
    pub variable_assignments: FxHashMap<String, FxHashSet<(u32, u32)>>,
}

impl FunctionAnalysisData {
    pub(crate) fn new(
        data_flow_graph: DataFlowGraph,
        file_source: &FileSource,
        comments: &Vec<&(Pos, Comment)>,
        all_custom_issues: &FxHashSet<String>,
        current_stmt_offset: Option<StmtStart>,
        current_stmt_end: Option<u32>,
        applicable_fixme_start: u32,
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
            inferred_yield_type: None,
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
            current_stmt_end,
            applicable_fixme_start,
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
            has_await: false,
            await_calls_count: 0,
            previously_used_fixme_positions: FxHashMap::default(),
            actual_service_calls: FxHashSet::default(),
            if_block_boundaries: Vec::new(),
            loop_boundaries: Vec::new(),
            for_loop_init_boundaries: Vec::new(),
            concurrent_block_boundaries: Vec::new(),
            definition_locations: FxHashMap::default(),
            variable_assignments: FxHashMap::default(),
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

        if issue.insertion_start.is_none() {
            issue.insertion_start = if let Some(expr_fixme_position) = self
                .expr_fixme_positions
                .get(&(issue.pos.start_offset, issue.pos.end_offset))
            {
                Some(*expr_fixme_position)
            } else {
                self.current_stmt_offset
            };
        }

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
        if let Some(insertion_start) = &issue.insertion_start {
            self.add_replacement(
                (insertion_start.offset, insertion_start.offset),
                Replacement::Substitute(
                    format!(
                        "/* HAKANA_FIXME[{}]{} */{}",
                        issue.kind.to_string(),
                        if let IssueKind::UnusedParameter
                        | IssueKind::UnusedAssignment
                        | IssueKind::UnusedInoutAssignment
                        | IssueKind::UnusedAssignmentInClosure
                        | IssueKind::UnusedAssignmentStatement
                        | IssueKind::UnusedStatement
                        | IssueKind::OnlyUsedInTests
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

        if self.covered_by_hh_fixme(
            &issue.kind,
            issue.pos.start_line,
            issue.pos.start_offset,
            issue.pos.end_offset,
        ) || self.covered_by_hh_fixme(
            &issue.kind,
            issue.pos.start_line - 1,
            issue.pos.start_offset,
            issue.pos.end_offset,
        ) {
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

        self.can_output_issue(issue)
    }

    fn can_output_issue(&mut self, issue: &Issue) -> bool {
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
                            && (issue.kind == IssueKind::UnusedAssignmentStatement
                                || issue.kind == IssueKind::UnusedInoutAssignment))
                    {
                        return Some((line_issue.1.0, line_issue.1.1));
                    }
                }
            }
        }

        None
    }

    fn covered_by_hh_fixme(
        &mut self,
        issue_kind: &IssueKind,
        issue_start_line: u32,
        issue_start_offset: u32,
        issue_end_offset: u32,
    ) -> bool {
        if let Some(fixmes) = self.hh_fixmes.get(&(issue_start_line as isize)) {
            for (hack_error, fixme_pos) in fixmes {
                if fixme_pos.start_offset() as u32 > issue_start_offset {
                    continue;
                }
                let fixme_offsets = (
                    fixme_pos.start_offset() as u32,
                    fixme_pos.end_offset() as u32,
                );
                if let Some(offset) = self.previously_used_fixme_positions.get(&fixme_offsets) {
                    if *offset != (issue_start_offset, issue_end_offset) {
                        continue;
                    }
                }

                if matches!(
                    (issue_kind, hack_error),
                    (
                        IssueKind::RedundantKeyCheck | IssueKind::ImpossibleKeyCheck,
                        4249 | 4250
                    )
                ) {
                    return true;
                }

                if hack_error_covers_issue(*hack_error, issue_kind) {
                    self.previously_used_fixme_positions
                        .insert(fixme_offsets, (issue_start_offset, issue_end_offset));
                    return true;
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

            if !self.can_output_issue(&issue) {
                return;
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
                if !self
                    .matched_ignore_positions
                    .contains(&(line_issue.1.0, line_issue.1.1))
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

fn hack_error_covers_issue(hack_error: isize, issue_kind: &IssueKind) -> bool {
    match hack_error {
        // Unify error
        4110 => matches!(
            issue_kind,
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
                | IssueKind::LessSpecificNestedAnyArgumentType
        ),
        // Type inference failed
        4297 => matches!(
            issue_kind,
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
                | IssueKind::MixedReturnStatement
        ),
        // RequiredFieldIsOptional
        4163 => matches!(
            issue_kind,
            IssueKind::InvalidArgument
                | IssueKind::InvalidReturnStatement
                | IssueKind::InvalidReturnType
                | IssueKind::InvalidReturnValue
                | IssueKind::LessSpecificArgument
                | IssueKind::LessSpecificNestedArgumentType
                | IssueKind::LessSpecificNestedReturnStatement
                | IssueKind::LessSpecificReturnStatement
                | IssueKind::PropertyTypeCoercion
                | IssueKind::PossiblyInvalidArgument
        ),
        4324 => matches!(
            issue_kind,
            IssueKind::InvalidArgument | IssueKind::PossiblyInvalidArgument
        ),
        4063 => matches!(
            issue_kind,
            IssueKind::MixedArrayAccess | IssueKind::PossiblyNullArrayAccess
        ),
        4064 => matches!(issue_kind, IssueKind::PossiblyNullPropertyFetch),
        4005 => matches!(issue_kind, IssueKind::MixedArrayAccess),
        2049 => matches!(
            issue_kind,
            IssueKind::NonExistentMethod
                | IssueKind::NonExistentFunction
                | IssueKind::NonExistentClass
        ),
        // Missing member
        4053 => matches!(
            issue_kind,
            IssueKind::NonExistentMethod | IssueKind::NonExistentXhpAttribute
        ),
        // Missing shape field or shape field unknown
        4057 | 4138 => matches!(
            issue_kind,
            IssueKind::InvalidArgument
                | IssueKind::PossiblyInvalidArgument
                | IssueKind::LessSpecificArgument
                | IssueKind::LessSpecificReturnStatement
                | IssueKind::InvalidReturnStatement
        ),
        4062 => matches!(issue_kind, IssueKind::MixedMethodCall),
        4321 | 4108 => matches!(
            issue_kind,
            IssueKind::UndefinedStringArrayOffset
                | IssueKind::UndefinedIntArrayOffset
                | IssueKind::ImpossibleNonnullEntryCheck
        ),
        4165 => matches!(
            issue_kind,
            IssueKind::PossiblyUndefinedStringArrayOffset
                | IssueKind::PossiblyUndefinedIntArrayOffset
        ),
        4107 => matches!(issue_kind, IssueKind::NonExistentFunction),
        4104 => matches!(issue_kind, IssueKind::TooFewArguments),
        4019 => matches!(issue_kind, IssueKind::NonExhaustiveSwitchStatement),
        4489 => matches!(
            issue_kind,
            IssueKind::NonExhaustiveSwitchStatement | IssueKind::NonEnumSwitchValue
        ),
        _ => false,
    }
}

fn get_hakana_fixmes_and_ignores(
    comments: &Vec<&(Pos, Comment)>,
    all_custom_issues: &FxHashSet<String>,
) -> BTreeMap<u32, Vec<(IssueKind, (u32, u32, u32, u32, bool))>> {
    let mut hakana_fixme_or_ignores = BTreeMap::new();
    for (pos, comment) in comments {
        match comment {
            Comment::CmtBlock(text) => {
                let trimmed_text = if let Some(trimmed_text) = text.strip_prefix('*') {
                    trimmed_text.trim()
                } else {
                    text.trim()
                };

                if let Some(Ok(issue_kind)) =
                    get_issue_from_comment(trimmed_text, all_custom_issues)
                {
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
            Comment::CmtLine(_) => {
                // do nothing — if we handle issues here things are much slower
            }
        }
    }
    hakana_fixme_or_ignores
}
