use super::{control_analyzer, if_analyzer};
use crate::reconciler::reconciler;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::control_action::ControlAction;
use crate::scope_context::loop_scope::LoopScope;
use crate::scope_context::{if_scope::IfScope, ScopeContext};
use crate::{statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo};
use oxidized::aast;
use oxidized::aast::Pos;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    if_cond_pos: &Pos,
    stmts: &aast::Block<(), ()>,
    tast_info: &mut TastInfo,
    if_scope: &mut IfScope,
    else_context: &mut ScopeContext,
    outer_context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) -> bool {
    if stmts.is_empty() && if_scope.negated_clauses.is_empty() && else_context.clauses.is_empty() {
        if_scope.final_actions.insert(ControlAction::None);
        if_scope.assigned_var_ids = Some(HashMap::new());
        if_scope.new_vars = Some(BTreeMap::new());
        if_scope.redefined_vars = Some(HashMap::new());
        if_scope.reasonable_clauses = Vec::new();

        return true;
    }

    else_context
        .clauses
        .extend(if_scope.negated_clauses.iter().map(|v| Rc::new(v.clone())));

    let else_clauses =
        hakana_algebra::simplify_cnf(else_context.clauses.iter().map(|v| &**v).collect());

    let else_types = hakana_algebra::get_truths_from_formula(
        else_clauses.iter().collect(),
        None,
        &mut HashSet::new(),
    )
    .0;

    else_context.clauses = else_clauses.into_iter().map(|v| Rc::new(v)).collect();

    let original_context = else_context.clone();

    if !else_types.is_empty() {
        let mut changed_var_ids = HashSet::new();

        reconciler::reconcile_keyed_types(
            &else_types,
            BTreeMap::new(),
            else_context,
            &mut changed_var_ids,
            &HashSet::new(),
            statements_analyzer,
            tast_info,
            if_cond_pos,
            false,
            false,
            &HashMap::new(),
        );

        else_context.clauses =
            ScopeContext::remove_reconciled_clause_refs(&else_context.clauses, &changed_var_ids).0;
    }

    let pre_stmts_assigned_var_ids = else_context.assigned_var_ids.clone();
    else_context.assigned_var_ids = HashMap::new();

    let pre_possibly_assigned_var_ids = else_context.possibly_assigned_var_ids.clone();
    else_context.possibly_assigned_var_ids = HashSet::new();

    if !statements_analyzer.analyze(stmts, tast_info, else_context, loop_scope) {
        return false;
    }

    for var_id in &else_context.parent_remove_vars {
        outer_context.remove_var_from_conflicting_clauses(var_id, None, None, tast_info);
    }

    let new_assigned_var_ids = else_context.assigned_var_ids.clone();
    else_context
        .assigned_var_ids
        .extend(pre_stmts_assigned_var_ids.clone());

    let new_possibly_assigned_var_ids = else_context.possibly_assigned_var_ids.clone();
    else_context
        .possibly_assigned_var_ids
        .extend(pre_possibly_assigned_var_ids.clone());

    let final_actions = if !stmts.is_empty() {
        control_analyzer::get_control_actions(
            statements_analyzer.get_codebase(),
            statements_analyzer.get_file_analyzer().resolved_names,
            stmts,
            Some(tast_info),
            Vec::new(),
            true,
        )
    } else {
        let mut none_set = HashSet::new();
        none_set.insert(ControlAction::None);
        none_set
    };

    let has_ending_statements =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::End);

    let has_leaving_statements = has_ending_statements
        || final_actions.len() > 0 && !final_actions.contains(&ControlAction::None);

    if_scope.final_actions.extend(final_actions);

    if !has_leaving_statements {
        if_analyzer::update_if_scope(
            statements_analyzer.get_codebase(),
            if_scope,
            else_context,
            &original_context,
            &new_assigned_var_ids,
            &new_possibly_assigned_var_ids,
            HashSet::new(),
            true,
        );

        if_scope.reasonable_clauses = Vec::new();
    }

    if !has_leaving_statements {
        if_scope
            .possibly_assigned_var_ids
            .extend(new_possibly_assigned_var_ids);
    }

    true
}
