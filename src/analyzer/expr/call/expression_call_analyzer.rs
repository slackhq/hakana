use std::rc::Rc;

use crate::expr::call::arguments_analyzer;

use crate::expr::call_analyzer::apply_effects;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::control_action::ControlAction;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::code_location::HPos;
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::{FnEffect, FunctionLikeInfo, MetaStart};
use hakana_code_info::functionlike_parameter::FunctionLikeParameter;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::get_mixed_any;
use hakana_code_info::ttype::template::TemplateResult;
use hakana_code_info::{VarId, EFFECT_CAN_THROW};
use hakana_str::StrId;
use indexmap::IndexMap;
use oxidized::ast::CallExpr;
use oxidized::pos::Pos;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &CallExpr,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    expression_analyzer::analyze(statements_analyzer, &expr.func, analysis_data, context)?;
    context.inside_general_use = was_inside_general_use;

    let lhs_type = analysis_data
        .get_rc_expr_type(expr.func.pos())
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));

    let mut stmt_type = None;

    let codebase = statements_analyzer.get_codebase();

    for lhs_type_part in &lhs_type.types {
        if let TAtomic::TClosure(closure) = &lhs_type_part {
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
                .get(&(closure.closure_id.0 .0, StrId(closure.closure_id.1)));

            let mut effects = closure.effects;

            if let Some(existing_storage) = existing_storage {
                if existing_storage.has_throw {
                    if let Some(ref mut effects) = effects {
                        *effects |= EFFECT_CAN_THROW;
                    }
                }
            }

            lambda_storage.effects = FnEffect::from_u8(&effects);

            lambda_storage.params = closure
                .params
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
            lambda_storage.return_type = closure.return_type.clone();

            lambda_storage.user_defined = true;

            let functionlike_id =
                FunctionLikeIdentifier::Closure(closure.closure_id.0, closure.closure_id.1);

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

            stmt_type = Some(hakana_code_info::ttype::combine_optional_union_types(
                stmt_type.as_ref(),
                match &closure.return_type {
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
        context.control_actions.insert(ControlAction::End);
    }

    analysis_data.set_expr_type(pos, stmt_type);

    Ok(())
}
