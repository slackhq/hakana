use std::sync::Arc;

use crate::expr::call::arguments_analyzer;

use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use function_context::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::get_mixed_any;
use hakana_type::template::TemplateResult;
use indexmap::IndexMap;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::Expr<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    if !expression_analyzer::analyze(
        statements_analyzer,
        expr.0,
        tast_info,
        context,
        if_body_context,
    ) {
        return false;
    }
    context.inside_general_use = was_inside_general_use;

    let lhs_type = tast_info
        .get_expr_type(expr.0.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    let mut stmt_type = None;

    let codebase = statements_analyzer.get_codebase();

    for (_, lhs_type_part) in &lhs_type.types {
        if let TAtomic::TClosure {
            params: closure_params,
            return_type: closure_return_type,
            effects,
        } = &lhs_type_part
        {
            let mut template_result = TemplateResult::new(IndexMap::new(), IndexMap::new());

            let closure_id = Arc::new(format!(
                "{}:{}",
                expr.0.pos().filename().to_string(),
                expr.0.pos()
            ));

            let mut lambda_storage = FunctionLikeInfo::new(closure_id.clone());
            lambda_storage.params = closure_params.clone();
            lambda_storage.return_type = closure_return_type.clone();
            lambda_storage.effects = FnEffect::from_u8(effects);

            let functionlike_id = FunctionLikeIdentifier::Function(closure_id);

            arguments_analyzer::check_arguments_match(
                statements_analyzer,
                expr.1,
                expr.2,
                expr.3,
                &functionlike_id,
                &lambda_storage,
                None,
                tast_info,
                context,
                if_body_context,
                &mut template_result,
                pos,
            );

            stmt_type = Some(hakana_type::combine_optional_union_types(
                stmt_type.as_ref(),
                closure_return_type.as_ref(),
                Some(codebase),
            ));
        }
    }

    let stmt_type = stmt_type.unwrap_or(get_mixed_any());

    if stmt_type.is_nothing() && !context.inside_loop {
        context.has_returned = true;
    }

    tast_info.set_expr_type(&pos, stmt_type);

    true
}
