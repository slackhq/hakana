use std::sync::LazyLock;

use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::issue::Issue;
use oxidized::aast;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::truthiness::container_migration::ContainerMigration;

mod container_migration;
mod implicit_boolean_conversion_migration;
mod int_migration;
mod nullable_object_migration;
mod nullable_string_migration;

use implicit_boolean_conversion_migration::ImplicitBooleanConversionMigration;
use int_migration::IntMigration;
use nullable_object_migration::NullableObjectMigration;
use nullable_string_migration::NullableStringMigration;

/// Is this a trivial expression that should be fine to repeat?
fn is_trivial(expr: &aast::Expr<(), ()>) -> bool {
    matches!(
        expr.2,
        aast::Expr_::Lvar(..)
            | aast::Expr_::ArrayGet(..)
            | aast::Expr_::ClassGet(..)
            | aast::Expr_::ObjGet(..)
    )
}

/// Is this expression the sole condition in an `if` statement?
fn is_sole_condition(statements_analyzer: &StatementsAnalyzer, pos: &Pos) -> bool {
    let file_contents = &statements_analyzer.file_analyzer.file_source.file_contents;
    (file_contents[..pos.start_offset()].ends_with("if (")
        || file_contents[..pos.start_offset()].ends_with("if (!"))
        && file_contents[pos.end_offset()..].starts_with(")")
}

pub(crate) fn check_implicit_boolean_conversion(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    mut expr: &aast::Expr<(), ()>,
) {
    let mut negation_depth: u32 = 0;
    let mut pos = expr.pos();
    let negation_start_offset = pos.start_offset();
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
                    Box::new(NullableStringMigration {
                        handle_nullable: true,
                    }),
                    Box::new(NullableStringMigration {
                        handle_nullable: false,
                    }),
                    Box::new(ContainerMigration {
                        handle_nullable: true,
                    }),
                    Box::new(ContainerMigration {
                        handle_nullable: false,
                    }),
                ]
            });

        if !analysis_data
            .insertions
            .contains_key(&(pos.end_offset() as u32))
            && let Some(migration) = TRUTHINESS_MIGRATIONS.iter().find(|m| m.matches(expr_type))
        {
            let issue = Issue::new(
                migration.kind(),
                "Only bool values can be used as a condition".to_string(),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            );

            if statements_analyzer.should_autofix(context, analysis_data, &issue) {
                // Get rid of all negations, but preserve any potential parentheses in between
                if negation_depth > 0 {
                    let negations = &statements_analyzer.file_analyzer.file_source.file_contents
                        [negation_start_offset..pos.start_offset()];
                    analysis_data.add_replacement(
                        (negation_start_offset as u32, pos.start_offset() as u32),
                        Replacement::Substitute(negations.replace("!", "")),
                    );
                }
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
