use crate::expression_analyzer::{self, add_decision_dataflow};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::{TAtomic, TNamedObject};
use hakana_code_info::ttype::{get_bool, get_literal_int};
use hakana_str::StrId;
use oxidized::ast::Binop;
use oxidized::ast_defs::Bop;
use oxidized::pos::Pos;
use oxidized::{aast, ast};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ast::Uop, &aast::Expr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    if let oxidized::ast_defs::Uop::Unot = expr.0 {
        context.inside_negation = !context.inside_negation;
    }
    expression_analyzer::analyze(statements_analyzer, expr.1, analysis_data, context, true)?;
    if let oxidized::ast_defs::Uop::Unot = expr.0 {
        context.inside_negation = !context.inside_negation;
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
        oxidized::ast_defs::Uop::Utild => {
            if let Some(stmt_type) = analysis_data.get_rc_expr_type(expr.1.pos()).cloned() {
                analysis_data.set_rc_expr_type(pos, stmt_type);
            }
        }
        oxidized::ast_defs::Uop::Unot => {
            if let Some(expr_type) = analysis_data.get_expr_type(expr.1.pos()).cloned()
                && expr_type.is_nullable()
                && expr_type.types.iter().all(|t| {
                    matches!(
                        t,
                        TAtomic::TNull | TAtomic::TObject | TAtomic::TNamedObject(..)
                    ) && !matches!(
                        t,
                        TAtomic::TNamedObject(TNamedObject {
                            name: StrId::CONTAINER
                                | StrId::KEYED_CONTAINER
                                | StrId::ANY_ARRAY
                                | StrId::TRAVERSABLE
                                | StrId::KEYED_TRAVERSABLE,
                            ..
                        })
                    )
                })
                && !analysis_data
                    .insertions
                    .contains_key(&(pos.end_offset() as u32))
            {
                let issue = Issue::new(
                    IssueKind::NonBoolCondition,
                    "Only bool values can be used as a condition".to_string(),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                );

                if statements_analyzer.should_autofix(context, analysis_data, &issue) {
                    analysis_data.add_replacement(
                        (pos.start_offset() as u32, pos.start_offset() as u32 + 1),
                        Replacement::Remove,
                    );
                    analysis_data.insert_at(pos.end_offset() as u32, " is null".to_string());
                } else {
                    analysis_data.maybe_add_issue(
                        issue,
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                None,
                pos,
                get_bool(),
            );
        }
        oxidized::ast_defs::Uop::Uplus => {
            if let Some(stmt_type) = analysis_data.get_rc_expr_type(expr.1.pos()).cloned() {
                analysis_data.set_rc_expr_type(pos, stmt_type);
            }
        }
        oxidized::ast_defs::Uop::Uminus => {
            if let Some(mut stmt_type) = analysis_data.get_expr_type(expr.1.pos()).cloned() {
                if let Some(value) = stmt_type.get_single_literal_int_value() {
                    stmt_type = get_literal_int(-value);
                }

                analysis_data.set_expr_type(pos, stmt_type);
            }
        }
        oxidized::ast_defs::Uop::Uincr
        | oxidized::ast_defs::Uop::Udecr
        | oxidized::ast_defs::Uop::Upincr
        | oxidized::ast_defs::Uop::Updecr => {
            context.inside_assignment_op = true;
            let analyzed_ok = expression_analyzer::analyze(
                statements_analyzer,
                &aast::Expr(
                    (),
                    pos.clone(),
                    aast::Expr_::Assign(Box::new((
                        expr.1.clone(),
                        None,
                        aast::Expr(
                            (),
                            pos.clone(),
                            aast::Expr_::Binop(Box::new(Binop {
                                bop: if expr.0.is_upincr() || expr.0.is_uincr() {
                                    Bop::Plus
                                } else {
                                    Bop::Minus
                                },
                                lhs: expr.1.clone(),
                                rhs: aast::Expr((), pos.clone(), aast::Expr_::Int("1".to_string())),
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
        oxidized::ast_defs::Uop::Usilence => {
            if let Some(stmt_type) = analysis_data.get_rc_expr_type(expr.1.pos()).cloned() {
                analysis_data.set_rc_expr_type(pos, stmt_type);
            }
        }
    }

    Ok(())
}
