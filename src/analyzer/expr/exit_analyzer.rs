use std::rc::Rc;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::control_action::ControlAction;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::code_location::HPos;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_parameter::FunctionLikeParameter;
use hakana_code_info::ttype::{get_arraykey, get_mixed_any, get_nothing};
use hakana_code_info::VarId;
use hakana_str::StrId;
use oxidized::ast_defs::Pos;
use oxidized::{aast, ast_defs};

use super::call::argument_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    args: &[(ast_defs::ParamKind, aast::Expr<(), ()>)],
    call_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let echo_param = FunctionLikeParameter::new(
        VarId(StrId::EMPTY),
        HPos::new(call_pos, *statements_analyzer.get_file_path()),
        HPos::new(call_pos, *statements_analyzer.get_file_path()),
    );

    for (i, (_, arg_expr)) in args.iter().enumerate() {
        context.inside_general_use = true;
        expression_analyzer::analyze(statements_analyzer, arg_expr, analysis_data, context)?;
        context.inside_general_use = false;

        let arg_type = analysis_data.get_rc_expr_type(arg_expr.pos()).cloned();

        // TODO handle exit taint sink

        context.inside_general_use = true;

        argument_analyzer::verify_type(
            statements_analyzer,
            &arg_type.unwrap_or(Rc::new(get_mixed_any())),
            &get_arraykey(false),
            &FunctionLikeIdentifier::Function(
                statements_analyzer.get_interner().get("exit").unwrap(),
            ),
            i,
            arg_expr,
            context,
            analysis_data,
            &echo_param,
            &None,
            false,
            true,
            call_pos,
        );

        context.inside_general_use = false;
    }

    context.has_returned = true;
    context.control_actions.insert(ControlAction::End);

    analysis_data.set_expr_type(call_pos, get_nothing());

    Ok(())
}
