use hakana_str::StrId;

use hakana_code_info::ttype::{get_mixed_any, get_nothing};
use hakana_code_info::{
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    t_union::TUnion,
};
use oxidized::{
    aast,
    ast_defs::{self, ParamKind, Pos},
};

use crate::{
    function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};

use super::atomic_static_call_analyzer::add_missing_method_refs;
use super::{arguments_analyzer::evaluate_arbitrary_param, existing_atomic_method_call_analyzer};

#[derive(Debug)]
pub(crate) struct AtomicMethodCallAnalysisResult {
    pub return_type: Option<TUnion>,
    pub has_valid_method_call_type: bool,
}

impl AtomicMethodCallAnalysisResult {
    pub(crate) fn new() -> Self {
        Self {
            return_type: None,
            has_valid_method_call_type: false,
        }
    }
}

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
        &aast::Expr<(), ()>,
        &aast::Expr<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    lhs_type_part: &TAtomic,
    lhs_var_id: &Option<String>,
    result: &mut AtomicMethodCallAnalysisResult,
) -> Result<(), AnalysisError> {
    match &lhs_type_part {
        TAtomic::TNamedObject {
            name: classlike_name,
            extra_types,
            ..
        } => {
            handle_method_call_on_named_object(
                result,
                classlike_name,
                extra_types,
                lhs_var_id,
                analysis_data,
                statements_analyzer,
                pos,
                expr,
                lhs_type_part,
                context,
            )?;
        }
        TAtomic::TReference {
            name: classlike_name,
            ..
        } => {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentClasslike,
                    format!(
                        "Unknown classlike {}",
                        statements_analyzer.get_interner().lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
        _ => {
            let mut mixed_with_any = false;

            if matches!(lhs_type_part, TAtomic::TNothing) {
                result.return_type = Some(get_nothing());
                return Ok(());
            } else if !lhs_type_part.is_mixed_with_any(&mut mixed_with_any) {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidMethodCall,
                        if let Some(lhs_var_id) = lhs_var_id {
                            format!(
                                "Cannot call method on {} with type {}",
                                lhs_var_id,
                                lhs_type_part.get_id(Some(statements_analyzer.get_interner()))
                            )
                        } else {
                            format!(
                                "Cannot call method on type {}",
                                lhs_type_part.get_id(Some(statements_analyzer.get_interner()))
                            )
                        },
                        statements_analyzer.get_hpos(&expr.0 .1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
                // todo handle invalid class invocation
                return Ok(());
            }

            analysis_data.maybe_add_issue(
                Issue::new(
                    if mixed_with_any {
                        IssueKind::MixedAnyMethodCall
                    } else {
                        IssueKind::MixedMethodCall
                    },
                    if let Some(lhs_var_id) = lhs_var_id {
                        format!(
                            "Cannot call method on {} with type {}",
                            lhs_var_id,
                            lhs_type_part.get_id(Some(statements_analyzer.get_interner()))
                        )
                    } else {
                        format!(
                            "Cannot call method on type {}",
                            lhs_type_part.get_id(Some(statements_analyzer.get_interner()))
                        )
                    },
                    statements_analyzer.get_hpos(&expr.0 .1),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            for (param_kind, arg_expr) in expr.3 {
                evaluate_arbitrary_param(
                    statements_analyzer,
                    arg_expr,
                    matches!(param_kind, ParamKind::Pinout(_)),
                    analysis_data,
                    context,
                )?;
            }

            // todo handle invalid class invocation
        }
    }

    Ok(())
}

pub(crate) fn handle_method_call_on_named_object(
    result: &mut AtomicMethodCallAnalysisResult,
    classlike_name: &StrId,
    extra_types: &Option<Vec<TAtomic>>,
    lhs_var_id: &Option<String>,
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    expr: (
        &aast::Expr<(), ()>,
        &aast::Expr<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<(ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    lhs_type_part: &TAtomic,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.get_codebase();

    result.has_valid_method_call_type = true;

    let mut classlike_names = vec![*classlike_name];

    if let Some(extra_types) = extra_types {
        for extra_atomic_type in extra_types {
            if let TAtomic::TNamedObject {
                name: extra_classlike_name,
                ..
            } = extra_atomic_type
            {
                classlike_names.push(*extra_classlike_name);
            }
        }
    }

    for classlike_name in &classlike_names {
        let does_class_exist = if lhs_var_id.clone().unwrap_or_default() == "$this" {
            true
        } else {
            // check whether class exists using long method which emits an issue
            // but for now we use the quick one

            codebase.class_or_interface_or_enum_or_trait_exists(classlike_name)
        };

        if !does_class_exist {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentClass,
                    format!(
                        "Class or interface {} does not exist",
                        statements_analyzer.get_interner().lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            return Ok(());
        }
    }

    if let aast::Expr_::Id(boxed) = &expr.1 .2 {
        let method_name =
            if let Some(method_name) = statements_analyzer.get_interner().get(&boxed.1) {
                method_name
            } else {
                return handle_nonexistent_method(
                    analysis_data,
                    statements_analyzer,
                    boxed,
                    classlike_name,
                    pos,
                    context,
                    expr.3,
                );
            };

        classlike_names.retain(|n| codebase.method_exists(n, &method_name));

        if classlike_names.is_empty() {
            return handle_nonexistent_method(
                analysis_data,
                statements_analyzer,
                boxed,
                classlike_name,
                pos,
                context,
                expr.3,
            );
        }

        let return_type_candidate = existing_atomic_method_call_analyzer::analyze(
            statements_analyzer,
            classlike_names[0], // todo intersect multiple return values
            &method_name,
            Some(expr.0),
            (expr.2, expr.3, expr.4),
            lhs_type_part,
            pos,
            Some(expr.1.pos()),
            analysis_data,
            context,
            lhs_var_id.as_ref(),
            Some(expr.0.pos()),
        )?;

        result.return_type = Some(hakana_code_info::ttype::add_optional_union_type(
            return_type_candidate,
            result.return_type.as_ref(),
            codebase,
        ));
    } else {
        for (param_kind, arg_expr) in expr.3 {
            evaluate_arbitrary_param(
                statements_analyzer,
                arg_expr,
                matches!(param_kind, ParamKind::Pinout(_)),
                analysis_data,
                context,
            )?;
        }

        result.return_type = Some(get_mixed_any());
    }

    Ok(())
}

fn handle_nonexistent_method(
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    id: &ast_defs::Id,
    classlike_name: &StrId,
    pos: &Pos,
    context: &mut BlockContext,
    expr_args: &Vec<(ParamKind, aast::Expr<(), ()>)>,
) -> Result<(), AnalysisError> {
    analysis_data.maybe_add_issue(
        Issue::new(
            IssueKind::NonExistentMethod,
            format!(
                "Method {}::{} does not exist",
                statements_analyzer.get_interner().lookup(classlike_name),
                &id.1
            ),
            statements_analyzer.get_hpos(pos),
            &context.function_context.calling_functionlike_id,
        ),
        statements_analyzer.get_config(),
        statements_analyzer.get_file_path_actual(),
    );

    if let Some(method_name) = statements_analyzer.get_interner().get(&id.1) {
        analysis_data
            .symbol_references
            .add_reference_to_class_member(
                &context.function_context,
                (*classlike_name, method_name),
                false,
            );

        let Some(classlike_info) = statements_analyzer
            .get_codebase()
            .classlike_infos
            .get(&classlike_name)
        else {
            return Err(AnalysisError::InternalError(
                "Cannot load classlike storage".to_string(),
                statements_analyzer.get_hpos(pos),
            ));
        };

        add_missing_method_refs(
            classlike_info,
            analysis_data,
            &context.function_context,
            method_name,
        );
    }

    for (param_kind, arg_expr) in expr_args {
        evaluate_arbitrary_param(
            statements_analyzer,
            arg_expr,
            matches!(param_kind, ParamKind::Pinout(_)),
            analysis_data,
            context,
        )?;
    }

    Ok(())
}
