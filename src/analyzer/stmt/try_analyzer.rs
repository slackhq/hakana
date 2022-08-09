use crate::scope_context::{
    control_action::ControlAction, loop_scope::LoopScope, FinallyScope, ScopeContext,
};
use crate::{
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo,
};
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_type::{combine_optional_union_types, combine_union_types, get_named_object};
use oxidized::aast;
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    collections::BTreeMap,
    rc::Rc,
};

use super::control_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Block<(), ()>,
        &Vec<aast::Catch<(), ()>>,
        &aast::Block<(), ()>,
    ),
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) -> bool {
    let mut all_catches_leave = true;

    let codebase = statements_analyzer.get_codebase();

    for catch in stmt.1.iter() {
        let catch_actions = control_analyzer::get_control_actions(
            codebase,
            statements_analyzer.get_file_analyzer().resolved_names,
            &catch.2,
            Some(tast_info),
            vec![],
            true,
        );

        all_catches_leave = all_catches_leave && !catch_actions.contains(&ControlAction::None);
    }

    let old_context = context.clone();

    let mut try_context = context.clone();

    if !stmt.2.is_empty() {
        try_context.finally_scope = Some(FinallyScope {
            vars_in_scope: BTreeMap::new(),
        });
    }

    let assigned_var_ids = context.assigned_var_ids.clone();
    context.assigned_var_ids = FxHashMap::default();

    let was_inside_try = context.inside_try;
    context.inside_try = true;

    if !statements_analyzer.analyze(stmt.0, tast_info, context, loop_scope) {
        return false;
    }

    context.inside_try = was_inside_try;

    if let Some(ref mut finally_scope) = try_context.finally_scope {
        for (var_id, var_type) in &context.vars_in_scope {
            finally_scope.vars_in_scope.insert(
                var_id.clone(),
                Rc::new(combine_optional_union_types(
                    if let Some(t) = finally_scope.vars_in_scope.get(var_id) {
                        Some(&t)
                    } else {
                        None
                    },
                    Some(var_type),
                    Some(codebase),
                )),
            );
        }
    }

    context.has_returned = false;
    try_context.has_returned = false;

    let try_block_control_actions = control_analyzer::get_control_actions(
        codebase,
        statements_analyzer.get_file_analyzer().resolved_names,
        stmt.0,
        Some(tast_info),
        vec![],
        true,
    );

    let newly_assigned_var_ids = context.assigned_var_ids.clone();

    context.assigned_var_ids.extend(assigned_var_ids);

    for (var_id, context_type) in context.vars_in_scope.iter_mut() {
        if let Some(try_type) = try_context.vars_in_scope.get(var_id).cloned() {
            try_context.vars_in_scope.insert(
                var_id.clone(),
                Rc::new(combine_union_types(
                    &try_type,
                    &context_type,
                    Some(codebase),
                    false,
                )),
            );
        } else {
            try_context
                .vars_in_scope
                .insert(var_id.clone(), context_type.clone());

            let mut context_type_inner = (**context_type).clone();
            context_type_inner.possibly_undefined_from_try = true;
            *context_type = Rc::new(context_type_inner);
        }
    }

    let try_leaves_loop = if let Some(loop_scope) = loop_scope {
        !loop_scope.final_actions.is_empty()
            && !loop_scope.final_actions.contains(&ControlAction::None)
    } else {
        false
    };

    for (assigned_var_id, _) in &newly_assigned_var_ids {
        if all_catches_leave {
            &mut try_context
        } else {
            &mut *context
        }
        .remove_var_from_conflicting_clauses(assigned_var_id, None, None, tast_info);
    }

    // at this point we have two contexts – $context, in which it is assumed that everything was fine,
    // and $try_context - which allows all variables to have the union of the values before and after
    // the try was applied
    let original_context = try_context.clone();

    let mut definitely_newly_assigned_var_ids = newly_assigned_var_ids.clone();

    let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

    for catch in stmt.1 {
        let mut catch_context = original_context.clone();
        catch_context.has_returned = false;

        for (var_id, after_try_type) in catch_context.vars_in_scope.clone() {
            if let Some(before_try_type) = old_context.vars_in_scope.get(&var_id) {
                catch_context.vars_in_scope.insert(
                    var_id.clone(),
                    Rc::new(combine_union_types(
                        &after_try_type,
                        &before_try_type,
                        Some(codebase),
                        false,
                    )),
                );
            } else {
                let mut better_type = (*after_try_type).clone();
                better_type.possibly_undefined_from_try = true;
                catch_context
                    .vars_in_scope
                    .insert(var_id, Rc::new(better_type));
            }
        }

        let catch_classlike_name =
            if let Some(resolved_name) = resolved_names.get(&catch.0 .0.start_offset()) {
                resolved_name
            } else {
                &catch.0 .1
            };

        // discard all clauses because crazy stuff may have happened in try block
        catch_context.clauses = vec![];

        let catch_var_id = &catch.1 .1 .1;

        let mut catch_type = get_named_object(catch_classlike_name.clone());

        catch_context.remove_descendants(
            catch_var_id,
            &catch_type,
            None,
            Some(statements_analyzer),
            tast_info,
        );

        let new_parent_node = DataFlowNode::get_for_assignment(
            catch_var_id.clone(),
            statements_analyzer.get_hpos(&catch.1 .0),
            None,
        );

        catch_type
            .parent_nodes
            .insert(new_parent_node.id.clone(), new_parent_node);

        catch_context
            .vars_in_scope
            .insert(catch_var_id.clone(), Rc::new(catch_type));

        let old_catch_assigned_var_ids = catch_context.assigned_var_ids.clone();

        catch_context.assigned_var_ids = FxHashMap::default();
        statements_analyzer.analyze(&catch.2, tast_info, &mut catch_context, loop_scope);

        // recalculate in case there's a nothing function call
        let catch_actions = control_analyzer::get_control_actions(
            codebase,
            statements_analyzer.get_file_analyzer().resolved_names,
            &catch.2,
            Some(tast_info),
            vec![],
            true,
        );

        let new_catch_assigned_var_ids = catch_context.assigned_var_ids.clone();
        catch_context
            .assigned_var_ids
            .extend(old_catch_assigned_var_ids);

        let catch_doesnt_leave_parent_scope = catch_actions.len() != 1
            || !matches!(
                catch_actions.iter().next().unwrap(),
                ControlAction::End | ControlAction::Continue | ControlAction::Break
            );

        if catch_doesnt_leave_parent_scope {
            definitely_newly_assigned_var_ids
                .retain(|var_id, _| new_catch_assigned_var_ids.contains_key(var_id));

            for (var_id, var_type) in &catch_context.vars_in_scope {
                if try_block_control_actions.len() == 1
                    && matches!(
                        try_block_control_actions.iter().next().unwrap(),
                        ControlAction::End
                    )
                {
                    context
                        .vars_in_scope
                        .insert(var_id.clone(), var_type.clone());
                } else if let Some(context_type) = context.vars_in_scope.get(var_id).cloned() {
                    context.vars_in_scope.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &context_type,
                            var_type,
                            Some(codebase),
                            false,
                        )),
                    );
                }
            }
        }

        if let Some(ref mut finally_scope) = try_context.finally_scope {
            for (var_id, var_type) in &catch_context.vars_in_scope {
                if let Some(finally_type) = finally_scope.vars_in_scope.get(var_id).cloned() {
                    finally_scope.vars_in_scope.insert(
                        var_id.clone(),
                        Rc::new(combine_union_types(
                            &finally_type,
                            &var_type,
                            Some(codebase),
                            false,
                        )),
                    );
                } else {
                    let mut var_type = (**var_type).clone();
                    var_type.possibly_undefined_from_try = true;
                    finally_scope
                        .vars_in_scope
                        .insert(var_id.clone(), Rc::new(var_type));
                }
            }
        }
    }

    if let Some(loop_scope) = loop_scope {
        if !try_leaves_loop && !loop_scope.final_actions.contains(&ControlAction::None) {
            loop_scope.final_actions.push(ControlAction::None);
        }
    }

    let mut finally_has_returned = false;

    if !stmt.2.is_empty() {
        if let Some(finally_scope) = try_context.finally_scope {
            let mut finally_context = context.clone();

            finally_context.assigned_var_ids = FxHashMap::default();
            finally_context.possibly_assigned_var_ids = FxHashSet::default();

            finally_context.vars_in_scope = finally_scope.vars_in_scope;

            statements_analyzer.analyze(&stmt.2, tast_info, context, loop_scope);

            finally_has_returned = finally_context.has_returned;

            for (var_id, _) in &finally_context.assigned_var_ids {
                if let Some(finally_type) = finally_context.vars_in_scope.get(var_id) {
                    if let Some(context_type) = context.vars_in_scope.get_mut(var_id) {
                        if context_type.possibly_undefined_from_try {
                            let mut context_type_inner = (**context_type).clone();
                            context_type_inner.possibly_undefined_from_try = false;
                            *context_type = Rc::new(context_type_inner);
                        }

                        *context_type = Rc::new(combine_union_types(
                            &context_type,
                            &finally_type,
                            Some(codebase),
                            false,
                        ));
                    } else {
                        context
                            .vars_in_scope
                            .insert(var_id.clone(), finally_type.clone());
                    }
                }
            }
        }
    }

    for (var_id, _) in &definitely_newly_assigned_var_ids {
        if let Some(context_type) = context.vars_in_scope.get_mut(var_id) {
            if context_type.possibly_undefined_from_try {
                let mut context_type_inner = (**context_type).clone();
                context_type_inner.possibly_undefined_from_try = false;
                *context_type = Rc::new(context_type_inner);
            }
        }
    }

    let body_has_returned = !try_block_control_actions.contains(&ControlAction::None);
    context.has_returned = (body_has_returned && all_catches_leave) || finally_has_returned;

    true
}
