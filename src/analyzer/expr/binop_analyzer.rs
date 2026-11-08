use crate::expression_analyzer::{self, add_decision_dataflow};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;

use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::comparison::union_type_comparator;
use hakana_code_info::ttype::{get_bool, get_int};
use hakana_parser::aast;
use hakana_parser::aast::Pos;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&aast::Bop, &aast::Expr, &aast::Expr),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    match &expr.0 {
        hakana_parser::aast::Bop::Plus
        | hakana_parser::aast::Bop::Minus
        | hakana_parser::aast::Bop::Star
        | hakana_parser::aast::Bop::Slash
        | hakana_parser::aast::Bop::Amp
        | hakana_parser::aast::Bop::Bar
        | hakana_parser::aast::Bop::Ltlt
        | hakana_parser::aast::Bop::Gtgt
        | hakana_parser::aast::Bop::Percent
        | hakana_parser::aast::Bop::Xor
        | hakana_parser::aast::Bop::Starstar => {
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

        hakana_parser::aast::Bop::Ampamp => {
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

        hakana_parser::aast::Bop::Barbar => {
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

        hakana_parser::aast::Bop::Eqeq
        | hakana_parser::aast::Bop::Eqeqeq
        | hakana_parser::aast::Bop::Diff
        | hakana_parser::aast::Bop::Diff2
        | hakana_parser::aast::Bop::Lt
        | hakana_parser::aast::Bop::Lte
        | hakana_parser::aast::Bop::Gt
        | hakana_parser::aast::Bop::Gte => {
            expression_analyzer::analyze(
                statements_analyzer,
                expr.1,
                analysis_data,
                context,
                true,
            )?;

            expression_analyzer::analyze(
                statements_analyzer,
                expr.2,
                analysis_data,
                context,
                true,
            )?;

            let lhs_type = analysis_data.get_rc_expr_type(expr.1.pos());
            let rhs_type = analysis_data.get_rc_expr_type(expr.2.pos());

            let interner = statements_analyzer.interner;

            if let (Some(lhs_type), Some(rhs_type)) = (lhs_type, rhs_type) {
                if is_resolvable(expr.1)
                    && is_resolvable(expr.2)
                    && (!lhs_type.is_single() || !rhs_type.is_single())
                    && !union_type_comparator::can_expression_types_be_identical(
                        statements_analyzer.codebase,
                        statements_analyzer.get_file_path(),
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
                    hakana_parser::aast::Bop::Eqeqeq | hakana_parser::aast::Bop::Diff2
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

        hakana_parser::aast::Bop::Dot => {
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

        hakana_parser::aast::Bop::Cmp => {
            expression_analyzer::analyze(
                statements_analyzer,
                expr.1,
                analysis_data,
                context,
                true,
            )?;

            expression_analyzer::analyze(
                statements_analyzer,
                expr.2,
                analysis_data,
                context,
                true,
            )?;

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

        hakana_parser::aast::Bop::QuestionQuestion => {
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
    }
}

fn is_resolvable(expr: &aast::Expr) -> bool {
    matches!(expr.2, aast::Expr_::Lvar(_) | aast::Expr_::ObjGet(_))
}
