use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::reconciler::reconciler;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer::handle_paradoxical_condition;
use crate::typed_ast::TastInfo;
use crate::{expression_analyzer, formula_generator};
use hakana_type::get_bool;
use oxidized::aast;

pub(crate) fn analyze<'expr, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    tast_info: &'tast mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let mut left_context = context.clone();

    let pre_referenced_var_ids = left_context.cond_referenced_var_ids.clone();
    let pre_assigned_var_ids = left_context.assigned_var_ids.clone();

    left_context.cond_referenced_var_ids.clear();
    left_context.assigned_var_ids.clear();

    left_context.reconciled_expression_clauses = Vec::new();

    tast_info.set_expr_type(&stmt_pos, get_bool());

    if !expression_analyzer::analyze(
        statements_analyzer,
        left,
        tast_info,
        &mut left_context,
        if_body_context,
    ) {
        return false;
    }

    if let Some(cond_type) = tast_info.get_expr_type(left.pos()).cloned() {
        handle_paradoxical_condition(statements_analyzer, tast_info, left.pos(), &cond_type);
    }

    let left_cond_id = (left.pos().start_offset(), left.pos().end_offset());

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );

    let left_clauses = formula_generator::get_formula(
        left_cond_id,
        left_cond_id,
        left,
        &assertion_context,
        tast_info,
        true,
        false,
    )
    .unwrap();

    for (var_id, var_type) in &left_context.vars_in_scope {
        if left_context.assigned_var_ids.contains_key(var_id) {
            context
                .vars_in_scope
                .insert(var_id.clone(), var_type.clone());
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
            let first = context_clauses.get(0).unwrap();
            if first.wedge && first.possibilities.len() == 0 {
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
            tast_info,
            left.pos(),
            true,
            !context.inside_negation,
            &FxHashMap::default(),
        );
    } else {
        right_context = left_context.clone()
    }

    let partitioned_clauses = ScopeContext::remove_reconciled_clause_refs(
        &{
            let mut c = left_context.clauses.clone();
            c.extend(left_clauses.into_iter().map(|v| Rc::new(v)));
            c
        },
        &changed_var_ids,
    );
    right_context.clauses = partitioned_clauses.0;

    if !expression_analyzer::analyze(
        statements_analyzer,
        right,
        tast_info,
        &mut right_context,
        if_body_context,
    ) {
        return false;
    }

    if let Some(cond_type) = tast_info.get_expr_type(right.pos()).cloned() {
        handle_paradoxical_condition(statements_analyzer, tast_info, right.pos(), &cond_type);
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

    if let Some(ref mut if_body_context) = if_body_context {
        if !context.inside_negation {
            context.vars_in_scope = right_context.vars_in_scope;

            if_body_context
                .vars_in_scope
                .extend(context.vars_in_scope.clone());

            if_body_context
                .cond_referenced_var_ids
                .extend(context.cond_referenced_var_ids.clone());
            if_body_context
                .assigned_var_ids
                .extend(context.assigned_var_ids.clone());

            if_body_context
                .reconciled_expression_clauses
                .extend(partitioned_clauses.1);

            if_body_context.allow_taints = right_context.allow_taints;
        } else {
            context.vars_in_scope = left_context.vars_in_scope;
        }
    } else {
        context.vars_in_scope = left_context.vars_in_scope;
    }

    true
}
