use crate::expression_analyzer::{self, add_decision_dataflow};
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::FunctionAnalysisData;

use hakana_type::{get_bool, get_int};
use oxidized::pos::Pos;
use oxidized::{aast, ast};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ast::Bop, &aast::Expr<(), ()>, &aast::Expr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
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
            );
            return true;
        }

        oxidized::ast_defs::Bop::Ampamp => {
            let result = crate::expr::binop::and_analyzer::analyze(
                statements_analyzer,
                pos,
                expr.1,
                expr.2,
                analysis_data,
                context,
                if_body_context,
            );

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                Some(expr.2),
                pos,
                get_bool(),
            );

            return result;
        }

        oxidized::ast_defs::Bop::Barbar => {
            let result = crate::expr::binop::or_analyzer::analyze(
                statements_analyzer,
                expr.1,
                expr.2,
                analysis_data,
                context,
                if_body_context,
            );

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                Some(expr.2),
                pos,
                get_bool(),
            );

            return result;
        }

        oxidized::ast_defs::Bop::Eqeq
        | oxidized::ast_defs::Bop::Eqeqeq
        | oxidized::ast_defs::Bop::Diff
        | oxidized::ast_defs::Bop::Diff2
        | oxidized::ast_defs::Bop::Lt
        | oxidized::ast_defs::Bop::Lte
        | oxidized::ast_defs::Bop::Gt
        | oxidized::ast_defs::Bop::Gte => {
            if !expression_analyzer::analyze(
                statements_analyzer,
                expr.1,
                analysis_data,
                context,
                if_body_context,
            ) {
                return false;
            }

            if !expression_analyzer::analyze(
                statements_analyzer,
                expr.2,
                analysis_data,
                context,
                if_body_context,
            ) {
                return false;
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

            return true;
        }

        oxidized::ast_defs::Bop::Dot => {
            crate::expr::binop::concat_analyzer::analyze(
                statements_analyzer,
                pos,
                expr.1,
                expr.2,
                analysis_data,
                context,
            );

            return true;
        }

        oxidized::ast_defs::Bop::Cmp => {
            if !expression_analyzer::analyze(
                statements_analyzer,
                expr.1,
                analysis_data,
                context,
                if_body_context,
            ) {
                return false;
            }

            if !expression_analyzer::analyze(
                statements_analyzer,
                expr.2,
                analysis_data,
                context,
                if_body_context,
            ) {
                return false;
            }

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                expr.1,
                Some(expr.2),
                pos,
                get_int(),
            );

            analysis_data.combine_effects(expr.1.pos(), expr.2.pos(), pos);

            return true;
        }

        oxidized::ast_defs::Bop::QuestionQuestion => {
            return crate::expr::binop::coalesce_analyzer::analyze(
                statements_analyzer,
                pos,
                expr.1,
                expr.2,
                analysis_data,
                context,
                if_body_context,
            );
        }

        oxidized::ast_defs::Bop::Eq(_) => {
            if crate::expr::binop::assignment_analyzer::analyze(
                statements_analyzer,
                (expr.0, expr.1, Some(expr.2)),
                pos,
                None,
                analysis_data,
                context,
                false,
            )
            .is_err()
            {
                return false;
            }
            return true;
        }
    }
}
