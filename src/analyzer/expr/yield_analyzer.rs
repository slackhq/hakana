use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use oxidized::aast;
use oxidized::pos::Pos;

pub(crate) fn analyze(
    pos: &Pos,
    field: &aast::Afield<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> Result<(), AnalysisError> {
    if let aast::Afield::AFkvalue(key_expr, _) = &field {
        expression_analyzer::analyze(
            statements_analyzer,
            key_expr,
            analysis_data,
            context,
            if_body_context,
        )?;
    };

    let value_expr = match &field {
        aast::Afield::AFvalue(value_expr) | aast::Afield::AFkvalue(_, value_expr) => value_expr,
    };

    expression_analyzer::analyze(
        statements_analyzer,
        value_expr,
        analysis_data,
        context,
        if_body_context,
    )?;

    if let Some(inferred_type) = analysis_data.expr_types.get(&(
        value_expr.pos().start_offset() as u32,
        value_expr.pos().end_offset() as u32,
    )) {
        if let GraphKind::FunctionBody = analysis_data.data_flow_graph.kind {
            let return_node = DataFlowNode::get_for_variable_sink(
                "yield".to_string(),
                statements_analyzer.get_hpos(pos),
            );

            for parent_node in &inferred_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    parent_node,
                    &return_node,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
            analysis_data.data_flow_graph.add_node(return_node);
        } else {
            // todo handle taint flows in yield
        }
    }

    Ok(())
}
