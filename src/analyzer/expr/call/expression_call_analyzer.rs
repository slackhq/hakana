use std::rc::Rc;

use crate::expr::call::arguments_analyzer;

use crate::expr::call_analyzer::apply_effects;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::{FnEffect, FunctionLikeInfo, MetaStart};
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::VarId;
use hakana_str::StrId;
use hakana_type::get_mixed_any;
use hakana_type::template::TemplateResult;
use indexmap::IndexMap;
use oxidized::ast::CallExpr;
use oxidized::pos::Pos;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &CallExpr,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> Result<(), AnalysisError> {
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    expression_analyzer::analyze(
        statements_analyzer,
        &expr.func,
        analysis_data,
        context,
        if_body_context,
    )?;
    context.inside_general_use = was_inside_general_use;

    let lhs_type = analysis_data
        .get_rc_expr_type(expr.func.pos())
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));

    let mut stmt_type = None;

    let codebase = statements_analyzer.get_codebase();

    for lhs_type_part in &lhs_type.types {
        if let TAtomic::TClosure {
            params: closure_params,
            return_type: closure_return_type,
            effects,
            closure_id,
        } = &lhs_type_part
        {
            let mut template_result = TemplateResult::new(IndexMap::new(), IndexMap::new());

            let mut lambda_storage = FunctionLikeInfo::new(
                statements_analyzer.get_hpos(pos),
                MetaStart {
                    start_offset: 0,
                    start_column: 0,
                    start_line: 0,
                },
            );
            let existing_storage = codebase
                .functionlike_infos
                .get(&(closure_id.0 .0, StrId(closure_id.1)));

            if let Some(existing_storage) = existing_storage {
                lambda_storage.has_throw = existing_storage.has_throw;
            }

            lambda_storage.effects = FnEffect::from_u8(effects);

            lambda_storage.params = closure_params
                .iter()
                .map(|fn_param| {
                    let mut param = FunctionLikeParameter::new(
                        VarId(StrId::EMPTY),
                        HPos::new(expr.func.pos(), *statements_analyzer.get_file_path()),
                        HPos::new(expr.func.pos(), *statements_analyzer.get_file_path()),
                    );
                    param.signature_type = fn_param.signature_type.as_ref().map(|t| (**t).clone());
                    param.is_inout = fn_param.is_inout;
                    param.is_variadic = fn_param.is_variadic;
                    param
                })
                .collect();
            lambda_storage.return_type = closure_return_type.clone().map(|t| (*t).clone());

            lambda_storage.user_defined = true;

            let functionlike_id = FunctionLikeIdentifier::Closure(closure_id.0, closure_id.1);

            arguments_analyzer::check_arguments_match(
                statements_analyzer,
                &expr.targs,
                &expr.args,
                &expr.unpacked_arg,
                &functionlike_id,
                &lambda_storage,
                None,
                analysis_data,
                context,
                if_body_context,
                &mut template_result,
                pos,
                None,
            )?;

            apply_effects(
                functionlike_id,
                &lambda_storage,
                analysis_data,
                pos,
                &expr.args,
            );

            stmt_type = Some(hakana_type::combine_optional_union_types(
                stmt_type.as_ref(),
                match closure_return_type {
                    Some(t) => Some(t),
                    None => None,
                },
                codebase,
            ));
        }
    }

    let stmt_type = stmt_type.unwrap_or(get_mixed_any());

    if stmt_type.is_nothing() && !context.inside_loop {
        context.has_returned = true;
    }

    analysis_data.set_expr_type(pos, stmt_type);

    Ok(())
}
