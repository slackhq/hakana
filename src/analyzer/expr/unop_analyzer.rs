use crate::expression_analyzer::{self, add_decision_dataflow};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::ttype::{get_bool, get_literal_int};
use hakana_parser::aast::Binop;
use hakana_parser::aast::Bop;

use hakana_parser::aast;

pub(crate) fn analyze<'ast>(
    statements_analyzer: &StatementsAnalyzer<'ast>,
    expr: (&aast::Uop, &aast::Expr<'ast>),
    outer_expr: &aast::Expr<'ast>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let arena = statements_analyzer.arena();
    let pos = outer_expr.pos();

    if let hakana_parser::aast::Uop::Unot = expr.0 {
        context.inside_negation = !context.inside_negation;
        expression_analyzer::analyze(statements_analyzer, expr.1, analysis_data, context, true)?;
        context.inside_negation = !context.inside_negation;
    } else {
        expression_analyzer::analyze(statements_analyzer, expr.1, analysis_data, context, true)?;
    }

    analysis_data.expr_effects.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        *analysis_data
            .expr_effects
            .get(&(
                expr.1.pos().start_offset() as u32,
                expr.1.pos().end_offset() as u32,
            ))
            .unwrap_or(&0),
    );

    match expr.0 {
        hakana_parser::aast::Uop::Utild => {
            if let Some(stmt_type) = analysis_data.get_rc_expr_type(expr.1.pos()).cloned() {
                analysis_data.set_rc_expr_type(pos, stmt_type);
            }
        }
        hakana_parser::aast::Uop::Unot => {
            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                None,
                pos,
                get_bool(),
            );
        }
        hakana_parser::aast::Uop::Uplus => {
            if let Some(stmt_type) = analysis_data.get_rc_expr_type(expr.1.pos()).cloned() {
                analysis_data.set_rc_expr_type(pos, stmt_type);
            }
        }
        hakana_parser::aast::Uop::Uminus => {
            if let Some(mut stmt_type) = analysis_data.get_expr_type(expr.1.pos()).cloned() {
                if let Some(value) = stmt_type.get_single_literal_int_value() {
                    stmt_type = get_literal_int(-value);
                }

                analysis_data.set_expr_type(pos, stmt_type);
            }
        }
        hakana_parser::aast::Uop::Uincr
        | hakana_parser::aast::Uop::Udecr
        | hakana_parser::aast::Uop::Upincr
        | hakana_parser::aast::Uop::Updecr => {
            context.inside_assignment_op = true;
            let analyzed_ok = expression_analyzer::analyze(
                statements_analyzer,
                &aast::Expr(
                    (),
                    *pos,
                    aast::Expr_::Assign(&*arena.alloc((
                        *expr.1,
                        None,
                        aast::Expr(
                            (),
                            *pos,
                            aast::Expr_::Binop(&*arena.alloc(Binop {
                                bop: if expr.0.is_upincr() || expr.0.is_uincr() {
                                    Bop::Plus
                                } else {
                                    Bop::Minus
                                },
                                lhs: *expr.1,
                                rhs: aast::Expr((), *pos, aast::Expr_::Int("1")),
                            })),
                        ),
                    ))),
                ),
                analysis_data,
                context,
                true,
            );
            context.inside_assignment_op = false;

            return analyzed_ok;
        }
        hakana_parser::aast::Uop::Usilence => {
            if let Some(stmt_type) = analysis_data.get_rc_expr_type(expr.1.pos()).cloned() {
                analysis_data.set_rc_expr_type(pos, stmt_type);
            }
        }
    }

    Ok(())
}
