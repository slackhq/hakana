use std::{collections::BTreeMap, rc::Rc};

use hakana_algebra::Clause;

use hakana_reflection_info::t_union::TUnion;
use hakana_type::combine_union_types;
use oxidized::aast;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    expression_analyzer, formula_generator,
    reconciler::reconciler,
    scope_analyzer::ScopeAnalyzer,
    scope_context::{control_action::ControlAction, loop_scope::LoopScope, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};

use super::{
    control_analyzer,
    if_conditional_analyzer::add_branch_dataflow,
    loop_::{assignment_map_visitor::get_assignment_map, tast_cleaner::clean_nodes},
};

pub(crate) fn analyze<'a>(
    statements_analyzer: &'a StatementsAnalyzer,
    stmts: &Vec<aast::Stmt<(), ()>>,
    pre_conditions: Vec<&aast::Expr<(), ()>>,
    post_expressions: Vec<&aast::Expr<(), ()>>,
    loop_scope: &'a mut Option<LoopScope>,
    loop_context: &'a mut ScopeContext,
    loop_parent_context: &'a mut ScopeContext,
    tast_info: &'a mut TastInfo,
    is_do: bool,
    always_enters_loop: bool,
) -> (bool, ScopeContext) {
    let (assignment_map, first_var_id) = get_assignment_map(
        pre_conditions.clone(),
        post_expressions.clone(),
        stmts,
        loop_context.function_context.calling_class.clone(),
    );

    let assignment_depth = if let Some(first_var_id) = first_var_id {
        get_assignment_map_depth(&first_var_id, &mut assignment_map.clone())
    } else {
        0
    };

    let original_protected_var_ids = loop_parent_context.protected_var_ids.clone();
    let mut always_assigned_before_loop_body_vars = FxHashSet::default();

    let mut pre_condition_clauses = Vec::new();

    let codebase = statements_analyzer.get_codebase();

    if !pre_conditions.is_empty() {
        let assertion_context = statements_analyzer
            .get_assertion_context(loop_context.function_context.calling_class.as_ref());

        for pre_condition in &pre_conditions {
            let pre_condition_id = (
                pre_condition.pos().start_offset(),
                pre_condition.pos().end_offset(),
            );

            pre_condition_clauses.push(
                formula_generator::get_formula(
                    pre_condition_id,
                    pre_condition_id,
                    pre_condition,
                    &assertion_context,
                    tast_info,
                    true,
                    false,
                )
                .unwrap(),
            )
        }
    } else {
        always_assigned_before_loop_body_vars =
            ScopeContext::get_new_or_updated_var_ids(loop_parent_context, loop_context);
    }

    let final_actions = control_analyzer::get_control_actions(
        codebase,
        statements_analyzer.get_file_analyzer().resolved_names,
        stmts,
        Some(tast_info),
        vec![],
        true,
    );

    let does_always_break =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::Break);

    let mut continue_context;

    let mut inner_do_context = None;

    if assignment_depth == 0 || does_always_break {
        continue_context = loop_context.clone();

        for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
            apply_pre_condition_to_loop_context(
                statements_analyzer,
                pre_condition,
                pre_condition_clauses.get(condition_offset).unwrap(),
                &mut continue_context,
                loop_parent_context,
                tast_info,
                is_do,
            );
        }

        continue_context.protected_var_ids = loop_scope.clone().unwrap().protected_var_ids.clone();

        statements_analyzer.analyze(stmts, tast_info, &mut continue_context, loop_scope);
        update_loop_scope_contexts(
            loop_scope,
            loop_context,
            &mut continue_context,
            loop_parent_context,
        );

        for post_expression in post_expressions {
            if !expression_analyzer::analyze(
                statements_analyzer,
                post_expression,
                tast_info,
                loop_context,
                &mut None,
            ) {
                return (false, continue_context);
            }
        }
    } else {
        let original_parent_context = loop_parent_context.clone();

        let mut pre_loop_context = loop_context.clone();

        tast_info.start_recording_issues();

        if !is_do {
            for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                apply_pre_condition_to_loop_context(
                    statements_analyzer,
                    pre_condition,
                    pre_condition_clauses.get(condition_offset).unwrap(),
                    loop_context,
                    loop_parent_context,
                    tast_info,
                    is_do,
                );
            }
        }

        continue_context = loop_context.clone();

        continue_context.protected_var_ids = loop_scope.clone().unwrap().protected_var_ids;

        statements_analyzer.analyze(stmts, tast_info, &mut continue_context, loop_scope);

        update_loop_scope_contexts(
            loop_scope,
            loop_context,
            &mut continue_context,
            &original_parent_context,
        );

        continue_context.protected_var_ids = original_protected_var_ids.clone();

        if is_do {
            inner_do_context = Some(continue_context.clone());

            for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                always_assigned_before_loop_body_vars.extend(apply_pre_condition_to_loop_context(
                    statements_analyzer,
                    pre_condition,
                    pre_condition_clauses.get(condition_offset).unwrap(),
                    &mut continue_context,
                    loop_parent_context,
                    tast_info,
                    is_do,
                ));
            }
        }

        for post_expression in &post_expressions {
            if !expression_analyzer::analyze(
                statements_analyzer,
                post_expression,
                tast_info,
                &mut continue_context,
                &mut None,
            ) {
                return (false, continue_context);
            }
        }

        let mut recorded_issues = tast_info.clear_currently_recorded_issues();
        tast_info.stop_recording_issues();

        let mut i = 0;

        while i < assignment_depth {
            let mut vars_to_remove = Vec::new();

            if let Some(loop_scope) = loop_scope {
                loop_scope.iteration_count += 1;
            }

            let mut has_changes = false;

            // reset the $continue_context to what it was before we started the analysis,
            // but union the types with what's in the loop scope

            for (var_id, continue_context_type) in &continue_context.vars_in_scope.clone() {
                if always_assigned_before_loop_body_vars.contains(var_id) {
                    // set the vars to whatever the while/foreach loop expects them to be
                    if let Some(pre_loop_context_type) = pre_loop_context.vars_in_scope.get(var_id)
                    {
                        if continue_context_type != pre_loop_context_type {
                            has_changes = true;
                        }
                    } else {
                        has_changes = true;
                    }
                } else if let Some(parent_context_type) =
                    original_parent_context.vars_in_scope.get(var_id)
                {
                    if continue_context_type != parent_context_type {
                        has_changes = true;

                        // widen the foreach context type with the initial context type
                        continue_context.vars_in_scope.insert(
                            var_id.clone(),
                            Rc::new(combine_union_types(
                                continue_context_type,
                                parent_context_type,
                                None,
                                false,
                            )),
                        );

                        // if there's a change, invalidate related clauses
                        pre_loop_context
                            .remove_var_from_conflicting_clauses(var_id, None, None, tast_info);

                        loop_parent_context
                            .possibly_assigned_var_ids
                            .insert(var_id.clone());
                    }

                    if let Some(loop_context_type) = loop_context.vars_in_scope.get(var_id) {
                        if continue_context_type != loop_context_type {
                            has_changes = true;
                        }

                        // widen the foreach context type with the initial context type
                        continue_context.vars_in_scope.insert(
                            var_id.clone(),
                            Rc::new(combine_union_types(
                                continue_context_type,
                                loop_context_type,
                                None,
                                false,
                            )),
                        );

                        // if there's a change, invalidate related clauses
                        pre_loop_context
                            .remove_var_from_conflicting_clauses(var_id, None, None, tast_info);
                    }
                } else {
                    // give an opportunity to redeemed UndefinedVariable issues
                    if !recorded_issues.is_empty() {
                        has_changes = true;
                    }

                    // if we're in a do block we don't want to remove vars before evaluating
                    // the where conditional
                    if !is_do {
                        vars_to_remove.push(var_id.clone());
                    }
                }
            }

            continue_context.has_returned = false;

            // if there are no changes to the types, no need to re-examine
            if !has_changes {
                break;
            }

            for var_id in vars_to_remove {
                continue_context.vars_in_scope.remove(&var_id);
            }

            continue_context.clauses = pre_loop_context.clauses.clone();

            tast_info.start_recording_issues();

            if !is_do {
                for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                    apply_pre_condition_to_loop_context(
                        statements_analyzer,
                        pre_condition,
                        pre_condition_clauses.get(condition_offset).unwrap(),
                        &mut continue_context,
                        loop_parent_context,
                        tast_info,
                        is_do,
                    );
                }
            }

            for var_id in &always_assigned_before_loop_body_vars {
                let pre_loop_context_type = pre_loop_context.vars_in_scope.get(var_id);

                if if let Some(continue_context_type) = continue_context.vars_in_scope.get(var_id) {
                    if let Some(pre_loop_context_type) = pre_loop_context_type {
                        continue_context_type.get_id() != pre_loop_context_type.get_id()
                    } else {
                        true
                    }
                } else {
                    true
                } {
                    if let Some(pre_loop_context_type) = pre_loop_context_type {
                        continue_context
                            .vars_in_scope
                            .insert(var_id.clone(), pre_loop_context_type.clone());
                    } else {
                        continue_context.vars_in_scope.remove(var_id);
                    }
                }
            }

            continue_context.clauses = pre_loop_context.clauses.clone();
            continue_context.protected_var_ids = loop_scope.clone().unwrap().protected_var_ids;

            clean_nodes(stmts, tast_info);

            statements_analyzer.analyze(stmts, tast_info, &mut continue_context, loop_scope);
            update_loop_scope_contexts(
                loop_scope,
                loop_context,
                &mut continue_context,
                &original_parent_context,
            );

            continue_context.protected_var_ids = original_protected_var_ids.clone();

            if is_do {
                inner_do_context = Some(continue_context.clone());

                for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                    apply_pre_condition_to_loop_context(
                        statements_analyzer,
                        pre_condition,
                        pre_condition_clauses.get(condition_offset).unwrap(),
                        &mut continue_context,
                        loop_parent_context,
                        tast_info,
                        is_do,
                    );
                }
            }

            for post_expression in &post_expressions {
                if !expression_analyzer::analyze(
                    statements_analyzer,
                    post_expression,
                    tast_info,
                    &mut continue_context,
                    &mut None,
                ) {
                    return (false, continue_context);
                }
            }

            recorded_issues = tast_info.clear_currently_recorded_issues();
            tast_info.stop_recording_issues();

            i += 1;
        }

        for recorded_issue in recorded_issues {
            // if we're not in any loops then this will just result in the issue being emitted
            tast_info.bubble_up_issue(recorded_issue);
        }
    }

    let cloned_loop_scope = loop_scope.clone().unwrap();

    let does_sometimes_break = cloned_loop_scope
        .final_actions
        .contains(&ControlAction::Break);
    let does_always_break = does_sometimes_break && cloned_loop_scope.final_actions.len() == 1;

    if does_sometimes_break {
        for (var_id, var_type) in &cloned_loop_scope.possibly_redefined_loop_parent_vars {
            loop_parent_context.vars_in_scope.insert(
                var_id.clone(),
                Rc::new(combine_union_types(
                    var_type,
                    loop_parent_context.vars_in_scope.get(var_id).unwrap(),
                    None,
                    false,
                )),
            );

            loop_parent_context
                .possibly_assigned_var_ids
                .insert(var_id.clone());
        }
    }

    for (var_id, var_type) in &loop_parent_context.vars_in_scope.clone() {
        if let Some(loop_context_type) = loop_context.vars_in_scope.get(var_id) {
            if loop_context_type.get_id() != var_type.get_id() {
                loop_parent_context.vars_in_scope.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        var_type,
                        loop_context_type,
                        None,
                        false,
                    )),
                );

                loop_parent_context
                    .remove_var_from_conflicting_clauses(var_id, None, None, tast_info);
            } else {
                if let Some(loop_parent_context_type) =
                    loop_parent_context.vars_in_scope.get_mut(var_id)
                {
                    if loop_parent_context_type != loop_context_type {
                        *loop_parent_context_type = Rc::new(combine_parent_nodes(
                            loop_context_type,
                            loop_parent_context_type,
                        ));
                    }
                }
            }
        }
    }

    if !does_always_break {
        for (var_id, var_type) in loop_parent_context.vars_in_scope.clone() {
            if let Some(continue_context_type) = continue_context.vars_in_scope.get_mut(&var_id) {
                if continue_context_type.is_mixed() {
                    *continue_context_type =
                        Rc::new(combine_parent_nodes(continue_context_type, &var_type));

                    loop_parent_context
                        .vars_in_scope
                        .insert(var_id.clone(), continue_context_type.clone());
                    loop_parent_context
                        .remove_var_from_conflicting_clauses(&var_id, None, None, tast_info);
                } else if continue_context_type.get_id() != var_type.get_id() {
                    loop_parent_context.vars_in_scope.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &var_type,
                            continue_context_type,
                            None,
                            false,
                        )),
                    );
                    loop_parent_context
                        .remove_var_from_conflicting_clauses(&var_id, None, None, tast_info);
                } else {
                    if let Some(loop_parent_context_type) =
                        loop_parent_context.vars_in_scope.get_mut(&var_id)
                    {
                        *loop_parent_context_type = Rc::new(combine_parent_nodes(
                            continue_context_type,
                            loop_parent_context_type,
                        ));
                    }
                }
            } else {
                loop_parent_context.vars_in_scope.remove(&var_id);
            }
        }
    }

    if !pre_conditions.is_empty() && !pre_condition_clauses.is_empty() && !does_sometimes_break {
        // if the loop contains an assertion and there are no break statements, we can negate that assertion
        // and apply it to the current context

        let negated_pre_condition_clauses = if let Ok(clauses) =
            hakana_algebra::negate_formula(pre_condition_clauses.into_iter().flatten().collect())
        {
            clauses
        } else {
            vec![]
        };

        let (negated_pre_condition_types, _) = hakana_algebra::get_truths_from_formula(
            negated_pre_condition_clauses.iter().collect(),
            None,
            &mut FxHashSet::default(),
        );

        if !negated_pre_condition_types.is_empty() {
            let mut changed_var_ids = FxHashSet::default();

            reconciler::reconcile_keyed_types(
                &negated_pre_condition_types,
                BTreeMap::new(),
                &mut continue_context,
                &mut changed_var_ids,
                &FxHashSet::default(),
                statements_analyzer,
                tast_info,
                pre_conditions.get(0).unwrap().pos(),
                true,
                false,
                &FxHashMap::default(),
            );

            for var_id in changed_var_ids {
                if let Some(reconciled_type) = continue_context.vars_in_scope.get(&var_id) {
                    if loop_parent_context.vars_in_scope.contains_key(&var_id) {
                        loop_parent_context
                            .vars_in_scope
                            .insert(var_id.clone(), reconciled_type.clone());
                    }

                    loop_parent_context
                        .remove_var_from_conflicting_clauses(&var_id, None, None, tast_info);
                }
            }
        }
    }

    if always_enters_loop {
        let does_sometimes_continue = loop_scope
            .clone()
            .unwrap()
            .final_actions
            .contains(&ControlAction::Continue);

        for (var_id, var_type) in &continue_context.vars_in_scope {
            // if there are break statements in the loop it's not certain
            // that the loop has finished executing, so the assertions at the end
            // the loop in the while conditional may not hold
            if does_sometimes_break || does_sometimes_continue {
                if let Some(possibly_defined_type) = cloned_loop_scope
                    .possibly_defined_loop_parent_vars
                    .get(var_id)
                {
                    loop_parent_context.vars_in_scope.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            var_type,
                            possibly_defined_type,
                            None,
                            false,
                        )),
                    );
                }
            } else {
                loop_parent_context
                    .vars_in_scope
                    .insert(var_id.clone(), var_type.clone());
            }
        }
    }

    if let Some(inner_do_context) = inner_do_context {
        return (true, inner_do_context);
    }

    (true, loop_context.clone())
}

fn get_assignment_map_depth(
    first_var_id: &String,
    assignment_map: &mut FxHashMap<String, FxHashSet<String>>,
) -> usize {
    let mut max_depth = 0;

    let assignment_var_ids = assignment_map.remove(first_var_id).unwrap();

    for assignment_var_id in assignment_var_ids {
        let mut depth = 1;

        if assignment_map.contains_key(&assignment_var_id) {
            depth += get_assignment_map_depth(&assignment_var_id, assignment_map);
        }

        if depth > max_depth {
            max_depth = depth
        }
    }

    max_depth
}

fn combine_parent_nodes(first: &TUnion, second: &TUnion) -> TUnion {
    let mut first = first.clone();
    first.parent_nodes.extend(second.parent_nodes.clone());
    first
}

fn apply_pre_condition_to_loop_context(
    statements_analyzer: &StatementsAnalyzer,
    pre_condition: &aast::Expr<(), ()>,
    pre_condition_clauses: &Vec<Clause>,
    loop_context: &mut ScopeContext,
    loop_parent_context: &mut ScopeContext,
    tast_info: &mut TastInfo,
    is_do: bool,
) -> FxHashSet<String> {
    let pre_referenced_var_ids = loop_context.cond_referenced_var_ids.clone();
    loop_context.cond_referenced_var_ids = FxHashSet::default();

    loop_context.inside_conditional = true;

    expression_analyzer::analyze(
        statements_analyzer,
        pre_condition,
        tast_info,
        loop_context,
        &mut None,
    );

    add_branch_dataflow(statements_analyzer, &pre_condition, tast_info);

    loop_context.inside_conditional = false;

    let mut new_referenced_var_ids = loop_context.cond_referenced_var_ids.clone();
    loop_context
        .cond_referenced_var_ids
        .extend(pre_referenced_var_ids);

    let always_assigned_before_loop_body_vars =
        ScopeContext::get_new_or_updated_var_ids(loop_context, loop_parent_context);

    loop_context.clauses = hakana_algebra::simplify_cnf({
        let mut clauses = loop_parent_context
            .clauses
            .iter()
            .map(|v| &**v)
            .collect::<Vec<_>>();
        clauses.extend(pre_condition_clauses.iter());
        clauses
    })
    .into_iter()
    .map(|v| Rc::new(v.clone()))
    .collect();

    let (reconcilable_while_types, active_while_types) = hakana_algebra::get_truths_from_formula(
        loop_context.clauses.iter().map(|v| &**v).collect(),
        Some((
            pre_condition.pos().start_offset(),
            pre_condition.pos().end_offset(),
        )),
        &mut new_referenced_var_ids,
    );

    if !reconcilable_while_types.is_empty() {
        reconciler::reconcile_keyed_types(
            &reconcilable_while_types,
            active_while_types,
            loop_context,
            &mut FxHashSet::default(),
            &new_referenced_var_ids,
            statements_analyzer,
            tast_info,
            pre_condition.pos(),
            true,
            false,
            &FxHashMap::default(),
        );
    }

    if is_do {
        return FxHashSet::default();
    }

    if !loop_context.clauses.is_empty() {
        let mut loop_context_clauses = loop_context.clauses.clone();

        for var_id in &always_assigned_before_loop_body_vars {
            loop_context_clauses = ScopeContext::filter_clauses(
                var_id,
                loop_context_clauses,
                None,
                Some(statements_analyzer),
                tast_info,
            );
        }

        loop_context.clauses = loop_context_clauses;
    }

    always_assigned_before_loop_body_vars
}

fn update_loop_scope_contexts(
    loop_scope: &mut Option<LoopScope>,
    loop_context: &mut ScopeContext,
    continue_context: &mut ScopeContext,
    pre_outer_context: &ScopeContext,
) {
    let loop_scope = loop_scope.as_mut().unwrap();

    if !loop_scope.final_actions.contains(&ControlAction::Continue) {
        loop_context.vars_in_scope = pre_outer_context.vars_in_scope.clone();
    } else {
        // $updated_loop_vars = [];

        // foreach ($loop_scope->redefined_loop_vars as $var => $type) {
        //     $continue_context->vars_in_scope[$var] = $type;
        //     $updated_loop_vars[$var] = true;
        // }

        let mut updated_loop_vars = FxHashSet::default();

        for (var, var_type) in &loop_scope.redefined_loop_vars {
            continue_context
                .vars_in_scope
                .insert(var.clone(), Rc::new(var_type.clone()));
            updated_loop_vars.insert(var);
        }

        for (var_id, var_type) in &loop_scope.possibly_redefined_loop_vars {
            if continue_context.has_variable(var_id) {
                if !updated_loop_vars.contains(var_id) {
                    continue_context.vars_in_scope.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &continue_context.vars_in_scope.get(var_id).unwrap(),
                            var_type,
                            None,
                            false,
                        )),
                    );
                } else {
                    continue_context.vars_in_scope.insert(
                        var_id.clone(),
                        Rc::new(combine_parent_nodes(
                            continue_context.vars_in_scope.get(var_id).unwrap(),
                            var_type,
                        )),
                    );
                }
            }
        }
    }
}
