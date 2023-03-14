use std::rc::Rc;

use crate::expression_analyzer;
use crate::typed_ast::TastInfo;
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    if !expression_analyzer::analyze(
        statements_analyzer,
        expr.1,
        tast_info,
        context,
        if_body_context,
    ) {
        return false;
    }

    let mut pipe_expr_type = tast_info
        .get_expr_type(&expr.1 .1)
        .cloned()
        .unwrap_or(get_mixed_any());

    if tast_info.data_flow_graph.kind == GraphKind::FunctionBody {
        let parent_node = DataFlowNode::get_for_variable_source(
            "$$".to_string(),
            statements_analyzer.get_hpos(&expr.1.pos()),
            false,
        );

        pipe_expr_type.parent_nodes.insert(parent_node.clone());
        tast_info.data_flow_graph.add_node(parent_node);
    }

    context
        .vars_in_scope
        .insert("$$".to_string(), Rc::new(pipe_expr_type));

    context.pipe_var_effects = *tast_info
        .expr_effects
        .get(&(expr.1 .1.start_offset(), expr.1 .1.end_offset()))
        .unwrap_or(&EFFECT_PURE);

    let analyzed_ok = expression_analyzer::analyze(
        statements_analyzer,
        expr.2,
        tast_info,
        context,
        if_body_context,
    );

    context.vars_in_scope.remove(&"$$".to_string());
    context.pipe_var_effects = EFFECT_PURE;

    tast_info.set_expr_type(
        &pos,
        tast_info
            .get_expr_type(&expr.2 .1)
            .cloned()
            .unwrap_or(get_mixed_any()),
    );

    tast_info.expr_effects.insert(
        (pos.start_offset(), pos.end_offset()),
        *tast_info
            .expr_effects
            .get(&(expr.2 .1.start_offset(), expr.2 .1.end_offset()))
            .unwrap_or(&EFFECT_IMPURE),
    );

    analyzed_ok
}
