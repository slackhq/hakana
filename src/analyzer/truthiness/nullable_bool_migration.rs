use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use oxidized::aast;
use oxidized::ast::Bop;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

use super::implicit_boolean_conversion_migration::ImplicitBooleanConversionMigration;

pub(super) struct NullableBoolMigration {}

impl ImplicitBooleanConversionMigration for NullableBoolMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        expr_type.is_nullable()
            && super::aliased_type_matches(expr_type, &|t| {
                matches!(t, TAtomic::TNull) || t.is_bool()
            })
    }

    fn migrate(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        self.migrate_shared(false, statements_analyzer, analysis_data, expr, pos);
    }

    fn migrate_negated(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        self.migrate_shared(true, statements_analyzer, analysis_data, expr, pos);
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolCondition
    }
}

impl NullableBoolMigration {
    fn migrate_shared(
        &self,
        negated: bool,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if let Some(bin_op) = expr.2.as_binop()
            && matches!(bin_op.bop, Bop::QuestionQuestion)
            && bin_op.rhs.2.is_null()
        {
            analysis_data.add_replacement(
                (
                    bin_op.rhs.pos().start_offset() as u32,
                    bin_op.rhs.pos().end_offset() as u32,
                ),
                Replacement::Substitute("false".to_string()),
            );
            if negated {
                analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
                analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
                analysis_data.insert_at(pos.start_offset() as u32, "!".to_string());
            }
            return;
        }

        let needs_parentheses = negated || !super::is_sole_condition(statements_analyzer, pos);
        if needs_parentheses {
            analysis_data.insert_at(
                pos.start_offset() as u32,
                if negated { "!(" } else { "(" }.to_string(),
            );
        }
        analysis_data.insert_at(
            pos.end_offset() as u32,
            format!(" ?? false{}", if needs_parentheses { ")" } else { "" }),
        );
    }
}
