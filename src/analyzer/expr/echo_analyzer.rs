use crate::expression_analyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::STR_ECHO;
use hakana_type::{get_arraykey, get_mixed_any};
use oxidized::ast_defs::Pos;
use oxidized::{aast, ast_defs};

use super::call::argument_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    args: &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
    call_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> bool {
    let echo_param = FunctionLikeParameter::new(
        "var".to_string(),
        HPos::new(call_pos, *statements_analyzer.get_file_path(), None),
        HPos::new(call_pos, *statements_analyzer.get_file_path(), None),
    );

    for (i, (_, arg_expr)) in args.iter().enumerate() {
        expression_analyzer::analyze(
            statements_analyzer,
            arg_expr,
            analysis_data,
            context,
            &mut None,
        );

        let arg_type = analysis_data.get_expr_type(arg_expr.pos()).cloned();

        if !argument_analyzer::verify_type(
            statements_analyzer,
            &arg_type.unwrap_or(get_mixed_any()),
            &get_arraykey(false),
            &FunctionLikeIdentifier::Function(STR_ECHO),
            i,
            arg_expr,
            context,
            analysis_data,
            &echo_param,
            &None,
            false,
            true,
            call_pos,
        ) {
            return false;
        }
    }

    // TODO handle mutations

    true
}
