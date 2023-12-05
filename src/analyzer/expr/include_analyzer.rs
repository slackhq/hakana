use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::STR_INCLUDE;
use hakana_type::{get_mixed_any, get_string};
use oxidized::aast;
use oxidized::ast_defs::Pos;

use super::call::argument_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    call_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    let echo_param = FunctionLikeParameter::new(
        "var".to_string(),
        HPos::new(call_pos, *statements_analyzer.get_file_path(), None),
        HPos::new(call_pos, *statements_analyzer.get_file_path(), None),
    );

    expression_analyzer::analyze(statements_analyzer, expr, analysis_data, context, &mut None)?;

    let arg_type = analysis_data.get_expr_type(expr.pos()).cloned();

    context.inside_general_use = true;

    argument_analyzer::verify_type(
        statements_analyzer,
        &arg_type.unwrap_or(get_mixed_any()),
        &get_string(),
        &FunctionLikeIdentifier::Function(STR_INCLUDE),
        0,
        expr,
        context,
        analysis_data,
        &echo_param,
        &None,
        false,
        true,
        call_pos,
    );

    context.inside_general_use = false;

    // TODO handle mutations

    Ok(())
}
