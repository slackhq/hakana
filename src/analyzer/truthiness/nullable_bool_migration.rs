use hakana_code_info::issue::IssueKind;
use hakana_code_info::{analysis_result::Replacement, t_atomic::TAtomic};

use hakana_code_info::t_union::TUnion;
use oxidized::ast::Bop;

use super::implicit_boolean_conversion_migration::{
    ImplicitBooleanConversionMigration, MigrationArgs,
};

pub(super) struct NullableBoolMigration {}

impl ImplicitBooleanConversionMigration for NullableBoolMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        expr_type.is_nullable()
            && expr_type.types.iter().all(|atomic| {
                matches!(
                    atomic,
                    TAtomic::TNull | TAtomic::TBool | TAtomic::TFalse | TAtomic::TTrue
                )
            })
    }

    fn migrate(&self, args: MigrationArgs) {
        self.migrate_shared(false, args);
    }

    fn migrate_negated(&self, args: MigrationArgs) {
        self.migrate_shared(true, args);
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolCondition
    }
}

impl NullableBoolMigration {
    fn migrate_shared(&self, negated: bool, args: MigrationArgs) {
        let MigrationArgs {
            statements_analyzer,
            analysis_data,
            expr,
            pos,
            ..
        } = args;
        if let Some(bin_op) = expr.2.as_binop()
            && matches!(bin_op.bop, Bop::QuestionQuestion)
            && bin_op.rhs.2.is_null()
        {
            analysis_data.add_replacement(
                (
                    bin_op.rhs.pos().start_offset() as u32,
                    bin_op.rhs.pos().end_offset() as u32,
                ),
                Replacement::Substitute("false".to_string()),
            );
            if negated {
                analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
                analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
                analysis_data.insert_at(pos.start_offset() as u32, "!".to_string());
            }
            return;
        }

        let needs_parentheses = negated
            || !super::is_sole_condition(statements_analyzer, pos)
            || expr
                .2
                .as_binop()
                .is_some_and(|binop| matches!(binop.bop, Bop::QuestionQuestion));
        if needs_parentheses {
            analysis_data.insert_at(
                pos.start_offset() as u32,
                if negated { "!(" } else { "(" }.to_string(),
            );
        }
        analysis_data.insert_at(
            pos.end_offset() as u32,
            format!(" ?? false{}", if needs_parentheses { ")" } else { "" }),
        );
    }
}
