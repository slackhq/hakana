use crate::expression_analyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_type::get_bool;
use oxidized::{aast, ast::Pos};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    context.inside_isset = true;
    let result = expression_analyzer::analyze(
        statements_analyzer,
        expr,
        tast_info,
        context,
        if_body_context,
    );
    context.inside_isset = false;

    tast_info.set_expr_type(&pos, get_bool());
    result
}
