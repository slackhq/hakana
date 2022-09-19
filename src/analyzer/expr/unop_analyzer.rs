use crate::expression_analyzer::{self, add_decision_dataflow};
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_type::{get_bool, get_literal_int};
use oxidized::ast_defs::Bop;
use oxidized::pos::Pos;
use oxidized::{aast, ast};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ast::Uop, &aast::Expr<(), ()>),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    expression_analyzer::analyze(
        statements_analyzer,
        expr.1,
        tast_info,
        context,
        if_body_context,
    );

    tast_info.expr_effects.insert(
        (pos.start_offset(), pos.end_offset()),
        *tast_info
            .expr_effects
            .get(&(expr.1.pos().start_offset(), expr.1.pos().end_offset()))
            .unwrap_or(&0),
    );

    match expr.0 {
        oxidized::ast_defs::Uop::Utild => {
            if let Some(stmt_type) = tast_info.get_expr_type(expr.1.pos()).cloned() {
                tast_info.set_expr_type(&pos, stmt_type);
            }
        }
        oxidized::ast_defs::Uop::Unot => {
            add_decision_dataflow(
                statements_analyzer,
                tast_info,
                expr.1,
                None,
                pos,
                get_bool(),
            );
        }
        oxidized::ast_defs::Uop::Uplus => {
            if let Some(stmt_type) = tast_info.get_expr_type(expr.1.pos()).cloned() {
                tast_info.set_expr_type(&pos, stmt_type);
            }
        }
        oxidized::ast_defs::Uop::Uminus => {
            if let Some(mut stmt_type) = tast_info.get_expr_type(expr.1.pos()).cloned() {
                if let Some(value) = stmt_type.get_single_literal_int_value() {
                    stmt_type = get_literal_int(-value);
                }

                tast_info.set_expr_type(&pos, stmt_type);
            }
        }
        oxidized::ast_defs::Uop::Uincr
        | oxidized::ast_defs::Uop::Udecr
        | oxidized::ast_defs::Uop::Upincr
        | oxidized::ast_defs::Uop::Updecr => {
            let analyzed_ok = expression_analyzer::analyze(
                statements_analyzer,
                &aast::Expr(
                    (),
                    pos.clone(),
                    aast::Expr_::Binop(Box::new((
                        Bop::Eq(None),
                        expr.1.clone(),
                        aast::Expr(
                            (),
                            pos.clone(),
                            aast::Expr_::Binop(Box::new((
                                if expr.0.is_upincr() || expr.0.is_uincr() {
                                    Bop::Plus
                                } else {
                                    Bop::Minus
                                },
                                expr.1.clone(),
                                aast::Expr((), pos.clone(), aast::Expr_::Int("1".to_string())),
                            ))),
                        ),
                    ))),
                ),
                tast_info,
                context,
                &mut None,
            );

            return analyzed_ok;
        }
        oxidized::ast_defs::Uop::Usilence => {
            if let Some(stmt_type) = tast_info.get_expr_type(expr.1.pos()).cloned() {
                tast_info.set_expr_type(&pos, stmt_type);
            }
        }
    }

    true
}
