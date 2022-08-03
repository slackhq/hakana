use crate::{
    scope_analyzer::ScopeAnalyzer,
    scope_context::{control_action::ControlAction, loop_scope::LoopScope, ScopeContext},
};

use crate::{statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    _tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) {
    let codebase = statements_analyzer.get_codebase();
    if let Some(loop_scope) = loop_scope {
        loop_scope.final_actions.push(ControlAction::Continue);

        let redefined_vars = context.get_redefined_vars(&loop_scope.parent_context_vars, false);

        for (var_id, var_type) in redefined_vars {
            loop_scope.possibly_redefined_loop_vars.insert(
                var_id.clone(),
                hakana_type::add_optional_union_type(
                    var_type,
                    loop_scope.possibly_redefined_loop_vars.get(&var_id),
                    Some(codebase),
                ),
            );
        }

        if loop_scope.iteration_count == 0 {
            for (_var_id, _var_type) in &context.vars_in_scope {
                // todo populate finally scope
            }
        }
    }

    context.has_returned = true;
}
