use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

use super::implicit_boolean_conversion_migration::ImplicitBooleanConversionMigration;

pub(super) struct NullableStringMigration {
    pub(super) handle_nullable: bool,
}

impl ImplicitBooleanConversionMigration for NullableStringMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        super::aliased_type_matches(expr_type, &|t| {
            matches!(
                t,
                TAtomic::TNull
                    | TAtomic::TString
                    | TAtomic::TStringWithFlags(..)
                    | TAtomic::TLiteralString { .. }
            )
        }) && (self.handle_nullable || !expr_type.is_nullable())
    }

    fn migrate(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if super::is_trivial(expr) {
            let expr_text = &statements_analyzer.file_analyzer.file_source.file_contents
                [pos.start_offset()..pos.end_offset()];
            let close_paren = if !super::is_sole_condition(statements_analyzer, pos) {
                analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
                ")"
            } else {
                ""
            };

            if self.handle_nullable {
                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(
                        " is nonnull && {expr_text} !== '' && {expr_text} !== '0'{close_paren}"
                    ),
                );
            } else {
                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(" !== '' && {expr_text} !== '0'{close_paren}"),
                );
            }
        } else {
            analysis_data.insert_at(
                pos.start_offset() as u32,
                "\\HH\\legacy_is_truthy(".to_string(),
            );
            analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
        }
    }

    fn migrate_negated(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if super::is_trivial(expr) {
            let expr_text = &statements_analyzer.file_analyzer.file_source.file_contents
                [pos.start_offset()..pos.end_offset()];
            let close_paren = if !super::is_sole_condition(statements_analyzer, pos) {
                analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
                ")"
            } else {
                ""
            };

            if self.handle_nullable {
                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(" is null || {expr_text} === '' || {expr_text} === '0'{close_paren}"),
                );
            } else {
                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(" === '' || {expr_text} === '0'{close_paren}"),
                );
            }
        } else {
            analysis_data.insert_at(
                pos.start_offset() as u32,
                "!\\HH\\legacy_is_truthy(".to_string(),
            );
            analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
        }
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolStringCondition
    }
}
