use oxidized::aast;

use crate::{
    expression_analyzer,
    scope_context::{loop_scope::LoopScope, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let pre_assigned_var_ids = context.assigned_var_ids.clone();
    context.assigned_var_ids.clear();

    for init_expr in stmt.0 {
        if !expression_analyzer::analyze(
            statements_analyzer,
            init_expr,
            tast_info,
            context,
            &mut None,
        ) {
            return false;
        }
    }

    context.assigned_var_ids.extend(pre_assigned_var_ids);

    let while_true = stmt.0.is_empty() && matches!(stmt.1, None) && stmt.2.is_empty();

    let mut for_context = context.clone();
    for_context.inside_loop = true;
    for_context.break_types.push(BreakContext::Loop);

    let (analysis_result, _) = loop_analyzer::analyze(
        statements_analyzer,
        stmt.3,
        if let Some(cond_expr) = stmt.1 {
            vec![cond_expr]
        } else {
            vec![]
        },
        stmt.2.iter().collect::<Vec<_>>(),
        &mut LoopScope::new(context.vars_in_scope.clone()),
        &mut for_context,
        context,
        tast_info,
        false,
        while_true,
    );

    if !analysis_result {
        return false;
    }

    // theoretically we could also port over always_enters_loop logic from Psalm here
    // but I'm not sure that would be massively useful

    // todo do we need to remove the loop scope from tast_info here? unsure

    return true;
}
