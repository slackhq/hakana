use super::control_analyzer::BreakContext;
use crate::{
    scope_analyzer::ScopeAnalyzer,
    scope_context::{control_action::ControlAction, loop_scope::LoopScope, ScopeContext},
};
use crate::{statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo};
use hakana_type::combine_optional_union_types;
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) {
    let mut leaving_switch = true;

    let codebase = statements_analyzer.get_codebase();
    if let Some(loop_scope) = loop_scope {
        if if let Some(last_break_type) = context.break_types.last() {
            last_break_type == &BreakContext::Switch
        } else {
            false
        } {
            loop_scope.final_actions.push(ControlAction::LeaveSwitch);
        } else {
            leaving_switch = false;
            loop_scope.final_actions.push(ControlAction::Break);
        }

        let mut removed_vars = FxHashSet::default();

        let redefined_vars =
            context.get_redefined_vars(&loop_scope.parent_context_vars, false, &mut removed_vars);

        for (var_id, var_type) in redefined_vars {
            loop_scope.possibly_redefined_loop_parent_vars.insert(
                var_id.clone(),
                hakana_type::add_optional_union_type(
                    var_type.clone(),
                    loop_scope.possibly_redefined_loop_parent_vars.get(&var_id),
                    Some(codebase),
                ),
            );
        }

        if loop_scope.iteration_count == 0 {
            for (var_id, var_type) in &context.vars_in_scope {
                if !loop_scope.parent_context_vars.contains_key(var_id) {
                    loop_scope.possibly_defined_loop_parent_vars.insert(
                        var_id.clone(),
                        combine_optional_union_types(
                            Some(var_type),
                            loop_scope.possibly_defined_loop_parent_vars.get(var_id),
                            Some(codebase),
                        ),
                    );
                }
            }
        }

        // todo populate finally scope
    }

    let case_scope = tast_info.case_scopes.last_mut();

    if let Some(case_scope) = case_scope {
        if leaving_switch {
            let mut new_break_vars = case_scope
                .break_vars
                .clone()
                .unwrap_or(FxHashMap::default());

            for (var_id, var_type) in &context.vars_in_scope {
                new_break_vars.insert(
                    var_id.clone(),
                    combine_optional_union_types(
                        Some(var_type),
                        new_break_vars.get(var_id),
                        Some(codebase),
                    ),
                );
            }

            case_scope.break_vars = Some(new_break_vars);
        }
    }

    context.has_returned = true;
}
