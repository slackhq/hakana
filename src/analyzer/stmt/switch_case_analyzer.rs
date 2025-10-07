use hakana_algebra::Clause;
use hakana_code_info::issue::IssueKind;

use hakana_code_info::issue::Issue;

use hakana_code_info::ttype::combine_union_types;

use hakana_code_info::ttype::combine_optional_union_types;
use hakana_code_info::var_name::VarName;
use oxidized::aast;
use oxidized::aast::CallExpr;
use oxidized::ast::Binop;
use oxidized::file_pos::FilePos;
use oxidized::pos_span_raw::PosSpanRaw;
use relative_path::RelativePath;

use crate::scope::CaseScope;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::stmt_analyzer::AnalysisError;

use rustc_hash::FxHashMap;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::reconciler;

use std::rc::Rc;

use crate::algebra_analyzer;

use crate::formula_generator;

use super::control_analyzer::BreakContext;
use super::if_conditional_analyzer::add_branch_dataflow;

use oxidized::ast_defs;

use hakana_code_info::ttype::get_mixed_any;

use crate::expression_analyzer;

use crate::scope::loop_scope::LoopScope;

use crate::scope::switch_scope::SwitchScope;

use rustc_hash::FxHashSet;

use crate::scope::control_action::ControlAction;

use crate::scope::BlockContext;

use crate::function_analysis_data::FunctionAnalysisData;

use oxidized::aast::Pos;

use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn analyze_case(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &Vec<aast::Case<(), ()>>,
        &Option<aast::DefaultCase<(), ()>>,
    ),
    switch_condition: &aast::Expr<(), ()>,
    condition_is_fake: bool,
    switch_var_id: &VarName,
    case_cond: Option<&aast::Expr<(), ()>>,
    case_pos: &Pos,
    case_stmts: Vec<aast::Stmt<(), ()>>,
    previous_empty_cases: &Vec<&aast::Case<(), ()>>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    original_context: &BlockContext,
    is_last: bool,
    switch_scope: &mut SwitchScope,
    loop_scope: &mut Option<LoopScope>,
) -> Result<ControlAction, AnalysisError> {
    let mut case_context = original_context.clone();

    let mut old_node_data = analysis_data.expr_types.clone();

    let mut case_equality_expr = None;

    if let Some(case_cond) = case_cond {
        expression_analyzer::analyze(statements_analyzer, case_cond, analysis_data, context, true)?;

        add_branch_dataflow(statements_analyzer, case_cond, analysis_data);

        if condition_is_fake {
            analysis_data.set_expr_type(
                switch_condition.pos(),
                if let Some(t) = context.locals.get(switch_var_id) {
                    (**t).clone()
                } else {
                    get_mixed_any()
                },
            );
        }

        let switch_cond_type = analysis_data
            .get_rc_expr_type(switch_condition.pos())
            .cloned()
            .unwrap_or(Rc::new(get_mixed_any()));

        case_equality_expr = Some(if !previous_empty_cases.is_empty() {
            for previous_empty_case in previous_empty_cases {
                expression_analyzer::analyze(
                    statements_analyzer,
                    &previous_empty_case.0,
                    analysis_data,
                    context,
                    true,
                )?;
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
                aast::Expr_::Call(Box::new(CallExpr {
                    func: aast::Expr(
                        (),
                        case_cond.pos().clone(),
                        aast::Expr_::Id(Box::new(oxidized::ast_defs::Id(
                            case_cond.pos().clone(),
                            "\\in_array".to_string(),
                        ))),
                    ),
                    targs: vec![],
                    args: vec![
                        aast::Argument::Anormal(switch_condition.clone()),
                        aast::Argument::Anormal(aast::Expr(
                            (),
                            case_cond.pos().clone(),
                            aast::Expr_::ValCollection(Box::new((
                                (case_cond.pos().clone(), oxidized::tast::VcKind::Vec),
                                None,
                                case_conds,
                            ))),
                        )),
                    ],
                    unpacked_arg: None,
                })),
            )
        } else if switch_cond_type.is_true() {
            case_cond.clone()
        } else {
            let adjusted_pos = case_cond.pos().to_raw_span();
            let adjusted_pos = Pos::from_lnum_bol_offset(
                Arc::new(RelativePath::EMPTY),
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
                aast::Expr_::Binop(Box::new(Binop {
                    bop: ast_defs::Bop::Eqeqeq,
                    lhs: switch_condition.clone(),
                    rhs: case_cond.clone(),
                })),
            )
        });
    }

    let mut leftover_statements = switch_scope.leftover_statements.clone();

    leftover_statements.extend(case_stmts);

    let case_stmts = leftover_statements;

    if (case_stmts.is_empty() || &case_stmts.last().unwrap().1 == &aast::Stmt_::Fallthrough)
        && !is_last
    {
        // this is safe for non-defaults, and defaults are always last
        let case_equality_expression = case_equality_expr.unwrap();
        let case_cond = case_cond.unwrap();

        switch_scope.leftover_case_equality_expr = Some(
            if let Some(leftover_case_equality_expr) = &switch_scope.leftover_case_equality_expr {
                let new_pos_start = leftover_case_equality_expr.1.to_raw_span().start;
                let new_pos_end = case_cond.pos().to_raw_span().end;

                aast::Expr(
                    (),
                    Pos::from_raw_span(
                        Arc::new(RelativePath::EMPTY),
                        PosSpanRaw {
                            start: new_pos_start,
                            end: new_pos_end,
                        },
                    ),
                    aast::Expr_::Binop(Box::new(Binop {
                        bop: ast_defs::Bop::Barbar,
                        lhs: leftover_case_equality_expr.clone(),
                        rhs: case_equality_expression,
                    })),
                )
            } else {
                case_equality_expression
            },
        );

        switch_scope.leftover_statements = vec![aast::Stmt(
            stmt.0.1.clone(),
            aast::Stmt_::If(Box::new((
                switch_scope.leftover_case_equality_expr.clone().unwrap(),
                aast::Block(case_stmts),
                aast::Block(vec![]),
            ))),
        )];

        analysis_data.expr_types = old_node_data;

        analysis_data.case_scopes.pop();

        return Ok(ControlAction::None);
    }

    if let Some(leftover_case_equality_expr) = &switch_scope.leftover_case_equality_expr {
        case_equality_expr = expand_case_equality_from_leftovers(
            switch_condition,
            case_equality_expr,
            leftover_case_equality_expr,
        );
    }

    // if let Some(case_equality_expr) = &case_equality_expr {
    // todo simplify long case equality expression
    // }

    case_context.break_types.push(BreakContext::Switch);

    switch_scope.leftover_statements = vec![];
    switch_scope.leftover_case_equality_expr = None;

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class,
        context.function_context.calling_functionlike_id,
    );

    let (case_clauses_for_negation, case_context_clauses) = get_new_clauses(
        statements_analyzer,
        case_cond,
        analysis_data,
        context,
        original_context,
        switch_scope,
        &case_equality_expr,
        &assertion_context,
    );

    case_context.clauses = case_context_clauses;

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
            analysis_data,
            case_pos,
            true,
            false,
            &FxHashMap::default(),
        );

        if !changed_var_ids.is_empty() {
            case_context.clauses = BlockContext::remove_reconciled_clause_refs(
                &case_context.clauses,
                &changed_var_ids,
            )
            .0;
        }
    }

    if !case_clauses_for_negation.is_empty() {
        if let Some(case_equality_expr) = &case_equality_expr {
            extend_switch_negated_clauses(
                analysis_data,
                switch_scope,
                &assertion_context,
                case_clauses_for_negation,
                case_equality_expr,
            );
        }
    }

    analysis_data.case_scopes.push(CaseScope::new());

    statements_analyzer.analyze(&case_stmts, analysis_data, &mut case_context, loop_scope)?;

    if analysis_data.case_scopes.is_empty() {
        return Ok(ControlAction::None);
    }

    let case_scope = analysis_data.case_scopes.pop().unwrap();

    let new_node_data = analysis_data.expr_types.clone();
    old_node_data.extend(new_node_data);
    analysis_data.expr_types = old_node_data;

    if case_context.control_actions.is_empty()
        || !case_context
            .control_actions
            .iter()
            .all(|a| a == &ControlAction::Return || a == &ControlAction::End)
    {
        handle_non_returning_case(
            statements_analyzer,
            switch_var_id,
            case_cond.is_none(),
            case_pos,
            analysis_data,
            context,
            &case_context,
            original_context,
            switch_scope,
        )?;
    }

    let codebase = statements_analyzer.codebase;

    if let Some(break_vars) = &case_scope.break_vars {
        if let Some(ref mut possibly_redefined_var_ids) = switch_scope.possibly_redefined_vars {
            for (var_id, var_type) in break_vars {
                possibly_redefined_var_ids.insert(
                    var_id.clone(),
                    combine_optional_union_types(
                        Some(var_type),
                        possibly_redefined_var_ids.get(var_id),
                        codebase,
                    ),
                );
            }
        } else {
            switch_scope.possibly_redefined_vars = Some(
                break_vars
                    .iter()
                    .filter(|(var_id, _)| context.locals.contains_key(*var_id))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }

        if let Some(ref mut new_locals) = switch_scope.new_locals {
            for (var_id, var_type) in new_locals.clone() {
                if let Some(break_var_type) = break_vars.get(&var_id) {
                    if case_context.locals.contains_key(&var_id) {
                        new_locals.insert(
                            var_id.clone(),
                            Rc::new(combine_union_types(
                                break_var_type,
                                &var_type,
                                codebase,
                                false,
                            )),
                        );
                    } else {
                        new_locals.remove(&var_id);
                    }
                } else {
                    new_locals.remove(&var_id);
                }
            }
        }

        if let Some(ref mut redefined_vars) = switch_scope.redefined_vars {
            for (var_id, var_type) in redefined_vars.clone() {
                if let Some(break_var_type) = break_vars.get(&var_id) {
                    redefined_vars.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            break_var_type,
                            &var_type,
                            codebase,
                            false,
                        )),
                    );
                } else {
                    redefined_vars.remove(&var_id);
                }
            }
        }
    }

    Ok(ControlAction::None)
}

fn extend_switch_negated_clauses(
    analysis_data: &mut FunctionAnalysisData,
    switch_scope: &mut SwitchScope,
    assertion_context: &formula_generator::AssertionContext<'_>,
    case_clauses_for_negation: Vec<Clause>,
    case_equality_expr: &aast::Expr<(), ()>,
) {
    let negated_case_clauses = hakana_algebra::negate_formula(case_clauses_for_negation)
        .unwrap_or_else(|_| {
            let case_equality_expr_id = (
                case_equality_expr.pos().start_offset() as u32,
                case_equality_expr.pos().end_offset() as u32,
            );

            formula_generator::get_formula(
                case_equality_expr_id,
                case_equality_expr_id,
                &aast::Expr(
                    (),
                    case_equality_expr.pos().clone(),
                    aast::Expr_::Unop(Box::new((ast_defs::Uop::Unot, case_equality_expr.clone()))),
                ),
                assertion_context,
                analysis_data,
                false,
                false,
            )
            .unwrap_or_default()
        });

    switch_scope.negated_clauses.extend(negated_case_clauses);
}

fn get_new_clauses(
    statements_analyzer: &StatementsAnalyzer<'_>,
    case_cond: Option<&aast::Expr<(), ()>>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    original_context: &BlockContext,
    switch_scope: &SwitchScope,
    case_equality_expr: &Option<aast::Expr<(), ()>>,
    assertion_context: &formula_generator::AssertionContext<'_>,
) -> (Vec<Clause>, Vec<Rc<Clause>>) {
    let case_clauses = if let Some(case_equality_expr) = case_equality_expr {
        let id = (
            case_equality_expr.pos().start_offset() as u32,
            case_equality_expr.pos().end_offset() as u32,
        );

        formula_generator::get_formula(
            id,
            id,
            case_equality_expr,
            assertion_context,
            analysis_data,
            false,
            false,
        )
        .unwrap()
    } else {
        vec![]
    };

    let mut entry_clauses =
        if !switch_scope.negated_clauses.is_empty() && switch_scope.negated_clauses.len() < 50 {
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

    let case_context_clauses = if !case_clauses.is_empty() {
        if let Some(case_cond) = case_cond {
            algebra_analyzer::check_for_paradox(
                statements_analyzer,
                &entry_clauses.iter().map(|v| Rc::new(v.clone())).collect(),
                &case_clauses,
                analysis_data,
                case_cond.pos(),
                &context.function_context.calling_functionlike_id,
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
    } else {
        entry_clauses
    }
    .into_iter()
    .map(|v| Rc::new(v.clone()))
    .collect();

    (case_clauses, case_context_clauses)
}

fn expand_case_equality_from_leftovers(
    switch_condition: &aast::Expr<(), ()>,
    case_equality_expr: Option<aast::Expr<(), ()>>,
    leftover_case_equality_expr: &aast::Expr<(), ()>,
) -> Option<aast::Expr<(), ()>> {
    let case_or_default_equality_expr = case_equality_expr.unwrap_or(aast::Expr(
        (),
        switch_condition.pos().clone(),
        aast::Expr_::Binop(Box::new(Binop {
            bop: ast_defs::Bop::Eqeqeq,
            lhs: switch_condition.clone(),
            rhs: switch_condition.clone(),
        })),
    ));

    return Some(aast::Expr(
        (),
        case_or_default_equality_expr.pos().clone(),
        aast::Expr_::Binop(Box::new(Binop {
            bop: ast_defs::Bop::Barbar,
            lhs: leftover_case_equality_expr.clone(),
            rhs: case_or_default_equality_expr.clone(),
        })),
    ));
}

pub(crate) fn handle_non_returning_case(
    statements_analyzer: &StatementsAnalyzer,
    switch_var_id: &VarName,
    is_default_case: bool,
    case_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    case_context: &BlockContext,
    original_context: &BlockContext,
    switch_scope: &mut SwitchScope,
) -> Result<(), AnalysisError> {
    if is_default_case {
        if let Some(switch_type) = case_context.locals.get(switch_var_id) {
            if switch_type.is_nothing() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::UselessDefaultCase,
                        "All possible case statements have been met, default is impossible here"
                            .to_string(),
                        statements_analyzer.get_hpos(case_pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                return Ok(());
            }
        }
    }

    let codebase = statements_analyzer.codebase;

    let mut removed_var_ids = FxHashSet::default();
    let case_redefined_vars =
        case_context.get_redefined_locals(&original_context.locals, false, &mut removed_var_ids);

    if let Some(ref mut possibly_redefined_var_ids) = switch_scope.possibly_redefined_vars {
        for (var_id, var_type) in &case_redefined_vars {
            possibly_redefined_var_ids.insert(
                var_id.clone(),
                combine_optional_union_types(
                    Some(var_type),
                    possibly_redefined_var_ids.get(var_id),
                    codebase,
                ),
            );
        }
    } else {
        switch_scope.possibly_redefined_vars = Some(
            case_redefined_vars
                .clone()
                .into_iter()
                .filter(|(var_id, _)| context.locals.contains_key(var_id))
                .collect(),
        );
    }

    if let Some(ref mut redefined_vars) = switch_scope.redefined_vars {
        for (var_id, var_type) in redefined_vars.clone() {
            if let Some(break_var_type) = case_redefined_vars.get(&var_id) {
                redefined_vars.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        break_var_type,
                        &var_type,
                        codebase,
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

    if let Some(ref mut new_locals) = switch_scope.new_locals {
        for (var_id, var_type) in new_locals.clone() {
            if case_context.locals.contains_key(&var_id) {
                new_locals.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        case_context.locals.get(&var_id).unwrap(),
                        &var_type,
                        codebase,
                        false,
                    )),
                );
            } else {
                new_locals.remove(&var_id);
            }
        }
    } else {
        switch_scope.new_locals = Some(
            case_context
                .locals
                .clone()
                .into_iter()
                .filter(|(k, _)| !context.locals.contains_key(k))
                .collect(),
        );
    }

    Ok(())
}
