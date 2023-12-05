use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::{expression_analyzer, stmt_analyzer::AnalysisError};
use hakana_type::get_bool;
use oxidized::{aast, ast::Pos};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> Result<(), AnalysisError> {
    context.inside_isset = true;
    expression_analyzer::analyze(
        statements_analyzer,
        expr,
        analysis_data,
        context,
        if_body_context,
    )?;
    context.inside_isset = false;
    analysis_data.copy_effects(expr.pos(), pos);

    analysis_data.set_expr_type(&pos, get_bool());
    Ok(())
}
