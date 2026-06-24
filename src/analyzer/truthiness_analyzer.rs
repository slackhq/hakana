use std::sync::LazyLock;

use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::{t_atomic::TAtomic, t_atomic::TNamedObject, t_union::TUnion};
use hakana_str::StrId;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;

/// Base trait for migrating implicit boolean conversions of some expression type.
trait ImplicitBooleanConversionMigration: Send + Sync {
    fn matches(&self, expr_type: &TUnion) -> bool;

    /// Migrate a regular implicit boolean conversion.
    fn migrate(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    );

    /// Migrate a negated (i.e. !$foo) boolean conversion.
    /// Useful if the migration should have different output,
    /// e.g. to migrate !$foo as `$foo === 0` instead of `!$foo !== 0`.
    fn migrate_negated(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    );
}

struct NullableObjectMigration {}

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
}

struct IntMigration {}

impl IntMigration {
    /// Check whether a function call is to one of the preg_* family of functions.
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

impl ImplicitBooleanConversionMigration for IntMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        expr_type.is_int()
    }

    fn migrate(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if self.is_preg(statements_analyzer, expr) {
            analysis_data.insert_at(pos.start_offset() as u32, "(int)".to_string());
            analysis_data.insert_at(pos.end_offset() as u32, " > 0".to_string());
        } else {
            analysis_data.insert_at(pos.end_offset() as u32, " !== 0".to_string());
        }
    }

    fn migrate_negated(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if self.is_preg(statements_analyzer, expr) {
            analysis_data.insert_at(pos.start_offset() as u32, "(int)".to_string());
            analysis_data.insert_at(pos.end_offset() as u32, " === 0".to_string());
        } else {
            analysis_data.insert_at(pos.end_offset() as u32, " === 0".to_string());
        }
    }
}

pub(crate) fn check_implicit_boolean_conversion(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    mut expr: &aast::Expr<(), ()>,
) {
    let mut negation_depth: u32 = 0;
    let mut pos = expr.pos();
    while let aast::Expr_::Unop(inner) = &expr.2
        && let oxidized::ast_defs::Uop::Unot = inner.0
    {
        expr = &inner.1;
        pos = expr.pos();
        negation_depth += 1;
    }

    let Some(expr_type) = analysis_data.get_rc_expr_type(expr.pos()) else {
        return;
    };

    let is_negated = negation_depth % 2 == 1;

    if !expr_type.is_bool() {
        static TRUTHINESS_MIGRATIONS: LazyLock<Vec<Box<dyn ImplicitBooleanConversionMigration>>> =
            LazyLock::new(|| {
                vec![
                    Box::new(NullableObjectMigration {}),
                    Box::new(IntMigration {}),
                ]
            });

        if !analysis_data
            .insertions
            .contains_key(&(pos.end_offset() as u32))
            && let Some(migration) = TRUTHINESS_MIGRATIONS.iter().find(|m| m.matches(&expr_type))
        {
            let issue = Issue::new(
                IssueKind::NonBoolCondition,
                "Only bool values can be used as a condition".to_string(),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            );

            if statements_analyzer.should_autofix(context, analysis_data, &issue) {
                analysis_data.add_replacement(
                    (
                        pos.start_offset() as u32 - negation_depth,
                        pos.start_offset() as u32,
                    ),
                    Replacement::Remove,
                );
                if is_negated {
                    migration.migrate_negated(statements_analyzer, analysis_data, expr, pos);
                } else {
                    migration.migrate(statements_analyzer, analysis_data, expr, pos);
                }
            } else {
                analysis_data.maybe_add_issue(
                    issue,
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }
    }
}
