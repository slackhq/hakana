use crate::expression_analyzer;
use crate::typed_ast::TastInfo;
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::{
    data_flow::{
        graph::GraphKind,
        node::DataFlowNode,
        path::{PathExpressionKind, PathKind},
    },
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_type::{get_mixed_any, get_nothing, wrap_atomic};
use oxidized::{aast, ast_defs::Pos};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    tuple_fields: &Vec<aast::Expr<(), ()>>,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let mut parent_nodes = FxHashMap::default();

    let mut known_items = BTreeMap::new();
    for (i, value_expr) in tuple_fields.iter().enumerate() {
        // Now check types of the values
        if !expression_analyzer::analyze(
            statements_analyzer,
            value_expr,
            tast_info,
            context,
            &mut None,
        ) {
            return false;
        }

        let value_item_type = tast_info
            .get_expr_type(&value_expr.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        if let Some(new_parent_node) = add_tuple_value_dataflow(
            statements_analyzer,
            &value_item_type,
            tast_info,
            i,
            value_expr,
        ) {
            parent_nodes.insert(new_parent_node.get_id().clone(), new_parent_node);
        }

        known_items.insert(i, (false, value_item_type));
    }

    let mut new_dict = wrap_atomic(TAtomic::TVec {
        known_count: Some(known_items.len()),
        known_items: if known_items.len() > 0 {
            Some(known_items)
        } else {
            None
        },
        type_param: get_nothing(),
        non_empty: true,
    });

    new_dict.parent_nodes = parent_nodes;

    tast_info.set_expr_type(&pos, new_dict);

    true
}

fn add_tuple_value_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    value_type: &TUnion,
    tast_info: &mut TastInfo,
    key_value: usize,
    value: &aast::Expr<(), ()>,
) -> Option<DataFlowNode> {
    if value_type.parent_nodes.is_empty()
        || (tast_info.data_flow_graph.kind == GraphKind::Taint && value_type.has_taintable_value())
    {
        return None;
    }

    let node_name = format!("array[{}]", key_value);

    let new_parent_node =
        DataFlowNode::get_for_assignment(node_name, statements_analyzer.get_hpos(value.pos()));
    tast_info.data_flow_graph.add_node(new_parent_node.clone());

    // TODO add taint event dispatches

    for (_, parent_node) in value_type.parent_nodes.iter() {
        tast_info.data_flow_graph.add_path(
            parent_node,
            &new_parent_node,
            PathKind::ExpressionAssignment(PathExpressionKind::ArrayValue, key_value.to_string()),
            None,
            None,
        );
    }

    return Some(new_parent_node);
}
