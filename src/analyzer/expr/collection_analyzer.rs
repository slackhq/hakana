use std::{collections::BTreeMap, str::FromStr, sync::Arc};

use hakana_reflection_info::{
    data_flow::{
        graph::{GraphKind, WholeProgramKind},
        node::DataFlowNode,
        path::{PathExpressionKind, PathKind},
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
use rustc_hash::FxHashSet;

use crate::typed_ast::TastInfo;
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

#[derive(Debug)]
pub(crate) struct ArrayCreationInfo {
    item_key_atomic_types: Vec<TAtomic>,
    item_value_atomic_types: Vec<TAtomic>,
    known_items: Vec<(TAtomic, TUnion)>,
    parent_nodes: FxHashSet<DataFlowNode>,
    effects: u8,
}

impl ArrayCreationInfo {
    pub fn new() -> Self {
        Self {
            item_key_atomic_types: Vec::new(),
            item_value_atomic_types: Vec::new(),
            parent_nodes: FxHashSet::default(),
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
    items: &Vec<oxidized::ast::Expr>,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    // if the array is empty, this special type allows us to match any other array type against it
    if items.is_empty() {
        match vc_kind {
            VcKind::Vec => {
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
            VcKind::Keyset => {
                tast_info.set_expr_type(&pos, get_keyset(get_nothing()));
            }
            VcKind::Vector => {
                tast_info.set_expr_type(
                    &pos,
                    wrap_atomic(TAtomic::TNamedObject {
                        name: statements_analyzer
                            .get_codebase()
                            .interner
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

        return true;
    }

    let codebase = statements_analyzer.get_codebase();
    let mut array_creation_info = ArrayCreationInfo::new();

    // Iterate through all of the items in this collection
    for (offset, item) in items.iter().enumerate() {
        // println!("item! {:?} ", item);
        analyze_vals_item(
            &statements_analyzer,
            context,
            &mut array_creation_info,
            item,
            vc_kind,
            tast_info,
            offset,
        );
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
                        codebase,
                        false,
                    )),
                    known_count: None,
                    non_empty: true,
                }
            });

            new_vec.parent_nodes = array_creation_info.parent_nodes;

            tast_info.set_expr_type(&pos, new_vec);
        }
        VcKind::Keyset => {
            let item_value_type = TUnion::new(type_combiner::combine(
                array_creation_info.item_value_atomic_types.clone(),
                codebase,
                false,
            ));

            let mut keyset = get_keyset(item_value_type);

            keyset.parent_nodes = array_creation_info.parent_nodes;

            tast_info.set_expr_type(&pos, keyset);
        }
        VcKind::Vector => {
            let mut new_vec = wrap_atomic(TAtomic::TNamedObject {
                name: codebase.interner.get("HH\\Vector").unwrap(),
                type_params: Some(vec![get_mixed_any()]),
                is_this: false,
                extra_types: None,
                remapped_params: false,
            });

            new_vec.parent_nodes = array_creation_info.parent_nodes;

            tast_info.set_expr_type(&pos, new_vec);
        }
        _ => {}
    }

    tast_info.expr_effects.insert(
        (pos.start_offset(), pos.end_offset()),
        array_creation_info.effects,
    );

    true
}

pub(crate) fn analyze_keyvals(
    statements_analyzer: &StatementsAnalyzer,
    kvc_kind: &oxidized::tast::KvcKind,
    items: &Vec<oxidized::tast::Field<(), ()>>,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    // if the array is empty, this special type allows us to match any other array type against it
    if items.is_empty() {
        tast_info.set_expr_type(
            &pos,
            wrap_atomic(TAtomic::TDict {
                known_items: None,
                params: None,
                non_empty: false,
                shape_name: None,
            }),
        );
        return true;
    }

    let codebase = statements_analyzer.get_codebase();
    let mut array_creation_info = ArrayCreationInfo::new();

    // Iterate through all of the items in this collection
    for item in items {
        // println!("item! {:?} ", item);
        analyze_keyvals_item(
            &statements_analyzer,
            context,
            &mut array_creation_info,
            item,
            kvc_kind,
            tast_info,
        );
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
        known_items: if known_items.len() > 0 {
            Some(known_items)
        } else {
            None
        },
        params: if array_creation_info.item_key_atomic_types.is_empty() {
            None
        } else {
            Some((
                TUnion::new(type_combiner::combine(
                    array_creation_info.item_key_atomic_types.clone(),
                    codebase,
                    false,
                )),
                TUnion::new(type_combiner::combine(
                    array_creation_info.item_value_atomic_types.clone(),
                    codebase,
                    false,
                )),
            ))
        },
        non_empty: true,
        shape_name: None,
    });

    new_dict.parent_nodes = array_creation_info.parent_nodes;

    tast_info.set_expr_type(&pos, new_dict);

    tast_info.expr_effects.insert(
        (pos.start_offset(), pos.end_offset()),
        array_creation_info.effects,
    );

    true
}

fn analyze_vals_item(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    array_creation_info: &mut ArrayCreationInfo,
    item_value: &Expr,
    container_type: &VcKind,
    tast_info: &mut TastInfo,
    offset: usize,
) -> bool {
    let key_item_type = get_literal_int(offset.try_into().unwrap());

    // Now check types of the values
    expression_analyzer::analyze(
        statements_analyzer,
        item_value,
        tast_info,
        context,
        &mut None,
    );

    array_creation_info.effects |= tast_info
        .expr_effects
        .get(&(
            item_value.pos().start_offset(),
            item_value.pos().end_offset(),
        ))
        .unwrap_or(&0);

    let value_item_type = tast_info
        .get_expr_type(&item_value.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    add_array_value_dataflow(
        statements_analyzer,
        &value_item_type,
        tast_info,
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

    true
}

fn analyze_keyvals_item(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    array_creation_info: &mut ArrayCreationInfo,
    item: &oxidized::tast::Field<(), ()>,
    container_type: &KvcKind,
    tast_info: &mut TastInfo,
) -> bool {
    // Analyze type for key
    expression_analyzer::analyze(statements_analyzer, &item.0, tast_info, context, &mut None);

    array_creation_info.effects |= tast_info
        .expr_effects
        .get(&(item.0.pos().start_offset(), item.0.pos().end_offset()))
        .unwrap_or(&0);

    let key_item_type = tast_info
        .get_expr_type(&item.0.pos())
        .cloned()
        .unwrap_or(get_arraykey(true));

    add_array_key_dataflow(
        statements_analyzer,
        &key_item_type,
        tast_info,
        item.0.pos(),
        array_creation_info,
    );

    // Now check types of the values
    expression_analyzer::analyze(statements_analyzer, &item.1, tast_info, context, &mut None);

    array_creation_info.effects |= tast_info
        .expr_effects
        .get(&(item.1.pos().start_offset(), item.1.pos().end_offset()))
        .unwrap_or(&0);

    let value_item_type = tast_info
        .get_expr_type(&item.1.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    add_array_value_dataflow(
        statements_analyzer,
        &value_item_type,
        tast_info,
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

    true
}

fn add_array_value_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    value_type: &TUnion,
    tast_info: &mut TastInfo,
    key_item_type: &TUnion,
    value: &oxidized::aast::Expr<(), ()>,
    array_creation_info: &mut ArrayCreationInfo,
) {
    if !value_type.parent_nodes.is_empty()
        && !(matches!(
            &tast_info.data_flow_graph.kind,
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        ) && !value_type.has_taintable_value())
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

        let new_parent_node =
            DataFlowNode::get_for_assignment(node_name, statements_analyzer.get_hpos(value.pos()));
        tast_info.data_flow_graph.add_node(new_parent_node.clone());

        // TODO add taint event dispatches

        for parent_node in value_type.parent_nodes.iter() {
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
                None,
                None,
            );
        }

        array_creation_info.parent_nodes.insert(new_parent_node);
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
        && !(matches!(
            &tast_info.data_flow_graph.kind,
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        ) && !key_item_type.has_taintable_value())
    {
        let node_name = "array".to_string();

        let new_parent_node =
            DataFlowNode::get_for_assignment(node_name, statements_analyzer.get_hpos(item_key_pos));
        tast_info.data_flow_graph.add_node(new_parent_node.clone());

        // TODO add taint event dispatches

        let key_item_single = if key_item_type.is_single() {
            Some(key_item_type.get_single())
        } else {
            None
        };

        for parent_node in key_item_type.parent_nodes.iter() {
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
                None,
                None,
            );
        }

        array_creation_info.parent_nodes.insert(new_parent_node);
    }
}
