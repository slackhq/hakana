use hakana_code_info::issue::IssueKind;
use hakana_code_info::{t_atomic::TAtomic, t_atomic::TNamedObject, t_union::TUnion};
use hakana_str::StrId;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

use super::implicit_boolean_conversion_migration::ImplicitBooleanConversionMigration;

pub(super) struct NullableObjectMigration {}

impl ImplicitBooleanConversionMigration for NullableObjectMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        expr_type.is_nullable()
            && expr_type.types.iter().all(|t| {
                matches!(
                    t,
                    TAtomic::TNull | TAtomic::TObject | TAtomic::TNamedObject(..)
                ) && !matches!(
                    t,
                    TAtomic::TNamedObject(TNamedObject {
                        name: StrId::CONTAINER
                            | StrId::KEYED_CONTAINER
                            | StrId::ANY_ARRAY
                            | StrId::TRAVERSABLE
                            | StrId::KEYED_TRAVERSABLE,
                        ..
                    })
                )
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
