use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::symbol_references::CallUsage;
use hakana_str::StrId;
use oxidized::{aast, ast_defs::Pos};

use crate::{
    function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
};

use super::expression_identifier::get_static_functionlike_id_from_call;

/// Mark only the result being discarded, not argument/receiver expressions.
/// Await and join consume a wait handle but do not consume its resulting value.
pub(crate) fn mark_discarded(
    expr: &aast::Expr<(), ()>,
    sa: &StatementsAnalyzer,
    data: &mut FunctionAnalysisData,
) {
    match &expr.2 {
        aast::Expr_::Await(inner) | aast::Expr_::Delay(inner) => {
            mark_discarded(inner, sa, data);
        }
        aast::Expr_::Call(call) => {
            data.discarded_call_results.insert((
                expr.pos().start_offset() as u32,
                expr.pos().end_offset() as u32,
            ));
            if get_static_functionlike_id_from_call(
                call,
                sa.interner,
                sa.file_analyzer.resolved_names,
            ) == Some(FunctionLikeIdentifier::Function(StrId::ASIO_JOIN))
                && let Some(arg) = call.args.first()
            {
                mark_discarded(arg.to_expr_ref(), sa, data);
            }
        }
        aast::Expr_::Eif(parts) => {
            if let Some(if_true) = &parts.1 {
                mark_discarded(if_true, sa, data);
            }
            mark_discarded(&parts.2, sa, data);
        }
        aast::Expr_::Pipe(parts) => {
            mark_discarded(&parts.2, sa, data);
            if let aast::Expr_::Call(call) = &parts.2.2
                && get_static_functionlike_id_from_call(
                    call,
                    sa.interner,
                    sa.file_analyzer.resolved_names,
                ) == Some(FunctionLikeIdentifier::Function(StrId::ASIO_JOIN))
                && call
                    .args
                    .first()
                    .is_some_and(|arg| match &arg.to_expr_ref().2 {
                        aast::Expr_::Dollardollar(_) => true,
                        aast::Expr_::Lvar(var) => var.1.1 == "$$",
                        _ => false,
                    })
            {
                mark_discarded(&parts.1, sa, data);
            }
        }
        _ => {}
    }
}

pub(crate) fn record(
    sa: &StatementsAnalyzer,
    data: &mut FunctionAnalysisData,
    context: &BlockContext,
    callee: &FunctionLikeIdentifier,
    usage: CallUsage,
) {
    if !sa.get_config().find_unused_definitions {
        return;
    }
    let non_production = !sa.file_analyzer.file_source.is_production_code
        || context
            .function_context
            .get_functionlike_info(sa.codebase)
            .is_some_and(|info| !info.is_production_code)
        || context
            .function_context
            .calling_class
            .and_then(|class| sa.codebase.classlike_infos.get(&class))
            .is_some_and(|info| !info.is_production_code);
    let callee = match callee {
        FunctionLikeIdentifier::Function(name) => (*name, StrId::EMPTY),
        FunctionLikeIdentifier::Method(_, _) => {
            let method = sa
                .codebase
                .get_declaring_method_id(&callee.as_method_identifier().unwrap());
            (method.0, method.1)
        }
        FunctionLikeIdentifier::Closure(_, _) => return,
    };
    let caller = context
        .function_context
        .calling_functionlike_id
        .map(|id| id.to_ref())
        .unwrap_or((
            context
                .function_context
                .calling_class
                .unwrap_or(sa.get_file_path().0),
            StrId::EMPTY,
        ));
    let calls = if non_production {
        &mut data.symbol_references.non_production_call_usage
    } else {
        &mut data.symbol_references.production_call_usage
    };
    calls
        .entry(caller)
        .or_default()
        .entry(callee)
        .or_default()
        .extend(&usage);
}

pub(crate) fn record_call(
    sa: &StatementsAnalyzer,
    data: &mut FunctionAnalysisData,
    context: &BlockContext,
    callee: &FunctionLikeIdentifier,
    info: &FunctionLikeInfo,
    args: &[aast::Argument<(), ()>],
    has_unpack: bool,
    pos: &Pos,
) {
    if !info.user_defined || info.is_closure {
        return;
    }
    let mut usage = CallUsage {
        return_value_used: !data
            .discarded_call_results
            .contains(&(pos.start_offset() as u32, pos.end_offset() as u32)),
        unknown_args: has_unpack,
        ..Default::default()
    };
    for arg in args {
        if let aast::Argument::Anamed(name, _) = arg {
            if let Some(offset) = info.params.iter().position(|p| {
                sa.interner.lookup(&p.name.0).trim_start_matches('$')
                    == name.1.trim_start_matches('$')
            }) {
                usage.named_args.insert(offset);
            } else {
                usage.unknown_args = true;
            }
        } else {
            usage.max_positional_args += 1;
        }
    }
    record(sa, data, context, callee, usage);
}
