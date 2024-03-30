use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::{expression_analyzer, stmt_analyzer::AnalysisError};
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::{
    data_flow::{node::DataFlowNode, path::PathKind},
    t_atomic::TAtomic,
    taint::SinkType,
};
use hakana_type::{get_literal_string, get_string, wrap_atomic};
use oxidized::aast;

pub(crate) fn analyze<'expr>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    let mut concat_nodes = get_concat_nodes(left);
    concat_nodes.push(right);

    for concat_node in &concat_nodes {
        expression_analyzer::analyze(
            statements_analyzer,
            concat_node,
            analysis_data,
            context,
            &mut None,
        )?;
    }

    let result_type =
        analyze_concat_nodes(concat_nodes, statements_analyzer, analysis_data, stmt_pos);

    // todo handle more string type combinations

    analysis_data.set_expr_type(stmt_pos, result_type);

    Ok(())
}

pub(crate) fn analyze_concat_nodes(
    concat_nodes: Vec<&aast::Expr<(), ()>>,
    statements_analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    stmt_pos: &aast::Pos,
) -> TUnion {
    let mut all_literals = true;

    let decision_node = DataFlowNode::get_for_composition(statements_analyzer.get_hpos(stmt_pos));

    let mut has_slash = false;
    let mut has_query = false;

    let mut string_content = Some("".to_string());

    for (i, concat_node) in concat_nodes.iter().enumerate() {
        if let aast::Expr_::String(simple_string) = &concat_node.2 {
            if simple_string == "" {
                continue;
            }

            if let Some(ref mut string_content) = string_content {
                *string_content += &simple_string.to_string();
            }

            if simple_string.contains(&b'/') {
                has_slash = true;
            }
            if simple_string.contains(&b'?') {
                has_query = true;
            }
        } else {
            let expr_type = analysis_data.expr_types.get(&(
                concat_node.pos().start_offset() as u32,
                concat_node.pos().end_offset() as u32,
            ));

            if let Some(expr_type) = expr_type {
                all_literals = all_literals && expr_type.all_literals();

                if let Some(str) = expr_type.get_single_literal_string_value() {
                    if str.contains('/') {
                        has_slash = true;
                    }
                    if str.contains('?') {
                        has_query = true;
                    }

                    if let Some(ref mut string_content) = string_content {
                        *string_content += &str;
                    }
                } else {
                    string_content = None;
                }

                for old_parent_node in &expr_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        old_parent_node,
                        &decision_node,
                        PathKind::Default,
                        vec![],
                        if i > 0 && (has_slash || has_query) {
                            vec![
                                SinkType::HtmlAttributeUri,
                                SinkType::CurlUri,
                                SinkType::RedirectUri,
                            ]
                        } else {
                            vec![]
                        },
                    );
                }
            } else {
                all_literals = false;
                string_content = None;
            }
        }
    }

    let mut result_type = if all_literals {
        if let Some(string_content) = string_content {
            get_literal_string(string_content)
        } else {
            wrap_atomic(TAtomic::TStringWithFlags(true, false, true))
        }
    } else {
        get_string()
    };

    result_type.parent_nodes.push(decision_node.clone());

    analysis_data.data_flow_graph.add_node(decision_node);

    result_type
}

pub(crate) fn get_concat_nodes(expr: &aast::Expr<(), ()>) -> Vec<&aast::Expr<(), ()>> {
    match &expr.2 {
        aast::Expr_::Binop(x) => {
            let (binop, e1, e2) = (&x.bop, &x.lhs, &x.rhs);
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
