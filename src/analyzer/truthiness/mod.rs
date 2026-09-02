use std::sync::LazyLock;

use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::issue::Issue;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use oxidized::pos::Pos;
use oxidized::{aast, ast::Bop};

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::truthiness::container_migration::ContainerMigration;
use crate::truthiness::mixed_migration::MixedMigration;

mod assorted_truthiness_migration;
mod container_migration;
mod implicit_boolean_conversion_migration;
mod int_truthiness_migration;
mod mixed_migration;
mod nullable_bool_migration;
mod string_truthiness_migration;

use assorted_truthiness_migration::AssortedTruthinessMigration;
use implicit_boolean_conversion_migration::{ImplicitBooleanConversionMigration, MigrationArgs};
use int_truthiness_migration::IntTruthinessMigration;
use nullable_bool_migration::NullableBoolMigration;
use string_truthiness_migration::StringTruthinessMigration;

/// Is this a trivial expression that should be fine to repeat?
fn is_trivial(expr: &aast::Expr<(), ()>) -> bool {
    match &expr.2 {
        aast::Expr_::Binop(op) if matches!(op.bop, Bop::QuestionQuestion) => {
            is_trivial(&op.lhs) && is_trivial(&op.rhs)
        }
        aast::Expr_::Lvar(..)
        | aast::Expr_::True
        | aast::Expr_::False
        | aast::Expr_::Null
        | aast::Expr_::Int(..)
        | aast::Expr_::String(..)
        | aast::Expr_::Float(..)
        | aast::Expr_::ArrayGet(..)
        | aast::Expr_::ClassGet(..)
        | aast::Expr_::ObjGet(..) => true,
        _ => false,
    }
}

fn aliased_types(expr_type: &TUnion) -> Box<dyn Iterator<Item = &TAtomic> + '_> {
    let it = expr_type
        .types
        .iter()
        .enumerate()
        .flat_map(|(i, t)| match t {
            TAtomic::TTypeAlias {
                as_type: Some(as_type),
                ..
            }
            | TAtomic::TGenericParam(hakana_code_info::t_atomic::TGenericParam {
                as_type, ..
            })
            | TAtomic::TClassTypeConstant { as_type, .. } => aliased_types(as_type),
            TAtomic::TEnum {
                as_type,
                underlying_type,
                ..
            }
            | TAtomic::TEnumLiteralCase {
                as_type,
                underlying_type,
                ..
            } => {
                if let Some(as_type) = as_type.as_ref().or(underlying_type.as_ref()) {
                    Box::new(std::iter::once(as_type.as_ref()))
                        as Box<dyn Iterator<Item = &TAtomic>>
                } else {
                    Box::new(expr_type.types[i..i + 1].iter()) as Box<dyn Iterator<Item = &TAtomic>>
                }
            }
            _ => Box::new(expr_type.types[i..i + 1].iter()),
        })
        .filter(|t| {
            !matches!(
                t,
                TAtomic::TNull | TAtomic::TBool | TAtomic::TFalse | TAtomic::TTrue
            )
        });

    Box::new(it)
}

/// Is this expression the sole condition in an `if` statement?
fn is_sole_condition(statements_analyzer: &StatementsAnalyzer, pos: &Pos) -> bool {
    let file_contents = &statements_analyzer.file_analyzer.file_source.file_contents;
    (file_contents[..pos.start_offset()].ends_with("if (")
        || file_contents[..pos.start_offset()].ends_with("if (!"))
        && file_contents[pos.end_offset()..].starts_with(")")
}

fn expand_trivial_expr_conds(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    expr: &aast::Expr<(), ()>,
    pos: &Pos,
    op: &str,
    conds: &[&str],
) {
    let is_nullcoalesce = expr
        .2
        .as_binop()
        .is_some_and(|binop| matches!(binop.bop, Bop::QuestionQuestion));

    let expr_text = {
        let expr_text = &statements_analyzer.file_analyzer.file_source.file_contents
            [pos.start_offset()..pos.end_offset()];

        if is_nullcoalesce {
            analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
            format!("({})", expr_text)
        } else {
            expr_text.to_string()
        }
    };

    let close_paren = if !is_sole_condition(statements_analyzer, pos) && conds.len() > 1 {
        analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
        ")"
    } else {
        ""
    };

    analysis_data.insert_at(
        pos.end_offset() as u32,
        format!(
            "{} {}{close_paren}",
            if is_nullcoalesce { ")" } else { "" },
            conds.join(&format!(" {op} {expr_text} "))
        ),
    );
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

    let Some(expr_type) = analysis_data.get_rc_expr_type(expr.pos()).cloned() else {
        return;
    };

    let is_negated = negation_depth % 2 == 1;

    if !expr_type.is_bool() {
        static TRUTHINESS_MIGRATIONS: LazyLock<Vec<Box<dyn ImplicitBooleanConversionMigration>>> =
            LazyLock::new(|| {
                vec![
                    Box::new(NullableBoolMigration {}),
                    Box::new(AssortedTruthinessMigration {}),
                    Box::new(IntTruthinessMigration {}),
                    Box::new(StringTruthinessMigration {}),
                    Box::new(ContainerMigration {}),
                    Box::new(MixedMigration {}),
                ]
            });

        if !analysis_data
            .insertions
            .contains_key(&(pos.start_offset() as u32))
            && !analysis_data
                .insertions
                .contains_key(&(pos.end_offset() as u32))
            && let Some(migration) = TRUTHINESS_MIGRATIONS.iter().find(|m| m.matches(&expr_type))
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
                    migration.migrate_negated(MigrationArgs {
                        statements_analyzer,
                        analysis_data,
                        expr,
                        pos,
                        expr_type,
                    });
                } else {
                    migration.migrate(MigrationArgs {
                        statements_analyzer,
                        analysis_data,
                        expr,
                        pos,
                        expr_type,
                    });
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
