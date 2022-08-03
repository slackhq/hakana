use crate::expression_analyzer;
use crate::typed_ast::TastInfo;
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_type::get_mixed_any;
use oxidized::{aast, aast_defs, ast_defs::Pos};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&aast_defs::Lid, &aast::Expr<(), ()>, &aast::Expr<(), ()>),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    if !expression_analyzer::analyze(
        statements_analyzer,
        expr.1,
        tast_info,
        context,
        if_body_context,
    ) {
        return false;
    }

    let pipe_expr_type = tast_info
        .get_expr_type(&expr.1 .1)
        .cloned()
        .unwrap_or(get_mixed_any());

    tast_info.pipe_expr_type = Some(pipe_expr_type);

    let analyzed_ok = expression_analyzer::analyze(
        statements_analyzer,
        expr.2,
        tast_info,
        context,
        if_body_context,
    );

    tast_info.pipe_expr_type = None;

    tast_info.set_expr_type(
        &pos,
        tast_info
            .get_expr_type(&expr.2 .1)
            .cloned()
            .unwrap_or(get_mixed_any()),
    );
    analyzed_ok
}
