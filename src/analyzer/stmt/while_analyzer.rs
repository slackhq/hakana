use super::{control_analyzer::BreakContext, loop_analyzer};
use crate::{
    function_analysis_data::FunctionAnalysisData,
    scope::{control_action::ControlAction, loop_scope::LoopScope, BlockContext},
    scope_analyzer::ScopeAnalyzer,
    statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};
use oxidized::{aast, ast_defs, pos::Pos};
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (&aast::Expr<(), ()>, &aast::Block<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let while_true = match &stmt.0 .2 {
        aast::Expr_::True => true,
        aast::Expr_::Int(value) => value.parse::<i64>().unwrap() > 0,
        _ => false,
    };

    let mut while_context = context.clone();

    while_context.inside_loop = true;
    while_context.break_types.push(BreakContext::Loop);

    let codebase = statements_analyzer.get_codebase();

    let mut loop_scope = LoopScope::new(context.locals.clone());

    let always_enters_loop = if while_true {
        true
    } else if let Some(stmt_cond_type) = analysis_data.get_expr_type(stmt.0.pos()) {
        stmt_cond_type.is_always_truthy()
    } else {
        false
    };

    let prev_loop_bounds = while_context.loop_bounds;
    while_context.loop_bounds = (pos.start_offset() as u32, pos.end_offset() as u32);

    let inner_loop_context = loop_analyzer::analyze(
        statements_analyzer,
        &stmt.1 .0,
        get_and_expressions(stmt.0),
        vec![],
        &mut loop_scope,
        &mut while_context,
        context,
        analysis_data,
        false,
        always_enters_loop,
    )?;

    while_context.loop_bounds = prev_loop_bounds;

    let can_leave_loop = !while_true || loop_scope.final_actions.contains(&ControlAction::Break);

    if always_enters_loop {
        if can_leave_loop {
            let has_break_or_continue = loop_scope.final_actions.contains(&ControlAction::Break)
                || loop_scope.final_actions.contains(&ControlAction::Continue);

            for (var_id, var_type) in inner_loop_context.locals {
                // if there are break statements in the loop it's not certain
                // that the loop has finished executing, so the assertions at the end
                // the loop in the while conditional may not hold
                if has_break_or_continue {
                    if let Some(possibly_defined_type) = loop_scope
                        .clone()
                        .possibly_defined_loop_parent_vars
                        .get(&var_id)
                    {
                        context.locals.insert(
                            var_id,
                            Rc::new(hakana_code_info::ttype::combine_union_types(
                                &var_type,
                                possibly_defined_type,
                                codebase,
                                false,
                            )),
                        );
                    }
                } else {
                    context.locals.insert(var_id.clone(), var_type.clone());
                }
            }
        } else {
            context.control_actions.insert(ControlAction::End);
            context.has_returned = true;
        }
    }

    // todo do we need to remove the loop scope from analysis_data here? unsure

    Ok(())
}

pub(crate) fn get_and_expressions(cond: &aast::Expr<(), ()>) -> Vec<&aast::Expr<(), ()>> {
    if let aast::Expr_::Binop(boxed) = &cond.2 {
        if let ast_defs::Bop::Ampamp = boxed.bop {
            let mut anded = get_and_expressions(&boxed.lhs);
            anded.extend(get_and_expressions(&boxed.rhs));
            return anded;
        }
    }

    vec![cond]
}
