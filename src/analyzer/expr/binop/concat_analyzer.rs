use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use hakana_reflection_info::{
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    t_atomic::TAtomic,
    taint::SinkType,
};
use hakana_type::{get_string, wrap_atomic};
use oxidized::aast;
use rustc_hash::FxHashSet;

pub(crate) fn analyze<'expr, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    tast_info: &'tast mut TastInfo,
    context: &mut ScopeContext,
) {
    let mut concat_nodes = get_concat_nodes(left);
    concat_nodes.push(right);

    let mut all_literals = true;

    let decision_node = if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
        DataFlowNode::get_for_composition(statements_analyzer.get_hpos(stmt_pos))
    } else {
        DataFlowNode::get_for_variable_sink(
            "composition".to_string(),
            statements_analyzer.get_hpos(stmt_pos),
        )
    };

    let mut has_slash = false;
    let mut has_query = false;

    for (i, concat_node) in concat_nodes.iter().enumerate() {
        expression_analyzer::analyze(
            statements_analyzer,
            concat_node,
            tast_info,
            context,
            &mut None,
        );

        let expr_type = tast_info.expr_types.get(&(
            concat_node.pos().start_offset(),
            concat_node.pos().end_offset(),
        ));

        if let Some(expr_type) = expr_type {
            all_literals = all_literals && expr_type.all_literals();

            if let Some(str) = expr_type
                .get_single_literal_string_value(&statements_analyzer.get_codebase().interner)
            {
                if str.contains("/") {
                    has_slash = true;
                }
                if str.contains("?") {
                    has_query = true;
                }
            }

            for (_, old_parent_node) in &expr_type.parent_nodes {
                tast_info.data_flow_graph.add_path(
                    old_parent_node,
                    &decision_node,
                    PathKind::Default,
                    None,
                    if i > 0 && (has_slash || has_query) {
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
        } else {
            all_literals = false;
        }
    }

    let mut result_type = if all_literals {
        wrap_atomic(TAtomic::TStringWithFlags(true, false, true))
    } else {
        get_string()
    };

    result_type
        .parent_nodes
        .insert(decision_node.get_id().clone(), decision_node.clone());

    // todo handle more string type combinations

    tast_info.data_flow_graph.add_node(decision_node.clone());

    tast_info.set_expr_type(&stmt_pos, result_type);
}

fn get_concat_nodes(expr: &aast::Expr<(), ()>) -> Vec<&aast::Expr<(), ()>> {
    match &expr.2 {
        aast::Expr_::Binop(x) => {
            let (binop, e1, e2) = (&x.0, &x.1, &x.2);
            match binop {
                oxidized::ast_defs::Bop::Dot => {
                    let mut concat_nodes = get_concat_nodes(e1);
                    concat_nodes.push(e2);
                    concat_nodes
                }
                _ => vec![expr],
            }
        }
        _ => vec![expr],
    }
}
