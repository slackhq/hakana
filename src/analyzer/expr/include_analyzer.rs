use std::path::Path;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::VarId;
use hakana_code_info::code_location::HPos;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_parameter::FunctionLikeParameter;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::ttype::{get_mixed_any, get_string};
use hakana_str::StrId;
use oxidized::aast;
use oxidized::ast_defs::Pos;

use super::call::argument_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    call_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let echo_param = FunctionLikeParameter::new(
        VarId(StrId::EMPTY),
        HPos::new(call_pos, *statements_analyzer.get_file_path()),
        HPos::new(call_pos, *statements_analyzer.get_file_path()),
    );

    expression_analyzer::analyze(statements_analyzer, expr, analysis_data, context, true)?;

    let arg_type = analysis_data.get_expr_type(expr.pos()).cloned();

    context.inside_general_use = true;

    argument_analyzer::verify_type(
        statements_analyzer,
        &arg_type.unwrap_or(get_mixed_any()),
        &get_string(),
        &FunctionLikeIdentifier::Function(StrId::INCLUDE),
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

    if let Some(first_arg_type) = analysis_data.get_rc_expr_type(expr.pos()).cloned() {
        if let Some(value) = first_arg_type.get_single_literal_string_value() {
            let path = Path::new(&value);
            if !path.exists() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::NonExistentFile,
                        format!("File {} does not exist", value),
                        statements_analyzer.get_hpos(expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }
    }

    context.inside_general_use = false;

    // TODO handle mutations

    Ok(())
}
