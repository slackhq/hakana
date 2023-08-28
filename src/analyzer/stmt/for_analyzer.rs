use oxidized::{aast, tast::Pos};

use crate::{
    expression_analyzer,
    function_analysis_data::FunctionAnalysisData,
    scope_context::{loop_scope::LoopScope, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};

use super::{control_analyzer::BreakContext, loop_analyzer};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &Vec<aast::Expr<(), ()>>,
        &Option<aast::Expr<(), ()>>,
        &Vec<aast::Expr<(), ()>>,
        &aast::Block<(), ()>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    let pre_assigned_var_ids = context.assigned_var_ids.clone();
    context.assigned_var_ids.clear();

    if let Some(last_comparison_expr) = stmt.2.last() {
        context.for_loop_init_bounds =
            Some((last_comparison_expr.pos().end_offset(), pos.end_offset()));
    }

    for init_expr in stmt.0 {
        expression_analyzer::analyze(
            statements_analyzer,
            init_expr,
            analysis_data,
            context,
            &mut None,
        )?;
    }

    context.for_loop_init_bounds = None;

    context.assigned_var_ids.extend(pre_assigned_var_ids);

    let while_true = stmt.0.is_empty() && matches!(stmt.1, None) && stmt.2.is_empty();

    let mut for_context = context.clone();
    for_context.inside_loop = true;
    for_context.break_types.push(BreakContext::Loop);

    loop_analyzer::analyze(
        statements_analyzer,
        &stmt.3 .0,
        if let Some(cond_expr) = stmt.1 {
            vec![cond_expr]
        } else {
            vec![]
        },
        stmt.2.iter().collect::<Vec<_>>(),
        &mut LoopScope::new(context.vars_in_scope.clone()),
        &mut for_context,
        context,
        analysis_data,
        false,
        while_true,
    )?;

    // theoretically we could also port over always_enters_loop logic from Psalm here
    // but I'm not sure that would be massively useful

    // todo do we need to remove the loop scope from analysis_data here? unsure

    Ok(())
}
