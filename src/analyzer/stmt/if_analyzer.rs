use super::control_analyzer;
use crate::reconciler;
use crate::scope::control_action::ControlAction;
use crate::scope::loop_scope::LoopScope;
use crate::scope::var_has_root;
use crate::scope::{if_scope::IfScope, BlockContext};
use crate::stmt_analyzer::AnalysisError;
use crate::{
    function_analysis_data::FunctionAnalysisData, statements_analyzer::StatementsAnalyzer,
};
use hakana_algebra::clause::ClauseKey;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::ttype::add_union_type;
use hakana_code_info::var_name::VarName;
use oxidized::aast;
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &aast::Block<(), ()>,
        &aast::Block<(), ()>,
    ),
    analysis_data: &mut FunctionAnalysisData,
    if_scope: &mut IfScope,
    mut cond_referenced_var_ids: FxHashSet<VarName>,
    if_context: &mut BlockContext,
    outer_context: &mut BlockContext,
    loop_scope: &mut Option<LoopScope>,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    let cond_object_id = (
        stmt.0.pos().start_offset() as u32,
        stmt.0.pos().end_offset() as u32,
    );

    let (reconcilable_if_types, active_if_types) = hakana_algebra::get_truths_from_formula(
        if_context.clauses.iter().map(|v| &**v).collect(),
        Some(cond_object_id),
        &mut cond_referenced_var_ids,
    );

    if !outer_context
        .clauses
        .iter()
        .any(|clause| !clause.possibilities.is_empty())
    {
        let omit_keys =
            outer_context
                .clauses
                .iter()
                .fold(FxHashSet::default(), |mut acc, clause| {
                    for k in clause.possibilities.keys() {
                        if let ClauseKey::Name(var_name) = k {
                            acc.insert(var_name);
                        }
                    }
                    acc
                });

        let (outer_context_truths, _) = hakana_algebra::get_truths_from_formula(
            outer_context.clauses.iter().map(|v| &**v).collect(),
            None,
            &mut FxHashSet::default(),
        );

        cond_referenced_var_ids
            .retain(|k| !omit_keys.contains(k) || outer_context_truths.contains_key(k));
    }

    // if the if has an || in the conditional, we cannot easily reason about it
    if !reconcilable_if_types.is_empty() {
        let mut changed_var_ids = FxHashSet::default();

        reconciler::reconcile_keyed_types(
            &reconcilable_if_types,
            active_if_types,
            if_context,
            &mut changed_var_ids,
            &cond_referenced_var_ids,
            statements_analyzer,
            analysis_data,
            stmt.0.pos(),
            true,
            false,
            &FxHashMap::default(),
        );

        if !changed_var_ids.is_empty() {
            if_context.clauses =
                BlockContext::remove_reconciled_clause_refs(&if_context.clauses, &changed_var_ids)
                    .0;

            for changed_var_id in &changed_var_ids {
                for (var_id, _) in if_context.locals.clone() {
                    if var_has_root(&var_id, changed_var_id)
                        && !changed_var_ids.contains(&var_id)
                        && !cond_referenced_var_ids.contains(&var_id)
                    {
                        if_context.locals.remove(&var_id);
                    }
                }
            }
        }
    }

    if_context.reconciled_expression_clauses = Vec::new();

    let assigned_var_ids = if_context.assigned_var_ids.clone();
    let possibly_assigned_var_ids = if_context.possibly_assigned_var_ids.clone();

    if_context.assigned_var_ids.clear();
    if_context.possibly_assigned_var_ids.clear();

    statements_analyzer.analyze(&stmt.1 .0, analysis_data, if_context, loop_scope)?;

    let final_actions = control_analyzer::get_control_actions(
        codebase,
        statements_analyzer.interner,
        statements_analyzer.file_analyzer.resolved_names,
        &stmt.1 .0,
        analysis_data,
        Vec::new(),
        true,
    );

    let has_ending_statements =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::End);

    let has_leaving_statements = has_ending_statements
        || !final_actions.is_empty() && !final_actions.contains(&ControlAction::None);

    let has_break_statement =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::Break);

    if_scope.if_actions.clone_from(&final_actions);
    if_scope.final_actions = final_actions;

    let new_assigned_var_ids = if_context.assigned_var_ids.clone();
    let new_possibly_assigned_var_ids = if_context.possibly_assigned_var_ids.clone();

    if_context.assigned_var_ids.extend(assigned_var_ids.clone());
    if_context
        .possibly_assigned_var_ids
        .extend(possibly_assigned_var_ids.clone());

    if !has_leaving_statements {
        update_if_scope(
            codebase,
            if_scope,
            if_context,
            outer_context,
            &new_assigned_var_ids,
            &new_possibly_assigned_var_ids,
            if_scope.if_cond_changed_var_ids.clone(),
            true,
        );

        let mut reasonable_clauses = if_scope.reasonable_clauses.clone();

        if !reasonable_clauses.is_empty() {
            for (var_id, _) in new_assigned_var_ids {
                reasonable_clauses = BlockContext::filter_clauses(
                    &var_id,
                    reasonable_clauses,
                    if let Some(t) = if_context.locals.get(&var_id) {
                        Some(t)
                    } else {
                        None
                    },
                    Some(statements_analyzer),
                    analysis_data,
                );
            }
        }

        if_scope.reasonable_clauses = reasonable_clauses;
    } else if !has_break_statement {
        if_scope.reasonable_clauses = Vec::new();
    }

    Ok(())
}

pub(crate) fn update_if_scope(
    codebase: &CodebaseInfo,
    if_scope: &mut IfScope,
    if_context: &BlockContext,
    outer_context: &BlockContext,
    assigned_var_ids: &FxHashMap<VarName, usize>,
    possibly_assigned_var_ids: &FxHashSet<VarName>,
    newly_reconciled_var_ids: FxHashSet<VarName>,
    update_new_vars: bool,
) {
    let redefined_vars = if_context.get_redefined_locals(
        &outer_context.locals,
        false,
        &mut if_scope.removed_var_ids,
    );

    if let Some(ref mut new_vars) = if_scope.new_vars {
        for (new_var_id, new_type) in new_vars.clone() {
            if let Some(if_var_type) = if_context.locals.get(&new_var_id) {
                new_vars.insert(
                    new_var_id,
                    hakana_code_info::ttype::add_union_type(new_type, if_var_type, codebase, false),
                );
            } else {
                new_vars.remove(&new_var_id);
            }
        }
    } else if update_new_vars {
        if_scope.new_vars = Some(
            if_context
                .locals
                .iter()
                .filter(|(k, _)| !outer_context.locals.contains_key(*k))
                .map(|(k, v)| (k.clone(), (**v).clone()))
                .collect(),
        );
    }

    let mut possibly_redefined_vars = redefined_vars.clone();

    for (var_id, _) in possibly_redefined_vars.clone() {
        if !possibly_assigned_var_ids.contains(&var_id)
            && newly_reconciled_var_ids.contains(&var_id)
        {
            possibly_redefined_vars.remove(&var_id);
        }
    }

    if let Some(ref mut scope_assigned_var_ids) = if_scope.assigned_var_ids {
        *scope_assigned_var_ids = assigned_var_ids
            .clone()
            .into_iter()
            .filter(|(k, _)| scope_assigned_var_ids.contains_key(k))
            .collect::<FxHashMap<_, _>>();
    } else {
        if_scope.assigned_var_ids = Some(assigned_var_ids.clone());
    }

    if_scope
        .possibly_assigned_var_ids
        .extend(possibly_assigned_var_ids.clone());

    if let Some(ref mut scope_redefined_vars) = if_scope.redefined_vars {
        for (redefined_var_id, scope_redefined_type) in scope_redefined_vars.clone() {
            if let Some(redefined_var_type) = redefined_vars.get(&redefined_var_id) {
                scope_redefined_vars.insert(
                    redefined_var_id.clone(),
                    hakana_code_info::ttype::combine_union_types(
                        redefined_var_type,
                        &scope_redefined_type,
                        codebase,
                        false,
                    ),
                );

                if let Some(outer_context_type) = outer_context.locals.get(&redefined_var_id) {
                    if scope_redefined_type == **outer_context_type {
                        scope_redefined_vars.remove(&redefined_var_id);
                    }
                }
            } else {
                scope_redefined_vars.remove(&redefined_var_id);
            }
        }

        let mut new_scoped_possibly_redefined_vars = FxHashMap::default();

        for (var_id, possibly_redefined_type) in possibly_redefined_vars {
            if let Some(existing_type) = if_scope.possibly_redefined_vars.get(&var_id) {
                new_scoped_possibly_redefined_vars.insert(
                    var_id.clone(),
                    add_union_type(possibly_redefined_type, existing_type, codebase, false),
                );
            } else {
                new_scoped_possibly_redefined_vars.insert(var_id, possibly_redefined_type);
            }
        }

        if_scope
            .possibly_redefined_vars
            .extend(new_scoped_possibly_redefined_vars);
    } else {
        if_scope.redefined_vars = Some(redefined_vars);
        if_scope.possibly_redefined_vars = possibly_redefined_vars;
    }
}
