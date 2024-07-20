use crate::{
    scope::{
        control_action::ControlAction, if_scope::IfScope, loop_scope::LoopScope, var_has_root,
        BlockContext,
    },
    stmt_analyzer::AnalysisError,
};
use hakana_algebra::Clause;
use hakana_reflection_info::{
    analysis_result::Replacement, issue::IssueKind, EFFECT_PURE, EFFECT_READ_GLOBALS,
    EFFECT_READ_PROPS,
};
use hakana_type::{combine_union_types, extend_dataflow_uniquely};
use oxidized::{aast, ast::Uop, ast_defs::Pos};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc};

use crate::{
    algebra_analyzer, formula_generator, function_analysis_data::FunctionAnalysisData, reconciler,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
};

use super::{
    else_analyzer, if_analyzer,
    if_conditional_analyzer::{self, add_branch_dataflow},
};

/**
System of type substitution and deletion

for example

x: A|null

if (x)
  (x: A)
  x = B  -- effects: remove A from the type of x, add B
else
  (x: null)
  x = C  -- effects: remove null from the type of x, add C


x: A|null

if (!x)
  (x: null)
  throw new Exception -- effects: remove null from the type of x
*/
pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &aast::Block<(), ()>,
        &aast::Block<(), ()>,
    ),
    stmt_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    loop_scope: &mut Option<LoopScope>,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.get_file_analyzer().codebase;

    let mut if_scope = IfScope::new();

    if stmt.0 .2.is_binop() || (stmt.0 .2.is_unop() && stmt.0 .2.as_unop().unwrap().1 .2.is_binop())
    {
        let mut none_hashset = FxHashSet::default();
        none_hashset.insert(ControlAction::None);
    }

    let if_conditional_scope = if_conditional_analyzer::analyze(
        statements_analyzer,
        stmt.0,
        analysis_data,
        context,
        &mut if_scope,
    )?;

    add_branch_dataflow(statements_analyzer, stmt.0, analysis_data);

    let mut if_body_context = if_conditional_scope.if_body_context;
    let post_if_context = if_conditional_scope.post_if_context;

    *context = if_conditional_scope.outer_context;

    let mut mixed_var_ids = Vec::new();

    for (var_id, var_type) in &if_body_context.locals {
        if var_type.is_mixed() && context.locals.contains_key(var_id) {
            mixed_var_ids.push(var_id);
        }
    }

    let cond_object_id = (
        stmt.0.pos().start_offset() as u32,
        stmt.0.pos().end_offset() as u32,
    );

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );

    let if_clauses = formula_generator::get_formula(
        cond_object_id,
        cond_object_id,
        stmt.0,
        &assertion_context,
        analysis_data,
        false,
        false,
    );

    let mut if_clauses = if_clauses.unwrap_or_default();

    if if_clauses.len() > 200 {
        if_clauses = Vec::new();
    }

    if_clauses = remove_clauses_with_mixed_vars(if_clauses, mixed_var_ids, cond_object_id);

    let entry_clauses = context
        .clauses
        .iter()
        .map(|v| (**v).clone())
        .collect::<Vec<_>>();

    // this will see whether any of the clauses in set A conflict with the clauses in set B
    algebra_analyzer::check_for_paradox(
        statements_analyzer,
        &context.clauses,
        &if_clauses,
        analysis_data,
        stmt.0.pos(),
        &context.function_context.calling_functionlike_id,
    );

    let if_clauses = hakana_algebra::simplify_cnf(if_clauses.iter().collect());

    let mut if_context_clauses = entry_clauses.clone();
    if_context_clauses.extend(if_clauses.clone());

    if_body_context.clauses = if entry_clauses.is_empty() {
        if_clauses.clone()
    } else {
        hakana_algebra::simplify_cnf(if_context_clauses.iter().collect())
    }
    .into_iter()
    .map(|v| Rc::new(v.clone()))
    .collect();

    if !if_body_context.reconciled_expression_clauses.is_empty() {
        let reconciled_expression_clauses = if_body_context
            .reconciled_expression_clauses
            .iter()
            .collect::<FxHashSet<_>>();

        if_body_context
            .clauses
            .retain(|c| !reconciled_expression_clauses.contains(c));

        if if_body_context.clauses.len() == 1
            && if_body_context.clauses.first().unwrap().wedge
            && if_body_context
                .clauses
                .first()
                .unwrap()
                .possibilities
                .is_empty()
        {
            if_body_context.clauses = Vec::new();
            if_body_context.reconciled_expression_clauses = Vec::new();
        }
    }

    // define this before we alter local clauses after reconciliation
    if_scope
        .reasonable_clauses
        .clone_from(&if_body_context.clauses);

    if let Ok(negated_if_clauses) = hakana_algebra::negate_formula(if_clauses) {
        if_scope.negated_clauses = negated_if_clauses;
    } else {
        if_scope.negated_clauses = formula_generator::get_formula(
            cond_object_id,
            cond_object_id,
            &aast::Expr(
                (),
                stmt.0 .1.clone(),
                aast::Expr_::Unop(Box::new((Uop::Unot, stmt.0.clone()))),
            ),
            &assertion_context,
            analysis_data,
            false,
            false,
        )
        .unwrap_or_default();
    }

    let (new_negated_types, _) = hakana_algebra::get_truths_from_formula(
        hakana_algebra::simplify_cnf({
            let mut c = context.clauses.iter().map(|v| &**v).collect::<Vec<_>>();
            c.extend(if_scope.negated_clauses.iter());
            c
        })
        .iter()
        .collect(),
        None,
        &mut FxHashSet::default(),
    );

    if_scope.negated_types = new_negated_types;

    let mut temp_else_context = post_if_context.clone();

    let mut changed_var_ids = FxHashSet::default();

    if !if_scope.negated_types.is_empty() {
        reconciler::reconcile_keyed_types(
            &if_scope.negated_types,
            BTreeMap::new(),
            &mut temp_else_context,
            &mut changed_var_ids,
            &FxHashSet::default(),
            statements_analyzer,
            analysis_data,
            stmt.0.pos(),
            true,
            false,
            &FxHashMap::default(),
        );
    }

    // we calculate the vars redefined in a hypothetical else statement to determine
    // which vars of the if we can safely change
    let mut pre_assignment_else_redefined_vars = FxHashMap::default();

    let mut removed_var_ids = FxHashSet::default();

    let temp_else_redefined_vars =
        temp_else_context.get_redefined_locals(&context.locals, true, &mut removed_var_ids);

    for (var_id, redefined_type) in temp_else_redefined_vars {
        if changed_var_ids.contains(&var_id) {
            pre_assignment_else_redefined_vars.insert(var_id, redefined_type);
        }
    }

    // check the if
    if_analyzer::analyze(
        statements_analyzer,
        stmt,
        analysis_data,
        &mut if_scope,
        if_conditional_scope.cond_referenced_var_ids,
        &mut if_body_context,
        context,
        loop_scope,
    )?;

    let mut else_context = post_if_context.clone();

    else_analyzer::analyze(
        statements_analyzer,
        stmt.0.pos(),
        stmt.2,
        analysis_data,
        &mut if_scope,
        &mut else_context,
        context,
        loop_scope,
    )?;

    if !if_scope.if_actions.is_empty() && !if_scope.if_actions.contains(&ControlAction::None) {
        context.clauses = else_context.clauses;
        for (var_id, var_type) in else_context.locals {
            context.locals.insert(var_id, var_type);
        }
        context.allow_taints = else_context.allow_taints;

        // TODO handle removal of mixed issues when followed by quick assertion
    }

    context
        .locals
        .retain(|var_id, _| !if_scope.removed_var_ids.contains(var_id));

    if !if_scope.final_actions.contains(&ControlAction::None) {
        context.has_returned = true;
    }

    if let Some(loop_scope) = loop_scope.as_mut() {
        loop_scope.final_actions.extend(if_scope.final_actions);
    }

    context
        .possibly_assigned_var_ids
        .extend(if_scope.possibly_assigned_var_ids);

    // vars can only be defined/redefined if there was an else (defined in every block)
    context
        .assigned_var_ids
        .extend(if_scope.assigned_var_ids.unwrap_or_default());

    if let Some(new_vars) = if_scope.new_vars {
        for (var_id, var_type) in new_vars {
            context.locals.insert(var_id, Rc::new(var_type));
        }
    }

    let mut reasonable_clauses = if_scope.reasonable_clauses;

    if let Some(redefined_vars) = if_scope.redefined_vars {
        for (var_id, var_type) in redefined_vars {
            reasonable_clauses = BlockContext::filter_clauses(
                &var_id,
                reasonable_clauses,
                Some(&var_type),
                Some(statements_analyzer),
                analysis_data,
            );

            if_scope.updated_vars.insert(var_id.clone());
            context.locals.insert(var_id.clone(), Rc::new(var_type));
        }
    }

    let reasonable_clauses_len = reasonable_clauses.len();

    if reasonable_clauses_len > 0
        && (reasonable_clauses_len > 1 || !reasonable_clauses.first().unwrap().wedge)
    {
        reasonable_clauses.extend(context.clauses.clone());
        context.clauses = hakana_algebra::simplify_cnf(
            reasonable_clauses
                .into_iter()
                .map(|v| (*v).clone())
                .collect::<Vec<_>>()
                .iter()
                .collect(),
        )
        .into_iter()
        .map(|v| Rc::new(v.clone()))
        .collect();
    }

    for (var_id, var_type) in if_scope.possibly_redefined_vars {
        if let Some(existing_var_type) = context.locals.get(&var_id) {
            if !if_scope.updated_vars.contains(&var_id) {
                let combined_type =
                    combine_union_types(existing_var_type, &var_type, codebase, false);

                if combined_type != var_type {
                    context.remove_descendants(&var_id, &combined_type, None, None, analysis_data);
                }

                context.locals.insert(var_id, Rc::new(combined_type));
            } else {
                let mut existing_var_type = (**existing_var_type).clone();
                extend_dataflow_uniquely(
                    &mut existing_var_type.parent_nodes,
                    var_type.parent_nodes,
                );
                context.locals.insert(var_id, Rc::new(existing_var_type));
            }
        }
    }

    if statements_analyzer
        .get_config()
        .issues_to_fix
        .contains(&IssueKind::EmptyBlock)
        && stmt.1.is_empty()
        && stmt.2.is_empty()
    {
        let effects = analysis_data
            .expr_effects
            .get(&(
                stmt.0 .1.start_offset() as u32,
                stmt.0 .1.end_offset() as u32,
            ))
            .unwrap_or(&0);

        if let EFFECT_PURE | EFFECT_READ_GLOBALS | EFFECT_READ_PROPS = *effects {
            analysis_data.add_replacement(
                (
                    stmt_pos.to_raw_span().start.beg_of_line() as u32,
                    stmt_pos.end_offset() as u32 + 1,
                ),
                Replacement::Remove,
            );
        } else {
            if !analysis_data.add_replacement(
                (
                    stmt_pos.start_offset() as u32,
                    stmt.0 .1.start_offset() as u32,
                ),
                Replacement::Remove,
            ) {
                return Ok(());
            }

            analysis_data.add_replacement(
                (stmt.0 .1.end_offset() as u32, stmt_pos.end_offset() as u32),
                Replacement::Substitute(";".to_string()),
            );
        }
    }

    Ok(())
}

pub(crate) fn remove_clauses_with_mixed_vars(
    if_clauses: Vec<Clause>,
    mut mixed_var_ids: Vec<&String>,
    cond_object_id: (u32, u32),
) -> Vec<Clause> {
    if_clauses
        .into_iter()
        .map(|c| {
            let keys = c.possibilities.keys().collect::<Vec<_>>();

            let mut new_mixed_var_ids = vec![];
            for i in &mixed_var_ids {
                if !keys.contains(i) {
                    new_mixed_var_ids.push(*i);
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
        .collect::<Vec<Clause>>()
}
