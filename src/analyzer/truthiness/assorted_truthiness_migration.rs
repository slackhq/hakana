use crate::truthiness::aliased_types;

use super::implicit_boolean_conversion_migration::{
    ImplicitBooleanConversionMigration, MigrationArgs,
};
use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::{TDict, TVec};
use hakana_code_info::{t_atomic::TAtomic, t_atomic::TNamedObject, t_union::TUnion};
use hakana_str::StrId;

pub(super) struct AssortedTruthinessMigration {}

impl AssortedTruthinessMigration {
    fn has_shape_with_only_opt_keys(&self, expr_type: &TUnion) -> bool {
        aliased_types(&expr_type).any(|t| {
            if let TAtomic::TDict(TDict { known_items, .. }) = t {
                known_items.as_ref().is_some_and(|known_items| {
                    known_items
                        .values()
                        .any(|(possibly_undefined, _)| *possibly_undefined)
                })
            } else {
                false
            }
        })
    }
}

impl ImplicitBooleanConversionMigration for AssortedTruthinessMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        let mut types = super::aliased_types(expr_type).peekable();

        types.peek().is_some()
            && types.all(|t| match t {
                TAtomic::TTrue
                | TAtomic::TObject
                | TAtomic::TClosure(_)
                | TAtomic::TLiteralClassname { .. }
                | TAtomic::TLiteralClassPtr { .. }
                | TAtomic::TClassPtr { .. }
                | TAtomic::TClassname { .. }
                | TAtomic::TTypename { .. }
                | TAtomic::TAwaitable { .. }
                | TAtomic::TResource
                | TAtomic::TObjectIntersection { .. }
                | TAtomic::TDict(TDict {
                    known_items: Some(..),
                    ..
                }) => true,
                TAtomic::TNamedObject(TNamedObject { name, .. }) => !matches!(
                    *name,
                    StrId::CONTAINER
                        | StrId::KEYED_CONTAINER
                        | StrId::ANY_ARRAY
                        | StrId::TRAVERSABLE
                        | StrId::KEYED_TRAVERSABLE
                ),

                // Tuples with at least one non-optional item are always truthy.
                TAtomic::TVec(TVec { known_items, .. }) => {
                    known_items.as_ref().is_some_and(|known_items| {
                        known_items
                            .values()
                            .any(|(possibly_undefined, _)| !possibly_undefined)
                    })
                }
                _ => false,
            })
    }

    fn migrate(&self, args: MigrationArgs) {
        let MigrationArgs {
            expr,
            expr_type,
            analysis_data,
            pos,
            statements_analyzer,
            ..
        } = args;

        if super::is_trivial(expr) || !(expr_type.is_nullable() && expr_type.has_bool()) {
            let mut conds = Vec::new();
            if expr_type.is_nullable() {
                conds.push("is nonnull");
            }

            if expr_type.has_bool() {
                conds.push("!== false");
            }

            if self.has_shape_with_only_opt_keys(&expr_type) {
                conds.push("!== shape()")
            }

            if conds.is_empty() {
                conds.push("is nonnull");
            }

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
    }

    fn migrate_negated(&self, args: MigrationArgs) {
        let MigrationArgs {
            expr,
            expr_type,
            analysis_data,
            pos,
            statements_analyzer,
            ..
        } = args;
        if super::is_trivial(expr) || !(expr_type.is_nullable() && expr_type.has_bool()) {
            let mut conds = Vec::new();
            if expr_type.is_nullable() {
                conds.push("is null");
            }

            if expr_type.has_bool() {
                conds.push("=== false");
            }

            if self.has_shape_with_only_opt_keys(&expr_type) {
                conds.push("=== shape()")
            }

            if conds.is_empty() {
                conds.push("is null");
            }

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
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolCondition
    }
}
