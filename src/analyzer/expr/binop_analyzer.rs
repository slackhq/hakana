use crate::expression_analyzer::{self, add_decision_dataflow};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;

use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::type_comparator::union_type_comparator;
use hakana_code_info::ttype::{get_bool, get_int};
use oxidized::pos::Pos;
use oxidized::{aast, ast};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ast::Bop, &aast::Expr<(), ()>, &aast::Expr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    match &expr.0 {
        oxidized::ast_defs::Bop::Plus
        | oxidized::ast_defs::Bop::Minus
        | oxidized::ast_defs::Bop::Star
        | oxidized::ast_defs::Bop::Slash
        | oxidized::ast_defs::Bop::Amp
        | oxidized::ast_defs::Bop::Bar
        | oxidized::ast_defs::Bop::Ltlt
        | oxidized::ast_defs::Bop::Gtgt
        | oxidized::ast_defs::Bop::Percent
        | oxidized::ast_defs::Bop::Xor
        | oxidized::ast_defs::Bop::Starstar => {
            crate::expr::binop::arithmetic_analyzer::analyze(
                statements_analyzer,
                pos,
                expr.0,
                expr.1,
                expr.2,
                analysis_data,
                context,
            )?;
            Ok(())
        }

        oxidized::ast_defs::Bop::Ampamp => {
            crate::expr::binop::and_analyzer::analyze(
                statements_analyzer,
                pos,
                expr.1,
                expr.2,
                analysis_data,
                context,
            )?;

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                Some(expr.2),
                pos,
                get_bool(),
            );

            Ok(())
        }

        oxidized::ast_defs::Bop::Barbar => {
            crate::expr::binop::or_analyzer::analyze(
                statements_analyzer,
                expr.1,
                expr.2,
                analysis_data,
                context,
            )?;

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                Some(expr.2),
                pos,
                get_bool(),
            );

            Ok(())
        }

        oxidized::ast_defs::Bop::Eqeq
        | oxidized::ast_defs::Bop::Eqeqeq
        | oxidized::ast_defs::Bop::Diff
        | oxidized::ast_defs::Bop::Diff2
        | oxidized::ast_defs::Bop::Lt
        | oxidized::ast_defs::Bop::Lte
        | oxidized::ast_defs::Bop::Gt
        | oxidized::ast_defs::Bop::Gte => {
            expression_analyzer::analyze(statements_analyzer, expr.1, analysis_data, context)?;

            expression_analyzer::analyze(statements_analyzer, expr.2, analysis_data, context)?;

            let lhs_type = analysis_data.get_rc_expr_type(expr.1.pos());
            let rhs_type = analysis_data.get_rc_expr_type(expr.2.pos());

            let interner = statements_analyzer.get_interner();

            if let (Some(lhs_type), Some(rhs_type)) = (lhs_type, rhs_type) {
                if is_resolvable(expr.1)
                    && is_resolvable(expr.2)
                    && (!lhs_type.is_single() || !rhs_type.is_single())
                    && !union_type_comparator::can_expression_types_be_identical(
                        statements_analyzer.get_codebase(),
                        lhs_type,
                        rhs_type,
                        true,
                    )
                {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::ImpossibleTypeComparison,
                            format!(
                                "Type {} cannot be compared to {}",
                                lhs_type.get_id(Some(interner)),
                                rhs_type.get_id(Some(interner)),
                            ),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                } else if matches!(
                    expr.0,
                    oxidized::ast_defs::Bop::Eqeqeq | oxidized::ast_defs::Bop::Diff2
                ) && lhs_type.types.len() == 1
                    && lhs_type.types[0] == rhs_type.types[0]
                    && matches!(lhs_type.types[0], TAtomic::TNamedObject { .. })
                {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::StrictObjectEquality,
                            format!(
                                "Strict equality compares {} objects by reference rather than value",
                                lhs_type.get_id(Some(interner)),
                            ),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                Some(expr.2),
                pos,
                get_bool(),
            );

            analysis_data.combine_effects(expr.1.pos(), expr.2.pos(), pos);

            Ok(())
        }

        oxidized::ast_defs::Bop::Dot => {
            crate::expr::binop::concat_analyzer::analyze(
                statements_analyzer,
                pos,
                expr.1,
                expr.2,
                analysis_data,
                context,
            )?;

            Ok(())
        }

        oxidized::ast_defs::Bop::Cmp => {
            expression_analyzer::analyze(statements_analyzer, expr.1, analysis_data, context)?;

            expression_analyzer::analyze(statements_analyzer, expr.2, analysis_data, context)?;

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                Some(expr.2),
                pos,
                get_int(),
            );

            analysis_data.combine_effects(expr.1.pos(), expr.2.pos(), pos);

            Ok(())
        }

        oxidized::ast_defs::Bop::QuestionQuestion => {
            crate::expr::binop::coalesce_analyzer::analyze(
                statements_analyzer,
                pos,
                expr.1,
                expr.2,
                analysis_data,
                context,
            )?;

            Ok(())
        }

        oxidized::ast_defs::Bop::Eq(_) => {
            crate::expr::binop::assignment_analyzer::analyze(
                statements_analyzer,
                (expr.0, expr.1, Some(expr.2)),
                pos,
                None,
                analysis_data,
                context,
                None,
            )?;

            Ok(())
        }
    }
}

fn is_resolvable(expr: &aast::Expr<(), ()>) -> bool {
    matches!(expr.2, aast::Expr_::Lvar(_) | aast::Expr_::ObjGet(_))
}
