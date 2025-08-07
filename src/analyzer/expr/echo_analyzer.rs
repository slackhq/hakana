use std::rc::Rc;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::code_location::HPos;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_parameter::FunctionLikeParameter;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::get_mixed_any;
use hakana_code_info::{VarId, EFFECT_IMPURE};
use hakana_str::StrId;
use oxidized::aast;
use oxidized::ast_defs::Pos;

use super::call::argument_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    args: &[aast::Argument<(), ()>],
    call_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let echo_param = FunctionLikeParameter::new(
        VarId(StrId::EMPTY),
        HPos::new(call_pos, *statements_analyzer.get_file_path()),
        HPos::new(call_pos, *statements_analyzer.get_file_path()),
    );

    for (i, arg) in args.iter().enumerate() {
        let arg_expr = arg.to_expr_ref();
        expression_analyzer::analyze(statements_analyzer, arg_expr, analysis_data, context, true)?;

        let arg_type = analysis_data.get_rc_expr_type(arg_expr.pos()).cloned();

        context.inside_general_use = true;

        argument_analyzer::verify_type(
            statements_analyzer,
            &arg_type.unwrap_or(Rc::new(get_mixed_any())),
            &TUnion::new(vec![TAtomic::TScalar, TAtomic::TNull]),
            &FunctionLikeIdentifier::Function(StrId::ECHO),
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

    analysis_data.expr_effects.insert(
        (call_pos.start_offset() as u32, call_pos.end_offset() as u32),
        EFFECT_IMPURE,
    );

    Ok(())
}
