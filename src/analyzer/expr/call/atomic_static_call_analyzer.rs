use hakana_code_info::{
    classlike_info::ClassLikeInfo,
    function_context::FunctionContext,
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    EFFECT_IMPURE,
};
use hakana_str::StrId;
use oxidized::{aast, ast_defs::Pos};

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
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
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

    let codebase = statements_analyzer.codebase;

    let method_name = statements_analyzer.interner.get(&expr.1 .1);

    if method_name.is_none() || !codebase.method_exists(&classlike_name, &method_name.unwrap()) {
        let Some(classlike_info) = codebase.classlike_infos.get(&classlike_name) else {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentClass,
                    format!(
                        "Class {} does not exist",
                        statements_analyzer.interner.lookup(&classlike_name),
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            if let Some(method_name) = method_name {
                analysis_data
                    .symbol_references
                    .add_reference_to_class_member(
                        &context.function_context,
                        (classlike_name, method_name),
                        false,
                    );
            } else {
                analysis_data.symbol_references.add_reference_to_symbol(
                    &context.function_context,
                    classlike_name,
                    false,
                );
            }

            return Ok(());
        };

        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentMethod,
                format!(
                    "Method {}::{} does not exist",
                    statements_analyzer.interner.lookup(&classlike_name),
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

        if let Some(method_name) = method_name {
            analysis_data
                .symbol_references
                .add_reference_to_class_member(
                    &context.function_context,
                    (classlike_name, method_name),
                    false,
                );

            add_missing_method_refs(
                classlike_info,
                analysis_data,
                &context.function_context,
                method_name,
            );
        }

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
        None,
    )?);

    Ok(())
}

pub(crate) fn add_missing_method_refs(
    classlike_info: &ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    function_context: &FunctionContext,
    method_name: StrId,
) {
    for parent_name in &classlike_info.all_parent_classes {
        analysis_data
            .symbol_references
            .add_reference_to_class_member(function_context, (*parent_name, method_name), false);
    }

    if classlike_info.is_abstract {
        for parent_name in &classlike_info.all_parent_interfaces {
            analysis_data
                .symbol_references
                .add_reference_to_class_member(
                    function_context,
                    (*parent_name, method_name),
                    false,
                );
        }
    }

    for trait_name in &classlike_info.used_traits {
        analysis_data
            .symbol_references
            .add_reference_to_class_member(function_context, (*trait_name, method_name), false);
    }
}
