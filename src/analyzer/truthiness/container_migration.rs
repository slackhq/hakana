use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::{TAtomic, TDict, TNamedObject, TVec};
use hakana_code_info::t_union::TUnion;
use hakana_str::StrId;
use oxidized::aast;
use oxidized::ast::Bop;
use oxidized::pos::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

use super::implicit_boolean_conversion_migration::ImplicitBooleanConversionMigration;

pub(super) struct ContainerMigration {
    pub(super) handle_nullable: bool,
}

impl ImplicitBooleanConversionMigration for ContainerMigration {
    fn matches(&self, expr_type: &TUnion) -> bool {
        if self.handle_nullable && !expr_type.is_nullable() {
            return false;
        }
        expr_type
            .types
            .iter()
            .filter(|t| !self.handle_nullable || !matches!(t, TAtomic::TNull))
            .all(|t| {
                !matches!(
                    t,
                    TAtomic::TDict(TDict {
                        known_items: Some(..),
                        ..
                    }) | TAtomic::TVec(TVec {
                        known_items: Some(..),
                        ..
                    })
                ) && (t.is_array_accessible_with_int_or_string_key()
                    || matches!(
                        t,
                        TAtomic::TNamedObject(TNamedObject {
                            name: StrId::MAP
                                | StrId::IMM_MAP
                                | StrId::SET
                                | StrId::IMM_SET
                                | StrId::VECTOR
                                | StrId::IMM_VECTOR,
                            ..
                        })
                    ))
            })
    }

    fn migrate(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        self.migrate_shared(false, statements_analyzer, analysis_data, expr, pos);
    }

    fn migrate_negated(
        &self,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        self.migrate_shared(true, statements_analyzer, analysis_data, expr, pos);
    }

    fn kind(&self) -> IssueKind {
        IssueKind::NonBoolContainerCondition
    }
}

impl ContainerMigration {
    fn migrate_shared(
        &self,
        negated: bool,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
        pos: &Pos,
    ) {
        if !self.handle_nullable {
            analysis_data.insert_at(
                pos.start_offset() as u32,
                format!("{}C\\is_empty(", if negated { "" } else { "!" }),
            );
            analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
            return;
        }

        if expr.2.is_call() {
            analysis_data.insert_at(
                pos.start_offset() as u32,
                format!("{}C\\is_empty(", if negated { "" } else { "!" }),
            );
            analysis_data.insert_at(pos.end_offset() as u32, " ?? vec[])".to_string());
            return;
        }

        if let Some(bin_op) = expr.2.as_binop()
            && matches!(bin_op.bop, Bop::QuestionQuestion)
            && bin_op.rhs.2.is_null()
        {
            let coalesce_rhs_start = bin_op.rhs.pos().start_offset() as u32;
            let coalesce_rhs_end = bin_op.rhs.pos().end_offset() as u32;
            analysis_data.insert_at(
                pos.start_offset() as u32,
                format!("{}C\\is_empty(", if negated { "" } else { "!" }),
            );
            analysis_data.add_replacement(
                (coalesce_rhs_start, coalesce_rhs_end),
                Replacement::Substitute("vec[])".to_string()),
            );
            return;
        }

        if super::is_trivial(expr) {
            let expr_text = &statements_analyzer.file_analyzer.file_source.file_contents
                [pos.start_offset()..pos.end_offset()];
            let close_paren = if !super::is_sole_condition(statements_analyzer, pos) {
                analysis_data.insert_at(pos.start_offset() as u32, "(".to_string());
                ")"
            } else {
                ""
            };

            if negated {
                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(" is null || C\\is_empty({expr_text}){close_paren}"),
                );
            } else {
                analysis_data.insert_at(
                    pos.end_offset() as u32,
                    format!(" is nonnull && !C\\is_empty({expr_text}){close_paren}"),
                );
            }
            return;
        }

        analysis_data.insert_at(
            pos.start_offset() as u32,
            format!("{}\\HH\\legacy_is_truthy(", if negated { "!" } else { "" }),
        );
        analysis_data.insert_at(pos.end_offset() as u32, ")".to_string());
    }
}
