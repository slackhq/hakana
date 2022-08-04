use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use crate::reconciler::{assertion_reconciler, reconciler};
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::if_scope::IfScope;
use crate::scope_context::{var_has_root, ScopeContext};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer::{self, add_branch_dataflow};
use crate::typed_ast::TastInfo;
use crate::{algebra_analyzer, expression_analyzer, formula_generator};
use hakana_algebra::Clause;
use hakana_reflection_info::assertion::Assertion;
use hakana_type::{add_union_type, combine_union_types, get_mixed_any};
use oxidized::aast;
use oxidized::ast_defs::Uop;
use oxidized::pos::Pos;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::Expr<(), ()>,
        &Option<aast::Expr<(), ()>>,
        &aast::Expr<(), ()>,
    ),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let codebase = statements_analyzer.get_codebase();

    let mut if_scope = IfScope::new();

    let if_conditional_scope = if_conditional_analyzer::analyze(
        statements_analyzer,
        &expr.0,
        tast_info,
        context,
        &mut if_scope,
    );

    add_branch_dataflow(statements_analyzer, &expr.0, tast_info);

    let mut if_context = if_conditional_scope.if_body_context;
    let post_if_context = if_conditional_scope.post_if_context;
    *context = if_conditional_scope.outer_context;
    let mut cond_referenced_var_ids = if_conditional_scope.cond_referenced_var_ids;
    let _assigned_in_conditional_var_ids = &if_conditional_scope.assigned_in_conditional_var_ids;

    let cond_object_id = (expr.0.pos().start_offset(), expr.0.pos().end_offset());

    let assertion_context =
        statements_analyzer.get_assertion_context(context.function_context.calling_class.as_ref());

    let if_clauses = formula_generator::get_formula(
        cond_object_id,
        cond_object_id,
        expr.0,
        &assertion_context,
        tast_info,
        false,
        false,
    );

    let mut if_clauses = if let Err(_) = if_clauses {
        Vec::new()
    } else {
        if_clauses.unwrap()
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
                .possibilities
                .iter()
                .map(|(k, _)| k)
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
                            None,
                        );
                    }
                }
            }

            return c;
        })
        .collect::<Vec<Clause>>();

    // this will see whether any of the clauses in set A conflict with the clauses in set B
    algebra_analyzer::check_for_paradox(
        statements_analyzer,
        &context.clauses,
        &if_clauses,
        tast_info,
        expr.0.pos(),
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
            .collect::<HashSet<_>>();

        ternary_clauses.retain(|c| !reconciled_expression_clauses.contains(c));

        if ternary_clauses.len() == 1
            && ternary_clauses.get(0).unwrap().wedge
            && ternary_clauses.get(0).unwrap().possibilities.is_empty()
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

    if_scope.reasonable_clauses = ternary_clauses.into_iter().map(|v| Rc::new(v)).collect();

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
            tast_info,
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
        &mut HashSet::new(),
    );

    if_scope.negated_types = new_negated_types;

    // if the if has an || in the conditional, we cannot easily reason about it
    if !reconcilable_if_types.is_empty() {
        let mut changed_var_ids = HashSet::new();

        reconciler::reconcile_keyed_types(
            &reconcilable_if_types,
            active_if_types,
            &mut if_context,
            &mut changed_var_ids,
            &cond_referenced_var_ids,
            statements_analyzer,
            tast_info,
            expr.0.pos(),
            true,
            false,
            &HashMap::new(),
        );
    }

    // we calculate the vars redefined in a hypothetical else statement to determine
    // which vars of the if we can safely change
    //let mut pre_assignment_else_redefined_vars = &HashMap::new();

    if_context.reconciled_expression_clauses = Vec::new();

    let stmt_cond_type = tast_info.get_expr_type(expr.0.pos()).cloned();

    let mut lhs_type = None;

    let mut changed_var_ids = HashSet::new();

    let mut temp_else_context = post_if_context.clone();
    // Check if there is an expression for the true case
    if let Some(if_branch) = expr.1 {
        if !expression_analyzer::analyze(
            statements_analyzer,
            if_branch,
            tast_info,
            &mut if_context,
            if_body_context,
        ) {
            return false;
        }

        let mut new_referenced_var_ids = context.cond_referenced_var_ids.clone();
        new_referenced_var_ids.extend(if_context.cond_referenced_var_ids.clone());

        temp_else_context = post_if_context;

        context.cond_referenced_var_ids = new_referenced_var_ids;

        if let Some(stmt_if_type) = tast_info.get_expr_type(if_branch.pos()) {
            lhs_type = Some(stmt_if_type.clone());
        }
    } else if let Some(cond_type) = &stmt_cond_type {
        let if_return_type_reconciled = assertion_reconciler::reconcile(
            &Assertion::Truthy,
            Some(cond_type),
            false,
            &None,
            statements_analyzer,
            tast_info,
            context.inside_loop,
            None,
            false,
            &mut reconciler::ReconciliationStatus::Ok,
            false,
            &HashMap::new(),
        );
        lhs_type = Some(if_return_type_reconciled);
    }

    if !if_scope.negated_types.is_empty() {
        reconciler::reconcile_keyed_types(
            &if_scope.negated_types,
            BTreeMap::new(),
            &mut temp_else_context,
            &mut changed_var_ids,
            &HashSet::new(),
            statements_analyzer,
            tast_info,
            expr.2.pos(),
            true,
            false,
            &HashMap::new(),
        );

        temp_else_context.clauses = ScopeContext::remove_reconciled_clause_refs(
            &temp_else_context.clauses,
            &changed_var_ids,
        )
        .0;
    }

    if !expression_analyzer::analyze(
        statements_analyzer,
        &expr.2,
        tast_info,
        &mut temp_else_context,
        if_body_context,
    ) {
        return false;
    }

    // we do this here so it's accurate, tast_info might get overwritten for the same position later
    let stmt_else_type = tast_info.get_expr_type(expr.2.pos()).cloned();

    let assign_var_ifs = if_context.assigned_var_ids.clone();
    let assign_var_else = temp_else_context.assigned_var_ids.clone();

    let assign_all = assign_var_ifs
        .clone()
        .into_iter()
        .filter(|(k, _)| assign_var_else.contains_key(k))
        .collect::<HashMap<_, _>>();

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
                    Some(codebase),
                    false,
                )),
            );
        }
    }

    let redef_var_ifs = if_context
        .get_redefined_vars(&context.vars_in_scope, false)
        .into_iter()
        .map(|(k, _)| k)
        .collect::<HashSet<_>>();
    let redef_var_else = temp_else_context
        .get_redefined_vars(&context.vars_in_scope, false)
        .into_iter()
        .map(|(k, _)| k)
        .collect::<HashSet<_>>();

    let redef_all = redef_var_ifs
        .iter()
        .filter(|k| redef_var_else.contains(*k))
        .collect::<HashSet<_>>();

    //these vars were changed in both branches
    for redef_var_id in redef_all {
        context.vars_in_scope.insert(
            redef_var_id.clone(),
            Rc::new(combine_union_types(
                &if_context.vars_in_scope[redef_var_id],
                &temp_else_context.vars_in_scope[redef_var_id],
                Some(codebase),
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
                        Some(codebase),
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
                    Some(codebase),
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
                tast_info.set_expr_type(&pos, stmt_else_type.clone());
                false
            } else if stmt_cond_type.is_always_truthy() {
                tast_info.set_expr_type(&pos, lhs_type.clone());
                false
            } else {
                true
            }
        } else {
            true
        } {
            let union_type = add_union_type(stmt_else_type, &lhs_type, Some(codebase), false);
            tast_info.set_expr_type(&pos, union_type);
        }
    } else {
        tast_info.set_expr_type(&pos, get_mixed_any());
    }

    true
}
