use std::rc::Rc;

use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use hakana_type::{add_union_type, combine_union_types, get_mixed_any, get_null};
use oxidized::aast;
use oxidized::ast_defs::ParamKind;
use rustc_hash::FxHashSet;

pub(crate) fn analyze<'expr, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    analysis_data: &'tast mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let mut root_expr = left;
    let mut root_not_left = false;
    let mut has_arrayget_key = false;

    loop {
        match &root_expr.2 {
            aast::Expr_::ArrayGet(boxed) => {
                root_expr = &boxed.0;
                root_not_left = true;
                if let Some(dim) = &boxed.1 {
                    if let aast::Expr_::ArrayGet(..) = dim.2 {
                        has_arrayget_key = true;
                    }
                }
            }
            aast::Expr_::ObjGet(boxed) => {
                root_expr = &boxed.0;
                root_not_left = true;
            }
            _ => {
                break;
            }
        }
    }

    let root_type = if root_not_left {
        expression_analyzer::analyze(
            statements_analyzer,
            root_expr,
            analysis_data,
            context,
            if_body_context,
        );

        analysis_data.get_expr_type(root_expr.pos()).cloned()
    } else {
        None
    };

    let mut replacement_left = None;

    if has_arrayget_key
        || matches!(
            root_expr.2,
            aast::Expr_::Call(..)
                | aast::Expr_::Cast(..)
                | aast::Expr_::Eif(..)
                | aast::Expr_::Binop(..)
                | aast::Expr_::As(..)
                | aast::Expr_::ClassConst(..)
                | aast::Expr_::Pipe(..)
        )
    {
        replacement_left = Some(get_left_expr(
            context,
            statements_analyzer,
            left,
            analysis_data,
            if_body_context,
            if has_arrayget_key { left } else { root_expr },
            has_arrayget_key,
        ));
    } else if let Some(root_type) = root_type {
        if root_type.has_typealias() {
            replacement_left = Some(get_left_expr(
                context,
                statements_analyzer,
                left,
                analysis_data,
                if_body_context,
                left,
                true,
            ));
        }
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

    let old_expr_types = analysis_data.expr_types.clone();
    analysis_data.expr_types = analysis_data.expr_types.clone();

    expression_analyzer::analyze(
        statements_analyzer,
        &ternary,
        analysis_data,
        context,
        if_body_context,
    );

    let ternary_type = analysis_data
        .get_expr_type(&pos)
        .cloned()
        .unwrap_or(get_mixed_any());
    analysis_data.expr_types = old_expr_types;

    analysis_data.set_expr_type(&pos, ternary_type);

    analysis_data.combine_effects(left.pos(), right.pos(), pos);

    true
}

fn get_left_expr(
    context: &mut ScopeContext,
    statements_analyzer: &StatementsAnalyzer,
    left: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    if_body_context: &mut Option<ScopeContext>,
    root_expr: &aast::Expr<(), ()>,
    make_nullable: bool,
) -> aast::Expr<(), ()> {
    let mut isset_context = context.clone();
    isset_context.inside_isset = true;
    expression_analyzer::analyze(
        statements_analyzer,
        root_expr,
        analysis_data,
        &mut isset_context,
        if_body_context,
    );
    let mut condition_type = analysis_data
        .get_rc_expr_type(root_expr.pos())
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));
    let root_expr_var_id = format!(
        "$tmp_coalesce_var {}",
        left.pos().start_offset().to_string()
    );

    if make_nullable && !condition_type.is_nullable_mixed() {
        condition_type = Rc::new(add_union_type(
            (*condition_type).clone(),
            &get_null(),
            statements_analyzer.get_codebase(),
            false,
        ));
    }

    let redefined_vars = isset_context
        .get_redefined_vars(&context.vars_in_scope, false, &mut FxHashSet::default())
        .into_iter()
        .map(|(k, _)| k)
        .collect::<FxHashSet<_>>();

    //these vars were changed in both branches
    for redef_var_id in &redefined_vars {
        context.vars_in_scope.insert(
            redef_var_id.clone(),
            Rc::new(combine_union_types(
                &isset_context.vars_in_scope[redef_var_id],
                &context.vars_in_scope[redef_var_id],
                statements_analyzer.get_codebase(),
                false,
            )),
        );
    }

    context
        .vars_in_scope
        .insert(root_expr_var_id.clone(), condition_type);

    if root_expr != left {
        let mut left = left.clone();

        let new_root_expr = aast::Expr(
            (),
            root_expr.pos().clone(),
            aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
                root_expr.pos().clone(),
                (5, root_expr_var_id.clone()),
            ))),
        );

        replace_expr_with_root(&mut left, new_root_expr);
        left
    } else {
        return aast::Expr(
            (),
            root_expr.pos().clone(),
            aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
                root_expr.pos().clone(),
                (5, root_expr_var_id.clone()),
            ))),
        );
    }
}

fn replace_expr_with_root(expr: &mut aast::Expr<(), ()>, root: aast::Expr<(), ()>) {
    match &expr.2 {
        aast::Expr_::ArrayGet(boxed) => {
            let mut left = boxed.0.clone();
            replace_expr_with_root(&mut left, root);
            *expr = aast::Expr(
                (),
                expr.pos().clone(),
                aast::Expr_::ArrayGet(Box::new((left, boxed.1.clone()))),
            );
        }
        aast::Expr_::ObjGet(boxed) => {
            let mut left = boxed.0.clone();
            replace_expr_with_root(&mut left, root);
            *expr = aast::Expr(
                (),
                expr.pos().clone(),
                aast::Expr_::ObjGet(Box::new((left, boxed.1.clone(), boxed.2, boxed.3))),
            );
        }
        _ => {
            *expr = root;
        }
    }
}
