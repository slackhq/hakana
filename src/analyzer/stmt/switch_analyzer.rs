use hakana_reflection_info::{
    codebase_info::CodebaseInfo,
    issue::{Issue, IssueKind},
};
use hakana_type::{combine_optional_union_types, combine_union_types, get_mixed_any};
use indexmap::IndexMap;
use ocamlrep::rc::RcOc;
use oxidized::{
    aast,
    aast::Pos,
    ast_defs::{self, ParamKind},
    file_pos::FilePos,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc};

use crate::{
    algebra_analyzer,
    expr::expression_identifier,
    expression_analyzer, formula_generator,
    reconciler::reconciler,
    scope_analyzer::ScopeAnalyzer,
    scope_context::{
        control_action::ControlAction, loop_scope::LoopScope, switch_scope::SwitchScope, CaseScope,
        ScopeContext,
    },
    statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};

use super::{
    control_analyzer::{self, BreakContext},
    if_conditional_analyzer::add_branch_dataflow,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &Vec<aast::Case<(), ()>>,
        &Option<aast::DefaultCase<(), ()>>,
    ),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) {
    let codebase = statements_analyzer.get_codebase();

    context.inside_conditional = true;

    if !expression_analyzer::analyze(statements_analyzer, stmt.0, tast_info, context, &mut None) {
        context.inside_conditional = false;
        return;
    }

    context.inside_conditional = false;

    add_branch_dataflow(statements_analyzer, &stmt.0, tast_info);

    let switch_var_id = if let Some(switch_var_id) = expression_identifier::get_extended_var_id(
        stmt.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
    ) {
        switch_var_id
    } else {
        let switch_var_id = format!("$-tmp_switch-{}", stmt.0 .1.start_offset());

        context.vars_in_scope.insert(
            switch_var_id.clone(),
            Rc::new(
                tast_info
                    .get_expr_type(&stmt.0 .1)
                    .cloned()
                    .unwrap_or(get_mixed_any()),
            ),
        );
        switch_var_id
    };

    let original_context = context.clone();

    let mut last_case_exit_type = ControlAction::Break;

    let mut case_exit_types = FxHashMap::default();

    let has_default = stmt.2.is_some();

    let mut case_action_map = FxHashMap::default();

    let mut cases = stmt
        .1
        .iter()
        .enumerate()
        .map(|(k, c)| (k, c))
        .collect::<IndexMap<_, _>>();
    cases.reverse();

    if let Some(default_case) = stmt.2 {
        update_case_exit_map(
            codebase,
            &default_case.1,
            tast_info,
            statements_analyzer.get_file_analyzer().resolved_names,
            &mut case_action_map,
            cases.len(),
            &mut last_case_exit_type,
            &mut case_exit_types,
        );
    }

    for (i, case) in &cases {
        update_case_exit_map(
            codebase,
            &case.1,
            tast_info,
            statements_analyzer.get_file_analyzer().resolved_names,
            &mut case_action_map,
            *i,
            &mut last_case_exit_type,
            &mut case_exit_types,
        );
    }

    let mut switch_scope = SwitchScope::new();

    let mut all_options_returned = true;

    cases.reverse();

    let mut condition_is_fake = false;

    let fake_switch_condition = if switch_var_id.starts_with("$-tmp_switch-") {
        condition_is_fake = true;

        Some(aast::Expr(
            (),
            stmt.0 .1.clone(),
            aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
                stmt.0 .1.clone(),
                (0, switch_var_id.clone()),
            ))),
        ))
    } else {
        None
    };

    let mut previous_empty_cases = vec![];

    for (i, case) in &cases {
        let case_exit_type = case_exit_types.get(&i).unwrap();

        if case_exit_type != &ControlAction::Return {
            all_options_returned = false;
        }

        let case_actions = case_action_map.get(&i).unwrap();

        if case.1.is_empty() {
            previous_empty_cases.push(*case);
            continue;
        }

        if !analyze_case(
            statements_analyzer,
            stmt,
            fake_switch_condition.as_ref().unwrap_or(stmt.0),
            condition_is_fake,
            &switch_var_id,
            Some(&case.0),
            &case.0.pos(),
            &case.1,
            &previous_empty_cases,
            tast_info,
            context,
            &original_context,
            case_exit_type,
            case_actions,
            *i == (cases.len() - 1) && stmt.2.is_none(),
            &mut switch_scope,
            loop_scope,
        ) {
            return;
        }

        previous_empty_cases = vec![];
    }

    if let Some(default_case) = stmt.2 {
        let i = cases.len();

        let case_exit_type = case_exit_types.get(&i).unwrap();

        if case_exit_type != &ControlAction::Return {
            all_options_returned = false;
        }

        let case_actions = case_action_map.get(&i).unwrap();

        if !analyze_case(
            statements_analyzer,
            stmt,
            fake_switch_condition.as_ref().unwrap_or(stmt.0),
            condition_is_fake,
            &switch_var_id,
            None,
            &default_case.0,
            &default_case.1,
            &previous_empty_cases,
            tast_info,
            context,
            &original_context,
            case_exit_type,
            case_actions,
            true,
            &mut switch_scope,
            loop_scope,
        ) {
            return;
        }
    }

    let mut possibly_redefined_vars = switch_scope
        .possibly_redefined_vars
        .unwrap_or(BTreeMap::new());
    if let Some(new_vars_in_scope) = switch_scope.new_vars_in_scope {
        possibly_redefined_vars.retain(|k, _| !new_vars_in_scope.contains_key(k));
        context.vars_in_scope.extend(new_vars_in_scope);
    }

    if let Some(redefined_vars) = &switch_scope.redefined_vars {
        possibly_redefined_vars.retain(|k, _| !redefined_vars.contains_key(k));
        context.vars_in_scope.extend(redefined_vars.clone());
    }

    for (var_id, var_type) in possibly_redefined_vars {
        if let Some(context_type) = context.vars_in_scope.get(&var_id).cloned() {
            context.vars_in_scope.insert(
                var_id.clone(),
                Rc::new(combine_union_types(
                    &var_type,
                    &context_type,
                    Some(codebase),
                    false,
                )),
            );
        }
    }

    tast_info
        .fully_matched_switch_offsets
        .insert(pos.start_offset());

    context
        .assigned_var_ids
        .extend(switch_scope.new_assigned_var_ids);
    context.has_returned = all_options_returned && has_default;
}

// here we desugar case statements to corresponding if ... else statements

fn analyze_case(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &Vec<aast::Case<(), ()>>,
        &Option<aast::DefaultCase<(), ()>>,
    ),
    switch_condition: &aast::Expr<(), ()>,
    condition_is_fake: bool,
    switch_var_id: &String,
    case_cond: Option<&aast::Expr<(), ()>>,
    case_pos: &Pos,
    case_stmts: &Vec<aast::Stmt<(), ()>>,
    previous_empty_cases: &Vec<&aast::Case<(), ()>>,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    original_context: &ScopeContext,
    case_exit_type: &ControlAction,
    case_actions: &FxHashSet<ControlAction>,
    is_last: bool,
    switch_scope: &mut SwitchScope,
    loop_scope: &mut Option<LoopScope>,
) -> bool {
    let has_ending_statements =
        case_actions.len() == 1 && case_actions.contains(&ControlAction::End);
    let has_leaving_statements = has_ending_statements
        || (case_actions.len() > 0 && !case_actions.contains(&ControlAction::None));

    let mut case_context = original_context.clone();

    let mut old_node_data = tast_info.expr_types.clone();

    let mut case_equality_expr = None;

    if let Some(case_cond) = case_cond {
        if !expression_analyzer::analyze(
            statements_analyzer,
            case_cond,
            tast_info,
            context,
            &mut None,
        ) {
            return false;
        }

        tast_info.expr_types = tast_info.expr_types.clone();

        if condition_is_fake {
            tast_info.set_expr_type(
                &switch_condition.pos(),
                if let Some(t) = context.vars_in_scope.get(switch_var_id) {
                    (**t).clone()
                } else {
                    get_mixed_any()
                },
            );
        }

        let switch_cond_type = tast_info
            .get_expr_type(switch_condition.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        case_equality_expr = Some(if !previous_empty_cases.is_empty() {
            for previous_empty_case in previous_empty_cases {
                if !expression_analyzer::analyze(
                    statements_analyzer,
                    &previous_empty_case.0,
                    tast_info,
                    context,
                    &mut None,
                ) {
                    return false;
                }
            }
            let mut case_conds = previous_empty_cases
                .clone()
                .into_iter()
                .map(|c| c.0.clone())
                .collect::<Vec<_>>();
            case_conds.push(case_cond.clone());
            aast::Expr(
                (),
                case_cond.pos().clone(),
                aast::Expr_::Call(Box::new((
                    aast::Expr(
                        (),
                        case_cond.pos().clone(),
                        aast::Expr_::Id(Box::new(oxidized::ast_defs::Id(
                            case_cond.pos().clone(),
                            "\\in_array".to_string(),
                        ))),
                    ),
                    vec![],
                    vec![
                        (ParamKind::Pnormal, switch_condition.clone()),
                        (
                            ParamKind::Pnormal,
                            aast::Expr(
                                (),
                                case_cond.pos().clone(),
                                aast::Expr_::Collection(Box::new((
                                    oxidized::ast_defs::Id(
                                        case_cond.pos().clone(),
                                        "vec".to_string(),
                                    ),
                                    None,
                                    case_conds
                                        .into_iter()
                                        .map(|e| aast::Afield::AFvalue(e))
                                        .collect(),
                                ))),
                            ),
                        ),
                    ],
                    None,
                ))),
            )
        } else {
            if switch_cond_type.is_true() {
                case_cond.clone()
            } else {
                let adjusted_pos = case_cond.pos().to_raw_span();
                let adjusted_pos = Pos::from_lnum_bol_offset(
                    RcOc::new(case_cond.pos().filename().clone()),
                    (
                        adjusted_pos.start.line() as usize,
                        adjusted_pos.start.beg_of_line() as usize,
                        adjusted_pos.start.offset() as usize - 1,
                    ),
                    (
                        adjusted_pos.end.line() as usize,
                        adjusted_pos.end.beg_of_line() as usize,
                        adjusted_pos.end.offset() as usize,
                    ),
                );

                aast::Expr(
                    (),
                    adjusted_pos,
                    aast::Expr_::Binop(Box::new((
                        ast_defs::Bop::Eqeqeq,
                        switch_condition.clone(),
                        case_cond.clone(),
                    ))),
                )
            }
        });
    }

    let mut leftover_statements = switch_scope.leftover_statements.clone();

    leftover_statements.extend(case_stmts.clone());

    let case_stmts = leftover_statements;

    if !has_leaving_statements && !is_last {
        // this is safe for non-defaults, and defaults are always last
        let case_equality_expression = case_equality_expr.unwrap();
        let case_cond = case_cond.unwrap();

        switch_scope.leftover_case_equality_expr = Some(
            if let Some(leftover_case_equality_expr) = &switch_scope.leftover_case_equality_expr {
                aast::Expr(
                    (),
                    case_cond.pos().clone(),
                    aast::Expr_::Binop(Box::new((
                        ast_defs::Bop::Barbar,
                        leftover_case_equality_expr.clone(),
                        case_equality_expression,
                    ))),
                )
            } else {
                case_equality_expression
            },
        );

        switch_scope.leftover_statements = vec![aast::Stmt(
            stmt.0 .1.clone(),
            aast::Stmt_::If(Box::new((
                switch_scope.leftover_case_equality_expr.clone().unwrap(),
                case_stmts,
                vec![],
            ))),
        )];

        tast_info.expr_types = old_node_data;

        tast_info.case_scopes.pop();

        return true;
    }

    if let Some(leftover_case_equality_expr) = &switch_scope.leftover_case_equality_expr {
        let case_or_default_equality_expr = case_equality_expr.unwrap_or(aast::Expr(
            (),
            switch_condition.pos().clone(),
            aast::Expr_::Binop(Box::new((
                ast_defs::Bop::Eqeqeq,
                switch_condition.clone(),
                switch_condition.clone(),
            ))),
        ));

        case_equality_expr = Some(aast::Expr(
            (),
            case_or_default_equality_expr.pos().clone(),
            aast::Expr_::Binop(Box::new((
                ast_defs::Bop::Barbar,
                leftover_case_equality_expr.clone(),
                case_or_default_equality_expr.clone(),
            ))),
        ));
    }

    // if let Some(case_equality_expr) = &case_equality_expr {
    // todo simplify long case equality expression
    // }

    case_context.break_types.push(BreakContext::Switch);

    switch_scope.leftover_statements = vec![];
    switch_scope.leftover_case_equality_expr = None;

    let assertion_context =
        statements_analyzer.get_assertion_context(context.function_context.calling_class.as_ref());

    let case_clauses = if let Some(case_equality_expr) = &case_equality_expr {
        let id = (
            case_equality_expr.pos().start_offset(),
            case_equality_expr.pos().end_offset(),
        );

        formula_generator::get_formula(
            id,
            id,
            case_equality_expr,
            &assertion_context,
            tast_info,
            false,
            false,
        )
        .unwrap()
    } else {
        vec![]
    };

    let mut entry_clauses =
        if switch_scope.negated_clauses.len() > 0 && switch_scope.negated_clauses.len() < 50 {
            hakana_algebra::simplify_cnf({
                let mut c = original_context
                    .clauses
                    .iter()
                    .map(|v| &**v)
                    .collect::<Vec<_>>();
                c.extend(switch_scope.negated_clauses.iter());
                c
            })
        } else {
            original_context
                .clauses
                .iter()
                .map(|v| (**v).clone())
                .collect::<Vec<_>>()
        };

    case_context.clauses = if !case_clauses.is_empty() && case_cond.is_some() {
        algebra_analyzer::check_for_paradox(
            statements_analyzer,
            &entry_clauses.iter().map(|v| Rc::new(v.clone())).collect(),
            &case_clauses,
            tast_info,
            case_cond.unwrap().pos(),
        );

        entry_clauses.extend(case_clauses.clone());

        if entry_clauses.len() < 50 {
            hakana_algebra::simplify_cnf(entry_clauses.iter().collect())
        } else {
            entry_clauses
        }
    } else {
        entry_clauses
    }
    .into_iter()
    .map(|v| Rc::new(v.clone()))
    .collect();

    let (reconcilable_if_types, _) = hakana_algebra::get_truths_from_formula(
        case_context.clauses.iter().map(|v| &**v).collect(),
        None,
        &mut FxHashSet::default(),
    );

    if !reconcilable_if_types.is_empty() {
        let mut changed_var_ids = FxHashSet::default();

        reconciler::reconcile_keyed_types(
            &reconcilable_if_types,
            BTreeMap::new(),
            &mut case_context,
            &mut changed_var_ids,
            &if case_cond.is_some() {
                FxHashSet::from_iter([switch_var_id.clone()])
            } else {
                FxHashSet::default()
            },
            statements_analyzer,
            tast_info,
            case_pos,
            true,
            false,
            &FxHashMap::default(),
        );

        if !changed_var_ids.is_empty() {
            case_context.clauses = ScopeContext::remove_reconciled_clause_refs(
                &case_context.clauses,
                &changed_var_ids,
            )
            .0;
        }
    }

    if !case_clauses.is_empty() {
        if let Some(case_equality_expr) = &case_equality_expr {
            let negated_case_clauses =
                if let Ok(negated_case_clauses) = hakana_algebra::negate_formula(case_clauses) {
                    negated_case_clauses
                } else {
                    let case_equality_expr_id = (
                        case_equality_expr.pos().start_offset(),
                        case_equality_expr.pos().end_offset(),
                    );

                    formula_generator::get_formula(
                        case_equality_expr_id,
                        case_equality_expr_id,
                        &aast::Expr(
                            (),
                            case_equality_expr.pos().clone(),
                            aast::Expr_::Unop(Box::new((
                                ast_defs::Uop::Unot,
                                case_equality_expr.clone(),
                            ))),
                        ),
                        &assertion_context,
                        tast_info,
                        false,
                        false,
                    )
                    .unwrap_or(vec![])
                };

            switch_scope.negated_clauses.extend(negated_case_clauses);
        }
    }

    tast_info.case_scopes.push(CaseScope::new());

    statements_analyzer.analyze(&case_stmts, tast_info, &mut case_context, loop_scope);

    if tast_info.case_scopes.is_empty() {
        return false;
    }

    let case_scope = tast_info.case_scopes.pop().unwrap();

    let new_node_data = tast_info.expr_types.clone();
    old_node_data.extend(new_node_data);
    tast_info.expr_types = old_node_data;

    if !matches!(case_exit_type, ControlAction::Return) {
        if !handle_non_returning_case(
            statements_analyzer,
            switch_var_id,
            case_cond.is_none(),
            case_pos,
            tast_info,
            context,
            &case_context,
            original_context,
            &case_exit_type,
            switch_scope,
        ) {
            return false;
        }
    }

    let codebase = statements_analyzer.get_codebase();

    if let Some(break_vars) = &case_scope.break_vars {
        if let Some(ref mut possibly_redefined_var_ids) = switch_scope.possibly_redefined_vars {
            for (var_id, var_type) in break_vars {
                possibly_redefined_var_ids.insert(
                    var_id.clone(),
                    combine_optional_union_types(
                        Some(var_type),
                        possibly_redefined_var_ids.get(var_id),
                        Some(codebase),
                    ),
                );
            }
        } else {
            switch_scope.possibly_redefined_vars = Some(
                break_vars
                    .into_iter()
                    .filter(|(var_id, _)| context.vars_in_scope.contains_key(*var_id))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }

        if let Some(ref mut new_vars_in_scope) = switch_scope.new_vars_in_scope {
            for (var_id, var_type) in new_vars_in_scope.clone() {
                if let Some(break_var_type) = break_vars.get(&var_id) {
                    if case_context.vars_in_scope.contains_key(&var_id) {
                        new_vars_in_scope.insert(
                            var_id.clone(),
                            Rc::new(combine_union_types(
                                &break_var_type,
                                &var_type,
                                Some(codebase),
                                false,
                            )),
                        );
                    } else {
                        new_vars_in_scope.remove(&var_id);
                    }
                } else {
                    new_vars_in_scope.remove(&var_id);
                }
            }
        }

        if let Some(ref mut redefined_vars) = switch_scope.redefined_vars {
            for (var_id, var_type) in redefined_vars.clone() {
                if let Some(break_var_type) = break_vars.get(&var_id) {
                    redefined_vars.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &break_var_type,
                            &var_type,
                            Some(codebase),
                            false,
                        )),
                    );
                } else {
                    redefined_vars.remove(&var_id);
                }
            }
        }
    }

    true
}

fn handle_non_returning_case(
    statements_analyzer: &StatementsAnalyzer,
    switch_var_id: &String,
    is_default_case: bool,
    case_pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    case_context: &ScopeContext,
    original_context: &ScopeContext,
    case_exit_type: &ControlAction,
    switch_scope: &mut SwitchScope,
) -> bool {
    if is_default_case {
        if let Some(switch_type) = case_context.vars_in_scope.get(switch_var_id) {
            if switch_type.is_nothing() {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::ParadoxicalCondition,
                        "All possible case statements have been met, default is impossible here"
                            .to_string(),
                        statements_analyzer.get_hpos(&case_pos),
                    ),
                    statements_analyzer.get_config(),
                );

                return false;
            }
        }
    }

    let codebase = statements_analyzer.get_codebase();

    if !matches!(case_exit_type, ControlAction::Continue) {
        let case_redefined_vars =
            case_context.get_redefined_vars(&original_context.vars_in_scope, false);

        if let Some(ref mut possibly_redefined_var_ids) = switch_scope.possibly_redefined_vars {
            for (var_id, var_type) in &case_redefined_vars {
                possibly_redefined_var_ids.insert(
                    var_id.clone(),
                    combine_optional_union_types(
                        Some(var_type),
                        possibly_redefined_var_ids.get(var_id),
                        Some(codebase),
                    ),
                );
            }
        } else {
            switch_scope.possibly_redefined_vars = Some(
                case_redefined_vars
                    .clone()
                    .into_iter()
                    .filter(|(var_id, _)| context.vars_in_scope.contains_key(&*var_id))
                    .collect(),
            );
        }

        if let Some(ref mut redefined_vars) = switch_scope.redefined_vars {
            for (var_id, var_type) in redefined_vars.clone() {
                if let Some(break_var_type) = case_redefined_vars.get(&var_id) {
                    redefined_vars.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &break_var_type,
                            &var_type,
                            Some(codebase),
                            false,
                        )),
                    );
                } else {
                    redefined_vars.remove(&var_id);
                }
            }
        } else {
            switch_scope.redefined_vars = Some(
                case_redefined_vars
                    .into_iter()
                    .map(|(k, v)| (k, Rc::new(v)))
                    .collect(),
            );
        }

        if let Some(ref mut new_vars_in_scope) = switch_scope.new_vars_in_scope {
            for (var_id, var_type) in new_vars_in_scope.clone() {
                if case_context.vars_in_scope.contains_key(&var_id) {
                    new_vars_in_scope.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &case_context.vars_in_scope.get(&var_id).unwrap(),
                            &var_type,
                            Some(codebase),
                            false,
                        )),
                    );
                } else {
                    new_vars_in_scope.remove(&var_id);
                }
            }
        } else {
            switch_scope.new_vars_in_scope = Some(
                case_context
                    .vars_in_scope
                    .clone()
                    .into_iter()
                    .filter(|(k, _)| !context.vars_in_scope.contains_key(k))
                    .collect(),
            );
        }
    }

    true
}

fn update_case_exit_map(
    codebase: &CodebaseInfo,
    case_stmts: &Vec<aast::Stmt<(), ()>>,
    tast_info: &mut TastInfo,
    resolved_names: &FxHashMap<usize, String>,
    case_action_map: &mut FxHashMap<usize, FxHashSet<ControlAction>>,
    i: usize,
    last_case_exit_type: &mut ControlAction,
    case_exit_types: &mut FxHashMap<usize, ControlAction>,
) {
    let case_actions = control_analyzer::get_control_actions(
        codebase,
        resolved_names,
        &case_stmts,
        Some(tast_info),
        vec![BreakContext::Switch],
        true,
    );

    case_action_map.insert(i, case_actions.clone());
    *last_case_exit_type = get_last_action(case_actions).unwrap_or(last_case_exit_type.clone());
    case_exit_types.insert(i, last_case_exit_type.clone());
}

fn get_last_action(case_actions: FxHashSet<ControlAction>) -> Option<ControlAction> {
    if !case_actions.contains(&ControlAction::None) {
        if case_actions.len() == 1 && case_actions.contains(&ControlAction::End) {
            return Some(ControlAction::Return);
        } else if case_actions.len() == 1 && case_actions.contains(&ControlAction::Continue) {
            return Some(ControlAction::Continue);
        } else if case_actions.contains(&ControlAction::LeaveSwitch) {
            return Some(ControlAction::Break);
        }
    } else if case_actions.len() != 1 {
        return Some(ControlAction::Break);
    }

    return None;
}
