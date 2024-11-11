use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::reconciler;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer;
use crate::stmt::if_conditional_analyzer::handle_paradoxical_condition;
use crate::stmt_analyzer::AnalysisError;
use crate::{expression_analyzer, formula_generator};
use crate::{function_analysis_data::FunctionAnalysisData, scope::if_scope::IfScope};
use hakana_code_info::ttype::combine_union_types;
use oxidized::ast::{Binop, Uop};
use oxidized::{aast, ast};

pub(crate) fn analyze<'expr>(
    statements_analyzer: &StatementsAnalyzer,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    let mut left_context;
    let mut left_referenced_var_ids;
    let left_assigned_var_ids;

    // we cap this at max depth of 4 to prevent quadratic behaviour
    // when analysing <expr> || <expr> || <expr> || <expr> || <expr>
    if !is_or(left, 3) {
        let mut if_scope = IfScope::default();

        let if_conditional_scope = if_conditional_analyzer::analyze(
            statements_analyzer,
            left,
            analysis_data,
            context,
            &mut if_scope,
        )?;

        left_context = if_conditional_scope.if_body_context;
        *context = if_conditional_scope.outer_context;
        left_referenced_var_ids = if_conditional_scope.cond_referenced_var_ids;
    } else {
        let pre_referenced_var_ids = context.cond_referenced_var_ids.clone();
        context.cond_referenced_var_ids = FxHashSet::default();

        let pre_assigned_var_ids = context.assigned_var_ids.clone();

        left_context = context.clone();
        left_context.assigned_var_ids = FxHashMap::default();

        let tmp_if_body_context = left_context.if_body_context;
        left_context.if_body_context = None;

        expression_analyzer::analyze(statements_analyzer, left, analysis_data, &mut left_context)?;

        left_context.if_body_context = tmp_if_body_context;

        for var_id in &left_context.parent_conflicting_clause_vars {
            context.remove_var_from_conflicting_clauses(var_id, None, None, analysis_data);
        }

        if let Some(cond_type) = analysis_data.get_rc_expr_type(left.pos()).cloned() {
            handle_paradoxical_condition(
                statements_analyzer,
                analysis_data,
                left.pos(),
                &context.function_context.calling_functionlike_id,
                &cond_type,
            );
        }

        let cloned_vars = context.locals.clone();
        for (var_id, left_type) in &left_context.locals {
            if let Some(context_type) = cloned_vars.get(var_id) {
                context.locals.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        context_type,
                        left_type,
                        codebase,
                        false,
                    )),
                );
            } else if left_context.assigned_var_ids.contains_key(var_id) {
                context.locals.insert(var_id.clone(), left_type.clone());
            }
        }

        left_referenced_var_ids = left_context.cond_referenced_var_ids.clone();
        left_context
            .cond_referenced_var_ids
            .extend(pre_referenced_var_ids);

        left_assigned_var_ids = left_context.assigned_var_ids.clone();
        left_context.assigned_var_ids.extend(pre_assigned_var_ids);

        left_referenced_var_ids.retain(|id| !left_assigned_var_ids.contains_key(id));
    }

    let left_cond_id = (
        left.pos().start_offset() as u32,
        left.pos().end_offset() as u32,
    );

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );

    let left_clauses = if let Ok(left_clauses) = formula_generator::get_formula(
        left_cond_id,
        left_cond_id,
        left,
        &assertion_context,
        analysis_data,
        true,
        false,
    ) {
        left_clauses
    } else {
        return Err(AnalysisError::UserError);
    };

    let mut negated_left_clauses =
        if let Ok(good_clauses) = hakana_algebra::negate_formula(left_clauses) {
            good_clauses
        } else if let Ok(good_clauses) = formula_generator::get_formula(
            left_cond_id,
            left_cond_id,
            &aast::Expr(
                (),
                left.pos().clone(),
                aast::Expr_::Unop(Box::new((Uop::Unot, left.clone()))),
            ),
            &assertion_context,
            analysis_data,
            false,
            false,
        ) {
            good_clauses
        } else {
            return Err(AnalysisError::UserError);
        };

    if !left_context.reconciled_expression_clauses.is_empty() {
        let left_reconciled_clauses_hashed = left_context
            .reconciled_expression_clauses
            .iter()
            .map(|v| &**v)
            .collect::<FxHashSet<_>>();

        negated_left_clauses.retain(|c| !left_reconciled_clauses_hashed.contains(c));

        if negated_left_clauses.len() == 1 {
            let first = negated_left_clauses.first().unwrap();
            if first.wedge && first.possibilities.is_empty() {
                negated_left_clauses = Vec::new();
            }
        }
    }

    let mut clauses_for_right_analysis = context.clauses.iter().map(|v| &**v).collect::<Vec<_>>();
    clauses_for_right_analysis.extend(negated_left_clauses.iter());

    let clauses_for_right_analysis = hakana_algebra::simplify_cnf(clauses_for_right_analysis);

    let (negated_type_assertions, active_negated_type_assertions) =
        hakana_algebra::get_truths_from_formula(
            clauses_for_right_analysis.iter().collect(),
            Some(left_cond_id),
            &mut left_referenced_var_ids,
        );

    let mut changed_var_ids = FxHashSet::default();

    let mut right_context = context.clone();

    // todo handle conditional assignment for inout refs

    if !negated_type_assertions.is_empty() {
        // while in an or, we allow scope to boil over to support
        // statements of the form if ($x === null || $x->foo())
        reconciler::reconcile_keyed_types(
            &negated_type_assertions,
            active_negated_type_assertions,
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
    }

    right_context.clauses = clauses_for_right_analysis
        .iter()
        .map(|v| Rc::new(v.clone()))
        .collect();

    if !changed_var_ids.is_empty() {
        let partiioned_clauses =
            BlockContext::remove_reconciled_clause_refs(&right_context.clauses, &changed_var_ids);
        right_context.clauses = partiioned_clauses.0;
        right_context
            .reconciled_expression_clauses
            .extend(partiioned_clauses.1);

        let partiioned_clauses =
            BlockContext::remove_reconciled_clause_refs(&context.clauses, &changed_var_ids);
        context.clauses = partiioned_clauses.0;
        context
            .reconciled_expression_clauses
            .extend(partiioned_clauses.1);
    }

    let pre_referenced_var_ids = right_context.cond_referenced_var_ids.clone();
    right_context.cond_referenced_var_ids = FxHashSet::default();

    let pre_assigned_var_ids = right_context.assigned_var_ids.clone();
    right_context.assigned_var_ids = FxHashMap::default();

    let tmp_if_body_context = right_context.if_body_context;
    right_context.if_body_context = None;

    expression_analyzer::analyze(
        statements_analyzer,
        right,
        analysis_data,
        &mut right_context,
    )?;

    right_context.if_body_context = tmp_if_body_context;

    if let Some(cond_type) = analysis_data.get_rc_expr_type(right.pos()).cloned() {
        handle_paradoxical_condition(
            statements_analyzer,
            analysis_data,
            right.pos(),
            &context.function_context.calling_functionlike_id,
            &cond_type,
        );
    }

    let mut right_referenced_var_ids = right_context.cond_referenced_var_ids.clone();
    right_context
        .cond_referenced_var_ids
        .extend(pre_referenced_var_ids);

    let right_assigned_var_ids = right_context.assigned_var_ids.clone();
    right_context.assigned_var_ids.extend(pre_assigned_var_ids);

    let right_cond_id = (
        right.pos().start_offset() as u32,
        right.pos().end_offset() as u32,
    );

    let right_clauses = formula_generator::get_formula(
        right_cond_id,
        right_cond_id,
        right,
        &assertion_context,
        analysis_data,
        true,
        false,
    );

    if right_clauses.is_err() {
        return Err(AnalysisError::UserError);
    }

    let mut clauses_for_right_analysis = BlockContext::remove_reconciled_clauses(
        &clauses_for_right_analysis,
        &right_assigned_var_ids.into_keys().collect::<FxHashSet<_>>(),
    )
    .0;

    clauses_for_right_analysis.extend(right_clauses.unwrap());

    let combined_right_clauses =
        hakana_algebra::simplify_cnf(clauses_for_right_analysis.iter().collect());

    let (right_type_assertions, active_right_type_assertions) =
        hakana_algebra::get_truths_from_formula(
            combined_right_clauses.iter().collect(),
            Some(right_cond_id),
            &mut right_referenced_var_ids,
        );

    // todo handle conditional assignment for inout refs

    if !right_type_assertions.is_empty() {
        let mut right_changed_var_ids = FxHashSet::default();

        reconciler::reconcile_keyed_types(
            &right_type_assertions,
            active_right_type_assertions,
            &mut right_context.clone(),
            &mut right_changed_var_ids,
            &right_referenced_var_ids,
            statements_analyzer,
            analysis_data,
            right.pos(),
            true,
            context.inside_negation,
            &FxHashMap::default(),
        );
    }

    // todo handle exit in right branch of if

    context
        .cond_referenced_var_ids
        .extend(right_context.cond_referenced_var_ids);
    context
        .assigned_var_ids
        .extend(right_context.assigned_var_ids);

    if let Some(if_body_context) = &context.if_body_context {
        let mut if_body_context_inner = if_body_context.borrow_mut();
        let left_vars = left_context.locals.clone();
        let if_vars = if_body_context_inner.locals.clone();
        for (var_id, right_type) in right_context.locals.clone() {
            if let Some(if_type) = if_vars.get(&var_id) {
                if_body_context_inner.locals.insert(
                    var_id,
                    Rc::new(combine_union_types(&right_type, if_type, codebase, false)),
                );
            } else if let Some(left_type) = left_vars.get(&var_id) {
                if_body_context_inner.locals.insert(
                    var_id,
                    Rc::new(combine_union_types(&right_type, left_type, codebase, false)),
                );
            }
        }

        if_body_context_inner
            .cond_referenced_var_ids
            .extend(context.cond_referenced_var_ids.clone());
        if_body_context_inner
            .assigned_var_ids
            .extend(context.assigned_var_ids.clone());
    }

    Ok(())
}

fn is_or(cond: &aast::Expr<(), ()>, max_nesting: usize) -> bool {
    if max_nesting == 0 {
        return true;
    }

    if let Some(Binop {
        bop: ast::Bop::Barbar,
        lhs: left,
        ..
    }) = cond.2.as_binop()
    {
        return is_or(left, max_nesting - 1);
    }

    false
}
