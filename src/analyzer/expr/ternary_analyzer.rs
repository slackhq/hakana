use std::collections::BTreeMap;
use std::rc::Rc;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler::{assertion_reconciler, reconciler};
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::if_scope::IfScope;
use crate::scope_context::{var_has_root, ScopeContext};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer::{self, add_branch_dataflow};
use crate::stmt_analyzer::AnalysisError;
use crate::{algebra_analyzer, expression_analyzer, formula_generator};
use hakana_algebra::Clause;
use hakana_reflection_info::assertion::Assertion;
use hakana_type::{add_union_type, combine_union_types, get_mixed_any};
use oxidized::aast;
use oxidized::ast_defs::Uop;
use oxidized::pos::Pos;
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::Expr<(), ()>,
        &Option<aast::Expr<(), ()>>,
        &aast::Expr<(), ()>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.get_codebase();

    let mut if_scope = IfScope::new();

    let if_conditional_scope = if_conditional_analyzer::analyze(
        statements_analyzer,
        expr.0,
        analysis_data,
        context,
        &mut if_scope,
    )?;

    analysis_data.copy_effects(expr.0.pos(), pos);

    add_branch_dataflow(statements_analyzer, expr.0, analysis_data);

    let mut if_context = if_conditional_scope.if_body_context;
    let post_if_context = if_conditional_scope.post_if_context;
    *context = if_conditional_scope.outer_context;
    let mut cond_referenced_var_ids = if_conditional_scope.cond_referenced_var_ids;

    let cond_object_id = (
        expr.0.pos().start_offset() as u32,
        expr.0.pos().end_offset() as u32,
    );

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );

    let if_clauses = formula_generator::get_formula(
        cond_object_id,
        cond_object_id,
        expr.0,
        &assertion_context,
        analysis_data,
        false,
        false,
    );

    let mut if_clauses = if let Ok(if_clauses) = if_clauses {
        if_clauses
    } else {
        vec![]
    };

    if if_clauses.len() > 200 {
        if_clauses = Vec::new();
    }

    let mut mixed_var_ids = Vec::new();

    for (var_id, var_type) in &if_context.vars_in_scope {
        if var_type.is_mixed() && context.vars_in_scope.contains_key(var_id) {
            mixed_var_ids.push(var_id);
        }
    }

    if_clauses = if_clauses
        .into_iter()
        .map(|c| {
            let keys = &c
                .possibilities.keys()
                .collect::<Vec<&String>>();

            let mut new_mixed_var_ids = vec![];
            for i in mixed_var_ids.clone() {
                if !keys.contains(&i) {
                    new_mixed_var_ids.push(i);
                }
            }
            mixed_var_ids = new_mixed_var_ids;

            for key in keys {
                for mixed_var_id in &mixed_var_ids {
                    if var_has_root(key, mixed_var_id) {
                        return Clause::new(
                            BTreeMap::new(),
                            cond_object_id,
                            cond_object_id,
                            Some(true),
                            None,
                            None,
                        );
                    }
                }
            }

            c
        })
        .collect::<Vec<Clause>>();

    // this will see whether any of the clauses in set A conflict with the clauses in set B
    algebra_analyzer::check_for_paradox(
        statements_analyzer,
        &context.clauses,
        &if_clauses,
        analysis_data,
        expr.0.pos(),
        &context.function_context.calling_functionlike_id,
    );

    let mut ternary_clauses = if_clauses.clone();
    ternary_clauses.extend(
        context
            .clauses
            .iter()
            .map(|v| (**v).clone())
            .collect::<Vec<_>>(),
    );

    if !if_context.reconciled_expression_clauses.is_empty() {
        let reconciled_expression_clauses = if_context
            .reconciled_expression_clauses
            .iter()
            .map(|v| &**v)
            .collect::<FxHashSet<_>>();

        ternary_clauses.retain(|c| !reconciled_expression_clauses.contains(c));

        if ternary_clauses.len() == 1
            && ternary_clauses.first().unwrap().wedge
            && ternary_clauses.first().unwrap().possibilities.is_empty()
        {
            ternary_clauses = Vec::new();
            if_context.reconciled_expression_clauses = Vec::new();
        }
    }

    let (reconcilable_if_types, active_if_types) = hakana_algebra::get_truths_from_formula(
        ternary_clauses.iter().collect(),
        Some(cond_object_id),
        &mut cond_referenced_var_ids,
    );

    if_scope.reasonable_clauses = ternary_clauses.into_iter().map(Rc::new).collect();

    if let Ok(negated_if_clauses) = hakana_algebra::negate_formula(if_clauses) {
        if_scope.negated_clauses = negated_if_clauses;
    } else {
        if_scope.negated_clauses = if let Ok(new_negated_clauses) = formula_generator::get_formula(
            cond_object_id,
            cond_object_id,
            &aast::Expr(
                (),
                expr.0 .1.clone(),
                aast::Expr_::Unop(Box::new((Uop::Unot, expr.0.clone()))),
            ),
            &assertion_context,
            analysis_data,
            false,
            false,
        ) {
            new_negated_clauses
        } else {
            Vec::new()
        };
    }

    let negated_clauses = hakana_algebra::simplify_cnf({
        let mut c = context.clauses.iter().map(|v| &**v).collect::<Vec<_>>();
        c.extend(if_scope.negated_clauses.iter());
        c
    });

    let (new_negated_types, _) = hakana_algebra::get_truths_from_formula(
        negated_clauses.iter().collect(),
        None,
        &mut FxHashSet::default(),
    );

    if_scope.negated_types = new_negated_types;

    // if the if has an || in the conditional, we cannot easily reason about it
    if !reconcilable_if_types.is_empty() {
        let mut changed_var_ids = FxHashSet::default();

        reconciler::reconcile_keyed_types(
            &reconcilable_if_types,
            active_if_types,
            &mut if_context,
            &mut changed_var_ids,
            &cond_referenced_var_ids,
            statements_analyzer,
            analysis_data,
            expr.0.pos(),
            true,
            false,
            &FxHashMap::default(),
        );
    }

    // we calculate the vars redefined in a hypothetical else statement to determine
    // which vars of the if we can safely change
    //let mut pre_assignment_else_redefined_vars = &FxHashMap::default();

    if_context.reconciled_expression_clauses = Vec::new();

    let stmt_cond_type = analysis_data.get_rc_expr_type(expr.0.pos()).cloned();

    let mut lhs_type = None;

    let mut changed_var_ids = FxHashSet::default();

    let mut temp_else_context = post_if_context.clone();
    // Check if there is an expression for the true case
    if let Some(if_branch) = expr.1 {
        expression_analyzer::analyze(
            statements_analyzer,
            if_branch,
            analysis_data,
            &mut if_context,
            if_body_context,
        )?;

        analysis_data.combine_effects(if_branch.pos(), pos, pos);

        let mut new_referenced_var_ids = context.cond_referenced_var_ids.clone();
        new_referenced_var_ids.extend(if_context.cond_referenced_var_ids.clone());

        temp_else_context = post_if_context;

        context.cond_referenced_var_ids = new_referenced_var_ids;

        if let Some(stmt_if_type) = analysis_data.get_expr_type(if_branch.pos()) {
            lhs_type = Some(stmt_if_type.clone());
        }
    } else if let Some(cond_type) = &stmt_cond_type {
        let if_return_type_reconciled = assertion_reconciler::reconcile(
            &Assertion::Truthy,
            Some(cond_type),
            false,
            None,
            statements_analyzer,
            analysis_data,
            context.inside_loop,
            None,
            &None,
            false,
            false,
            &FxHashMap::default(),
        );
        lhs_type = Some(if_return_type_reconciled);
    }

    if !if_scope.negated_types.is_empty() {
        reconciler::reconcile_keyed_types(
            &if_scope.negated_types,
            BTreeMap::new(),
            &mut temp_else_context,
            &mut changed_var_ids,
            &FxHashSet::default(),
            statements_analyzer,
            analysis_data,
            expr.2.pos(),
            true,
            false,
            &FxHashMap::default(),
        );

        temp_else_context.clauses = ScopeContext::remove_reconciled_clause_refs(
            &temp_else_context.clauses,
            &changed_var_ids,
        )
        .0;
    }

    expression_analyzer::analyze(
        statements_analyzer,
        expr.2,
        analysis_data,
        &mut temp_else_context,
        if_body_context,
    )?;

    analysis_data.combine_effects(expr.2.pos(), pos, pos);

    // we do this here so it's accurate, analysis_data might get overwritten for the same position later
    let stmt_else_type = analysis_data.get_rc_expr_type(expr.2.pos()).cloned();

    let assign_var_ifs = if_context.assigned_var_ids.clone();
    let assign_var_else = temp_else_context.assigned_var_ids.clone();

    let assign_all = assign_var_ifs
        .clone()
        .into_iter()
        .filter(|(k, _)| assign_var_else.contains_key(k))
        .collect::<FxHashMap<_, _>>();

    //if the same var was assigned in both branches
    for var_id in assign_all.iter() {
        if if_context.vars_in_scope.contains_key(var_id.0)
            && temp_else_context.vars_in_scope.contains_key(var_id.0)
        {
            context.vars_in_scope.insert(
                var_id.0.clone(),
                Rc::new(combine_union_types(
                    &if_context.vars_in_scope[var_id.0],
                    &temp_else_context.vars_in_scope[var_id.0],
                    codebase,
                    false,
                )),
            );
        }
    }

    let mut removed_vars = FxHashSet::default();

    let redef_var_ifs = if_context
        .get_redefined_vars(&context.vars_in_scope, false, &mut removed_vars).into_keys()
        .collect::<FxHashSet<_>>();
    let redef_var_else = temp_else_context
        .get_redefined_vars(&context.vars_in_scope, false, &mut removed_vars).into_keys()
        .collect::<FxHashSet<_>>();

    let redef_all = redef_var_ifs
        .iter()
        .filter(|k| redef_var_else.contains(*k))
        .collect::<FxHashSet<_>>();

    //these vars were changed in both branches
    for redef_var_id in redef_all {
        context.vars_in_scope.insert(
            redef_var_id.clone(),
            Rc::new(combine_union_types(
                &if_context.vars_in_scope[redef_var_id],
                &temp_else_context.vars_in_scope[redef_var_id],
                codebase,
                false,
            )),
        );
    }

    //these vars were changed in the if and existed before
    for redef_var_ifs_id in &redef_var_ifs {
        if context.vars_in_scope.contains_key(redef_var_ifs_id) {
            if temp_else_context
                .vars_in_scope
                .contains_key(redef_var_ifs_id)
            {
                context.vars_in_scope.insert(
                    redef_var_ifs_id.clone(),
                    Rc::new(combine_union_types(
                        &context.vars_in_scope[redef_var_ifs_id],
                        &temp_else_context.vars_in_scope[redef_var_ifs_id],
                        codebase,
                        false,
                    )),
                );
            } else {
                context.vars_in_scope.remove(redef_var_ifs_id);
            }
        }
    }

    //these vars were changed in the else and existed before
    for redef_var_else_id in &redef_var_else {
        if context.vars_in_scope.contains_key(redef_var_else_id) {
            context.vars_in_scope.insert(
                redef_var_else_id.clone(),
                Rc::new(combine_union_types(
                    &context.vars_in_scope[redef_var_else_id],
                    &temp_else_context.vars_in_scope[redef_var_else_id],
                    codebase,
                    false,
                )),
            );
        }
    }

    context
        .cond_referenced_var_ids
        .extend(temp_else_context.cond_referenced_var_ids);

    if let Some((lhs_type, stmt_else_type)) = if let Some(lhs_type) = lhs_type {
        if let Some(stmt_else_type) = stmt_else_type {
            if stmt_else_type.is_nothing() {
                *context = if_context;
            }
            Some((lhs_type, stmt_else_type))
        } else {
            None
        }
    } else {
        None
    } {
        if if let Some(stmt_cond_type) = stmt_cond_type {
            if stmt_cond_type.is_always_falsy() {
                analysis_data.set_rc_expr_type(pos, stmt_else_type.clone());
                false
            } else if stmt_cond_type.is_always_truthy() {
                analysis_data.set_expr_type(pos, lhs_type.clone());
                false
            } else {
                true
            }
        } else {
            true
        } {
            let union_type = if stmt_else_type.is_nothing() {
                lhs_type
            } else {
                add_union_type((*stmt_else_type).clone(), &lhs_type, codebase, false)
            };

            analysis_data.set_expr_type(pos, union_type);
        }
    } else {
        analysis_data.set_expr_type(pos, get_mixed_any());
    }

    Ok(())
}
