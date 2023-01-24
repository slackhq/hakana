use std::sync::Arc;

use hakana_reflection_info::StrId;
use rustc_hash::FxHashMap;

use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_type::get_mixed_any;
use hakana_type::template::TemplateResult;
use indexmap::IndexMap;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::call::{
    arguments_analyzer, expression_call_analyzer, function_call_analyzer, instance_call_analyzer,
    static_call_analyzer,
};

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
    let function_name_expr = expr.0;

    match &function_name_expr.2 {
        aast::Expr_::Id(boxed_id) => {
            return function_call_analyzer::analyze(
                statements_analyzer,
                ((&boxed_id.0, &boxed_id.1), expr.1, expr.2, expr.3),
                pos,
                tast_info,
                context,
                if_body_context,
            );
        }
        aast::Expr_::ObjGet(boxed) => {
            let (lhs_expr, rhs_expr, nullfetch, prop_or_method) =
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

            match prop_or_method {
                ast_defs::PropOrMethod::IsMethod => {
                    return instance_call_analyzer::analyze(
                        statements_analyzer,
                        (lhs_expr, rhs_expr, expr.1, expr.2, expr.3),
                        &pos,
                        tast_info,
                        context,
                        if_body_context,
                        matches!(nullfetch, ast_defs::OgNullFlavor::OGNullsafe),
                    )
                }
                _ => {
                    return expression_call_analyzer::analyze(
                        statements_analyzer,
                        (expr.0, expr.1, expr.2, expr.3),
                        pos,
                        tast_info,
                        context,
                        if_body_context,
                    );
                }
            }
        }
        aast::Expr_::ClassConst(boxed) => {
            let (class_id, rhs_expr) = (&boxed.0, &boxed.1);

            return static_call_analyzer::analyze(
                statements_analyzer,
                (class_id, rhs_expr, expr.1, expr.2, expr.3),
                &pos,
                tast_info,
                context,
                if_body_context,
            );
        }
        _ => {
            return expression_call_analyzer::analyze(
                statements_analyzer,
                (expr.0, expr.1, expr.2, expr.3),
                pos,
                tast_info,
                context,
                if_body_context,
            );
        }
    };
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

pub(crate) fn get_generic_param_for_offset(
    classlike_name: &StrId,
    template_name: &StrId,
    template_extended_params: &FxHashMap<StrId, IndexMap<StrId, Arc<TUnion>>>,
    found_generic_params: &FxHashMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,
) -> Arc<TUnion> {
    if let Some(found_generic_param) =
        if let Some(result_map) = found_generic_params.get(template_name) {
            result_map.get(classlike_name)
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
                        if extended_param_name == template_name && defining_entity == classlike_name
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
    tast_info: &mut TastInfo,
    method_id: &MethodIdentifier,
    functionlike_storage: &FunctionLikeInfo,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    template_result: &mut TemplateResult,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    pos: &Pos,
) -> bool {
    let codebase = statements_analyzer.get_codebase();

    let calling_class_storage = codebase.classlike_infos.get(&method_id.0).unwrap();

    let functionlike_id = FunctionLikeIdentifier::Method(method_id.0.clone(), method_id.1.clone());

    if !arguments_analyzer::check_arguments_match(
        statements_analyzer,
        call_expr.0,
        call_expr.1,
        call_expr.2,
        &functionlike_id,
        functionlike_storage,
        Some(calling_class_storage),
        tast_info,
        context,
        if_body_context,
        template_result,
        pos,
    ) {
        return false;
    }

    let effect = match functionlike_storage.effects {
        FnEffect::Unknown | FnEffect::None | FnEffect::Arg(_) => 0,
        FnEffect::Some(effect) => effect,
    };

    if let Some(existing_effects) = tast_info
        .expr_effects
        .get_mut(&(pos.start_offset(), pos.end_offset()))
    {
        *existing_effects |= effect;
    } else {
        tast_info
            .expr_effects
            .insert((pos.start_offset(), pos.end_offset()), effect);
    }

    for arg in call_expr.2 {
        tast_info.combine_effects(&arg.1, pos, pos);
    }

    if !template_result.template_types.is_empty() {
        check_template_result(statements_analyzer, template_result, pos, &functionlike_id);
    }

    return true;
}
