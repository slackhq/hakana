use std::rc::Rc;

use hakana_code_info::ttype::combine_union_types;
use rustc_hash::FxHashSet;

use crate::scope::{BlockContext, control_action::ControlAction, loop_scope::LoopScope};

use crate::{
    function_analysis_data::FunctionAnalysisData, statements_analyzer::StatementsAnalyzer,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    _analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    loop_scope: &mut Option<LoopScope>,
) {
    let codebase = statements_analyzer.codebase;
    if let Some(loop_scope) = loop_scope {
        loop_scope.final_actions.insert(ControlAction::Continue);
        context.control_actions.insert(ControlAction::Continue);

        let mut removed_var_ids = FxHashSet::default();

        let redefined_vars = context.get_redefined_locals(
            &loop_scope.parent_context_vars,
            false,
            &mut removed_var_ids,
        );

        for (var_id, var_type) in redefined_vars {
            loop_scope.possibly_redefined_loop_vars.insert(
                var_id.clone(),
                hakana_code_info::ttype::add_optional_union_type(
                    var_type,
                    loop_scope.possibly_redefined_loop_vars.get(&var_id),
                    codebase,
                ),
            );
        }

        // if loop_scope.iteration_count == 0 {
        //     for (_var_id, _var_type) in &context.locals {
        //         // todo populate finally scope
        //     }
        // }

        if let Some(finally_scope) = context.finally_scope.clone() {
            let mut finally_scope = (*finally_scope).borrow_mut();
            for (var_id, var_type) in &context.locals {
                if let Some(finally_type) = finally_scope.locals.get_mut(var_id) {
                    *finally_type =
                        Rc::new(combine_union_types(finally_type, var_type, codebase, false));
                } else {
                    finally_scope
                        .locals
                        .insert(var_id.clone(), var_type.clone());
                }
            }
        }
    }

    context.has_returned = true;
}
