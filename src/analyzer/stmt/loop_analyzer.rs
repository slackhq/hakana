use std::{collections::BTreeMap, rc::Rc};

use hakana_algebra::Clause;

use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::{combine_union_types, extend_dataflow_uniquely};
use hakana_code_info::var_name::VarName;
use oxidized::aast;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    expression_analyzer, formula_generator,
    function_analysis_data::FunctionAnalysisData,
    reconciler,
    scope::{BlockContext, control_action::ControlAction, loop_scope::LoopScope},
    statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
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
    loop_scope: &'a mut LoopScope,
    loop_context: &'a mut BlockContext,
    loop_parent_context: &'a mut BlockContext,
    analysis_data: &'a mut FunctionAnalysisData,
    is_do: bool,
    always_enters_loop: bool,
) -> Result<BlockContext, AnalysisError> {
    let (assignment_map, first_var_id) =
        get_assignment_map(&pre_conditions, &post_expressions, stmts);

    let assignment_depth = if let Some(first_var_id) = first_var_id {
        get_assignment_map_depth(&first_var_id, &mut assignment_map.clone())
    } else {
        0
    };

    let mut always_assigned_before_loop_body_vars = FxHashSet::default();

    let mut pre_condition_clauses = Vec::new();

    let codebase = statements_analyzer.codebase;

    if !pre_conditions.is_empty() {
        let assertion_context = statements_analyzer.get_assertion_context(
            loop_context.function_context.calling_class,
            loop_context.function_context.calling_functionlike_id,
        );

        for pre_condition in &pre_conditions {
            let pre_condition_id = (
                pre_condition.pos().start_offset() as u32,
                pre_condition.pos().end_offset() as u32,
            );

            pre_condition_clauses.push(
                formula_generator::get_formula(
                    pre_condition_id,
                    pre_condition_id,
                    pre_condition,
                    &assertion_context,
                    analysis_data,
                    true,
                    false,
                )
                .unwrap(),
            )
        }
    } else {
        always_assigned_before_loop_body_vars =
            BlockContext::get_new_or_updated_locals(loop_parent_context, loop_context);
    }

    let final_actions = control_analyzer::get_control_actions(
        codebase,
        statements_analyzer.interner,
        statements_analyzer.file_analyzer.resolved_names,
        stmts,
        analysis_data,
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
                analysis_data,
                is_do,
            )?;
        }

        let mut wrapped_loop_scope = Some(loop_scope.clone());

        statements_analyzer.analyze(
            stmts,
            analysis_data,
            &mut continue_context,
            &mut wrapped_loop_scope,
        )?;
        *loop_scope = wrapped_loop_scope.unwrap();
        update_loop_scope_contexts(
            loop_scope,
            loop_context,
            &mut continue_context,
            loop_parent_context,
            statements_analyzer,
        );

        loop_context.inside_loop_exprs = true;
        for post_expression in post_expressions {
            expression_analyzer::analyze(
                statements_analyzer,
                post_expression,
                analysis_data,
                loop_context,
                true,
            )?;
        }
        loop_context.inside_loop_exprs = true;
    } else {
        let original_parent_context = loop_parent_context.clone();

        let mut pre_loop_context = loop_context.clone();

        analysis_data.start_recording_issues();

        if !is_do {
            for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                apply_pre_condition_to_loop_context(
                    statements_analyzer,
                    pre_condition,
                    pre_condition_clauses.get(condition_offset).unwrap(),
                    loop_context,
                    loop_parent_context,
                    analysis_data,
                    is_do,
                )?;
            }
        }

        continue_context = loop_context.clone();

        let mut wrapped_loop_scope = Some(loop_scope.clone());

        statements_analyzer.analyze(
            stmts,
            analysis_data,
            &mut continue_context,
            &mut wrapped_loop_scope,
        )?;

        *loop_scope = wrapped_loop_scope.unwrap();

        update_loop_scope_contexts(
            loop_scope,
            loop_context,
            &mut continue_context,
            &original_parent_context,
            statements_analyzer,
        );

        if is_do {
            inner_do_context = Some(continue_context.clone());

            for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                always_assigned_before_loop_body_vars.extend(apply_pre_condition_to_loop_context(
                    statements_analyzer,
                    pre_condition,
                    pre_condition_clauses.get(condition_offset).unwrap(),
                    &mut continue_context,
                    loop_parent_context,
                    analysis_data,
                    is_do,
                )?);
            }
        }

        continue_context.inside_loop_exprs = true;
        for post_expression in &post_expressions {
            expression_analyzer::analyze(
                statements_analyzer,
                post_expression,
                analysis_data,
                &mut continue_context,
                true,
            )?;
        }
        continue_context.inside_loop_exprs = false;

        let mut recorded_issues = analysis_data.clear_currently_recorded_issues();
        analysis_data.stop_recording_issues();

        let mut i = 0;

        while i < assignment_depth {
            let mut vars_to_remove = Vec::new();

            loop_scope.iteration_count += 1;

            let mut has_changes = false;

            // reset the $continue_context to what it was before we started the analysis,
            // but union the types with what's in the loop scope

            if pre_loop_context
                .locals
                .iter()
                .any(|(var_id, _)| !continue_context.locals.contains_key(var_id))
            {
                has_changes = true;
            }

            let mut different_from_pre_loop_types = FxHashSet::default();

            for (var_id, continue_context_type) in continue_context.locals.clone() {
                if always_assigned_before_loop_body_vars.contains(&var_id) {
                    // set the vars to whatever the while/foreach loop expects them to be
                    if let Some(pre_loop_context_type) = pre_loop_context.locals.get(&var_id) {
                        if continue_context_type != *pre_loop_context_type {
                            different_from_pre_loop_types.insert(var_id.clone());
                            has_changes = true;
                        }
                    } else {
                        has_changes = true;
                    }
                } else if let Some(parent_context_type) =
                    original_parent_context.locals.get(&var_id)
                {
                    if continue_context_type != *parent_context_type {
                        has_changes = true;

                        // widen the foreach context type with the initial context type
                        continue_context.locals.insert(
                            var_id.clone(),
                            Rc::new(combine_union_types(
                                &continue_context_type,
                                parent_context_type,
                                statements_analyzer.codebase,
                                false,
                            )),
                        );

                        // if there's a change, invalidate related clauses
                        pre_loop_context.remove_var_from_conflicting_clauses(
                            &var_id,
                            None,
                            None,
                            analysis_data,
                        );

                        loop_parent_context
                            .possibly_assigned_var_ids
                            .insert(var_id.clone());
                    }

                    if let Some(loop_context_type) = loop_context.locals.get(&var_id) {
                        if continue_context_type != *loop_context_type {
                            has_changes = true;
                        }

                        // widen the foreach context type with the initial context type
                        continue_context.locals.insert(
                            var_id.clone(),
                            Rc::new(combine_union_types(
                                &continue_context_type,
                                loop_context_type,
                                codebase,
                                false,
                            )),
                        );

                        // if there's a change, invalidate related clauses
                        pre_loop_context.remove_var_from_conflicting_clauses(
                            &var_id,
                            None,
                            None,
                            analysis_data,
                        );
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
                continue_context.locals.remove(&var_id);
            }

            continue_context
                .clauses
                .clone_from(&pre_loop_context.clauses);

            analysis_data.start_recording_issues();

            if !is_do {
                for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                    apply_pre_condition_to_loop_context(
                        statements_analyzer,
                        pre_condition,
                        pre_condition_clauses.get(condition_offset).unwrap(),
                        &mut continue_context,
                        loop_parent_context,
                        analysis_data,
                        is_do,
                    )?;
                }
            }

            for var_id in &always_assigned_before_loop_body_vars {
                let pre_loop_context_type = pre_loop_context.locals.get(var_id);

                if if different_from_pre_loop_types.contains(var_id) {
                    true
                } else if continue_context.locals.contains_key(var_id) {
                    pre_loop_context_type.is_none()
                } else {
                    true
                } {
                    if let Some(pre_loop_context_type) = pre_loop_context_type {
                        continue_context
                            .locals
                            .insert(var_id.clone(), pre_loop_context_type.clone());
                    } else {
                        continue_context.locals.remove(var_id);
                    }
                }
            }

            continue_context
                .clauses
                .clone_from(&pre_loop_context.clauses);

            clean_nodes(stmts, analysis_data);

            let mut wrapped_loop_scope = Some(loop_scope.clone());

            statements_analyzer.analyze(
                stmts,
                analysis_data,
                &mut continue_context,
                &mut wrapped_loop_scope,
            )?;

            *loop_scope = wrapped_loop_scope.unwrap();

            update_loop_scope_contexts(
                loop_scope,
                loop_context,
                &mut continue_context,
                &original_parent_context,
                statements_analyzer,
            );

            if is_do {
                inner_do_context = Some(continue_context.clone());

                for (condition_offset, pre_condition) in pre_conditions.iter().enumerate() {
                    apply_pre_condition_to_loop_context(
                        statements_analyzer,
                        pre_condition,
                        pre_condition_clauses.get(condition_offset).unwrap(),
                        &mut continue_context,
                        loop_parent_context,
                        analysis_data,
                        is_do,
                    )?;
                }
            }

            continue_context.inside_loop_exprs = true;
            for post_expression in &post_expressions {
                expression_analyzer::analyze(
                    statements_analyzer,
                    post_expression,
                    analysis_data,
                    &mut continue_context,
                    true,
                )?;
            }
            continue_context.inside_loop_exprs = false;

            recorded_issues = analysis_data.clear_currently_recorded_issues();
            analysis_data.stop_recording_issues();

            i += 1;
        }

        for recorded_issue in recorded_issues {
            // if we're not in any loops then this will just result in the issue being emitted
            analysis_data.bubble_up_issue(recorded_issue);
        }
    }

    let cloned_loop_scope = loop_scope.clone();

    let does_sometimes_break = cloned_loop_scope
        .final_actions
        .contains(&ControlAction::Break);
    let does_always_break = does_sometimes_break && cloned_loop_scope.final_actions.len() == 1;

    if does_sometimes_break {
        if let Some(mut inner_do_context_inner) = inner_do_context {
            for (var_id, possibly_redefined_var_type) in
                &cloned_loop_scope.possibly_redefined_loop_parent_vars
            {
                if let Some(do_context_type) = inner_do_context_inner.locals.get_mut(var_id) {
                    *do_context_type = if do_context_type == possibly_redefined_var_type {
                        possibly_redefined_var_type.clone()
                    } else {
                        Rc::new(combine_union_types(
                            possibly_redefined_var_type,
                            do_context_type,
                            codebase,
                            false,
                        ))
                    };
                }

                loop_parent_context
                    .possibly_assigned_var_ids
                    .insert(var_id.clone());
            }

            inner_do_context = Some(inner_do_context_inner);
        } else {
            for (var_id, var_type) in &cloned_loop_scope.possibly_redefined_loop_parent_vars {
                if let Some(loop_parent_context_type) = loop_parent_context.locals.get_mut(var_id) {
                    *loop_parent_context_type = Rc::new(combine_union_types(
                        var_type,
                        loop_parent_context_type,
                        codebase,
                        false,
                    ));
                }

                loop_parent_context
                    .possibly_assigned_var_ids
                    .insert(var_id.clone());
            }
        }
    }

    for (var_id, var_type) in &loop_parent_context.locals.clone() {
        if let Some(loop_context_type) = loop_context.locals.get(var_id) {
            if loop_context_type != var_type {
                loop_parent_context.locals.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        var_type,
                        loop_context_type,
                        codebase,
                        false,
                    )),
                );

                loop_parent_context.remove_var_from_conflicting_clauses(
                    var_id.as_str(),
                    None,
                    None,
                    analysis_data,
                );
            } else if let Some(loop_parent_context_type) =
                loop_parent_context.locals.get_mut(var_id)
            {
                if loop_parent_context_type != loop_context_type {
                    *loop_parent_context_type = Rc::new({
                        let mut first = (**loop_context_type).clone();
                        extend_dataflow_uniquely(
                            &mut first.parent_nodes,
                            loop_parent_context_type.parent_nodes.clone(),
                        );
                        first
                    });
                }
            }
        }
    }

    if !does_always_break {
        for (var_id, var_type) in loop_parent_context.locals.clone() {
            if let Some(continue_context_type) = continue_context.locals.get_mut(&var_id) {
                if continue_context_type.is_mixed() {
                    *continue_context_type = Rc::new({
                        let second: &TUnion = &var_type;
                        let mut first = (**continue_context_type).clone();
                        extend_dataflow_uniquely(
                            &mut first.parent_nodes,
                            second.parent_nodes.clone(),
                        );
                        first
                    });

                    loop_parent_context
                        .locals
                        .insert(var_id.clone(), continue_context_type.clone());
                    loop_parent_context.remove_var_from_conflicting_clauses(
                        &var_id,
                        None,
                        None,
                        analysis_data,
                    );
                } else if continue_context_type != &var_type {
                    loop_parent_context.locals.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &var_type,
                            continue_context_type,
                            codebase,
                            false,
                        )),
                    );
                    loop_parent_context.remove_var_from_conflicting_clauses(
                        &var_id,
                        None,
                        None,
                        analysis_data,
                    );
                } else if let Some(loop_parent_context_type) =
                    loop_parent_context.locals.get_mut(&var_id)
                {
                    *loop_parent_context_type = Rc::new({
                        let mut first = (**continue_context_type).clone();
                        extend_dataflow_uniquely(
                            &mut first.parent_nodes,
                            loop_parent_context_type.parent_nodes.clone(),
                        );
                        first
                    });
                }
            } else {
                loop_parent_context.locals.remove(&var_id);
            }
        }
    }

    if !pre_conditions.is_empty() && !pre_condition_clauses.is_empty() && !does_sometimes_break {
        // if the loop contains an assertion and there are no break statements, we can negate that assertion
        // and apply it to the current context

        let negated_pre_condition_clauses =
            hakana_algebra::negate_formula(pre_condition_clauses.into_iter().flatten().collect())
                .unwrap_or_default();

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
                analysis_data,
                pre_conditions.first().unwrap().pos(),
                true,
                false,
                &FxHashMap::default(),
            );

            for var_id in changed_var_ids {
                if let Some(reconciled_type) = continue_context.locals.get(&var_id) {
                    if loop_parent_context.locals.contains_key(&var_id) {
                        loop_parent_context
                            .locals
                            .insert(var_id.clone(), reconciled_type.clone());
                    }

                    loop_parent_context.remove_var_from_conflicting_clauses(
                        &var_id,
                        None,
                        None,
                        analysis_data,
                    );
                }
            }
        }
    }

    if always_enters_loop {
        let does_sometimes_continue = loop_scope
            .clone()
            .final_actions
            .contains(&ControlAction::Continue);

        for (var_id, var_type) in &continue_context.locals {
            // if there are break statements in the loop it's not certain
            // that the loop has finished executing, so the assertions at the end
            // the loop in the while conditional may not hold
            if does_sometimes_break || does_sometimes_continue {
                if let Some(possibly_defined_type) = cloned_loop_scope
                    .possibly_defined_loop_parent_vars
                    .get(var_id)
                {
                    loop_parent_context.locals.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            var_type,
                            possibly_defined_type,
                            codebase,
                            false,
                        )),
                    );
                }
            } else {
                loop_parent_context
                    .locals
                    .insert(var_id.clone(), var_type.clone());
            }
        }
    }

    if let Some(inner_do_context) = inner_do_context {
        return Ok(inner_do_context);
    }

    Ok(loop_context.clone())
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

fn apply_pre_condition_to_loop_context(
    statements_analyzer: &StatementsAnalyzer,
    pre_condition: &aast::Expr<(), ()>,
    pre_condition_clauses: &[Clause],
    loop_context: &mut BlockContext,
    loop_parent_context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    is_do: bool,
) -> Result<FxHashSet<VarName>, AnalysisError> {
    let pre_referenced_var_ids = loop_context.cond_referenced_var_ids.clone();
    loop_context.cond_referenced_var_ids = FxHashSet::default();

    loop_context.inside_conditional = true;

    loop_context.inside_loop_exprs = true;

    expression_analyzer::analyze(
        statements_analyzer,
        pre_condition,
        analysis_data,
        loop_context,
        true,
    )?;

    add_branch_dataflow(statements_analyzer, pre_condition, analysis_data);

    loop_context.inside_loop_exprs = false;
    loop_context.inside_conditional = false;

    let mut new_referenced_var_ids = loop_context.cond_referenced_var_ids.clone();
    loop_context
        .cond_referenced_var_ids
        .extend(pre_referenced_var_ids);

    let always_assigned_before_loop_body_vars =
        BlockContext::get_new_or_updated_locals(loop_context, loop_parent_context);

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
            pre_condition.pos().start_offset() as u32,
            pre_condition.pos().end_offset() as u32,
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
            analysis_data,
            pre_condition.pos(),
            true,
            false,
            &FxHashMap::default(),
        );
    }

    if is_do {
        return Ok(FxHashSet::default());
    }

    if !loop_context.clauses.is_empty() {
        let mut loop_context_clauses = loop_context.clauses.clone();

        for var_id in &always_assigned_before_loop_body_vars {
            loop_context_clauses = BlockContext::filter_clauses(
                var_id,
                loop_context_clauses,
                None,
                Some(statements_analyzer),
                analysis_data,
            );
        }

        loop_context.clauses = loop_context_clauses;
    }

    Ok(always_assigned_before_loop_body_vars)
}

fn update_loop_scope_contexts(
    loop_scope: &mut LoopScope,
    loop_context: &mut BlockContext,
    continue_context: &mut BlockContext,
    pre_outer_context: &BlockContext,
    statements_analyzer: &StatementsAnalyzer,
) {
    if !loop_scope.final_actions.contains(&ControlAction::Continue) {
        loop_context.locals = pre_outer_context.locals.clone();
    } else {
        // $updated_loop_vars = [];

        // foreach ($loop_scope->redefined_loop_vars as $var => $type) {
        //     $continue_context->locals[$var] = $type;
        //     $updated_loop_vars[$var] = true;
        // }

        for (var_id, var_type) in &loop_scope.possibly_redefined_loop_vars {
            if continue_context.has_variable(var_id) {
                continue_context.locals.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        continue_context.locals.get(var_id).unwrap(),
                        var_type,
                        statements_analyzer.codebase,
                        false,
                    )),
                );
            }
        }
    }
}
