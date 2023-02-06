use std::rc::Rc;

use hakana_reflection_info::issue::{Issue, IssueKind};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use crate::{expr::expression_identifier, expression_analyzer};
use hakana_reflection_info::{
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    t_atomic::TAtomic,
    t_union::TUnion,
    taint::SinkType,
};
use hakana_type::{get_mixed_any, type_combiner};
use oxidized::{aast, ast, ast_defs::Pos};

pub(crate) fn analyze<'expr: 'tast, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    operator: &'expr ast::Bop,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    tast_info: &'tast mut TastInfo,
    context: &mut ScopeContext,
) {
    expression_analyzer::analyze(statements_analyzer, left, tast_info, context, &mut None);
    expression_analyzer::analyze(statements_analyzer, right, tast_info, context, &mut None);

    let fallback = get_mixed_any();
    let e1_type = match tast_info.get_rc_expr_type(&left.1).cloned() {
        Some(var_type) => var_type,
        None => Rc::new(fallback.clone()),
    };

    if e1_type.is_mixed() {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::MixedOperand,
                "Operand has a mixed type".to_string(),
                statements_analyzer.get_hpos(&left.1),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let e1_var_id = if context.inside_loop {
        expression_identifier::get_var_id(
            left,
            None,
            statements_analyzer.get_file_analyzer().get_file_source(),
            &FxHashMap::default(),
            None,
        )
    } else {
        None
    };

    let e2_type = match tast_info.get_rc_expr_type(&right.1).cloned() {
        Some(var_type) => var_type,
        None => Rc::new(fallback),
    };

    if e2_type.is_mixed() {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::MixedOperand,
                "Operand has a mixed type".to_string(),
                statements_analyzer.get_hpos(&right.1),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let e2_var_id = if context.inside_loop {
        expression_identifier::get_var_id(
            right,
            None,
            statements_analyzer.get_file_analyzer().get_file_source(),
            &FxHashMap::default(),
            None,
        )
    } else {
        None
    };

    let has_loop_variable = e1_var_id.is_some() || e2_var_id.is_some();

    let zero = TAtomic::TLiteralInt { value: 0 };

    let mut results = vec![];

    for mut e1_type_atomic in &e1_type.types {
        if let TAtomic::TFalse = e1_type_atomic {
            if e1_type.ignore_falsable_issues {
                continue;
            }
            e1_type_atomic = &zero;
        }
        for mut e2_type_atomic in &e2_type.types {
            if let TAtomic::TFalse = e2_type_atomic {
                if e2_type.ignore_falsable_issues {
                    continue;
                }
                e2_type_atomic = &zero;
            }

            results.push(if has_loop_variable {
                match (e1_type_atomic, e2_type_atomic) {
                    (
                        TAtomic::TInt | TAtomic::TLiteralInt { .. },
                        TAtomic::TInt | TAtomic::TLiteralInt { .. },
                    ) => match operator {
                        oxidized::ast_defs::Bop::Slash => TAtomic::TNum,
                        _ => TAtomic::TInt,
                    },
                    _ => TAtomic::TFloat,
                }
            } else {
                match (e1_type_atomic, e2_type_atomic) {
                    (
                        TAtomic::TLiteralInt { value: e1_value },
                        TAtomic::TLiteralInt { value: e2_value },
                    ) => match operator {
                        oxidized::ast_defs::Bop::Plus => TAtomic::TLiteralInt {
                            value: e1_value + e2_value,
                        },
                        oxidized::ast_defs::Bop::Minus => TAtomic::TLiteralInt {
                            value: e1_value - e2_value,
                        },
                        oxidized::ast_defs::Bop::Amp => TAtomic::TLiteralInt {
                            value: e1_value & e2_value,
                        },
                        oxidized::ast_defs::Bop::Bar => TAtomic::TLiteralInt {
                            value: e1_value | e2_value,
                        },
                        oxidized::ast_defs::Bop::Ltlt => TAtomic::TLiteralInt {
                            value: e1_value << e2_value,
                        },
                        oxidized::ast_defs::Bop::Gtgt => TAtomic::TLiteralInt {
                            value: e1_value >> e2_value,
                        },
                        oxidized::ast_defs::Bop::Percent => TAtomic::TLiteralInt {
                            value: e1_value % e2_value,
                        },
                        oxidized::ast_defs::Bop::Slash => TAtomic::TNum,
                        _ => TAtomic::TInt,
                    },
                    (
                        TAtomic::TInt | TAtomic::TLiteralInt { .. },
                        TAtomic::TInt | TAtomic::TLiteralInt { .. },
                    ) => match operator {
                        oxidized::ast_defs::Bop::Slash => TAtomic::TNum,
                        _ => TAtomic::TInt,
                    },
                    _ => TAtomic::TFloat,
                }
            });
        }
    }

    let result_type = TUnion::new(if results.len() == 1 {
        results
    } else {
        type_combiner::combine(results, statements_analyzer.get_codebase(), false)
    });

    assign_arithmetic_type(
        statements_analyzer,
        tast_info,
        result_type,
        left,
        right,
        stmt_pos,
    );
}

pub(crate) fn assign_arithmetic_type(
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    cond_type: TUnion,
    lhs_expr: &aast::Expr<(), ()>,
    rhs_expr: &aast::Expr<(), ()>,
    expr_pos: &Pos,
) {
    let mut cond_type = cond_type;
    let decision_node = if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
        DataFlowNode::get_for_composition(statements_analyzer.get_hpos(expr_pos))
    } else {
        DataFlowNode::get_for_variable_sink(
            "composition".to_string(),
            statements_analyzer.get_hpos(expr_pos),
        )
    };

    tast_info.data_flow_graph.add_node(decision_node.clone());

    if let Some(lhs_type) = tast_info
        .expr_types
        .get(&(lhs_expr.1.start_offset(), lhs_expr.1.end_offset()))
    {
        cond_type.parent_nodes.insert(decision_node.clone());

        for old_parent_node in &lhs_type.parent_nodes {
            tast_info.data_flow_graph.add_path(
                old_parent_node,
                &decision_node,
                PathKind::Default,
                None,
                None,
            );
        }
    }

    if let Some(rhs_type) = tast_info
        .expr_types
        .get(&(rhs_expr.1.start_offset(), rhs_expr.1.end_offset()))
    {
        cond_type.parent_nodes.insert(decision_node.clone());

        for old_parent_node in &rhs_type.parent_nodes {
            tast_info.data_flow_graph.add_path(
                old_parent_node,
                &decision_node,
                PathKind::Default,
                None,
                if cond_type.has_string() {
                    Some(FxHashSet::from_iter([
                        SinkType::HtmlAttributeUri,
                        SinkType::CurlUri,
                        SinkType::RedirectUri,
                    ]))
                } else {
                    None
                },
            );
        }
    }

    tast_info.set_expr_type(&expr_pos, cond_type);
}
