use hakana_reflection_info::{
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    EFFECT_IMPURE,
};
use hakana_str::StrId;
use oxidized::{
    aast,
    ast_defs::{self, Pos},
};

use crate::{
    function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};

use super::{
    atomic_method_call_analyzer::{
        handle_method_call_on_named_object, AtomicMethodCallAnalysisResult,
    },
    existing_atomic_method_call_analyzer,
};

/**
 * This is a bunch of complex logic to handle the potential for missing methods and intersection types.
 *
 * The happy path (i.e 99% of method calls) is handled in ExistingAtomicMethodCallAnalyzer
 *
 * @internal
 */
pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::ClassId<(), ()>,
        &(Pos, String),
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    if_body_context: &mut Option<BlockContext>,
    lhs_type_part: &TAtomic,
    classlike_name: Option<StrId>,
    result: &mut AtomicMethodCallAnalysisResult,
) -> Result<(), AnalysisError> {
    if let TAtomic::TNamedObject {
        name, extra_types, ..
    } = &lhs_type_part
    {
        if let aast::ClassId_::CIexpr(lhs_expr) = &expr.0 .2 {
            if !matches!(&lhs_expr.2, aast::Expr_::Id(_)) {
                return handle_method_call_on_named_object(
                    result,
                    name,
                    extra_types,
                    &None,
                    analysis_data,
                    statements_analyzer,
                    pos,
                    (
                        lhs_expr,
                        &aast::Expr::new(
                            (),
                            expr.1 .0.clone(),
                            aast::Expr_::Id(Box::new(oxidized::ast::Id(
                                expr.0 .1.clone(),
                                expr.1 .1.clone(),
                            ))),
                        ),
                        (expr.2),
                        (expr.3),
                        (expr.4),
                    ),
                    lhs_type_part,
                    context,
                    if_body_context,
                );
            }
        }
    }

    let classlike_name = if let Some(classlike_name) = classlike_name {
        classlike_name
    } else {
        match &lhs_type_part {
            TAtomic::TNamedObject { name, .. } => *name,
            TAtomic::TClassname { as_type, .. } | TAtomic::TGenericClassname { as_type, .. } => {
                let as_type = *as_type.clone();
                if let TAtomic::TNamedObject { name, .. } = as_type {
                    // todo check class name and register usage
                    name
                } else {
                    return Ok(());
                }
            }
            TAtomic::TLiteralClassname { name } => *name,
            TAtomic::TGenericParam { as_type, .. }
            | TAtomic::TClassTypeConstant { as_type, .. } => {
                let classlike_name =
                    if let TAtomic::TNamedObject { name, .. } = &as_type.types.first().unwrap() {
                        name
                    } else {
                        return Ok(());
                    };

                *classlike_name
            }
            _ => {
                if lhs_type_part.is_mixed() {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::MixedMethodCall,
                            "Method called on unknown object".to_string(),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }

                return Ok(());
            }
        }
    };

    let codebase = statements_analyzer.get_codebase();

    let method_name = statements_analyzer.get_interner().get(&expr.1 .1);

    if method_name.is_none() || !codebase.method_exists(&classlike_name, &method_name.unwrap()) {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentMethod,
                format!(
                    "Method {}::{} does not exist",
                    statements_analyzer.get_interner().lookup(&classlike_name),
                    &expr.1 .1
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        analysis_data.expr_effects.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            EFFECT_IMPURE,
        );

        return Ok(());
    }

    result.return_type = Some(existing_atomic_method_call_analyzer::analyze(
        statements_analyzer,
        classlike_name,
        &method_name.unwrap(),
        None,
        (expr.2, expr.3, expr.4),
        lhs_type_part,
        pos,
        Some(&expr.1 .0),
        analysis_data,
        context,
        if_body_context,
        None,
        None,
    )?);

    Ok(())
}
