use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use hakana_reflection_info::{
    codebase_info::CodebaseInfo,
    data_flow::{
        graph::{GraphKind, WholeProgramKind},
        node::{DataFlowNode, DataFlowNodeKind},
        path::{ArrayDataKind, PathKind},
    },
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_str::StrId;
use hakana_type::{
    combine_union_types, get_arrayish_params, get_arraykey, get_int, get_mixed_any, get_nothing,
    template::TemplateBound, type_combiner, wrap_atomic,
};
use oxidized::{
    aast::{self, Expr},
    ast_defs::Pos,
};

use crate::{
    expr::{expression_identifier, fetch::array_fetch_analyzer},
    function_analysis_data::FunctionAnalysisData,
    stmt_analyzer::AnalysisError,
};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

use super::instance_property_assignment_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, Option<&Expr<(), ()>>, &Pos),
    assign_value_type: TUnion,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    let mut root_array_expr = (expr.0, expr.1, pos);
    let mut array_exprs = Vec::new();

    while let aast::Expr_::ArrayGet(boxed) = &root_array_expr.0 .2 {
        array_exprs.push(root_array_expr);
        root_array_expr = (&boxed.0, boxed.1.as_ref(), &root_array_expr.0 .1);
    }

    array_exprs.push(root_array_expr);
    let root_array_expr = root_array_expr.0;

    expression_analyzer::analyze(
        statements_analyzer,
        root_array_expr,
        analysis_data,
        context,
        &mut None,
    )?;

    let mut root_type = analysis_data
        .get_expr_type(root_array_expr.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    if root_type.is_mixed() {
        expression_analyzer::analyze(
            statements_analyzer,
            expr.0,
            analysis_data,
            context,
            &mut None,
        )?;

        if let Some(dim_expr) = expr.1 {
            expression_analyzer::analyze(
                statements_analyzer,
                dim_expr,
                analysis_data,
                context,
                &mut None,
            )?;
        }
    }

    let mut current_type = root_type.clone();

    let root_var_id = expression_identifier::get_var_id(
        root_array_expr,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some((
            statements_analyzer.get_codebase(),
            statements_analyzer.get_interner(),
        )),
    );

    let current_dim = analyze_nested_array_assignment(
        statements_analyzer,
        array_exprs,
        assign_value_type,
        analysis_data,
        context,
        root_var_id.clone(),
        &mut root_type,
        &mut current_type,
    )?;

    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
        if let Some(root_var_id) = &root_var_id {
            if let aast::Expr_::Lvar(_) = &root_array_expr.2 {
                analysis_data
                    .data_flow_graph
                    .add_node(DataFlowNode::get_for_variable_source(
                        root_var_id.clone(),
                        statements_analyzer.get_hpos(root_array_expr.pos()),
                        false,
                        false,
                    ));
            }
        }
    }

    let root_is_string = root_type.has_string();

    let mut key_values = Vec::new();

    let dim_type = current_dim.map(|current_dim| {
        analysis_data
            .get_rc_expr_type(current_dim.pos())
            .cloned()
            .unwrap_or(Rc::new(get_arraykey(true)))
    });

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
            dim_type,
        )
    } else if !root_is_string {
        update_array_assignment_child_type(
            statements_analyzer,
            analysis_data,
            expr.2,
            dim_type,
            context,
            current_type,
            root_type,
        )
    } else {
        root_type
    };

    if let aast::Expr_::ObjGet(lhs_root) = &root_array_expr.2 {
        instance_property_assignment_analyzer::analyze(
            statements_analyzer,
            (&lhs_root.0, &lhs_root.1),
            pos,
            None,
            &root_type,
            analysis_data,
            context,
        )?;
    }

    if let Some(root_var_id) = &root_var_id {
        context
            .vars_in_scope
            .insert(root_var_id.clone(), Rc::new(root_type.clone()));
    }

    analysis_data.set_expr_type(&root_array_expr.1, root_type);

    // StaticPropertyAssignmentAnalyzer (do we need it?)

    Ok(())
}

pub(crate) fn update_type_with_key_values(
    statements_analyzer: &StatementsAnalyzer,
    mut new_type: TUnion,
    current_type: TUnion,
    key_values: &Vec<TAtomic>,
    key_type: Option<Rc<TUnion>>,
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

    new_type
}

fn update_atomic_given_key(
    mut atomic_type: TAtomic,
    key_values: &Vec<TAtomic>,
    key_type: Option<Rc<TUnion>>,
    has_matching_item: &mut bool,
    current_type: &TUnion,
    codebase: &CodebaseInfo,
) -> TAtomic {
    if let TAtomic::TGenericParam { .. } = atomic_type {
        // TODO
    }
    if !key_values.is_empty() {
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

                    *type_param = Box::new(combine_union_types(
                        type_param,
                        &wrap_atomic(key_value.clone()),
                        codebase,
                        true,
                    ));
                }
                TAtomic::TDict {
                    ref mut known_items,
                    ref mut non_empty,
                    ref mut shape_name,
                    ..
                } => {
                    let key = match key_value {
                        TAtomic::TLiteralString { value } => Some(DictKey::String(value.clone())),
                        TAtomic::TLiteralInt { value } => Some(DictKey::Int(*value as u64)),
                        TAtomic::TEnumLiteralCase {
                            enum_name,
                            member_name,
                            ..
                        } => Some(DictKey::Enum(*enum_name, *member_name)),
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
        let arrayish_params = get_arrayish_params(&atomic_type, codebase);

        match atomic_type {
            TAtomic::TVec {
                ref mut known_items,
                ref mut type_param,
                ref mut known_count,
                ..
            } => {
                *type_param = Box::new(hakana_type::add_union_type(
                    arrayish_params.unwrap().1,
                    current_type,
                    codebase,
                    false,
                ));

                *known_items = None;
                *known_count = None;
            }
            TAtomic::TKeyset {
                ref mut type_param, ..
            } => {
                *type_param = Box::new(hakana_type::add_union_type(
                    arrayish_params.unwrap().1,
                    current_type,
                    codebase,
                    false,
                ));
            }
            TAtomic::TDict {
                ref mut known_items,
                params: ref mut existing_params,
                ref mut non_empty,
                ref mut shape_name,
                ..
            } => {
                let params = arrayish_params.unwrap();
                let key_type = key_type.clone().unwrap_or(Rc::new(get_int()));

                *existing_params = Some((
                    Box::new(hakana_type::add_union_type(
                        params.0, &key_type, codebase, false,
                    )),
                    Box::new(hakana_type::add_union_type(
                        params.1,
                        current_type,
                        codebase,
                        false,
                    )),
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
    analysis_data: &mut FunctionAnalysisData,
    expr_var_pos: &aast::Pos,
    mut parent_expr_type: TUnion,
    child_expr_type: &TUnion,
    var_var_id: Option<String>,
    key_values: &Vec<TAtomic>,
    inside_general_use: bool,
) -> TUnion {
    if let GraphKind::WholeProgram(WholeProgramKind::Taint) = analysis_data.data_flow_graph.kind {
        if !child_expr_type.has_taintable_value() {
            return parent_expr_type;
        }
    }

    let parent_node = DataFlowNode::get_for_assignment(
        var_var_id.unwrap_or("array-assignment".to_string()),
        statements_analyzer.get_hpos(expr_var_pos),
    );

    if inside_general_use && analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
        let pos = statements_analyzer.get_hpos(expr_var_pos);

        let assignment_node = DataFlowNode {
            id: parent_node.id.clone() + "_sink",
            kind: DataFlowNodeKind::VariableUseSink { pos },
        };

        analysis_data.data_flow_graph.add_path(
            &parent_node,
            &assignment_node,
            PathKind::Default,
            None,
            None,
        );

        analysis_data.data_flow_graph.add_node(assignment_node);
    }

    analysis_data.data_flow_graph.add_node(parent_node.clone());

    let old_parent_nodes = parent_expr_type.parent_nodes.clone();

    parent_expr_type.parent_nodes = vec![parent_node.clone()];

    for old_parent_node in old_parent_nodes {
        analysis_data.data_flow_graph.add_path(
            &old_parent_node,
            &parent_node,
            PathKind::Default,
            None,
            None,
        );
    }

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
                            if let Some(value) = literal_value.get_literal_string_value() {
                                value
                            } else if let Some(value) = literal_value.get_literal_int_value() {
                                value.to_string()
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    _ => {
                        continue;
                    }
                };

                analysis_data.data_flow_graph.add_path(
                    child_parent_node,
                    &parent_node,
                    PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, key_value),
                    None,
                    None,
                );
            }
        } else {
            analysis_data.data_flow_graph.add_path(
                child_parent_node,
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
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    key_type: Option<Rc<TUnion>>,
    context: &mut ScopeContext,
    value_type: TUnion,
    root_type: TUnion,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();
    let mut collection_types = Vec::new();

    if let Some(key_type) = &key_type {
        let key_type = if key_type.is_mixed() {
            Rc::new(get_arraykey(true))
        } else {
            key_type.clone()
        };

        for original_type in &root_type.types {
            match original_type {
                TAtomic::TVec { known_items, .. } => collection_types.push(TAtomic::TVec {
                    type_param: Box::new(value_type.clone()),
                    known_items: known_items.clone(),
                    known_count: None,
                    non_empty: true,
                }),
                TAtomic::TDict { known_items, .. } => collection_types.push(TAtomic::TDict {
                    params: Some((Box::new((*key_type).clone()), Box::new(value_type.clone()))),
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
                    type_param: Box::new(value_type.clone()),
                }),
                TAtomic::TTypeVariable { name } => {
                    if let Some((_, upper_bounds)) =
                        analysis_data.type_variable_bounds.get_mut(name)
                    {
                        let mut bound = TemplateBound::new(
                            wrap_atomic(TAtomic::TNamedObject {
                                name: StrId::KEYED_CONTAINER,
                                type_params: Some(vec![(*key_type).clone(), value_type.clone()]),
                                is_this: false,
                                extra_types: None,
                                remapped_params: false,
                            }),
                            0,
                            None,
                            None,
                        );
                        bound.pos = Some(statements_analyzer.get_hpos(pos));
                        upper_bounds.push(bound);
                    }

                    collection_types.push(original_type.clone());
                }
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
                        type_param: Box::new(get_nothing()),
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
                        type_param: Box::new(value_type.clone()),
                        known_items: None,
                        known_count: None,
                        non_empty: true,
                    }
                }),
                TAtomic::TDict { .. } => {
                    // should not happen, but works at runtime
                    collection_types.push(TAtomic::TDict {
                        params: Some((Box::new(get_int()), Box::new(value_type.clone()))),
                        known_items: None,
                        non_empty: true,
                        shape_name: None,
                    })
                }
                TAtomic::TKeyset { .. } => collection_types.push(TAtomic::TKeyset {
                    type_param: Box::new(value_type.clone()),
                }),
                TAtomic::TMixed | TAtomic::TMixedWithFlags(..) => {
                    // todo handle illegal
                    collection_types.push(TAtomic::TMixedWithFlags(true, false, false, false))
                }
                TAtomic::TTypeVariable { name } => {
                    if let Some((_, upper_bounds)) =
                        analysis_data.type_variable_bounds.get_mut(name)
                    {
                        let mut bound = TemplateBound::new(
                            wrap_atomic(TAtomic::TNamedObject {
                                name: StrId::CONTAINER,
                                type_params: Some(vec![value_type.clone()]),
                                is_this: false,
                                extra_types: None,
                                remapped_params: false,
                            }),
                            0,
                            None,
                            None,
                        );
                        bound.pos = Some(statements_analyzer.get_hpos(pos));
                        upper_bounds.push(bound);
                    }

                    collection_types.push(original_type.clone());
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

    if let Some(new_child_type) = new_child_type {
        new_child_type
    } else {
        hakana_type::add_union_type(root_type, &array_assignment_type, codebase, true)
    }
}

pub(crate) fn analyze_nested_array_assignment<'a>(
    statements_analyzer: &StatementsAnalyzer,
    mut array_exprs: Vec<(&'a Expr<(), ()>, Option<&'a Expr<(), ()>>, &aast::Pos)>,
    assign_value_type: TUnion,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    root_var_id: Option<String>,
    root_type: &mut TUnion,
    last_array_expr_type: &mut TUnion,
) -> Result<Option<&'a Expr<(), ()>>, AnalysisError> {
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

            expression_analyzer::analyze(
                statements_analyzer,
                dim,
                analysis_data,
                context,
                &mut None,
            )?;

            context.inside_general_use = was_inside_general_use;
            let dim_type = analysis_data.get_rc_expr_type(dim.pos()).cloned();
            array_expr_offset_type = if let Some(dim_type) = dim_type {
                array_expr_offset_atomic_types = get_array_assignment_offset_types(&dim_type);

                Some(dim_type)
            } else {
                Some(Rc::new(get_arraykey(true)))
            };

            var_id_additions.push(
                if let Some(dim_id) = expression_identifier::get_dim_id(
                    dim,
                    Some((
                        statements_analyzer.get_codebase(),
                        statements_analyzer.get_interner(),
                    )),
                    statements_analyzer.get_file_analyzer().resolved_names,
                ) {
                    format!("[{}]", dim_id)
                } else if let Some(dim_id) = expression_identifier::get_var_id(
                    dim,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some((
                        statements_analyzer.get_codebase(),
                        statements_analyzer.get_interner(),
                    )),
                ) {
                    format!("[{}]", dim_id)
                } else {
                    full_var_id = false;
                    "[-unknown-]".to_string()
                },
            );
        } else {
            var_id_additions.push("[-unknown-]".to_string());
            full_var_id = false;
        }

        let mut array_expr_var_type =
            if let Some(t) = analysis_data.get_rc_expr_type(array_expr.0.pos()) {
                t.clone()
            } else {
                return Ok(array_expr.1);
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

            analysis_data.set_rc_expr_type(array_expr.0.pos(), array_expr_var_type.clone());
        } else if let Some(parent_var_id) = parent_var_id.to_owned() {
            if context.vars_in_scope.contains_key(&parent_var_id) {
                let scoped_type = context.vars_in_scope.get(&parent_var_id).unwrap();
                analysis_data.set_rc_expr_type(array_expr.0.pos(), scoped_type.clone());

                array_expr_var_type = scoped_type.clone();
            }
        }

        let new_offset_type = array_expr_offset_type.clone().unwrap_or(Rc::new(get_int()));

        context.inside_assignment = true;

        let mut array_expr_type = array_fetch_analyzer::get_array_access_type_given_offset(
            statements_analyzer,
            analysis_data,
            *array_expr,
            &array_expr_var_type,
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
            analysis_data.set_expr_type(array_expr.2, assign_value_type.clone());

            array_expr_var_type_inner = add_array_assignment_dataflow(
                statements_analyzer,
                analysis_data,
                array_expr.0.pos(),
                array_expr_var_type_inner,
                &assign_value_type,
                expression_identifier::get_var_id(
                    array_expr.0,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some((
                        statements_analyzer.get_codebase(),
                        statements_analyzer.get_interner(),
                    )),
                ),
                &array_expr_offset_atomic_types,
                context.inside_general_use,
            );
        } else {
            analysis_data.set_expr_type(array_expr.2, array_expr_type.clone());
        }

        analysis_data.set_expr_type(array_expr.0.pos(), array_expr_var_type_inner.clone());

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

        parent_var_id.clone_from(&extended_var_id);
    }

    array_exprs.reverse();

    let first_stmt = &array_exprs.remove(0);

    if let Some(root_var_id) = &root_var_id {
        if analysis_data.get_expr_type(first_stmt.0.pos()).is_some() {
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
        let mut array_expr_type = analysis_data.get_expr_type(array_expr.2).unwrap().clone();

        let dim_type = if let Some(current_dim) = last_array_expr_dim {
            analysis_data.get_rc_expr_type(current_dim.pos()).cloned()
        } else {
            None
        };

        let key_values = if let Some(dim_type) = &dim_type {
            get_array_assignment_offset_types(dim_type)
        } else {
            vec![]
        };

        let mut parent_array_var_id = None;

        let array_expr_id = if let Some(var_var_id) = expression_identifier::get_var_id(
            array_expr.0,
            context.function_context.calling_class.as_ref(),
            statements_analyzer.get_file_analyzer().resolved_names,
            Some((
                statements_analyzer.get_codebase(),
                statements_analyzer.get_interner(),
            )),
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

        let array_type = analysis_data
            .get_expr_type(array_expr.0.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        // recalculate dim_type
        let dim_type = if let Some(current_dim) = array_expr.1 {
            analysis_data.get_rc_expr_type(current_dim.pos())
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
            analysis_data,
            array_expr.0.pos(),
            array_type,
            &array_expr_type,
            parent_array_var_id,
            &key_values,
            context.inside_general_use,
        );

        let is_first = i == array_exprs.len() - 1;

        if is_first {
            *root_type = array_type;
        } else {
            analysis_data.set_expr_type(array_expr.0.pos(), array_type);
        }

        var_id_additions.pop();
    }

    Ok(last_array_expr_dim)
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
