use std::{collections::BTreeMap, str::FromStr, sync::Arc};

use hakana_reflection_info::{
    data_flow::{
        graph::{GraphKind, WholeProgramKind},
        node::DataFlowNode,
        path::{ArrayDataKind, PathKind},
    },
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_type::{
    get_arraykey, get_keyset, get_literal_int, get_mixed_any, get_nothing, type_combiner,
    wrap_atomic,
};
use oxidized::{
    ast::Expr,
    ast_defs::Pos,
    tast::{KvcKind, VcKind},
};

use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{function_analysis_data::FunctionAnalysisData, stmt_analyzer::AnalysisError};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

#[derive(Debug)]
pub(crate) struct ArrayCreationInfo {
    item_key_atomic_types: Vec<TAtomic>,
    item_value_atomic_types: Vec<TAtomic>,
    known_items: Vec<(TAtomic, TUnion)>,
    parent_nodes: Vec<DataFlowNode>,
    effects: u8,
}

impl ArrayCreationInfo {
    pub fn new() -> Self {
        Self {
            item_key_atomic_types: Vec::new(),
            item_value_atomic_types: Vec::new(),
            parent_nodes: Vec::new(),
            known_items: Vec::new(),
            effects: 0,
        }
    }
}

#[derive(Debug, PartialEq)]
enum TContainerType {
    Vec,
    Dict,
    Keyset,
    Vector,
}

impl FromStr for TContainerType {
    type Err = ();

    fn from_str(input: &str) -> Result<TContainerType, Self::Err> {
        match input {
            "vec" => Ok(TContainerType::Vec),
            "dict" => Ok(TContainerType::Dict),
            "keyset" => Ok(TContainerType::Keyset),
            "Vector" => Ok(TContainerType::Vector),
            _ => Err(()),
        }
    }
}

pub(crate) fn analyze_vals(
    statements_analyzer: &StatementsAnalyzer,
    vc_kind: &oxidized::tast::VcKind,
    items: &[oxidized::ast::Expr],
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    // if the array is empty, this special type allows us to match any other array type against it
    if items.is_empty() {
        match vc_kind {
            VcKind::Vec => {
                analysis_data.set_expr_type(
                    pos,
                    wrap_atomic(TAtomic::TVec {
                        known_items: None,
                        type_param: Box::new(get_nothing()),
                        known_count: Some(0),
                        non_empty: false,
                    }),
                );
            }
            VcKind::Keyset => {
                analysis_data.set_expr_type(pos, get_keyset(get_nothing()));
            }
            VcKind::Vector => {
                analysis_data.set_expr_type(
                    pos,
                    wrap_atomic(TAtomic::TNamedObject {
                        name: statements_analyzer
                            .get_interner()
                            .get("HH\\Vector")
                            .unwrap(),
                        type_params: Some(vec![get_mixed_any()]),
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    }),
                );
            }
            _ => {}
        }

        return Ok(());
    }

    let codebase = statements_analyzer.get_codebase();
    let mut array_creation_info = ArrayCreationInfo::new();

    // Iterate through all of the items in this collection
    for (offset, item) in items.iter().enumerate() {
        // println!("item! {:?} ", item);
        analyze_vals_item(
            statements_analyzer,
            context,
            &mut array_creation_info,
            item,
            vc_kind,
            analysis_data,
            offset,
        )?;
    }

    match vc_kind {
        VcKind::Vec => {
            let types = array_creation_info.item_value_atomic_types.clone();

            let mut known_items = BTreeMap::new();

            if array_creation_info.item_key_atomic_types.len() < 20 {
                for (offset, (key_type, value_type)) in
                    array_creation_info.known_items.into_iter().enumerate()
                {
                    if let TAtomic::TLiteralInt {
                        value: key_literal_value,
                        ..
                    } = key_type
                    {
                        if (offset as i64) == key_literal_value {
                            known_items.insert(offset, (false, value_type));
                        }
                    }
                }
            }

            let mut new_vec = wrap_atomic(if !known_items.is_empty() {
                TAtomic::TVec {
                    known_items: Some(known_items),
                    type_param: Box::new(get_nothing()),
                    known_count: Some(types.len()),
                    non_empty: true,
                }
            } else {
                TAtomic::TVec {
                    known_items: None,
                    type_param: Box::new(TUnion::new(type_combiner::combine(
                        array_creation_info.item_value_atomic_types.clone(),
                        codebase,
                        false,
                    ))),
                    known_count: None,
                    non_empty: true,
                }
            });

            new_vec.parent_nodes = array_creation_info.parent_nodes;

            analysis_data.set_expr_type(pos, new_vec);
        }
        VcKind::Keyset => {
            let item_value_type = TUnion::new(type_combiner::combine(
                array_creation_info.item_value_atomic_types.clone(),
                codebase,
                false,
            ));

            let mut keyset = get_keyset(item_value_type);

            keyset.parent_nodes = array_creation_info.parent_nodes;

            analysis_data.set_expr_type(pos, keyset);
        }
        VcKind::Vector => {
            let mut new_vec = wrap_atomic(TAtomic::TNamedObject {
                name: statements_analyzer
                    .get_interner()
                    .get("HH\\Vector")
                    .unwrap(),
                type_params: Some(vec![get_mixed_any()]),
                is_this: false,
                extra_types: None,
                remapped_params: false,
            });

            new_vec.parent_nodes = array_creation_info.parent_nodes;

            analysis_data.set_expr_type(pos, new_vec);
        }
        _ => {}
    }

    analysis_data.expr_effects.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        array_creation_info.effects,
    );

    Ok(())
}

pub(crate) fn analyze_keyvals(
    statements_analyzer: &StatementsAnalyzer,
    kvc_kind: &oxidized::tast::KvcKind,
    items: &Vec<oxidized::tast::Field<(), ()>>,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    // if the array is empty, this special type allows us to match any other array type against it
    if items.is_empty() {
        analysis_data.set_expr_type(
            pos,
            wrap_atomic(TAtomic::TDict {
                known_items: None,
                params: None,
                non_empty: false,
                shape_name: None,
            }),
        );
        return Ok(());
    }

    let codebase = statements_analyzer.get_codebase();
    let mut array_creation_info = ArrayCreationInfo::new();

    // Iterate through all of the items in this collection
    for item in items {
        // println!("item! {:?} ", item);
        analyze_keyvals_item(
            statements_analyzer,
            context,
            &mut array_creation_info,
            item,
            kvc_kind,
            analysis_data,
        )?;
    }

    let mut known_items = BTreeMap::new();

    if array_creation_info.item_key_atomic_types.len() < 20 {
        for (key_type, value_type) in array_creation_info.known_items.into_iter() {
            if let TAtomic::TLiteralString {
                value: key_literal_value,
                ..
            } = key_type
            {
                known_items.insert(
                    DictKey::String(key_literal_value),
                    (false, Arc::new(value_type)),
                );
            }
        }
    }

    let mut new_dict = wrap_atomic(TAtomic::TDict {
        known_items: if !known_items.is_empty() {
            Some(known_items)
        } else {
            None
        },
        params: if array_creation_info.item_key_atomic_types.is_empty() {
            None
        } else {
            Some((
                Box::new(TUnion::new(type_combiner::combine(
                    array_creation_info.item_key_atomic_types.clone(),
                    codebase,
                    false,
                ))),
                Box::new(TUnion::new(type_combiner::combine(
                    array_creation_info.item_value_atomic_types.clone(),
                    codebase,
                    false,
                ))),
            ))
        },
        non_empty: true,
        shape_name: None,
    });

    new_dict.parent_nodes = array_creation_info.parent_nodes;

    analysis_data.set_expr_type(pos, new_dict);

    analysis_data.expr_effects.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        array_creation_info.effects,
    );

    Ok(())
}

fn analyze_vals_item(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    array_creation_info: &mut ArrayCreationInfo,
    item_value: &Expr,
    container_type: &VcKind,
    analysis_data: &mut FunctionAnalysisData,
    offset: usize,
) -> Result<(), AnalysisError> {
    let key_item_type = get_literal_int(offset.try_into().unwrap());

    // Now check types of the values
    expression_analyzer::analyze(
        statements_analyzer,
        item_value,
        analysis_data,
        context,
        &mut None,
    )?;

    array_creation_info.effects |= analysis_data
        .expr_effects
        .get(&(
            item_value.pos().start_offset() as u32,
            item_value.pos().end_offset() as u32,
        ))
        .unwrap_or(&0);

    let value_item_type = analysis_data
        .get_expr_type(item_value.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    add_array_value_dataflow(
        statements_analyzer,
        &value_item_type,
        analysis_data,
        &key_item_type,
        item_value,
        array_creation_info,
    );

    if key_item_type.is_single() && key_item_type.has_int() && matches!(container_type, VcKind::Vec)
    {
        array_creation_info
            .known_items
            .push((key_item_type.get_single_owned(), value_item_type));
    } else {
        let key_type_values = key_item_type.types.clone();
        // This is a lot simpler than the PHP mess, the type here can be
        // either int or string, and no other weird behavior.
        array_creation_info
            .item_key_atomic_types
            .extend(key_type_values);
        array_creation_info
            .item_value_atomic_types
            .extend(value_item_type.types);
    }

    Ok(())
}

fn analyze_keyvals_item(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    array_creation_info: &mut ArrayCreationInfo,
    item: &oxidized::tast::Field<(), ()>,
    container_type: &KvcKind,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    // Analyze type for key
    expression_analyzer::analyze(
        statements_analyzer,
        &item.0,
        analysis_data,
        context,
        &mut None,
    )?;

    array_creation_info.effects |= analysis_data
        .expr_effects
        .get(&(
            item.0.pos().start_offset() as u32,
            item.0.pos().end_offset() as u32,
        ))
        .unwrap_or(&0);

    let key_item_type = analysis_data
        .get_expr_type(item.0.pos())
        .cloned()
        .unwrap_or(get_arraykey(true));

    add_array_key_dataflow(
        statements_analyzer,
        &key_item_type,
        analysis_data,
        item.0.pos(),
        array_creation_info,
    );

    // Now check types of the values
    expression_analyzer::analyze(
        statements_analyzer,
        &item.1,
        analysis_data,
        context,
        &mut None,
    )?;

    array_creation_info.effects |= analysis_data
        .expr_effects
        .get(&(
            item.1.pos().start_offset() as u32,
            item.1.pos().end_offset() as u32,
        ))
        .unwrap_or(&0);

    let value_item_type = analysis_data
        .get_expr_type(item.1.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    add_array_value_dataflow(
        statements_analyzer,
        &value_item_type,
        analysis_data,
        &key_item_type,
        &item.1,
        array_creation_info,
    );

    if key_item_type.is_single()
        && key_item_type.has_string()
        && matches!(container_type, KvcKind::Dict)
    {
        array_creation_info
            .known_items
            .push((key_item_type.get_single_owned(), value_item_type));
    } else {
        let key_type_values = key_item_type.types.clone();
        // This is a lot simpler than the PHP mess, the type here can be
        // either int or string, and no other weird behavior.
        array_creation_info
            .item_key_atomic_types
            .extend(key_type_values);
        array_creation_info
            .item_value_atomic_types
            .extend(value_item_type.types);
    }

    Ok(())
}

fn add_array_value_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    value_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    key_item_type: &TUnion,
    value: &oxidized::aast::Expr<(), ()>,
    array_creation_info: &mut ArrayCreationInfo,
) {
    if value_type.parent_nodes.is_empty()
        || (matches!(
            &analysis_data.data_flow_graph.kind,
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        ) && !value_type.has_taintable_value())
    {
        return;
    }

    let mut node_name = "array".to_string();

    let key_item_single = if key_item_type.is_single() {
        Some(key_item_type.get_single())
    } else {
        None
    };

    if let Some(key_item_single) = key_item_single {
        if let TAtomic::TLiteralString { value, .. } = key_item_single {
            node_name = format!("array[{}]", value);
        } else if let TAtomic::TLiteralInt { value, .. } = key_item_single {
            node_name = format!("array[{}]", value);
        }
    }

    let new_parent_node =
        DataFlowNode::get_for_assignment(node_name, statements_analyzer.get_hpos(value.pos()));
    analysis_data
        .data_flow_graph
        .add_node(new_parent_node.clone());

    // TODO add taint event dispatches

    for parent_node in value_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            parent_node,
            &new_parent_node,
            if let Some(key_item_single) = key_item_single {
                if let TAtomic::TLiteralInt {
                    value: key_value, ..
                } = key_item_single
                {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, key_value.to_string())
                } else if let TAtomic::TLiteralString {
                    value: key_value, ..
                } = key_item_single
                {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, key_value.clone())
                } else {
                    PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue)
                }
            } else {
                PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue)
            },
            vec![],
            vec![],
        );
    }

    array_creation_info.parent_nodes.push(new_parent_node);
}

fn add_array_key_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    key_item_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    item_key_pos: &Pos,
    array_creation_info: &mut ArrayCreationInfo,
) {
    if key_item_type.parent_nodes.is_empty()
        || (matches!(
            &analysis_data.data_flow_graph.kind,
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        ) && !key_item_type.has_taintable_value())
    {
        return;
    }

    let node_name = "array".to_string();

    let new_parent_node =
        DataFlowNode::get_for_assignment(node_name, statements_analyzer.get_hpos(item_key_pos));
    analysis_data
        .data_flow_graph
        .add_node(new_parent_node.clone());

    // TODO add taint event dispatches

    let key_item_single = if key_item_type.is_single() {
        Some(key_item_type.get_single())
    } else {
        None
    };

    for parent_node in key_item_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            parent_node,
            &new_parent_node,
            if let Some(key_item_single) = key_item_single {
                if let TAtomic::TLiteralInt {
                    value: key_value, ..
                } = key_item_single
                {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayKey, key_value.to_string())
                } else if let TAtomic::TLiteralString {
                    value: key_value, ..
                } = key_item_single
                {
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayKey, key_value.clone())
                } else {
                    PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayKey)
                }
            } else {
                PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayKey)
            },
            vec![],
            vec![],
        );
    }

    array_creation_info.parent_nodes.push(new_parent_node);
}
