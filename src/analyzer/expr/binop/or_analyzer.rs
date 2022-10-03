use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::reconciler::reconciler;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer;
use crate::stmt::if_conditional_analyzer::handle_paradoxical_condition;
use crate::{expression_analyzer, formula_generator};
use crate::{scope_context::if_scope::IfScope, typed_ast::TastInfo};
use hakana_type::combine_union_types;
use oxidized::ast::Uop;
use oxidized::{aast, ast};

pub(crate) fn analyze<'expr, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    tast_info: &'tast mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let codebase = statements_analyzer.get_codebase();

    let mut left_context;
    let mut left_referenced_var_ids;
    let left_assigned_var_ids;

    // we cap this at max depth of 4 to prevent quadratic behaviour
    // when analysing <expr> || <expr> || <expr> || <expr> || <expr>
    if !is_or(left, 3) {
        let mut if_scope = IfScope::new();

        let if_conditional_scope = if_conditional_analyzer::analyze(
            statements_analyzer,
            left,
            tast_info,
            context,
            &mut if_scope,
        );

        left_context = if_conditional_scope.if_body_context;
        *context = if_conditional_scope.outer_context;
        left_referenced_var_ids = if_conditional_scope.cond_referenced_var_ids;
    } else {
        let pre_referenced_var_ids = context.cond_referenced_var_ids.clone();
        context.cond_referenced_var_ids = FxHashSet::default();

        let pre_assigned_var_ids = context.assigned_var_ids.clone();

        left_context = context.clone();
        left_context.assigned_var_ids = FxHashMap::default();

        if !expression_analyzer::analyze(
            statements_analyzer,
            left,
            tast_info,
            &mut left_context,
            &mut None,
        ) {
            return false;
        }

        for var_id in &left_context.parent_conflicting_clause_vars {
            context.remove_var_from_conflicting_clauses(var_id, None, None, tast_info);
        }

        if let Some(cond_type) = tast_info.get_expr_type(left.pos()).cloned() {
            handle_paradoxical_condition(statements_analyzer, tast_info, left.pos(), &cond_type);
        }

        let cloned_vars = context.vars_in_scope.clone();
        for (var_id, left_type) in &left_context.vars_in_scope {
            if let Some(context_type) = cloned_vars.get(var_id) {
                context.vars_in_scope.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        context_type,
                        left_type,
                        codebase,
                        false,
                    )),
                );
            } else if left_context.assigned_var_ids.contains_key(var_id) {
                context
                    .vars_in_scope
                    .insert(var_id.clone(), left_type.clone());
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

    let left_cond_id = (left.pos().start_offset(), left.pos().end_offset());

    let assertion_context =
        statements_analyzer.get_assertion_context(context.function_context.calling_class.as_ref());

    let left_clauses = if let Ok(left_clauses) = formula_generator::get_formula(
        left_cond_id,
        left_cond_id,
        left,
        &assertion_context,
        tast_info,
        true,
        false,
    ) {
        left_clauses
    } else {
        return false;
    };

    let mut negated_left_clauses =
        if let Ok(good_clauses) = hakana_algebra::negate_formula(left_clauses) {
            good_clauses
        } else {
            if let Ok(good_clauses) = formula_generator::get_formula(
                left_cond_id,
                left_cond_id,
                &aast::Expr(
                    (),
                    left.pos().clone(),
                    aast::Expr_::Unop(Box::new((Uop::Unot, left.clone()))),
                ),
                &assertion_context,
                tast_info,
                false,
                false,
            ) {
                good_clauses
            } else {
                return false;
            }
        };

    if !left_context.reconciled_expression_clauses.is_empty() {
        let left_reconciled_clauses_hashed = left_context
            .reconciled_expression_clauses
            .iter()
            .map(|v| &**v)
            .collect::<FxHashSet<_>>();

        negated_left_clauses = negated_left_clauses
            .into_iter()
            .filter(|c| !left_reconciled_clauses_hashed.contains(c))
            .collect::<Vec<_>>();

        if negated_left_clauses.len() == 1 {
            let first = negated_left_clauses.get(0).unwrap();
            if first.wedge && first.possibilities.len() == 0 {
                negated_left_clauses = Vec::new();
            }
        }
    }

    let clauses_for_right_analysis = hakana_algebra::simplify_cnf({
        let mut clauses = context.clauses.iter().map(|v| &**v).collect::<Vec<_>>();
        clauses.extend(negated_left_clauses.iter());
        clauses
    });

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
            tast_info,
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
            ScopeContext::remove_reconciled_clause_refs(&right_context.clauses, &changed_var_ids);
        right_context.clauses = partiioned_clauses.0;
        right_context
            .reconciled_expression_clauses
            .extend(partiioned_clauses.1);

        let partiioned_clauses =
            ScopeContext::remove_reconciled_clause_refs(&context.clauses, &changed_var_ids);
        context.clauses = partiioned_clauses.0;
        context
            .reconciled_expression_clauses
            .extend(partiioned_clauses.1);
    }

    let pre_referenced_var_ids = right_context.cond_referenced_var_ids.clone();
    right_context.cond_referenced_var_ids = FxHashSet::default();

    let pre_assigned_var_ids = right_context.assigned_var_ids.clone();
    right_context.assigned_var_ids = FxHashMap::default();

    if !expression_analyzer::analyze(
        statements_analyzer,
        right,
        tast_info,
        &mut right_context,
        &mut None,
    ) {
        return false;
    }

    if let Some(cond_type) = tast_info.get_expr_type(right.pos()).cloned() {
        handle_paradoxical_condition(statements_analyzer, tast_info, right.pos(), &cond_type);
    }

    let _right_referenced_var_ids = right_context.cond_referenced_var_ids.clone();
    right_context
        .cond_referenced_var_ids
        .extend(pre_referenced_var_ids);

    let right_assigned_var_ids = right_context.assigned_var_ids.clone();
    right_context.assigned_var_ids.extend(pre_assigned_var_ids);

    let right_cond_id = (right.pos().start_offset(), right.pos().end_offset());

    let right_clauses = formula_generator::get_formula(
        right_cond_id,
        right_cond_id,
        right,
        &assertion_context,
        tast_info,
        true,
        false,
    );

    if let Err(_) = right_clauses {
        return false;
    }

    let mut clauses_for_right_analysis = ScopeContext::remove_reconciled_clauses(
        &clauses_for_right_analysis,
        &right_assigned_var_ids
            .into_iter()
            .map(|(k, _)| k)
            .collect::<FxHashSet<_>>(),
    )
    .0;

    clauses_for_right_analysis.extend(right_clauses.unwrap());

    let combined_right_clauses =
        hakana_algebra::simplify_cnf(clauses_for_right_analysis.iter().collect());

    let mut right_referenced_var_ids = FxHashSet::default();

    let (right_type_assertions, active_right_type_assertions) =
        hakana_algebra::get_truths_from_formula(
            combined_right_clauses.iter().collect(),
            Some(right_cond_id),
            &mut right_referenced_var_ids,
        );

    if !right_type_assertions.is_empty() {
        let mut tmp_context = right_context.clone();
        reconciler::reconcile_keyed_types(
            &right_type_assertions,
            active_right_type_assertions,
            &mut tmp_context,
            &mut FxHashSet::default(),
            &right_referenced_var_ids,
            statements_analyzer,
            tast_info,
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

    if let Some(ref mut if_body_context) = if_body_context {
        let left_vars = left_context.vars_in_scope.clone();
        let if_vars = if_body_context.vars_in_scope.clone();
        for (var_id, right_type) in right_context.vars_in_scope.clone() {
            if let Some(if_type) = if_vars.get(&var_id) {
                if_body_context.vars_in_scope.insert(
                    var_id,
                    Rc::new(combine_union_types(
                        &right_type,
                        &if_type,
                        codebase,
                        false,
                    )),
                );
            } else if let Some(left_type) = left_vars.get(&var_id) {
                if_body_context.vars_in_scope.insert(
                    var_id,
                    Rc::new(combine_union_types(
                        &right_type,
                        &left_type,
                        codebase,
                        false,
                    )),
                );
            }
        }

        if_body_context
            .cond_referenced_var_ids
            .extend(context.cond_referenced_var_ids.clone());
        if_body_context
            .assigned_var_ids
            .extend(context.assigned_var_ids.clone());
    }

    true
}

fn is_or(cond: &aast::Expr<(), ()>, max_nesting: usize) -> bool {
    if max_nesting == 0 {
        return true;
    }

    if let Some((bop, left, _)) = cond.2.as_binop() {
        if let ast::Bop::Barbar = bop {
            return is_or(left, max_nesting - 1);
        }
    }

    false
}
