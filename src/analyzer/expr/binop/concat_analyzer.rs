use crate::expr::fetch::class_constant_fetch_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::{expression_analyzer, stmt_analyzer::AnalysisError};
use bstr::ByteSlice;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::{get_string, wrap_atomic};
use hakana_code_info::{
    data_flow::{node::DataFlowNode, path::PathKind},
    t_atomic::TAtomic,
    taint::SinkType,
};
use oxidized::aast;

pub(crate) fn analyze<'expr>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let mut concat_nodes = get_concat_nodes(left);
    concat_nodes.push(right);

    for concat_node in &concat_nodes {
        expression_analyzer::analyze(
            statements_analyzer,
            concat_node,
            analysis_data,
            context,
            true,
        )?;
    }

    let result_type = analyze_concat_nodes(
        concat_nodes,
        statements_analyzer,
        analysis_data,
        context,
        stmt_pos,
    );

    // todo handle more string type combinations

    analysis_data.set_expr_type(stmt_pos, result_type);

    Ok(())
}

pub(crate) fn analyze_concat_nodes(
    concat_nodes: Vec<&aast::Expr<(), ()>>,
    statements_analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    stmt_pos: &aast::Pos,
) -> TUnion {
    let mut all_literals = true;

    let decision_node = DataFlowNode::get_for_composition(statements_analyzer.get_hpos(stmt_pos));

    let mut has_slash = false;
    let mut has_query = false;
    let mut nonempty_string = false;

    let mut existing_literal_string_values: Option<Vec<String>> = Some(vec!["".to_string()]);

    for (i, concat_node) in concat_nodes.iter().enumerate() {
        let mut new_literal_string_values = vec![];

        if let aast::Expr_::String(simple_string) = &concat_node.2 {
            if let Some(existing_literal_string_values) = &existing_literal_string_values {
                for val in existing_literal_string_values {
                    new_literal_string_values.push(val.clone() + simple_string.to_str().unwrap());
                }
            }

            if simple_string != "" {
                nonempty_string = true;
            }

            if simple_string.contains(&b'/') {
                has_slash = true;
            }
            if simple_string.contains(&b'?') {
                has_query = true;
            }
        } else {
            let expr_type = analysis_data
                .expr_types
                .get(&(
                    concat_node.pos().start_offset() as u32,
                    concat_node.pos().end_offset() as u32,
                ))
                .map(|t| t.clone());

            if let Some(expr_type) = expr_type {
                let mut local_nonempty_string = true;
                for t in &expr_type.types {
                    match t {
                        TAtomic::TLiteralString { value, .. } => {
                            if value.contains('/') {
                                has_slash = true;
                            }
                            if value.contains('?') {
                                has_query = true;
                            }

                            if value == "" {
                                local_nonempty_string = false;
                            }

                            if let Some(existing_literal_string_values) =
                                &existing_literal_string_values
                            {
                                for val in existing_literal_string_values {
                                    new_literal_string_values.push(val.clone() + value);
                                }
                            }
                        }
                        TAtomic::TStringWithFlags(is_truthy, _, true) => {
                            if !is_truthy {
                                local_nonempty_string = false;
                            }
                        }
                        TAtomic::TLiteralInt { .. }
                        | TAtomic::TEnumLiteralCase { .. }
                        | TAtomic::TEnum { .. } => {
                            existing_literal_string_values = None;
                            local_nonempty_string = false;
                        }
                        _ => {
                            local_nonempty_string = false;
                            all_literals = false;
                            existing_literal_string_values = None;
                            break;
                        }
                    }
                }

                class_constant_fetch_analyzer::check_class_ptr_used_as_string(
                    statements_analyzer,
                    context,
                    analysis_data,
                    &expr_type,
                    concat_node,
                );

                if local_nonempty_string {
                    nonempty_string = true;
                }

                for old_parent_node in &expr_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        &old_parent_node.id,
                        &decision_node.id,
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
                nonempty_string = false;
                all_literals = false;
                existing_literal_string_values = None;
            }
        }

        if existing_literal_string_values.is_some() && !new_literal_string_values.is_empty() {
            existing_literal_string_values = Some(new_literal_string_values);
        } else {
            existing_literal_string_values = None;
        }
    }

    let mut result_type = if all_literals {
        if let Some(existing_literal_string_values) = existing_literal_string_values {
            TUnion::new(
                existing_literal_string_values
                    .into_iter()
                    .map(|s| TAtomic::TLiteralString { value: s })
                    .collect(),
            )
        } else {
            wrap_atomic(TAtomic::TStringWithFlags(
                nonempty_string,
                nonempty_string,
                true,
            ))
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
