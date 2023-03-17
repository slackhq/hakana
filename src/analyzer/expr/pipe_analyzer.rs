use std::rc::Rc;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::{EFFECT_IMPURE, EFFECT_PURE};
use hakana_type::get_mixed_any;
use oxidized::{aast, aast_defs, ast_defs::Pos};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&aast_defs::Lid, &aast::Expr<(), ()>, &aast::Expr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    if !expression_analyzer::analyze(
        statements_analyzer,
        expr.1,
        analysis_data,
        context,
        if_body_context,
    ) {
        return false;
    }

    let mut pipe_expr_type = analysis_data
        .get_expr_type(&expr.1 .1)
        .cloned()
        .unwrap_or(get_mixed_any());

    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
        let parent_node = DataFlowNode::get_for_variable_source(
            "$$".to_string(),
            statements_analyzer.get_hpos(&expr.1.pos()),
            false,
        );

        pipe_expr_type.parent_nodes.insert(parent_node.clone());
        analysis_data.data_flow_graph.add_node(parent_node);
    }

    context
        .vars_in_scope
        .insert("$$".to_string(), Rc::new(pipe_expr_type));

    context.pipe_var_effects = *analysis_data
        .expr_effects
        .get(&(expr.1 .1.start_offset(), expr.1 .1.end_offset()))
        .unwrap_or(&EFFECT_PURE);

    let analyzed_ok = expression_analyzer::analyze(
        statements_analyzer,
        expr.2,
        analysis_data,
        context,
        if_body_context,
    );

    context.vars_in_scope.remove(&"$$".to_string());
    context.pipe_var_effects = EFFECT_PURE;

    analysis_data.set_expr_type(
        &pos,
        analysis_data
            .get_expr_type(&expr.2 .1)
            .cloned()
            .unwrap_or(get_mixed_any()),
    );

    analysis_data.expr_effects.insert(
        (pos.start_offset(), pos.end_offset()),
        *analysis_data
            .expr_effects
            .get(&(expr.2 .1.start_offset(), expr.2 .1.end_offset()))
            .unwrap_or(&EFFECT_IMPURE),
    );

    analyzed_ok
}
