use crate::{
    expression_analyzer, scope_analyzer::ScopeAnalyzer, scope_context::ScopeContext,
    statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo,
};
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
use oxidized::{
    aast,
    ast_defs::{Pos, ShapeFieldName},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    shape_fields: &Vec<(ShapeFieldName, aast::Expr<(), ()>)>,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let codebase = statements_analyzer.get_codebase();

    let mut parent_nodes = HashMap::new();

    let mut all_pure = true;

    let mut known_items = BTreeMap::new();
    for (name, value_expr) in shape_fields {
        let name = if let ShapeFieldName::SFlitStr((_, name)) = name {
            Some(name.to_string())
        } else if let ShapeFieldName::SFclassConst(lhs, name) = name {
            let mut lhs_name = &lhs.1;
            if let Some(resolved_name) = statements_analyzer
                .get_file_analyzer()
                .resolved_names
                .get(&lhs.0.start_offset())
            {
                lhs_name = resolved_name;
            }
            let constant_type =
                codebase.get_class_constant_type(&lhs_name, &name.1, HashSet::new());

            if let Some(constant_type) = constant_type {
                if let Some(name) = constant_type.get_single_literal_string_value() {
                    Some(name)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

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

        if !tast_info.pure_exprs.contains(&(
            value_expr.pos().start_offset(),
            value_expr.pos().end_offset(),
        )) {
            all_pure = false;
        }

        if let Some(name) = name {
            let value_item_type = tast_info
                .get_expr_type(&value_expr.pos())
                .cloned()
                .unwrap_or(get_mixed_any());

            if let Some(new_parent_node) = add_shape_value_dataflow(
                statements_analyzer,
                &value_item_type,
                tast_info,
                &name.to_string(),
                value_expr,
            ) {
                parent_nodes.insert(new_parent_node.id.clone(), new_parent_node);
            }

            known_items.insert(name.to_string(), (false, Arc::new(value_item_type)));
        }
    }

    if all_pure {
        tast_info
            .pure_exprs
            .insert((pos.start_offset(), pos.end_offset()));
    }

    let mut new_dict = wrap_atomic(TAtomic::TDict {
        known_items: if known_items.len() > 0 {
            Some(known_items)
        } else {
            None
        },
        enum_items: None,
        key_param: get_nothing(),
        value_param: get_nothing(),
        non_empty: true,
        shape_name: None,
    });

    new_dict.parent_nodes = parent_nodes;

    tast_info.set_expr_type(&pos, new_dict);

    true
}

fn add_shape_value_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    value_type: &TUnion,
    tast_info: &mut TastInfo,
    key_value: &String,
    value: &aast::Expr<(), ()>,
) -> Option<DataFlowNode> {
    if value_type.parent_nodes.is_empty()
        || (tast_info.data_flow_graph.kind == GraphKind::Taint && !value_type.has_taintable_value())
    {
        return None;
    }

    let node_name = format!("array[{}]", key_value);

    let new_parent_node = DataFlowNode::get_for_assignment(
        node_name,
        statements_analyzer.get_hpos(value.pos()),
        None,
    );
    tast_info.data_flow_graph.add_node(new_parent_node.clone());

    // TODO add taint event dispatches

    for (_, parent_node) in value_type.parent_nodes.iter() {
        tast_info.data_flow_graph.add_path(
            parent_node,
            &new_parent_node,
            PathKind::ExpressionAssignment(PathExpressionKind::ArrayValue, key_value.clone()),
            HashSet::new(),
            HashSet::new(),
        );
    }

    return Some(new_parent_node);
}
