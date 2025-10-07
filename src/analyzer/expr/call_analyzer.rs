use std::sync::Arc;

use hakana_code_info::code_location::HPos;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::ttype::comparison::type_comparison_result::TypeComparisonResult;
use hakana_code_info::ttype::comparison::union_type_comparator;
use hakana_code_info::ttype::template::standin_type_replacer::get_relevant_bounds;
use hakana_code_info::{
    EFFECT_CAN_THROW, EFFECT_IMPURE, EFFECT_PURE, EFFECT_WRITE_PROPS, GenericParent,
};
use hakana_str::StrId;
use itertools::Itertools;
use oxidized::ast::CallExpr;
use rustc_hash::FxHashMap;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_code_info::method_identifier::MethodIdentifier;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::get_mixed_any;
use hakana_code_info::ttype::template::{TemplateBound, TemplateResult};
use indexmap::IndexMap;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::call::{
    arguments_analyzer, expression_call_analyzer, function_call_analyzer, instance_call_analyzer,
    static_call_analyzer,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &CallExpr,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    is_sub_expression: bool,
) -> Result<(), AnalysisError> {
    let function_name_expr = &expr.func;

    match &function_name_expr.2 {
        aast::Expr_::Id(boxed_id) => function_call_analyzer::analyze(
            statements_analyzer,
            (
                (&boxed_id.0, &boxed_id.1),
                &expr.targs,
                &expr.args,
                &expr.unpacked_arg,
            ),
            pos,
            analysis_data,
            context,
            is_sub_expression,
        ),
        aast::Expr_::ObjGet(boxed) => {
            let (lhs_expr, rhs_expr, nullfetch, prop_or_method) =
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

            match prop_or_method {
                ast_defs::PropOrMethod::IsMethod => instance_call_analyzer::analyze(
                    statements_analyzer,
                    (
                        lhs_expr,
                        rhs_expr,
                        &expr.targs,
                        &expr.args,
                        &expr.unpacked_arg,
                    ),
                    pos,
                    analysis_data,
                    context,
                    matches!(nullfetch, ast_defs::OgNullFlavor::OGNullsafe),
                ),
                _ => {
                    expression_call_analyzer::analyze(
                        statements_analyzer,
                        expr,
                        pos,
                        analysis_data,
                        context,
                    )?;
                    Ok(())
                }
            }
        }
        aast::Expr_::ClassConst(boxed) => {
            let (class_id, rhs_expr) = (&boxed.0, &boxed.1);

            static_call_analyzer::analyze(
                statements_analyzer,
                (
                    class_id,
                    rhs_expr,
                    &expr.targs,
                    &expr.args,
                    &expr.unpacked_arg,
                ),
                pos,
                analysis_data,
                context,
            )
        }
        _ => expression_call_analyzer::analyze(
            statements_analyzer,
            expr,
            pos,
            analysis_data,
            context,
        ),
    }
}

/**
  This method looks for problems with a generated TemplateResult.

  The TemplateResult object contains upper bounds and lower bounds for each template param.

  Those upper bounds represent a series of constraints like

  Lower bound:
  T >: X (the type param T matches X, or is a supertype of X)
  Upper bound:
  T <: Y (the type param T matches Y, or is a subtype of Y)
  Equality (currently represented as an upper bound with a special flag)
  T = Z  (the template T must match Z)

  This method attempts to reconcile those constraints.

  Valid constraints:

  T <: int|float, T >: int --- implies T is an int
  T = int --- implies T is an int

  Invalid constraints:

  T <: int|string, T >: string|float --- implies T <: int and T >: float, which is impossible
  T = int, T = string --- implies T is a string _and_ and int, which is impossible
*/
pub(crate) fn check_template_result(
    _statements_analyzer: &StatementsAnalyzer,
    _template_result: &mut TemplateResult,
    _pos: &Pos,
    _functionlike_id: &FunctionLikeIdentifier,
) {
}

pub(crate) fn reconcile_lower_bounds_with_upper_bounds(
    lower_bounds: &Vec<TemplateBound>,
    upper_bounds: &Vec<TemplateBound>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    pos: HPos,
) {
    let codebase = statements_analyzer.codebase;
    let interner = statements_analyzer.interner;

    let relevant_lower_bounds = get_relevant_bounds(lower_bounds);

    // println!("{:#?}", lower_bounds);
    // println!("{:#?}", upper_bounds);

    let mut union_comparison_result = TypeComparisonResult::new();

    let mut has_issue = false;

    for relevant_lower_bound in &relevant_lower_bounds {
        for upper_bound in upper_bounds {
            if !union_type_comparator::is_contained_by(
                codebase,
                statements_analyzer.get_file_path(),
                &relevant_lower_bound.bound_type,
                &upper_bound.bound_type,
                false,
                false,
                false,
                &mut union_comparison_result,
            ) {
                has_issue = true;
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::IncompatibleTypeParameters,
                        format!(
                            "Type {} should be a subtype of {}",
                            relevant_lower_bound.bound_type.get_id(Some(interner)),
                            upper_bound.bound_type.get_id(Some(interner))
                        ),
                        relevant_lower_bound
                            .pos
                            .unwrap_or(upper_bound.pos.unwrap_or(pos)),
                        &None,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }
    }

    if !has_issue && relevant_lower_bounds.len() > 1 {
        let bounds_with_equality = lower_bounds
            .iter()
            .filter(|bound| bound.equality_bound_classlike.is_some())
            .collect::<Vec<_>>();

        if bounds_with_equality.is_empty() {
            return;
        }

        let equality_strings = bounds_with_equality
            .iter()
            .map(|bound| bound.bound_type.get_id(Some(interner)))
            .unique()
            .collect::<Vec<_>>();

        if equality_strings.len() > 1 {
            has_issue = true;
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::IncompatibleTypeParameters,
                    format!(
                        "Incompatible types found for {} (must have only one of {})",
                        "type variable",
                        equality_strings.join(", "),
                    ),
                    bounds_with_equality[0].pos.unwrap_or(pos),
                    &None,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        } else {
            'outer: for lower_bound in lower_bounds {
                if lower_bound.equality_bound_classlike.is_none() {
                    for bound_with_equality in &bounds_with_equality {
                        if union_type_comparator::is_contained_by(
                            codebase,
                            statements_analyzer.get_file_path(),
                            &lower_bound.bound_type,
                            &bound_with_equality.bound_type,
                            false,
                            false,
                            false,
                            &mut TypeComparisonResult::new(),
                        ) {
                            continue 'outer;
                        }
                    }

                    has_issue = true;
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::IncompatibleTypeParameters,
                            format!(
                                "Incompatible types found for {} ({} is not in {})",
                                "type variable",
                                lower_bound.bound_type.get_id(Some(interner)),
                                equality_strings.join(", "),
                            ),
                            pos,
                            &None,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }
        }
    }

    if !has_issue && upper_bounds.len() > 1 {
        let upper_bounds_with_equality = upper_bounds
            .iter()
            .filter(|bound| bound.equality_bound_classlike.is_some())
            .enumerate()
            .collect::<Vec<_>>();

        if upper_bounds_with_equality.is_empty() {
            return;
        }

        for (i, upper_bound_with_equality) in upper_bounds_with_equality {
            for (j, upper_bound) in upper_bounds.iter().enumerate() {
                if i == j {
                    continue;
                }

                if !union_type_comparator::can_expression_types_be_identical(
                    codebase,
                    statements_analyzer.get_file_path(),
                    &upper_bound_with_equality.bound_type,
                    &upper_bound.bound_type,
                    false,
                ) {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::IncompatibleTypeParameters,
                            format!(
                                "Incompatible types found for {} ({} is not in {})",
                                "type variable",
                                upper_bound.bound_type.get_id(Some(interner)),
                                upper_bound_with_equality.bound_type.get_id(Some(interner)),
                            ),
                            pos,
                            &None,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }
        }
    }
}

pub(crate) fn get_generic_param_for_offset(
    classlike_name: &StrId,
    template_name: &StrId,
    template_extended_params: &FxHashMap<StrId, IndexMap<StrId, Arc<TUnion>>>,
    found_generic_params: &FxHashMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
) -> Arc<TUnion> {
    if let Some(found_generic_param) =
        if let Some(result_map) = found_generic_params.get(template_name) {
            result_map
                .iter()
                .filter(|(e, _)| e == &GenericParent::ClassLike(*classlike_name))
                .map(|(_, v)| v)
                .next()
        } else {
            None
        }
    {
        found_generic_param.clone()
    } else {
        for (extended_class_name, type_map) in template_extended_params {
            for (extended_template_name, extended_type) in type_map {
                for extended_atomic_type in &extended_type.types {
                    if let TAtomic::TGenericParam {
                        param_name: extended_param_name,
                        defining_entity,
                        ..
                    } = &extended_atomic_type
                    {
                        if extended_param_name == template_name
                            && defining_entity == &GenericParent::ClassLike(*classlike_name)
                        {
                            return get_generic_param_for_offset(
                                extended_class_name,
                                extended_template_name,
                                template_extended_params,
                                found_generic_params,
                            );
                        }
                    }
                }
            }
        }

        Arc::new(get_mixed_any())
    }
}

pub(crate) fn check_method_args(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    method_id: &MethodIdentifier,
    functionlike_storage: &FunctionLikeInfo,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    lhs_type_part: Option<&TAtomic>,
    template_result: &mut TemplateResult,
    context: &mut BlockContext,
    pos: &Pos,
    method_name_pos: Option<&Pos>,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    let functionlike_id = FunctionLikeIdentifier::Method(method_id.0, method_id.1);

    arguments_analyzer::check_arguments_match(
        statements_analyzer,
        call_expr.0,
        call_expr.1,
        call_expr.2,
        &functionlike_id,
        functionlike_storage,
        Some((method_id.0, lhs_type_part)),
        analysis_data,
        context,
        template_result,
        pos,
        method_name_pos,
    )?;

    apply_effects(
        FunctionLikeIdentifier::Method(method_id.0, method_id.1),
        functionlike_storage,
        analysis_data,
        pos,
        call_expr.1,
    );

    if !template_result.template_types.is_empty() {
        check_template_result(statements_analyzer, template_result, pos, &functionlike_id);
    }

    if !functionlike_storage.is_production_code
        && functionlike_storage.user_defined
        && context.function_context.is_production(codebase)
    {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::TestOnlyCall,
                format!(
                    "Cannot call test-only function {} from non-test context",
                    method_id.to_string(statements_analyzer.interner),
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        )
    }

    Ok(())
}

pub(crate) fn apply_effects(
    functionlike_id: FunctionLikeIdentifier,
    function_storage: &FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    expr_args: &Vec<aast::Argument<(), ()>>,
) {
    if functionlike_id == FunctionLikeIdentifier::Function(StrId::ASIO_JOIN) {
        analysis_data.expr_effects.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            EFFECT_IMPURE,
        );
        return;
    }

    match function_storage.effects {
        FnEffect::Some(stored_effects) => {
            if stored_effects > EFFECT_WRITE_PROPS {
                analysis_data.expr_effects.insert(
                    (pos.start_offset() as u32, pos.end_offset() as u32),
                    stored_effects,
                );
            }
        }
        FnEffect::Arg(arg_offset) => {
            if let Some(arg) = expr_args.get(arg_offset as usize) {
                let arg_expr = arg.to_expr_ref();
                if let Some(arg_type) = analysis_data.expr_types.get(&(
                    arg_expr.pos().start_offset() as u32,
                    arg_expr.pos().end_offset() as u32,
                )) {
                    for arg_atomic_type in &arg_type.types {
                        if let TAtomic::TClosure(closure) = arg_atomic_type {
                            if let Some(evaluated_effects) = closure.effects {
                                analysis_data.expr_effects.insert(
                                    (pos.start_offset() as u32, pos.end_offset() as u32),
                                    evaluated_effects,
                                );
                            } else {
                                analysis_data.expr_effects.insert(
                                    (pos.start_offset() as u32, pos.end_offset() as u32),
                                    EFFECT_IMPURE,
                                );
                            }
                        }
                    }
                }
            }
        }
        FnEffect::Pure => {
            analysis_data.expr_effects.insert(
                (pos.start_offset() as u32, pos.end_offset() as u32),
                if function_storage.has_throw {
                    EFFECT_CAN_THROW
                } else {
                    EFFECT_PURE
                },
            );
        }
        FnEffect::Unknown => {
            // yet to be computed
        }
    }

    for arg in expr_args {
        analysis_data.combine_effects(arg.to_expr_ref().pos(), pos, pos);
    }
}
