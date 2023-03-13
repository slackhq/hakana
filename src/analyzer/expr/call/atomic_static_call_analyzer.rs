use hakana_reflection_info::{
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    EFFECT_IMPURE,
};
use oxidized::{
    aast,
    ast_defs::{self, Pos},
};

use crate::{
    scope_analyzer::ScopeAnalyzer, scope_context::ScopeContext,
    statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo,
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    lhs_type_part: &TAtomic,
    result: &mut AtomicMethodCallAnalysisResult,
) {
    let classlike_name = match &lhs_type_part {
        TAtomic::TNamedObject {
            name, extra_types, ..
        } => {
            match &expr.0 .2 {
                aast::ClassId_::CIexpr(lhs_expr) => {
                    if !matches!(&lhs_expr.2, aast::Expr_::Id(_)) {
                        handle_method_call_on_named_object(
                            result,
                            name,
                            extra_types,
                            &None,
                            tast_info,
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
                                &expr.2,
                                &expr.3,
                                &expr.4,
                            ),
                            lhs_type_part,
                            context,
                            if_body_context,
                        );
                        return;
                    }
                }
                _ => {}
            }

            name.clone()
        }
        TAtomic::TClassname { as_type, .. } | TAtomic::TGenericClassname { as_type, .. } => {
            let as_type = *as_type.clone();
            if let TAtomic::TNamedObject { name, .. } = as_type {
                // todo check class name and register usage
                name
            } else {
                return;
            }
        }
        TAtomic::TLiteralClassname { name } => name.clone(),
        TAtomic::TGenericParam { as_type, .. } => {
            let mut classlike_name = None;
            for generic_param_type in &as_type.types {
                if let TAtomic::TNamedObject { name, .. } = generic_param_type {
                    classlike_name = Some(name.clone());
                    break;
                } else {
                    return;
                }
            }

            if let Some(classlike_name) = classlike_name {
                classlike_name
            } else {
                // todo emit issue
                return;
            }
        }
        _ => {
            if lhs_type_part.is_mixed() {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedMethodCall,
                        "Method called on unknown object".to_string(),
                        statements_analyzer.get_hpos(&pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }

            return;
        }
    };

    let codebase = statements_analyzer.get_codebase();

    let method_name = statements_analyzer.get_interner().get(&expr.1 .1);

    if method_name.is_none() || !codebase.method_exists(&classlike_name, &method_name.unwrap()) {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentMethod,
                format!(
                    "Method {}::{} does not exist",
                    statements_analyzer.get_interner().lookup(&classlike_name),
                    &expr.1 .1
                ),
                statements_analyzer.get_hpos(&pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        tast_info
            .expr_effects
            .insert((pos.start_offset(), pos.end_offset()), EFFECT_IMPURE);

        return;
    }

    result.return_type = Some(existing_atomic_method_call_analyzer::analyze(
        statements_analyzer,
        classlike_name,
        &method_name.unwrap(),
        (expr.2, expr.3, expr.4),
        lhs_type_part,
        pos,
        tast_info,
        context,
        if_body_context,
        None,
        None,
        result,
    ));
}
