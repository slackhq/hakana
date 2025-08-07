use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer::handle_paradoxical_condition;
use crate::stmt_analyzer::AnalysisError;
use crate::{expression_analyzer, formula_generator};
use hakana_code_info::ttype::get_bool;
use oxidized::aast;

pub(crate) fn analyze<'expr>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let mut left_context = context.clone();

    let pre_referenced_var_ids = left_context.cond_referenced_var_ids.clone();
    let pre_assigned_var_ids = left_context.assigned_var_ids.clone();

    left_context.cond_referenced_var_ids.clear();
    left_context.assigned_var_ids.clear();

    left_context.reconciled_expression_clauses = Vec::new();

    analysis_data.set_expr_type(stmt_pos, get_bool());

    expression_analyzer::analyze(statements_analyzer, left, analysis_data, &mut left_context, true,)?;

    if let Some(cond_type) = analysis_data.get_rc_expr_type(left.pos()).cloned() {
        handle_paradoxical_condition(
            statements_analyzer,
            analysis_data,
            left.pos(),
            &context.function_context.calling_functionlike_id,
            &cond_type,
        );
    }

    let left_cond_id = (
        left.pos().start_offset() as u32,
        left.pos().end_offset() as u32,
    );

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );

    let left_clauses = formula_generator::get_formula(
        left_cond_id,
        left_cond_id,
        left,
        &assertion_context,
        analysis_data,
        true,
        false,
    )
    .unwrap();

    for (var_id, var_type) in &left_context.locals {
        if left_context.assigned_var_ids.contains_key(var_id) {
            context.locals.insert(var_id.clone(), var_type.clone());
        }
    }

    let mut left_referenced_var_ids = left_context.cond_referenced_var_ids.clone();
    context
        .cond_referenced_var_ids
        .extend(pre_referenced_var_ids);

    context.assigned_var_ids.extend(pre_assigned_var_ids);

    let mut context_clauses = left_context
        .clauses
        .iter()
        .map(|v| (&**v))
        .collect::<Vec<_>>();
    context_clauses.extend(left_clauses.iter());

    if !left_context.reconciled_expression_clauses.is_empty() {
        let left_reconciled_clauses_hashed = left_context
            .reconciled_expression_clauses
            .iter()
            .map(|v| &**v)
            .collect::<FxHashSet<_>>();

        context_clauses.retain(|c| !left_reconciled_clauses_hashed.contains(c));

        if context_clauses.len() == 1 {
            let first = context_clauses.first().unwrap();
            if first.wedge && first.possibilities.is_empty() {
                context_clauses = Vec::new();
            }
        }
    }

    let simplified_clauses = hakana_algebra::simplify_cnf(context_clauses);

    let (left_assertions, active_left_assertions) = hakana_algebra::get_truths_from_formula(
        simplified_clauses.iter().collect(),
        Some(left_cond_id),
        &mut left_referenced_var_ids,
    );

    let mut changed_var_ids = FxHashSet::default();

    let mut right_context;

    // todo handle conditional assignment for inout refs

    if !left_assertions.is_empty() {
        right_context = context.clone();
        // while in an and, we allow scope to boil over to support
        // statements of the form if ($x && $x->foo())
        reconciler::reconcile_keyed_types(
            &left_assertions,
            active_left_assertions,
            &mut right_context,
            &mut changed_var_ids,
            &left_referenced_var_ids,
            statements_analyzer,
            analysis_data,
            left.pos(),
            true,
            !context.inside_negation,
            &FxHashMap::default(),
        );
    } else {
        right_context = left_context.clone()
    }

    let partitioned_clauses = BlockContext::remove_reconciled_clause_refs(
        &{
            let mut c = left_context.clauses.clone();
            c.extend(left_clauses.into_iter().map(Rc::new));
            c
        },
        &changed_var_ids,
    );
    right_context.clauses = partitioned_clauses.0;

    expression_analyzer::analyze(
        statements_analyzer,
        right,
        analysis_data,
        &mut right_context,
        true,
    )?;

    if let Some(cond_type) = analysis_data.get_rc_expr_type(right.pos()).cloned() {
        handle_paradoxical_condition(
            statements_analyzer,
            analysis_data,
            right.pos(),
            &context.function_context.calling_functionlike_id,
            &cond_type,
        );
    }

    context.cond_referenced_var_ids = left_context.cond_referenced_var_ids;
    context
        .cond_referenced_var_ids
        .extend(right_context.cond_referenced_var_ids);

    if context.inside_conditional {
        context.assigned_var_ids = left_context.assigned_var_ids;
        context
            .assigned_var_ids
            .extend(right_context.assigned_var_ids);
    }

    if let Some(if_body_context) = &context.if_body_context {
        let mut if_body_context_inner = if_body_context.borrow_mut();

        if !context.inside_negation {
            context.locals = right_context.locals;

            if_body_context_inner.locals.extend(context.locals.clone());

            if_body_context_inner
                .cond_referenced_var_ids
                .extend(context.cond_referenced_var_ids.clone());
            if_body_context_inner
                .assigned_var_ids
                .extend(context.assigned_var_ids.clone());

            if_body_context_inner
                .reconciled_expression_clauses
                .extend(partitioned_clauses.1);

            if_body_context_inner.allow_taints = right_context.allow_taints;
        } else {
            context.locals = left_context.locals;
        }
    } else {
        context.locals = left_context.locals;
    }

    Ok(())
}
