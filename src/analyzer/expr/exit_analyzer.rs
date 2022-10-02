use std::sync::Arc;

use crate::expression_analyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use function_context::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_type::{get_arraykey, get_mixed_any, get_nothing};
use oxidized::ast_defs::Pos;
use oxidized::{aast, ast_defs};

use super::call::argument_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    args: &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
    call_pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let echo_param = FunctionLikeParameter::new("var".to_string());

    for (i, (_, arg_expr)) in args.iter().enumerate() {
        context.inside_general_use = true;
        expression_analyzer::analyze(statements_analyzer, arg_expr, tast_info, context, &mut None);
        context.inside_general_use = false;

        let arg_type = tast_info.get_expr_type(arg_expr.pos()).cloned();

        // TODO handle exit taint sink

        argument_analyzer::verify_type(
            statements_analyzer,
            &arg_type.unwrap_or(get_mixed_any()),
            &get_arraykey(false),
            &FunctionLikeIdentifier::Function(Arc::new("exit".to_string())),
            i,
            arg_expr,
            context,
            tast_info,
            &echo_param,
            &None,
            false,
            true,
            call_pos,
        );
    }

    tast_info.set_expr_type(&call_pos, get_nothing());

    true
}
