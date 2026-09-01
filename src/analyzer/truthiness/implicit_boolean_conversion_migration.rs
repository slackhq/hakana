use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_union::TUnion;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

pub(super) trait ImplicitBooleanConversionMigration: Send + Sync {
    fn matches(&self, expr_type: &TUnion) -> bool;

    fn migrate(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    );

    fn migrate_negated(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    );

    fn kind(&self) -> IssueKind;
}
