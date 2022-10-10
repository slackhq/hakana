use crate::expr::expression_identifier;
use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::get_mixed_any;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::atomic_method_call_analyzer::{self, AtomicMethodCallAnalysisResult};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::Expr<(), ()>,
        &aast::Expr<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    nullsafe: bool,
) -> bool {
    let was_inside_general_use = context.inside_general_use;

    context.inside_general_use = true;

    if !expression_analyzer::analyze(
        statements_analyzer,
        expr.0,
        tast_info,
        context,
        if_body_context,
    ) {
        context.inside_general_use = was_inside_general_use;
        return false;
    }

    if let aast::Expr_::Id(_) = &expr.1 .2 {
        // do nothing
    } else {
        if !expression_analyzer::analyze(
            statements_analyzer,
            expr.1,
            tast_info,
            context,
            if_body_context,
        ) {
            context.inside_general_use = was_inside_general_use;
            return false;
        }
    }

    let lhs_var_id = expression_identifier::get_var_id(
        &expr.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some(statements_analyzer.get_codebase()),
    );

    let class_type = tast_info
        .get_expr_type(expr.0.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    let mut analysis_result = AtomicMethodCallAnalysisResult::new();

    if class_type.is_null() || class_type.is_void() {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::MethodCallOnNull,
                "Cannot call method on null value".to_string(),
                statements_analyzer.get_hpos(&expr.1.pos()),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual()
        );
    } else {
        if class_type.is_nullable() && !nullsafe {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::PossibleMethodCallOnNull,
                    "Cannot call method on null value".to_string(),
                    statements_analyzer.get_hpos(&expr.1.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual()
            )
        }

        if class_type.is_mixed() {
            for (_, origin) in &class_type.parent_nodes {
                tast_info.data_flow_graph.add_mixed_data(origin, pos);
            }
        }

        for lhs_atomic_type in &class_type.types {
            if let TAtomic::TNull = lhs_atomic_type {
                continue; // handled above
            }

            if let TAtomic::TFalse = lhs_atomic_type {
                if class_type.ignore_falsable_issues {
                    continue;
                }
            }

            atomic_method_call_analyzer::analyze(
                statements_analyzer,
                expr,
                pos,
                tast_info,
                context,
                if_body_context,
                lhs_atomic_type,
                &lhs_var_id,
                &mut analysis_result,
            );
        }
    }

    if tast_info
        .expr_effects
        .get(&(pos.start_offset(), pos.end_offset()))
        .unwrap_or(&0)
        >= &crate::typed_ast::WRITE_PROPS
    {
        context.remove_mutable_object_vars();
    }

    if let Some(mut stmt_type) = analysis_result.return_type {
        if nullsafe && !stmt_type.is_mixed() {
            stmt_type.add_type(TAtomic::TNull);
        }
        if stmt_type.is_nothing() && !context.inside_loop {
            context.has_returned = true;
        }
        tast_info.set_expr_type(&pos, stmt_type);
    }

    true
}
