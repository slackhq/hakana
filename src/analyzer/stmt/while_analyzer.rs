use super::{control_analyzer::BreakContext, loop_analyzer};
use crate::{
    scope_analyzer::ScopeAnalyzer,
    scope_context::{control_action::ControlAction, loop_scope::LoopScope, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
    typed_ast::FunctionAnalysisData,
};
use oxidized::{aast, ast_defs};
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (&aast::Expr<(), ()>, &aast::Block<(), ()>),
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> bool {
    let while_true = match &stmt.0 .2 {
        aast::Expr_::True => true,
        aast::Expr_::Int(value) => value.parse::<i64>().unwrap() > 0,
        _ => false,
    };

    let mut while_context = context.clone();

    while_context.inside_loop = true;
    while_context.break_types.push(BreakContext::Loop);

    let codebase = statements_analyzer.get_codebase();

    let mut loop_scope = LoopScope::new(context.vars_in_scope.clone());

    let (analysis_result, inner_loop_context) = loop_analyzer::analyze(
        statements_analyzer,
        &stmt.1 .0,
        get_and_expressions(stmt.0),
        vec![],
        &mut loop_scope,
        &mut while_context,
        context,
        analysis_data,
        false,
        false,
    );

    if !analysis_result {
        return false;
    }

    let always_enters_loop = if while_true {
        true
    } else {
        if let Some(stmt_cond_type) = analysis_data.get_expr_type(stmt.0.pos()) {
            stmt_cond_type.is_always_truthy()
        } else {
            false
        }
    };

    let can_leave_loop = !while_true || loop_scope.final_actions.contains(&ControlAction::Break);

    if always_enters_loop && can_leave_loop {
        let has_break_or_continue = loop_scope.final_actions.contains(&ControlAction::Break)
            || loop_scope.final_actions.contains(&ControlAction::Continue);

        for (var_id, var_type) in inner_loop_context.vars_in_scope {
            // if there are break statements in the loop it's not certain
            // that the loop has finished executing, so the assertions at the end
            // the loop in the while conditional may not hold
            if has_break_or_continue {
                if let Some(possibly_defined_type) = loop_scope
                    .clone()
                    .possibly_defined_loop_parent_vars
                    .get(&var_id)
                {
                    context.vars_in_scope.insert(
                        var_id,
                        Rc::new(hakana_type::combine_union_types(
                            &var_type,
                            &possibly_defined_type,
                            codebase,
                            false,
                        )),
                    );
                }
            } else {
                context
                    .vars_in_scope
                    .insert(var_id.clone(), var_type.clone());
            }
        }
    }

    // todo do we need to remove the loop scope from analysis_data here? unsure

    return true;
}

pub(crate) fn get_and_expressions(cond: &aast::Expr<(), ()>) -> Vec<&aast::Expr<(), ()>> {
    if let aast::Expr_::Binop(boxed) = &cond.2 {
        if let ast_defs::Bop::Ampamp = boxed.0 {
            let mut anded = get_and_expressions(&boxed.1);
            anded.extend(get_and_expressions(&boxed.2));
            return anded;
        }
    }

    return vec![cond];
}
