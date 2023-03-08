use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use hakana_reflection_info::{
    codebase_info::CodebaseInfo,
    data_flow::{
        graph::{GraphKind, WholeProgramKind},
        node::DataFlowNode,
        path::{ArrayDataKind, PathKind},
    },
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_type::{
    combine_union_types, get_arrayish_params, get_arraykey, get_int, get_mixed_any, get_nothing,
    type_combiner, wrap_atomic,
};
use oxidized::{
    aast::{self, Expr},
    ast_defs::Pos,
};
use rustc_hash::FxHashSet;

use crate::{
    expr::{expression_identifier, fetch::array_fetch_analyzer},
    typed_ast::TastInfo,
};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, Option<&Expr<(), ()>>, &Pos),
    assign_value_type: TUnion,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let mut root_array_expr = (expr.0, expr.1, pos);
    let mut array_exprs = Vec::new();

    while let aast::Expr_::ArrayGet(boxed) = &root_array_expr.0 .2 {
        array_exprs.push(root_array_expr);
        root_array_expr = (&boxed.0, boxed.1.as_ref(), &root_array_expr.0 .1);
    }

    array_exprs.push(root_array_expr.clone());
    let root_array_expr = root_array_expr.0;

    if expression_analyzer::analyze(
        statements_analyzer,
        root_array_expr,
        tast_info,
        context,
        &mut None,
    ) == false
    {
        // fall through
    }

    let mut root_type = tast_info
        .get_expr_type(root_array_expr.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    if root_type.is_mixed() {
        expression_analyzer::analyze(statements_analyzer, expr.0, tast_info, context, &mut None);

        if let Some(dim_expr) = expr.1 {
            expression_analyzer::analyze(
                statements_analyzer,
                dim_expr,
                tast_info,
                context,
                &mut None,
            );
        }
    }

    let mut current_type = root_type.clone();

    let root_var_id = expression_identifier::get_var_id(
        &root_array_expr,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some(statements_analyzer.get_codebase()),
    );

    let current_dim = analyze_nested_array_assignment(
        statements_analyzer,
        array_exprs,
        assign_value_type,
        tast_info,
        context,
        root_var_id.clone(),
        &mut root_type,
        &mut current_type,
    );

    if tast_info.data_flow_graph.kind == GraphKind::FunctionBody {
        if let Some(root_var_id) = &root_var_id {
            if let aast::Expr_::Lvar(_) = &root_array_expr.2 {
                tast_info
                    .data_flow_graph
                    .add_node(DataFlowNode::get_for_variable_source(
                        root_var_id.clone(),
                        statements_analyzer.get_hpos(root_array_expr.pos()),
                    ));
            }
        }
    }

    let root_is_string = root_type.has_string();

    let mut key_values = Vec::new();

    let dim_type = if let Some(current_dim) = current_dim {
        Some(
            tast_info
                .get_expr_type(current_dim.pos())
                .cloned()
                .unwrap_or(get_arraykey(true)),
        )
    } else {
        None
    };

    if let Some(dim_type) = &dim_type {
        for key_atomic_type in &dim_type.types {
            match key_atomic_type {
                TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TEnumLiteralCase { .. } => {
                    key_values.push(key_atomic_type.clone());
                }

                _ => (),
            }
        }
    }

    root_type = if !key_values.is_empty() {
        update_type_with_key_values(
            statements_analyzer,
            root_type,
            current_type,
            &key_values,
            dim_type.as_ref(),
        )
    } else if !root_is_string {
        update_array_assignment_child_type(
            statements_analyzer.get_codebase(),
            dim_type.as_ref(),
            context,
            current_type,
            root_type,
        )
    } else {
        root_type
    };

    if let Some(root_var_id) = &root_var_id {
        context
            .vars_in_scope
            .insert(root_var_id.clone(), Rc::new(root_type.clone()));
    }

    tast_info.set_expr_type(&root_array_expr.1, root_type);

    // InstancePropertyAssignmentAnalyzer (do we need it?)

    // StaticPropertyAssignmentAnalyzer (do we need it?)

    true
}

pub(crate) fn update_type_with_key_values(
    statements_analyzer: &StatementsAnalyzer,
    mut new_type: TUnion,
    current_type: TUnion,
    key_values: &Vec<TAtomic>,
    key_type: Option<&TUnion>,
) -> TUnion {
    let mut has_matching_item = false;
    let codebase = statements_analyzer.get_codebase();

    new_type.types = new_type
        .types
        .into_iter()
        .map(|atomic_type| {
            update_atomic_given_key(
                atomic_type,
                key_values,
                key_type.clone(),
                &mut has_matching_item,
                &current_type,
                codebase,
            )
        })
        .collect();

    return new_type;
}

fn update_atomic_given_key(
    mut atomic_type: TAtomic,
    key_values: &Vec<TAtomic>,
    key_type: Option<&TUnion>,
    has_matching_item: &mut bool,
    current_type: &TUnion,
    codebase: &CodebaseInfo,
) -> TAtomic {
    if let TAtomic::TGenericParam { .. } = atomic_type {
        // TODO
    }
    if key_values.len() > 0 {
        for key_value in key_values {
            // TODO also strings
            match atomic_type {
                TAtomic::TVec {
                    ref mut known_items,
                    ref mut non_empty,
                    ..
                } => {
                    if let TAtomic::TLiteralInt {
                        value: key_value, ..
                    } = key_value
                    {
                        *has_matching_item = true;

                        if let Some(known_items) = known_items {
                            if let Some((pu, entry)) = known_items.get_mut(&(*key_value as usize)) {
                                *entry = current_type.clone();
                                *pu = false;
                            } else {
                                known_items
                                    .insert(*key_value as usize, (false, current_type.clone()));
                            }
                        } else {
                            *known_items = Some(BTreeMap::from([(
                                *key_value as usize,
                                (false, current_type.clone()),
                            )]));
                        }

                        *non_empty = true;
                    }
                }
                TAtomic::TKeyset {
                    ref mut type_param, ..
                } => {
                    *has_matching_item = true;

                    *type_param = combine_union_types(
                        type_param,
                        &wrap_atomic(key_value.clone()),
                        codebase,
                        true,
                    );
                }
                TAtomic::TDict {
                    ref mut known_items,
                    ref mut non_empty,
                    ref mut shape_name,
                    ..
                } => {
                    let key = match key_value {
                        TAtomic::TLiteralString { value } => Some(DictKey::String(value.clone())),
                        TAtomic::TLiteralInt { value } => Some(DictKey::Int(*value as u32)),
                        TAtomic::TEnumLiteralCase {
                            enum_name,
                            member_name,
                            ..
                        } => Some(DictKey::Enum(enum_name.clone(), member_name.clone())),
                        _ => None,
                    };
                    if let Some(key) = key {
                        *has_matching_item = true;

                        if let Some(known_items) = known_items {
                            if let Some((pu, entry)) = known_items.get_mut(&key) {
                                *entry = Arc::new(current_type.clone());
                                *pu = false;
                            } else {
                                *shape_name = None;
                                known_items.insert(key, (false, Arc::new(current_type.clone())));
                            }
                        } else {
                            *known_items = Some(BTreeMap::from([(
                                key,
                                (false, Arc::new(current_type.clone())),
                            )]));
                        }

                        *non_empty = true;
                    }
                }
                _ => {}
            }
        }
    } else {
        let key_type = key_type.cloned().unwrap_or(get_int());

        let arrayish_params = get_arrayish_params(&atomic_type, codebase);

        match atomic_type {
            TAtomic::TVec {
                ref mut known_items,
                ref mut type_param,
                ref mut known_count,
                ..
            } => {
                *type_param = hakana_type::add_union_type(
                    arrayish_params.unwrap().1,
                    &current_type,
                    codebase,
                    false,
                );

                *known_items = None;
                *known_count = None;
            }
            TAtomic::TKeyset {
                ref mut type_param, ..
            } => {
                *type_param = hakana_type::add_union_type(
                    arrayish_params.unwrap().1,
                    &current_type,
                    codebase,
                    false,
                );
            }
            TAtomic::TDict {
                ref mut known_items,
                params: ref mut existing_params,
                ref mut non_empty,
                ref mut shape_name,
                ..
            } => {
                let params = arrayish_params.unwrap();

                *existing_params = Some((
                    hakana_type::add_union_type(params.0, &key_type, codebase, false),
                    hakana_type::add_union_type(params.1, &current_type, codebase, false),
                ));
                *known_items = None;
                *shape_name = None;
                *non_empty = true;
            }
            _ => (),
        }
    }
    atomic_type
}

fn add_array_assignment_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    expr_var_pos: &aast::Pos,
    mut parent_expr_type: TUnion,
    child_expr_type: &TUnion,
    var_var_id: Option<String>,
    key_values: &Vec<TAtomic>,
) -> TUnion {
    if let GraphKind::WholeProgram(WholeProgramKind::Taint) = tast_info.data_flow_graph.kind {
        if !child_expr_type.has_taintable_value() {
            return parent_expr_type;
        }
    }

    let parent_node = DataFlowNode::get_for_assignment(
        var_var_id.unwrap_or("array-assignment".to_string()),
        statements_analyzer.get_hpos(expr_var_pos),
    );

    tast_info.data_flow_graph.add_node(parent_node.clone());

    let old_parent_nodes = parent_expr_type.parent_nodes.clone();

    parent_expr_type.parent_nodes = FxHashSet::from_iter([parent_node.clone()]);

    for old_parent_node in old_parent_nodes {
        tast_info.data_flow_graph.add_path(
            &old_parent_node,
            &parent_node,
            PathKind::Default,
            None,
            None,
        );
    }

    let codebase = statements_analyzer.get_codebase();

    for child_parent_node in &child_expr_type.parent_nodes {
        if !key_values.is_empty() {
            for key_value in key_values {
                let key_value = match key_value {
                    TAtomic::TLiteralString { value, .. } => value.clone(),
                    TAtomic::TLiteralInt { value, .. } => value.to_string(),
                    TAtomic::TEnumLiteralCase {
                        enum_name,
                        member_name,
                        ..
                    } => {
                        if let Some(literal_value) = statements_analyzer
                            .get_codebase()
                            .get_classconst_literal_value(enum_name, member_name)
                        {
                            if let Some(value) =
                                literal_value.get_literal_string_value(&codebase.interner)
                            {
                                value
                            } else if let Some(value) = literal_value.get_literal_int_value() {
                                value.to_string()
                            } else {
                                println!("{},", key_value.get_id(Some(&codebase.interner)));
                                panic!()
                            }
                        } else {
                            println!("{},", key_value.get_id(Some(&codebase.interner)));
                            panic!();
                        }
                    }
                    _ => {
                        println!("{},", key_value.get_id(Some(&codebase.interner)));
                        panic!()
                    }
                };

                tast_info.data_flow_graph.add_path(
                    &child_parent_node,
                    &parent_node,
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, key_value),
                    None,
                    None,
                );
            }
        } else {
            tast_info.data_flow_graph.add_path(
                &child_parent_node,
                &parent_node,
                PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                None,
                None,
            );
        }
    }

    parent_expr_type
}

/*
 * Updates an array when the $key used does not have literals
*/
fn update_array_assignment_child_type(
    codebase: &CodebaseInfo,
    key_type: Option<&TUnion>,
    context: &mut ScopeContext,
    value_type: TUnion,
    root_type: TUnion,
) -> TUnion {
    let mut collection_types = Vec::new();

    if let Some(key_type) = key_type {
        let key_type = if key_type.is_mixed() {
            get_arraykey(true)
        } else {
            key_type.clone()
        };

        for original_type in &root_type.types {
            match original_type {
                TAtomic::TVec { known_items, .. } => collection_types.push(TAtomic::TVec {
                    type_param: value_type.clone(),
                    known_items: if let Some(known_items) = known_items {
                        Some(
                            known_items
                                .iter()
                                .map(|(k, v)| (k.clone(), (v.0, v.1.clone())))
                                .collect::<BTreeMap<_, _>>(),
                        )
                    } else {
                        None
                    },
                    known_count: None,
                    non_empty: true,
                }),
                TAtomic::TDict { known_items, .. } => collection_types.push(TAtomic::TDict {
                    params: Some((key_type.clone(), value_type.clone())),
                    known_items: if let Some(known_items) = known_items {
                        let known_item = Arc::new(value_type.clone());
                        Some(
                            known_items
                                .iter()
                                .map(|(k, v)| (k.clone(), (v.0, known_item.clone())))
                                .collect::<BTreeMap<_, _>>(),
                        )
                    } else {
                        None
                    },
                    non_empty: true,
                    shape_name: None,
                }),
                TAtomic::TKeyset { .. } => collection_types.push(TAtomic::TKeyset {
                    type_param: value_type.clone(),
                }),
                _ => collection_types.push(TAtomic::TMixedWithFlags(true, false, false, false)),
            }
        }
    } else {
        for original_type in &root_type.types {
            match original_type {
                TAtomic::TVec {
                    known_items,
                    type_param,
                    ..
                } => collection_types.push(if !context.inside_loop && type_param.is_nothing() {
                    TAtomic::TVec {
                        type_param: get_nothing(),
                        known_items: Some(BTreeMap::from([(
                            if let Some(known_items) = known_items {
                                known_items.len()
                            } else {
                                0
                            },
                            (false, value_type.clone()),
                        )])),
                        known_count: None,
                        non_empty: true,
                    }
                } else {
                    TAtomic::TVec {
                        type_param: value_type.clone(),
                        known_items: None,
                        known_count: None,
                        non_empty: true,
                    }
                }),
                TAtomic::TDict { .. } => {
                    // should not happen, but works at runtime
                    collection_types.push(TAtomic::TDict {
                        params: Some((get_int(), value_type.clone())),
                        known_items: None,
                        non_empty: true,
                        shape_name: None,
                    })
                }
                TAtomic::TKeyset { .. } => collection_types.push(TAtomic::TKeyset {
                    type_param: value_type.clone(),
                }),
                TAtomic::TMixed | TAtomic::TMixedWithFlags(..) => {
                    // todo handle illegal
                    collection_types.push(TAtomic::TMixedWithFlags(true, false, false, false))
                }
                _ => collection_types.push(TAtomic::TMixedWithFlags(true, false, false, false)),
            }
        }
    }

    let new_child_type: Option<TUnion> = None;

    if key_type.is_none() && !context.inside_loop {
        // todo update counts
    }

    let array_assignment_type =
        TUnion::new(type_combiner::combine(collection_types, codebase, false));

    let new_child_type = if let Some(new_child_type) = new_child_type {
        new_child_type
    } else {
        hakana_type::add_union_type(root_type, &array_assignment_type, codebase, true)
    };

    new_child_type
}

pub(crate) fn analyze_nested_array_assignment<'a>(
    statements_analyzer: &StatementsAnalyzer,
    mut array_exprs: Vec<(&'a Expr<(), ()>, Option<&'a Expr<(), ()>>, &aast::Pos)>,
    assign_value_type: TUnion,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    root_var_id: Option<String>,
    root_type: &mut TUnion,
    last_array_expr_type: &mut TUnion,
) -> Option<&'a Expr<(), ()>> {
    let mut var_id_additions = Vec::new();
    let mut last_array_expr_dim = None;
    let mut extended_var_id = None;
    let mut parent_var_id: Option<String> = None;
    let mut full_var_id = true;

    // First go from the root element up, and go as far as we can to figure out what
    // array types there are
    array_exprs.reverse();
    for (i, array_expr) in array_exprs.iter().enumerate() {
        let mut array_expr_offset_type = None;
        let mut array_expr_offset_atomic_types = vec![];

        if let Some(dim) = array_expr.1 {
            let was_inside_general_use = context.inside_general_use;
            context.inside_general_use = true;

            if expression_analyzer::analyze(statements_analyzer, dim, tast_info, context, &mut None)
                == false
            {
                context.inside_general_use = was_inside_general_use;

                return array_expr.1;
            }

            context.inside_general_use = was_inside_general_use;
            let dim_type = tast_info.get_expr_type(dim.pos()).cloned();
            array_expr_offset_type = if let Some(dim_type) = dim_type {
                array_expr_offset_atomic_types = get_array_assignment_offset_types(&dim_type);

                Some(dim_type)
            } else {
                Some(get_arraykey(true))
            };

            var_id_additions.push(
                if let Some(dim_id) = expression_identifier::get_dim_id(
                    dim,
                    Some(statements_analyzer.get_codebase()),
                    &statements_analyzer.get_file_analyzer().resolved_names,
                ) {
                    format!("[{}]", dim_id)
                } else {
                    if let Some(dim_id) = expression_identifier::get_var_id(
                        dim,
                        context.function_context.calling_class.as_ref(),
                        statements_analyzer.get_file_analyzer().get_file_source(),
                        statements_analyzer.get_file_analyzer().resolved_names,
                        Some(statements_analyzer.get_codebase()),
                    ) {
                        format!("[{}]", dim_id)
                    } else {
                        full_var_id = false;
                        "[-unknown-]".to_string()
                    }
                },
            );
        } else {
            var_id_additions.push("[-unknown-]".to_string());
            full_var_id = false;
        }

        let mut array_expr_var_type =
            if let Some(t) = tast_info.get_rc_expr_type(array_expr.0.pos()) {
                t.clone()
            } else {
                return array_expr.1;
            };

        if array_expr_var_type.is_nothing() && !context.inside_loop {
            // TODO this assumption is dangerous!
            // We see this inside loops, and it may be
            // necessary to create an TUnknownEmptyArray or similar
            // if this becomes a real problem
            let atomic = wrap_atomic(TAtomic::TDict {
                known_items: None,
                params: None,
                non_empty: false,
                shape_name: None,
            });
            array_expr_var_type = Rc::new(atomic);

            tast_info.set_rc_expr_type(&array_expr.0.pos(), array_expr_var_type.clone());
        } else if let Some(parent_var_id) = parent_var_id.to_owned() {
            if context.vars_in_scope.contains_key(&parent_var_id) {
                let scoped_type = context.vars_in_scope.get(&parent_var_id).unwrap();
                tast_info.set_rc_expr_type(&array_expr.0.pos(), scoped_type.clone());

                array_expr_var_type = scoped_type.clone();
            }
        }

        let new_offset_type = array_expr_offset_type.clone().unwrap_or(get_int());

        context.inside_assignment = true;

        let mut array_expr_type = array_fetch_analyzer::get_array_access_type_given_offset(
            statements_analyzer,
            tast_info,
            *array_expr,
            (*array_expr_var_type).clone(),
            &new_offset_type,
            true,
            &extended_var_id,
            context,
        );

        context.inside_assignment = false;

        let is_last = i == array_exprs.len() - 1;

        let mut array_expr_var_type_inner = (*array_expr_var_type).clone();

        if is_last {
            array_expr_type = assign_value_type.clone();
            tast_info.set_expr_type(&array_expr.2, assign_value_type.clone());

            array_expr_var_type_inner = add_array_assignment_dataflow(
                statements_analyzer,
                tast_info,
                array_expr.0.pos(),
                array_expr_var_type_inner,
                &assign_value_type,
                expression_identifier::get_var_id(
                    array_expr.0,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().get_file_source(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some(statements_analyzer.get_codebase()),
                ),
                &array_expr_offset_atomic_types,
            );
        } else {
            tast_info.set_expr_type(&array_expr.2, array_expr_type.clone());
        }

        tast_info.set_expr_type(array_expr.0.pos(), array_expr_var_type_inner.clone());

        if let Some(root_var_id) = &root_var_id {
            extended_var_id = Some(root_var_id.to_owned() + &var_id_additions.join("").to_string());

            if let Some(parent_var_id) = &parent_var_id {
                if full_var_id && parent_var_id.contains("[$") {
                    context.vars_in_scope.insert(
                        parent_var_id.clone(),
                        Rc::new(array_expr_var_type_inner.clone()),
                    );
                    context
                        .possibly_assigned_var_ids
                        .insert(parent_var_id.clone());
                }
            } else {
                *root_type = array_expr_var_type_inner.clone();

                context.vars_in_scope.insert(
                    root_var_id.clone(),
                    Rc::new(array_expr_var_type_inner.clone()),
                );
                context
                    .possibly_assigned_var_ids
                    .insert(root_var_id.clone());
            }
        }

        *last_array_expr_type = array_expr_type;
        last_array_expr_dim = array_expr.1;

        parent_var_id = extended_var_id.clone();
    }

    array_exprs.reverse();

    let first_stmt = &array_exprs.remove(0);

    if let Some(root_var_id) = &root_var_id {
        if let Some(_) = tast_info.get_expr_type(first_stmt.0.pos()) {
            let extended_var_id = root_var_id.clone() + var_id_additions.join("").as_str();

            if full_var_id && extended_var_id.contains("[$") {
                context
                    .vars_in_scope
                    .insert(extended_var_id.clone(), Rc::new(assign_value_type.clone()));
                context.possibly_assigned_var_ids.insert(extended_var_id);
            }
        }
    }

    var_id_additions.pop();

    for (i, array_expr) in array_exprs.iter().enumerate() {
        let mut array_expr_type = tast_info.get_expr_type(array_expr.2).unwrap().clone();

        let dim_type = if let Some(current_dim) = last_array_expr_dim {
            tast_info.get_expr_type(current_dim.pos())
        } else {
            None
        };

        let key_values = if let Some(dim_type) = dim_type {
            get_array_assignment_offset_types(dim_type)
        } else {
            vec![]
        };

        let mut parent_array_var_id = None;

        let array_expr_id = if let Some(var_var_id) = expression_identifier::get_var_id(
            array_expr.0,
            context.function_context.calling_class.as_ref(),
            statements_analyzer.get_file_analyzer().get_file_source(),
            statements_analyzer.get_file_analyzer().resolved_names,
            Some(statements_analyzer.get_codebase()),
        ) {
            parent_array_var_id = Some(var_var_id.clone());
            Some(format!(
                "{}{}",
                var_var_id,
                var_id_additions.last().unwrap()
            ))
        } else {
            None
        };

        array_expr_type = update_type_with_key_values(
            statements_analyzer,
            array_expr_type,
            last_array_expr_type.clone(),
            &key_values,
            dim_type,
        );

        *last_array_expr_type = array_expr_type.clone();
        last_array_expr_dim = array_expr.1;

        if let Some(array_expr_id) = &array_expr_id {
            if array_expr_id.contains("[$") {
                context
                    .vars_in_scope
                    .insert(array_expr_id.clone(), Rc::new(array_expr_type.clone()));
                context
                    .possibly_assigned_var_ids
                    .insert(array_expr_id.clone());
            }
        }

        let array_type = tast_info
            .get_expr_type(array_expr.0.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        // recalculate dim_type
        let dim_type = if let Some(current_dim) = array_expr.1 {
            tast_info.get_expr_type(current_dim.pos())
        } else {
            None
        };

        let key_values = if let Some(dim_type) = dim_type {
            get_array_assignment_offset_types(dim_type)
        } else {
            vec![]
        };

        let array_type = add_array_assignment_dataflow(
            statements_analyzer,
            tast_info,
            array_expr.0.pos(),
            array_type,
            &array_expr_type,
            parent_array_var_id,
            &key_values,
        );

        let is_first = i == array_exprs.len() - 1;

        if is_first {
            *root_type = array_type;
        } else {
            tast_info.set_expr_type(&array_expr.0.pos(), array_type);
        }

        var_id_additions.pop();
    }

    last_array_expr_dim
}

fn get_array_assignment_offset_types(child_stmt_dim_type: &TUnion) -> Vec<TAtomic> {
    let mut valid_offset_types = vec![];
    for single_atomic in &child_stmt_dim_type.types {
        match single_atomic {
            TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TEnumLiteralCase { .. } => valid_offset_types.push(single_atomic.clone()),

            _ => (),
        }
    }

    valid_offset_types
}
