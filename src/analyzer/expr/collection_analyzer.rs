use std::{
    collections::{BTreeMap, HashMap, HashSet},
    str::FromStr,
    sync::Arc,
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
use hakana_type::{
    get_arraykey, get_keyset, get_literal_int, get_mixed_any, get_nothing, type_combiner,
    wrap_atomic,
};
use oxidized::{
    aast::{self, Afield, CollectionTarg},
    ast_defs::{Id, Pos},
};

use crate::typed_ast::TastInfo;
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

#[derive(Debug)]
pub(crate) struct ArrayCreationInfo {
    item_key_atomic_types: Vec<TAtomic>,
    item_value_atomic_types: Vec<TAtomic>,
    known_items: Vec<(TAtomic, TUnion)>,
    parent_nodes: HashMap<String, DataFlowNode>,
    all_pure: bool,
}

impl ArrayCreationInfo {
    pub fn new() -> Self {
        Self {
            item_key_atomic_types: Vec::new(),
            item_value_atomic_types: Vec::new(),
            parent_nodes: HashMap::new(),
            known_items: Vec::new(),
            all_pure: true,
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

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Id, &Option<CollectionTarg<()>>, &Vec<Afield<(), ()>>),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    // The Id contains the type of container we have
    let container_type = if let Ok(container_type) = TContainerType::from_str(&expr.0 .1) {
        container_type
    } else {
        return true;
    };

    // if the array is empty, this special type allows us to match any other array type against it
    if !expr.2.is_empty() {
        let codebase = statements_analyzer.get_codebase();
        let mut array_creation_info = ArrayCreationInfo::new();

        // Iterate through all of the items in this collection
        for (offset, item) in expr.2.iter().enumerate() {
            // println!("item! {:?} ", item);
            analyze_array_item(
                &statements_analyzer,
                context,
                &mut array_creation_info,
                &item,
                matches!(container_type, TContainerType::Keyset),
                tast_info,
                offset,
            );
        }

        match container_type {
            TContainerType::Vec => {
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

                let mut new_vec = wrap_atomic(if known_items.len() > 0 {
                    TAtomic::TVec {
                        known_items: Some(known_items),
                        type_param: get_nothing(),
                        known_count: Some(types.len()),
                        non_empty: true,
                    }
                } else {
                    TAtomic::TVec {
                        known_items: None,
                        type_param: TUnion::new(type_combiner::combine(
                            array_creation_info.item_value_atomic_types.clone(),
                            Some(codebase),
                            false,
                        )),
                        known_count: None,
                        non_empty: true,
                    }
                });

                new_vec.parent_nodes = array_creation_info.parent_nodes;

                tast_info.set_expr_type(&pos, new_vec);
            }
            TContainerType::Keyset => {
                let item_value_type = TUnion::new(type_combiner::combine(
                    array_creation_info.item_value_atomic_types.clone(),
                    Some(codebase),
                    false,
                ));

                let mut keyset = get_keyset(item_value_type);

                keyset.parent_nodes = array_creation_info.parent_nodes;

                tast_info.set_expr_type(&pos, keyset);
            }
            TContainerType::Dict => {
                let mut known_items = BTreeMap::new();

                if array_creation_info.item_key_atomic_types.len() < 20 {
                    for (key_type, value_type) in array_creation_info.known_items.into_iter() {
                        if let TAtomic::TLiteralString {
                            value: key_literal_value,
                            ..
                        } = key_type
                        {
                            known_items
                                .insert(key_literal_value.clone(), (false, Arc::new(value_type)));
                        }
                    }
                }

                let mut new_dict = wrap_atomic(TAtomic::TDict {
                    known_items: if known_items.len() > 0 {
                        Some(known_items)
                    } else {
                        None
                    },
                    enum_items: None,
                    key_param: if array_creation_info.item_key_atomic_types.is_empty() {
                        get_nothing()
                    } else {
                        TUnion::new(type_combiner::combine(
                            array_creation_info.item_key_atomic_types.clone(),
                            Some(codebase),
                            false,
                        ))
                    },
                    value_param: if array_creation_info.item_value_atomic_types.is_empty() {
                        get_nothing()
                    } else {
                        TUnion::new(type_combiner::combine(
                            array_creation_info.item_value_atomic_types.clone(),
                            Some(codebase),
                            false,
                        ))
                    },
                    non_empty: true,
                    shape_name: None,
                });

                new_dict.parent_nodes = array_creation_info.parent_nodes;

                tast_info.set_expr_type(&pos, new_dict);
            }
            TContainerType::Vector => {
                let mut new_vec = wrap_atomic(TAtomic::TNamedObject {
                    name: "HH\\Vector".to_string(),
                    type_params: Some(vec![get_mixed_any()]),
                    is_this: false,
                    extra_types: None,
                    remapped_params: false,
                });

                new_vec.parent_nodes = array_creation_info.parent_nodes;

                tast_info.set_expr_type(&pos, new_vec);
            }
        }

        if array_creation_info.all_pure {
            tast_info
                .pure_exprs
                .insert((pos.start_offset(), pos.end_offset()));
        }

        return true;
    }

    // fallthrough: create empty array
    match container_type {
        TContainerType::Vec => {
            tast_info.set_expr_type(
                &pos,
                wrap_atomic(TAtomic::TVec {
                    known_items: None,
                    type_param: get_nothing(),
                    known_count: Some(0),
                    non_empty: false,
                }),
            );
        }
        TContainerType::Dict => {
            tast_info.set_expr_type(
                &pos,
                wrap_atomic(TAtomic::TDict {
                    known_items: None,
                    enum_items: None,
                    key_param: get_nothing(),
                    value_param: get_nothing(),
                    non_empty: false,
                    shape_name: None,
                }),
            );
        }
        TContainerType::Keyset => {
            tast_info.set_expr_type(&pos, get_keyset(get_nothing()));
        }
        TContainerType::Vector => {
            tast_info.set_expr_type(
                &pos,
                wrap_atomic(TAtomic::TNamedObject {
                    name: "HH\\Vector".to_string(),
                    type_params: Some(vec![get_mixed_any()]),
                    is_this: false,
                    extra_types: None,
                    remapped_params: false,
                }),
            );
        }
    }

    tast_info
        .pure_exprs
        .insert((pos.start_offset(), pos.end_offset()));

    true
}

fn analyze_array_item(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    array_creation_info: &mut ArrayCreationInfo,
    item: &Afield<(), ()>,
    is_keyset: bool,
    tast_info: &mut TastInfo,
    offset: usize,
) -> bool {
    // Special handling for dict-like arrays
    let (key_item_type, value) = match item {
        Afield::AFkvalue(item_key, item_key_value) => {
            // Analyze type for key
            expression_analyzer::analyze(
                statements_analyzer,
                item_key,
                tast_info,
                context,
                &mut None,
            );

            if !tast_info
                .pure_exprs
                .contains(&(item_key.pos().start_offset(), item_key.pos().end_offset()))
            {
                array_creation_info.all_pure = false;
            }

            let key_item_type = tast_info
                .get_expr_type(&item_key.pos())
                .cloned()
                .unwrap_or(get_arraykey());

            add_array_key_dataflow(
                statements_analyzer,
                &key_item_type,
                tast_info,
                item_key.pos(),
                array_creation_info,
            );

            (key_item_type, item_key_value)
        }
        Afield::AFvalue(item_value) => {
            let key_item_type = get_literal_int(offset.try_into().unwrap());

            // key is an int in vec-like arrays
            // array_creation_info.item_key_atomic_types.push(get_int());

            (key_item_type, item_value)
        }
    };

    // Now check types of the values
    expression_analyzer::analyze(statements_analyzer, value, tast_info, context, &mut None);

    if !tast_info
        .pure_exprs
        .contains(&(value.pos().start_offset(), value.pos().end_offset()))
    {
        array_creation_info.all_pure = false;
    }

    let value_item_type = tast_info
        .get_expr_type(&value.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    add_array_value_dataflow(
        statements_analyzer,
        &value_item_type,
        tast_info,
        &key_item_type,
        value,
        array_creation_info,
    );

    if key_item_type.is_single() && !is_keyset {
        array_creation_info.known_items.push((
            key_item_type.types.into_iter().next().unwrap().1,
            value_item_type,
        ));
    } else {
        let key_type_values = key_item_type.types.values().cloned();
        // This is a lot simpler than the PHP mess, the type here can be
        // either int or string, and no other weird behavior.
        array_creation_info
            .item_key_atomic_types
            .extend(key_type_values);
        array_creation_info.item_value_atomic_types.extend(
            value_item_type
                .types
                .into_iter()
                .map(|(_, v)| v)
                .collect::<Vec<_>>(),
        );
    }

    true
}

fn add_array_value_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    value_type: &TUnion,
    tast_info: &mut TastInfo,
    key_item_type: &TUnion,
    value: &aast::Expr<(), ()>,
    array_creation_info: &mut ArrayCreationInfo,
) {
    if !value_type.parent_nodes.is_empty()
        && !(tast_info.data_flow_graph.kind == GraphKind::Taint
            && !value_type.has_taintable_value())
    {
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
                if let Some(key_item_single) = key_item_single {
                    if let TAtomic::TLiteralInt {
                        value: key_value, ..
                    } = key_item_single
                    {
                        PathKind::ExpressionAssignment(
                            PathExpressionKind::ArrayValue,
                            key_value.to_string(),
                        )
                    } else if let TAtomic::TLiteralString {
                        value: key_value, ..
                    } = key_item_single
                    {
                        PathKind::ExpressionAssignment(
                            PathExpressionKind::ArrayValue,
                            key_value.clone(),
                        )
                    } else {
                        PathKind::UnknownExpressionAssignment(PathExpressionKind::ArrayValue)
                    }
                } else {
                    PathKind::UnknownExpressionAssignment(PathExpressionKind::ArrayValue)
                },
                HashSet::new(),
                HashSet::new(),
            );
        }

        array_creation_info
            .parent_nodes
            .insert(new_parent_node.id.clone(), new_parent_node);
    }
}

fn add_array_key_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    key_item_type: &TUnion,
    tast_info: &mut TastInfo,
    item_key_pos: &Pos,
    array_creation_info: &mut ArrayCreationInfo,
) {
    if !key_item_type.parent_nodes.is_empty()
        && !(tast_info.data_flow_graph.kind == GraphKind::Taint
            && !key_item_type.has_taintable_value())
    {
        let node_name = "array".to_string();

        let new_parent_node = DataFlowNode::get_for_assignment(
            node_name,
            statements_analyzer.get_hpos(item_key_pos),
            None,
        );
        tast_info.data_flow_graph.add_node(new_parent_node.clone());

        // TODO add taint event dispatches

        let key_item_single = if key_item_type.is_single() {
            Some(key_item_type.get_single())
        } else {
            None
        };

        for (_, parent_node) in key_item_type.parent_nodes.iter() {
            tast_info.data_flow_graph.add_path(
                parent_node,
                &new_parent_node,
                if let Some(key_item_single) = key_item_single {
                    if let TAtomic::TLiteralInt {
                        value: key_value, ..
                    } = key_item_single
                    {
                        PathKind::ExpressionAssignment(
                            PathExpressionKind::ArrayKey,
                            key_value.to_string(),
                        )
                    } else if let TAtomic::TLiteralString {
                        value: key_value, ..
                    } = key_item_single
                    {
                        PathKind::ExpressionAssignment(
                            PathExpressionKind::ArrayKey,
                            key_value.clone(),
                        )
                    } else {
                        PathKind::UnknownExpressionAssignment(PathExpressionKind::ArrayKey)
                    }
                } else {
                    PathKind::UnknownExpressionAssignment(PathExpressionKind::ArrayKey)
                },
                HashSet::new(),
                HashSet::new(),
            );
        }

        array_creation_info
            .parent_nodes
            .insert(new_parent_node.id.clone(), new_parent_node);
    }
}
