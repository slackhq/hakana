use crate::expression_analyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::{get_string, wrap_atomic};
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

    let left_expr_type = tast_info
        .expr_types
        .get(&(left.pos().start_offset(), left.pos().end_offset()));
    let right_expr_type = tast_info
        .expr_types
        .get(&(left.pos().start_offset(), left.pos().end_offset()));

    let result_type =
        if let (Some(left_expr_type), Some(right_expr_type)) = (left_expr_type, right_expr_type) {
            if left_expr_type.all_literals() && right_expr_type.all_literals() {
                wrap_atomic(TAtomic::TStringWithFlags(true, false, true))
            } else {
                get_string()
            }
        } else {
            get_string()
        };

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
