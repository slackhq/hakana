use crate::expression_analyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_type::get_string;
use oxidized::aast;

use super::arithmetic_analyzer::assign_arithmetic_type;

pub(crate) fn analyze<'expr, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    tast_info: &'tast mut TastInfo,
    context: &mut ScopeContext,
) {
    expression_analyzer::analyze(statements_analyzer, left, tast_info, context, &mut None);
    expression_analyzer::analyze(statements_analyzer, right, tast_info, context, &mut None);

    let result_type = get_string();

    // todo handle more string type combinations
    assign_arithmetic_type(
        statements_analyzer,
        tast_info,
        result_type,
        left,
        right,
        stmt_pos,
    );
}
