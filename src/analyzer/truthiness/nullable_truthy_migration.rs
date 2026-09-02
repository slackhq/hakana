use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::{TDict, TVec};
use hakana_code_info::{t_atomic::TAtomic, t_atomic::TNamedObject, t_union::TUnion};
use hakana_str::StrId;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

use super::implicit_boolean_conversion_migration::ImplicitBooleanConversionMigration;

pub(super) struct NullableTruthyMigration {}

impl ImplicitBooleanConversionMigration for NullableTruthyMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        expr_type.is_nullable()
            && super::aliased_type_matches(expr_type, &|t| match t {
                TAtomic::TNull
                | TAtomic::TTrue
                | TAtomic::TObject
                | TAtomic::TClosure(_)
                | TAtomic::TLiteralClassname { .. }
                | TAtomic::TLiteralClassPtr { .. }
                | TAtomic::TClassPtr { .. }
                | TAtomic::TClassname { .. }
                | TAtomic::TTypename { .. }
                | TAtomic::TAwaitable { .. }
                | TAtomic::TObjectIntersection { .. } => true,
                TAtomic::TNamedObject(TNamedObject { name, .. }) => !matches!(
                    *name,
                    StrId::CONTAINER
                        | StrId::KEYED_CONTAINER
                        | StrId::ANY_ARRAY
                        | StrId::TRAVERSABLE
                        | StrId::KEYED_TRAVERSABLE
                ),
                // Shapes with at least one non-optional key are always truthy.
                TAtomic::TDict(TDict { known_items, .. }) => {
                    known_items.as_ref().is_some_and(|known_items| {
                        known_items
                            .values()
                            .any(|(possibly_undefined, _)| !possibly_undefined)
                    })
                }
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

    fn migrate(
        &self,
        _statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        _expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        analysis_data.insert_at(pos.end_offset() as u32, " is nonnull".to_string());
    }

    fn migrate_negated(
        &self,
        _statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        _expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        analysis_data.insert_at(pos.end_offset() as u32, " is null".to_string());
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolCondition
    }
}
