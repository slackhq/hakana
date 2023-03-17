use rustc_hash::FxHashSet;

use hakana_reflection_info::{
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    t_union::TUnion,
    StrId,
};
use hakana_type::get_mixed_any;
use oxidized::{
    aast,
    ast_defs::{self, ParamKind, Pos},
};

use crate::{
    scope_analyzer::ScopeAnalyzer, scope_context::ScopeContext,
    statements_analyzer::StatementsAnalyzer, function_analysis_data::FunctionAnalysisData,
};

use super::{arguments_analyzer::evaluate_arbitrary_param, existing_atomic_method_call_analyzer};

#[derive(Debug)]
pub(crate) struct AtomicMethodCallAnalysisResult {
    pub return_type: Option<TUnion>,
    pub has_valid_method_call_type: bool,
    pub existent_method_ids: FxHashSet<String>,
}

impl AtomicMethodCallAnalysisResult {
    pub(crate) fn new() -> Self {
        Self {
            return_type: None,
            has_valid_method_call_type: false,
            existent_method_ids: FxHashSet::default(),
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
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    lhs_type_part: &TAtomic,
    lhs_var_id: &Option<String>,
    result: &mut AtomicMethodCallAnalysisResult,
) {
    match &lhs_type_part {
        TAtomic::TNamedObject {
            name: classlike_name,
            extra_types,
            ..
        } => {
            if !handle_method_call_on_named_object(
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
                if_body_context,
            ) {
                return;
            }
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
                    statements_analyzer.get_hpos(&pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
        _ => {
            let mut mixed_with_any = false;

            if !lhs_type_part.is_mixed_with_any(&mut mixed_with_any) {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidMethodCall,
                        if let Some(lhs_var_id) = lhs_var_id {
                            format!(
                                "Cannot call method on {} with type {}",
                                lhs_var_id,
                                lhs_type_part.get_id(Some(&statements_analyzer.get_interner()))
                            )
                        } else {
                            format!(
                                "Cannot call method on type {}",
                                lhs_type_part.get_id(Some(&statements_analyzer.get_interner()))
                            )
                        },
                        statements_analyzer.get_hpos(&expr.0 .1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
                // todo handle invalid class invocation
                return;
            } else {
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
                                lhs_type_part.get_id(Some(&statements_analyzer.get_interner()))
                            )
                        } else {
                            format!(
                                "Cannot call method on type {}",
                                lhs_type_part.get_id(Some(&statements_analyzer.get_interner()))
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
                        if_body_context,
                    );
                }

                // todo handle invalid class invocation
                return;
            }
        }
    }
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
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
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

            codebase.class_or_interface_or_enum_exists(&classlike_name)
        };

        if !does_class_exist {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentClass,
                    format!(
                        "Class or interface {} does not exist",
                        statements_analyzer.get_interner().lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(&pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            return false;
        }
    }

    if let aast::Expr_::Id(boxed) = &expr.1 .2 {
        let method_name =
            if let Some(method_name) = statements_analyzer.get_interner().get(&boxed.1) {
                method_name
            } else {
                handle_nonexistent_method(
                    analysis_data,
                    statements_analyzer,
                    &boxed,
                    classlike_name,
                    pos,
                    context,
                    &expr.3,
                    if_body_context,
                );

                return false;
            };

        classlike_names.retain(|n| codebase.method_exists(n, &method_name));

        if classlike_names.is_empty() {
            handle_nonexistent_method(
                analysis_data,
                statements_analyzer,
                &boxed,
                classlike_name,
                pos,
                context,
                &expr.3,
                if_body_context,
            );

            return false;
        }

        let return_type_candidate = existing_atomic_method_call_analyzer::analyze(
            statements_analyzer,
            classlike_names[0], // todo intersect multiple return values
            &method_name,
            (expr.2, expr.3, expr.4),
            &lhs_type_part,
            pos,
            analysis_data,
            context,
            if_body_context,
            lhs_var_id.as_ref(),
            Some(expr.0.pos()),
            result,
        );

        result.return_type = Some(hakana_type::add_optional_union_type(
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
                if_body_context,
            );
        }

        result.return_type = Some(get_mixed_any());
        return false;
    }

    return true;
}

fn handle_nonexistent_method(
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    id: &ast_defs::Id,
    classlike_name: &StrId,
    pos: &Pos,
    context: &mut ScopeContext,
    expr_args: &Vec<(ParamKind, aast::Expr<(), ()>)>,
    if_body_context: &mut Option<ScopeContext>,
) {
    analysis_data.maybe_add_issue(
        Issue::new(
            IssueKind::NonExistentMethod,
            format!(
                "Method {}::{} does not exist",
                statements_analyzer.get_interner().lookup(classlike_name),
                &id.1
            ),
            statements_analyzer.get_hpos(&pos),
            &context.function_context.calling_functionlike_id,
        ),
        statements_analyzer.get_config(),
        statements_analyzer.get_file_path_actual(),
    );

    for (param_kind, arg_expr) in expr_args {
        evaluate_arbitrary_param(
            statements_analyzer,
            arg_expr,
            matches!(param_kind, ParamKind::Pinout(_)),
            analysis_data,
            context,
            if_body_context,
        );
    }
}
