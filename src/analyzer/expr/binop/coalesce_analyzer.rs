use std::rc::Rc;

use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;

use crate::expression_analyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::get_mixed_any;
use oxidized::aast;
use oxidized::ast_defs::ParamKind;

pub(crate) fn analyze<'expr, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    tast_info: &'tast mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let mut root_expr = left.clone();
    let mut root_not_left = false;

    loop {
        match root_expr.2 {
            aast::Expr_::ArrayGet(boxed) => {
                root_expr = boxed.0;
                root_not_left = true;
            }
            aast::Expr_::ObjGet(boxed) => {
                root_expr = boxed.0;
                root_not_left = true;
            }
            _ => {
                break;
            }
        }
    }

    let mut replacement_left: Option<aast::Expr<(), ()>> = None;

    if matches!(
        root_expr.2,
        aast::Expr_::Call(..)
            | aast::Expr_::Cast(..)
            | aast::Expr_::Eif(..)
            | aast::Expr_::Binop(..)
            | aast::Expr_::As(..)
    ) {
        let left_var_id = format!(
            "$<tmp coalesce var>{}",
            left.pos().start_offset().to_string()
        );

        let mut isset_context = context.clone();
        isset_context.inside_isset = true;

        expression_analyzer::analyze(
            statements_analyzer,
            left,
            tast_info,
            &mut isset_context,
            if_body_context,
        );

        let mut condition_type = tast_info
            .get_expr_type(left.pos())
            .cloned()
            .unwrap_or(get_mixed_any());
        if root_not_left && !condition_type.is_nullable_mixed() {
            condition_type.add_type(TAtomic::TNull);
        }

        context
            .vars_in_scope
            .insert(left_var_id.clone(), Rc::new(condition_type));

        replacement_left = Some(aast::Expr(
            (),
            left.pos().clone(),
            aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
                left.pos().clone(),
                (5, left_var_id.clone()),
            ))),
        ));
    }

    let ternary = aast::Expr(
        (),
        pos.clone(),
        aast::Expr_::Eif(Box::new((
            aast::Expr(
                (),
                left.pos().clone(),
                aast::Expr_::Call(Box::new((
                    aast::Expr(
                        (),
                        left.pos().clone(),
                        aast::Expr_::Id(Box::new(oxidized::ast_defs::Id(
                            left.pos().clone(),
                            "isset".to_string(),
                        ))),
                    ),
                    vec![],
                    vec![(
                        ParamKind::Pnormal,
                        replacement_left.clone().unwrap_or(left.clone()),
                    )],
                    None,
                ))),
            ),
            Some(replacement_left.unwrap_or(left.clone())),
            right.clone(),
        ))),
    );

    let old_expr_types = tast_info.expr_types.clone();
    tast_info.expr_types = tast_info.expr_types.clone();

    expression_analyzer::analyze(
        statements_analyzer,
        &ternary,
        tast_info,
        context,
        if_body_context,
    );

    let ternary_type = tast_info
        .get_expr_type(&pos)
        .cloned()
        .unwrap_or(get_mixed_any());
    tast_info.expr_types = old_expr_types;

    tast_info.set_expr_type(&pos, ternary_type);

    if tast_info
        .pure_exprs
        .contains(&(left.pos().start_offset(), left.pos().end_offset()))
        && tast_info
            .pure_exprs
            .contains(&(right.pos().start_offset(), right.pos().end_offset()))
    {
        tast_info
            .pure_exprs
            .insert((pos.start_offset(), pos.end_offset()));
    }

    true
}
