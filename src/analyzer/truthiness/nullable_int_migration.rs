use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_str::StrId;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

use super::implicit_boolean_conversion_migration::ImplicitBooleanConversionMigration;

pub(super) struct NullableIntMigration {
    pub(super) handle_nullable: bool,
}

impl NullableIntMigration {
    fn is_preg(&self, statements_analyzer: &StatementsAnalyzer, expr: &aast::Expr<(), ()>) -> bool {
        if let aast::Expr_::Call(call) = &expr.2
            && let aast::Expr_::Id(id) = &call.func.2
            && matches!(
                statements_analyzer.interner.get(id.name()),
                Some(
                    StrId::PREG_MATCH
                        | StrId::PREG_MATCH_ALL
                        | StrId::PREG_MATCH_ALL_WITH_MATCHES
                        | StrId::PREG_MATCH_ALL_WITH_MATCHES_AND_ERROR
                        | StrId::PREG_MATCH_WITH_ERROR
                        | StrId::PREG_MATCH_WITH_MATCHES
                        | StrId::PREG_MATCH_WITH_MATCHES_AND_ERROR
                )
            )
        {
            return true;
        }

        false
    }
}

impl ImplicitBooleanConversionMigration for NullableIntMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        if self.handle_nullable {
            return expr_type.is_nullable()
                && super::aliased_type_matches(expr_type, &|t| {
                    matches!(t, TAtomic::TNull) || t.is_int()
                });
        }
        super::aliased_type_matches(expr_type, &|t| t.is_int())
    }

    fn migrate(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if self.handle_nullable {
            if super::is_trivial(expr) {
                let expr_text = &statements_analyzer.file_analyzer.file_source.file_contents
                    [pos.start_offset()..pos.end_offset()];
                let close_paren = if !super::is_sole_condition(statements_analyzer, pos) {
                    analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
                    ")"
                } else {
                    ""
                };

                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(" is nonnull && {expr_text} !== 0{close_paren}"),
                );
                return;
            }

            analysis_data.insert_at(
                pos.start_offset() as u32,
                "\\HH\\legacy_is_truthy(".to_string(),
            );
            analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
            return;
        }

        if self.is_preg(statements_analyzer, expr) {
            analysis_data.insert_at(pos.start_offset() as u32, "(int)".to_string());
            analysis_data.insert_at(pos.end_offset() as u32, " > 0".to_string());
            return;
        }

        analysis_data.insert_at(pos.end_offset() as u32, " !== 0".to_string());
    }

    fn migrate_negated(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if self.handle_nullable {
            if super::is_trivial(expr) {
                let expr_text = &statements_analyzer.file_analyzer.file_source.file_contents
                    [pos.start_offset()..pos.end_offset()];
                let close_paren = if !super::is_sole_condition(statements_analyzer, pos) {
                    analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
                    ")"
                } else {
                    ""
                };

                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(" is null || {expr_text} === 0{close_paren}"),
                );
                return;
            }

            analysis_data.insert_at(
                pos.start_offset() as u32,
                "!\\HH\\legacy_is_truthy(".to_string(),
            );
            analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
            return;
        }

        if self.is_preg(statements_analyzer, expr) {
            analysis_data.insert_at(pos.start_offset() as u32, "(int)".to_string());
            analysis_data.insert_at(pos.end_offset() as u32, " === 0".to_string());
            return;
        }
        analysis_data.insert_at(pos.end_offset() as u32, " === 0".to_string());
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolNumericCondition
    }
}
