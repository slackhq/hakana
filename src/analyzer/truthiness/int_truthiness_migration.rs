use hakana_code_info::issue::IssueKind;

use hakana_code_info::t_union::TUnion;
use hakana_str::StrId;
use oxidized::aast;

use crate::statements_analyzer::StatementsAnalyzer;

use super::implicit_boolean_conversion_migration::{
    ImplicitBooleanConversionMigration, MigrationArgs,
};

pub(super) struct IntTruthinessMigration {}

impl IntTruthinessMigration {
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

impl ImplicitBooleanConversionMigration for IntTruthinessMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        let mut it = super::aliased_types(expr_type).peekable();

        it.peek().is_some() && it.all(|t| t.is_int())
    }

    fn migrate(&self, args: MigrationArgs) {
        let MigrationArgs {
            statements_analyzer,
            analysis_data,
            expr,
            pos,
            expr_type,
        } = args;
        if expr_type.is_nullable() || expr_type.has_bool() {
            if super::is_trivial(expr) {
                let mut conds = vec![];

                if expr_type.is_nullable() {
                    conds.push("is nonnull");
                }

                if expr_type.has_bool() {
                    conds.push("!== false");
                }

                conds.push("!== 0");

                super::expand_trivial_expr_conds(
                    statements_analyzer,
                    analysis_data,
                    expr,
                    pos,
                    "&&",
                    &conds,
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

    fn migrate_negated(&self, args: MigrationArgs) {
        let MigrationArgs {
            statements_analyzer,
            analysis_data,
            expr,
            pos,
            expr_type,
        } = args;
        if expr_type.is_nullable() || expr_type.has_bool() {
            if super::is_trivial(expr) {
                let mut conds = vec![];

                if expr_type.is_nullable() {
                    conds.push("is null");
                }

                if expr_type.has_bool() {
                    conds.push("=== false");
                }

                conds.push("=== 0");

                super::expand_trivial_expr_conds(
                    statements_analyzer,
                    analysis_data,
                    expr,
                    pos,
                    "||",
                    &conds,
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
