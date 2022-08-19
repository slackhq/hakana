use crate::expression_analyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use oxidized::aast;
use oxidized::pos::Pos;

pub(crate) fn analyze(
    pos: &Pos,
    field: &aast::Afield<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) {
    match &field {
        aast::Afield::AFkvalue(key_expr, _) => {
            expression_analyzer::analyze(
                statements_analyzer,
                key_expr,
                tast_info,
                context,
                if_body_context,
            );
        }
        _ => {}
    };

    let value_expr = match &field {
        aast::Afield::AFvalue(value_expr) | aast::Afield::AFkvalue(_, value_expr) => value_expr,
    };

    expression_analyzer::analyze(
        statements_analyzer,
        value_expr,
        tast_info,
        context,
        if_body_context,
    );

    if let Some(inferred_type) = tast_info.expr_types.get(&(
        value_expr.pos().start_offset(),
        value_expr.pos().end_offset(),
    )) {
        if let GraphKind::FunctionBody = tast_info.data_flow_graph.kind {
            let return_node = DataFlowNode::get_for_variable_sink(
                "yield".to_string(),
                statements_analyzer.get_hpos(pos),
            );

            for (_, parent_node) in &inferred_type.parent_nodes {
                tast_info.data_flow_graph.add_path(
                    &parent_node,
                    &return_node,
                    PathKind::Default,
                    None,
                    None,
                );
            }
            tast_info.data_flow_graph.add_node(return_node);
        } else {
            // todo handle taint flows in yield
        }
    }
}
