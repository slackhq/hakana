use crate::{
    expression_analyzer, function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    statements_analyzer::StatementsAnalyzer, stmt_analyzer::AnalysisError,
};
use hakana_code_info::ttype::{get_mixed_any, wrap_atomic};
use hakana_code_info::{
    code_location::StmtStart,
    data_flow::{
        graph::{GraphKind, WholeProgramKind},
        node::DataFlowNode,
        path::{ArrayDataKind, PathKind},
    },
    t_atomic::{DictKey, TAtomic, TDict},
    t_union::TUnion,
};
use oxidized::{
    aast,
    ast_defs::{Pos, ShapeFieldName},
};
use rustc_hash::FxHashSet;
use std::{collections::BTreeMap, sync::Arc};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    shape_fields: &Vec<(ShapeFieldName, aast::Expr<(), ()>)>,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    let mut parent_nodes = vec![];

    let mut effects = 0;

    let mut known_items = BTreeMap::new();
    for (name, value_expr) in shape_fields {
        let start_pos = match name {
            ShapeFieldName::SFlitStr(name) => &name.0,
            ShapeFieldName::SFclassConst(lhs, _) => &lhs.0,
            ShapeFieldName::SFclassname(_) => todo!(),
        };

        if let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset {
            if current_stmt_offset.line != start_pos.line() as u32 {
                *current_stmt_offset = StmtStart {
                    offset: start_pos.start_offset() as u32,
                    line: start_pos.line() as u32,
                    column: start_pos.to_raw_span().start.column() as u16,
                    add_newline: true,
                };
            }
        }

        let name = match name {
            ShapeFieldName::SFlitStr(name) => Some(DictKey::String(name.1.to_string())),
            ShapeFieldName::SFclassConst(lhs, name) => {
                let lhs_name = if let Some(name) = statements_analyzer
                    .file_analyzer
                    .resolved_names
                    .get(&(lhs.0.start_offset() as u32))
                {
                    name
                } else {
                    return Err(AnalysisError::InternalError(
                        format!("unknown classname at pos {}", &lhs.1),
                        statements_analyzer.get_hpos(&lhs.0),
                    ));
                };

                let constant_type = codebase.get_class_constant_type(
                    lhs_name,
                    false,
                    &statements_analyzer.interner.get(&name.1).unwrap(),
                    FxHashSet::default(),
                );

                if let Some(constant_type) = constant_type {
                    if constant_type.is_single() {
                        let single = constant_type.get_single_owned();

                        match single {
                            TAtomic::TEnumLiteralCase {
                                enum_name,
                                member_name,
                                ..
                            } => Some(DictKey::Enum(enum_name, member_name)),
                            TAtomic::TLiteralString { value } => Some(DictKey::String(value)),
                            _ => None,
                        }
                    } else {
                        println!(
                            "surprising union type {}",
                            constant_type.get_id(Some(statements_analyzer.interner))
                        );
                        panic!();
                    }
                } else {
                    return Err(AnalysisError::InternalError(
                        format!(
                            "unknown constant {}::{}",
                            statements_analyzer.interner.lookup(lhs_name),
                            &name.1
                        ),
                        statements_analyzer.get_hpos(&name.0),
                    ));
                }
            }
            ShapeFieldName::SFclassname(_) => todo!(),
        };

        // Now check types of the values
        expression_analyzer::analyze(statements_analyzer, value_expr, analysis_data, context)?;

        effects |= analysis_data
            .expr_effects
            .get(&(
                value_expr.pos().start_offset() as u32,
                value_expr.pos().end_offset() as u32,
            ))
            .unwrap_or(&0);

        if let Some(name) = name {
            let value_item_type = analysis_data
                .get_expr_type(value_expr.pos())
                .cloned()
                .unwrap_or(get_mixed_any());

            if let Some(new_parent_node) = add_shape_value_dataflow(
                statements_analyzer,
                &value_item_type,
                analysis_data,
                &match &name {
                    DictKey::Int(i) => i.to_string(),
                    DictKey::String(k) => k.clone(),
                    DictKey::Enum(class_name, member_name) => {
                        statements_analyzer.interner.lookup(class_name).to_string()
                            + "::"
                            + statements_analyzer.interner.lookup(member_name)
                    }
                },
                value_expr,
            ) {
                parent_nodes.push(new_parent_node);
            }

            known_items.insert(name, (false, Arc::new(value_item_type)));
        }
    }

    analysis_data.expr_effects.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        effects,
    );

    let mut new_dict = wrap_atomic(TAtomic::TDict(TDict {
        known_items: if !known_items.is_empty() {
            Some(known_items)
        } else {
            None
        },
        params: None,
        non_empty: true,
        shape_name: None,
    }));

    if !parent_nodes.is_empty() {
        let dict_node = DataFlowNode::get_for_composition(statements_analyzer.get_hpos(pos));

        for child_node in parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &child_node.id,
                &dict_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }

        analysis_data.data_flow_graph.add_node(dict_node.clone());

        new_dict.parent_nodes = vec![dict_node];
    }

    analysis_data.set_expr_type(pos, new_dict);

    Ok(())
}

fn add_shape_value_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    value_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    key_value: &String,
    value: &aast::Expr<(), ()>,
) -> Option<DataFlowNode> {
    if value_type.parent_nodes.is_empty()
        || (matches!(
            &analysis_data.data_flow_graph.kind,
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        ) && !value_type.has_taintable_value())
    {
        return None;
    }

    let new_parent_node = DataFlowNode::get_for_array_item(
        key_value.to_string(),
        statements_analyzer.get_hpos(value.pos()),
    );
    analysis_data
        .data_flow_graph
        .add_node(new_parent_node.clone());

    // TODO add taint event dispatches

    for parent_node in value_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            &parent_node.id,
            &new_parent_node.id,
            PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, key_value.clone()),
            vec![],
            vec![],
        );
    }

    Some(new_parent_node)
}
