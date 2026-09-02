use super::implicit_boolean_conversion_migration::{
    ImplicitBooleanConversionMigration, MigrationArgs,
};
use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;

pub(super) struct StringTruthinessMigration {}

impl ImplicitBooleanConversionMigration for StringTruthinessMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        let mut it = super::aliased_types(expr_type).peekable();

        it.peek().is_some()
            && it.all(|t| {
                matches!(
                    t,
                    TAtomic::TString
                        | TAtomic::TStringWithFlags(..)
                        | TAtomic::TLiteralString { .. }
                )
            })
    }

    fn migrate(&self, args: MigrationArgs) {
        let MigrationArgs {
            statements_analyzer,
            analysis_data,
            expr,
            pos,
            expr_type,
        } = args;
        if super::is_trivial(expr) {
            let mut conds = vec![];

            if expr_type.is_nullable() {
                conds.push("is nonnull");
            }

            if expr_type.has_bool() {
                conds.push("!== false");
            }

            conds.push("!== ''");
            conds.push("!== '0'");

            super::expand_trivial_expr_conds(
                statements_analyzer,
                analysis_data,
                expr,
                pos,
                "&&",
                &conds,
            );
        } else {
            analysis_data.insert_at(
                pos.start_offset() as u32,
                "\\HH\\legacy_is_truthy(".to_string(),
            );
            analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
        }
    }

    fn migrate_negated(&self, args: MigrationArgs) {
        let MigrationArgs {
            statements_analyzer,
            analysis_data,
            expr,
            pos,
            expr_type,
        } = args;
        if super::is_trivial(expr) {
            let mut conds = vec![];

            if expr_type.is_nullable() {
                conds.push("is null");
            }

            if expr_type.has_bool() {
                conds.push("=== false");
            }

            conds.push("=== ''");
            conds.push("=== '0'");

            super::expand_trivial_expr_conds(
                statements_analyzer,
                analysis_data,
                expr,
                pos,
                "||",
                &conds,
            );
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
