use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind};
use hakana_code_info::ttype::get_mixed_any;
use hakana_reflector::typehint_resolver::get_type_from_hint;
use oxidized::aast;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr_pos: &aast::Pos,
    hint: &aast::Hint,
    inner_expr: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    expression_analyzer::analyze(
        statements_analyzer,
        inner_expr,
        analysis_data,
        context,
        true,
    )?;

    let expr_type = analysis_data
        .get_rc_expr_type(inner_expr.pos())
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));

    let mut hint_type = get_type_from_hint(
        &hint.1,
        None,
        statements_analyzer.get_type_resolution_context(),
        &FxHashMap::default(),
        *statements_analyzer.get_file_path(),
        hint.0.start_offset() as u32,
    )
    .unwrap();

    // todo emit issues about redundant casts

    hint_type.parent_nodes.clone_from(&expr_type.parent_nodes);
    if analysis_data.data_flow_graph.kind != GraphKind::FunctionBody
        && !hint_type.parent_nodes.is_empty()
    {
        let mut removed = hint_type.scalar_taint_removals();
        removed.extend(expr_type.scalar_taint_removals());
        if !removed.is_empty() {
            let node = DataFlowNode::get_for_composition(statements_analyzer.get_hpos(expr_pos));
            for parent in &expr_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &parent.id,
                    &node.id,
                    PathKind::Default,
                    vec![],
                    removed.clone(),
                );
            }
            analysis_data.data_flow_graph.add_node(node.clone());
            hint_type.parent_nodes = vec![node];
        }
    }

    analysis_data.set_expr_type(expr_pos, hint_type);

    analysis_data.expr_effects.insert(
        (expr_pos.start_offset() as u32, expr_pos.end_offset() as u32),
        *analysis_data
            .expr_effects
            .get(&(
                inner_expr.pos().start_offset() as u32,
                inner_expr.pos().end_offset() as u32,
            ))
            .unwrap_or(&0),
    );

    Ok(())
}
