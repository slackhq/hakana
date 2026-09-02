use std::rc::Rc;

use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_union::TUnion;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

pub(super) struct MigrationArgs<'a, 'sa> {
    pub statements_analyzer: &'a StatementsAnalyzer<'sa>,
    pub analysis_data: &'a mut FunctionAnalysisData,
    pub expr: &'a aast::Expr<(), ()>,
    pub pos: &'a Pos,
    pub expr_type: Rc<TUnion>,
}

pub(super) trait ImplicitBooleanConversionMigration: Send + Sync {
    fn matches(&self, expr_type: &TUnion) -> bool;

    fn migrate(&self, args: MigrationArgs);

    fn migrate_negated(&self, args: MigrationArgs);

    fn kind(&self) -> IssueKind;
}
