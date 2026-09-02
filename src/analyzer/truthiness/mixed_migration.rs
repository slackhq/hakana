use hakana_code_info::{issue::IssueKind, t_atomic::TAtomic, t_union::TUnion};

use crate::truthiness::{ImplicitBooleanConversionMigration, MigrationArgs};

pub(super) struct MixedMigration {}

impl ImplicitBooleanConversionMigration for MixedMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        expr_type.types.iter().any(|t| {
            matches!(
                t,
                TAtomic::TMixed | TAtomic::TMixedWithFlags(..) | TAtomic::TMixedFromLoopIsset
            )
        })
    }

    fn migrate(&self, args: MigrationArgs) {
        let MigrationArgs {
            analysis_data, pos, ..
        } = args;
        analysis_data.insert_at(
            pos.start_offset() as u32,
            "\\HH\\legacy_is_truthy(".to_string(),
        );
        analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
    }

    fn migrate_negated(&self, args: MigrationArgs) {
        let MigrationArgs {
            analysis_data, pos, ..
        } = args;
        analysis_data.insert_at(
            pos.start_offset() as u32,
            "!\\HH\\legacy_is_truthy(".to_string(),
        );
        analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolCondition
    }
}
