use std::rc::Rc;

use hakana_code_info::{
    data_flow::{
        graph::{GraphKind, WholeProgramKind},
        node::DataFlowNode,
        path::{ArrayDataKind, PathKind},
    },
    issue::{Issue, IssueKind},
    t_atomic::{DictKey, TAtomic, TDict},
    t_union::TUnion,
};
use hakana_str::StrId;
use hakana_code_info::ttype::{
    add_optional_union_type, add_union_type, get_arraykey, get_int, get_mixed_any,
    get_mixed_maybe_from_loop, get_nothing, get_null, get_string,
    comparison::{type_comparison_result::TypeComparisonResult, union_type_comparator},
};
use oxidized::{aast, ast_defs::Pos};

use crate::{
    expr::expression_identifier, function_analysis_data::FunctionAnalysisData,
    stmt_analyzer::AnalysisError,
};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope::BlockContext, statements_analyzer::StatementsAnalyzer};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&aast::Expr<(), ()>, Option<&aast::Expr<(), ()>>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    keyed_array_var_id: Option<String>,
) -> Result<(), AnalysisError> {
    let extended_var_id = expression_identifier::get_var_id(
        expr.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some((
            statements_analyzer.get_codebase(),
            statements_analyzer.get_interner(),
        )),
    );

    let mut used_key_type;

    if let Some(dim) = expr.1 {
        let was_inside_use = context.inside_general_use;
        context.inside_general_use = true;

        context.inside_unset = false;

        expression_analyzer::analyze(statements_analyzer, dim, analysis_data, context)?;

        context.inside_general_use = was_inside_use;

        used_key_type = if let Some(dim_type) = analysis_data.get_expr_type(dim.pos()) {
            dim_type.clone()
        } else {
            get_arraykey(true)
        };
    } else {
        used_key_type = get_int();
    }

    expression_analyzer::analyze(statements_analyzer, expr.0, analysis_data, context)?;

    if let Some(keyed_array_var_id) = &keyed_array_var_id {
        if context.has_variable(keyed_array_var_id) {
            let mut stmt_type = context.locals.remove(keyed_array_var_id).unwrap();

            add_array_fetch_dataflow_rc(
                statements_analyzer,
                expr.0,
                analysis_data,
                Some(keyed_array_var_id.clone()),
                &mut stmt_type,
                &mut used_key_type,
            );

            analysis_data.set_rc_expr_type(pos, stmt_type.clone());

            context
                .locals
                .insert(keyed_array_var_id.clone(), stmt_type.clone());

            return Ok(());
        }
    }

    let stmt_var_type = analysis_data.get_rc_expr_type(expr.0.pos()).cloned();

    if let Some(stmt_var_type) = stmt_var_type {
        // maybe todo handle access on null

        let mut stmt_type_inner = get_array_access_type_given_offset(
            statements_analyzer,
            analysis_data,
            (expr.0, expr.1, pos),
            &stmt_var_type,
            &used_key_type,
            false,
            &extended_var_id,
            context,
        );

        if let Some(keyed_array_var_id) = &keyed_array_var_id {
            let can_store_result = context.inside_assignment || !stmt_var_type.is_mixed();

            if !context.inside_isset && can_store_result && keyed_array_var_id.contains("[$") {
                context
                    .locals
                    .insert(keyed_array_var_id.clone(), Rc::new(stmt_type_inner.clone()));
            }
        }

        add_array_fetch_dataflow(
            statements_analyzer,
            expr.0.pos(),
            analysis_data,
            keyed_array_var_id.clone(),
            &mut stmt_type_inner,
            &mut used_key_type,
        );

        analysis_data.set_expr_type(pos, stmt_type_inner.clone());
    }

    if let Some(dim_expr) = expr.1 {
        analysis_data.combine_effects(expr.0.pos(), dim_expr.pos(), pos);
    }

    Ok(())
}

/**
 * Used to create a path between a variable $foo and $foo["a"]
 */
pub(crate) fn add_array_fetch_dataflow_rc(
    statements_analyzer: &StatementsAnalyzer,
    array_expr: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    keyed_array_var_id: Option<String>,
    value_type: &mut Rc<TUnion>,
    key_type: &mut TUnion,
) {
    let value_type_inner = Rc::make_mut(value_type);
    add_array_fetch_dataflow(
        statements_analyzer,
        array_expr.pos(),
        analysis_data,
        keyed_array_var_id,
        value_type_inner,
        key_type,
    );
}

/**
 * Used to create a path between a variable $foo and $foo["a"]
 */
pub(crate) fn add_array_fetch_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    array_expr_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    keyed_array_var_id: Option<String>,
    value_type: &mut TUnion,
    key_type: &mut TUnion,
) {
    if let GraphKind::WholeProgram(WholeProgramKind::Taint) = &analysis_data.data_flow_graph.kind {
        if !value_type.has_taintable_value() {
            return;
        }
    }

    if let Some(stmt_var_type) = analysis_data.expr_types.get(&(
        array_expr_pos.start_offset() as u32,
        array_expr_pos.end_offset() as u32,
    )) {
        if !stmt_var_type.parent_nodes.is_empty() {
            // TODO Add events dispatchers

            let node_name = if let Some(keyed_array_var_id) = &keyed_array_var_id {
                keyed_array_var_id.clone()
            } else {
                "arrayvalue-fetch".to_string()
            };
            let new_parent_node = DataFlowNode::get_for_local_string(
                node_name,
                statements_analyzer.get_hpos(array_expr_pos),
            );
            analysis_data
                .data_flow_graph
                .add_node(new_parent_node.clone());

            let key_type_single = if key_type.is_single() {
                Some(key_type.get_single())
            } else {
                None
            };

            let dim_value = if let Some(key_type_single) = key_type_single {
                if let TAtomic::TLiteralString { value, .. } = key_type_single {
                    Some(value.clone())
                } else if let TAtomic::TLiteralInt { value, .. } = key_type_single {
                    Some(value.to_string())
                } else {
                    None
                }
            } else {
                None
            };

            let mut array_key_node = None;

            if keyed_array_var_id.is_none() && dim_value.is_none() {
                let fetch_node = DataFlowNode::get_for_local_string(
                    "arraykey-fetch".to_string(),
                    statements_analyzer.get_hpos(array_expr_pos),
                );
                analysis_data.data_flow_graph.add_node(fetch_node.clone());
                array_key_node = Some(fetch_node);
                analysis_data
                    .data_flow_graph
                    .add_node(new_parent_node.clone());
            }

            for parent_node in stmt_var_type.parent_nodes.iter() {
                analysis_data.data_flow_graph.add_path(
                    parent_node,
                    &new_parent_node,
                    if let Some(dim_value) = dim_value.clone() {
                        PathKind::ArrayFetch(ArrayDataKind::ArrayValue, dim_value.to_string())
                    } else {
                        PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)
                    },
                    vec![],
                    vec![],
                );

                if let Some(array_key_node) = array_key_node.clone() {
                    analysis_data.data_flow_graph.add_path(
                        parent_node,
                        &array_key_node,
                        PathKind::UnknownArrayFetch(ArrayDataKind::ArrayKey),
                        vec![],
                        vec![],
                    );
                }
            }

            value_type.parent_nodes.push(new_parent_node.clone());

            if let Some(array_key_node) = &array_key_node {
                key_type.parent_nodes.push(array_key_node.clone());
            }
        }
    }
}

/**
 * Complex Method to be refactored.
 * Good type/bad type behaviour could be mutualised with ArrayAnalyzer
 */
pub(crate) fn get_array_access_type_given_offset(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    stmt: (&aast::Expr<(), ()>, Option<&aast::Expr<(), ()>>, &Pos),
    array_type: &TUnion,
    offset_type: &TUnion,
    in_assignment: bool,
    extended_var_id: &Option<String>,
    context: &BlockContext,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let mut has_valid_expected_offset = false;

    if offset_type.is_null() {
        // TODO append issue
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NullArrayOffset,
                format!(
                    "Cannot access value on variable {} using null offset",
                    extended_var_id.clone().unwrap_or("".to_string())
                ),
                statements_analyzer.get_hpos(stmt.2),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    if offset_type.is_nullable() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::PossiblyNullArrayOffset,
                format!(
                    "Cannot access value on variable {} using nullable offset",
                    extended_var_id.clone().unwrap_or("".to_string())
                ),
                statements_analyzer.get_hpos(stmt.2),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let mut array_atomic_types = array_type.types.iter().collect::<Vec<_>>();

    let mut stmt_type = None;

    while let Some(mut atomic_var_type) = array_atomic_types.pop() {
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. }
        | TAtomic::TTypeAlias {
            as_type: Some(as_type),
            ..
        } = atomic_var_type
        {
            array_atomic_types.extend(&as_type.types);
            continue;
        }

        match atomic_var_type {
            TAtomic::TKeyset { .. } | TAtomic::TVec { .. } => {
                let new_type = handle_array_access_on_vec(
                    statements_analyzer,
                    stmt.2,
                    analysis_data,
                    context,
                    atomic_var_type.clone(),
                    offset_type.clone(),
                    in_assignment,
                    &mut has_valid_expected_offset,
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(existing_type, &new_type, codebase, false));
                } else {
                    stmt_type = Some(new_type);
                }
            }
            TAtomic::TDict(TDict { .. }) => {
                let new_type = handle_array_access_on_dict(
                    statements_analyzer,
                    stmt.2,
                    analysis_data,
                    context,
                    atomic_var_type,
                    offset_type,
                    in_assignment,
                    &mut has_valid_expected_offset,
                    context.inside_isset || context.inside_unset,
                    &mut false,
                    &mut false,
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(existing_type, &new_type, codebase, false));
                } else {
                    stmt_type = Some(new_type);
                }
            }
            TAtomic::TString | TAtomic::TStringWithFlags(..) | TAtomic::TLiteralString { .. } => {
                let new_type = handle_array_access_on_string(
                    statements_analyzer,
                    atomic_var_type.clone(),
                    offset_type.clone(),
                    &mut Vec::new(),
                    &mut has_valid_expected_offset,
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(existing_type, &new_type, codebase, false));
                } else {
                    stmt_type = Some(new_type);
                }
            }
            TAtomic::TMixed
            | TAtomic::TMixedWithFlags(true, ..)
            | TAtomic::TMixedWithFlags(_, true, ..)
            | TAtomic::TMixedWithFlags(_, _, _, true)
            | TAtomic::TNothing => {
                let new_type = handle_array_access_on_mixed(
                    statements_analyzer,
                    stmt.2,
                    analysis_data,
                    context,
                    atomic_var_type,
                    array_type,
                    stmt_type.clone(),
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(existing_type, &new_type, codebase, false));
                } else {
                    stmt_type = Some(new_type);
                }

                has_valid_expected_offset = true;
            }
            TAtomic::TNull => {
                if in_assignment {
                } else {
                    if !context.inside_isset {
                        analysis_data.maybe_add_issue(
                            Issue::new(
                                IssueKind::PossiblyNullArrayAccess,
                                "Unsafe array access on null".to_string(),
                                statements_analyzer.get_hpos(stmt.0.pos()),
                                &context.function_context.calling_functionlike_id,
                            ),
                            statements_analyzer.get_config(),
                            statements_analyzer.get_file_path_actual(),
                        );
                    }

                    stmt_type = Some(add_optional_union_type(
                        get_null(),
                        stmt_type.as_ref(),
                        codebase,
                    ));
                }

                has_valid_expected_offset = true;
            }
            TAtomic::TNamedObject {
                name, type_params, ..
            } => match *name {
                StrId::KEYED_CONTAINER | StrId::ANY_ARRAY => {
                    if let Some(type_params) = type_params {
                        if let Some(existing_type) = stmt_type {
                            stmt_type = Some(add_union_type(
                                existing_type,
                                type_params.get(1).unwrap(),
                                codebase,
                                false,
                            ));
                        } else {
                            stmt_type = Some(type_params.get(1).unwrap().clone());
                        }

                        has_valid_expected_offset = true;
                    }
                }
                StrId::CONTAINER => {
                    if let Some(type_params) = type_params {
                        if let Some(existing_type) = stmt_type {
                            stmt_type = Some(add_union_type(
                                existing_type,
                                type_params.first().unwrap(),
                                codebase,
                                false,
                            ));
                        } else {
                            stmt_type = Some(type_params.first().unwrap().clone());
                        }

                        has_valid_expected_offset = true;
                    }
                }
                StrId::XHP_CHILD => {
                    let new_type = handle_array_access_on_mixed(
                        statements_analyzer,
                        stmt.2,
                        analysis_data,
                        context,
                        atomic_var_type,
                        array_type,
                        stmt_type.clone(),
                    );

                    if let Some(existing_type) = stmt_type {
                        stmt_type = Some(add_union_type(existing_type, &new_type, codebase, false));
                    } else {
                        stmt_type = Some(new_type);
                    }

                    has_valid_expected_offset = true;
                }
                _ => {}
            },
            _ => {
                has_valid_expected_offset = true;
            }
        }
    }

    if !has_valid_expected_offset {
        let mut mixed_with_any = false;
        if offset_type.is_mixed_with_any(&mut mixed_with_any) {
            for origin in &offset_type.parent_nodes {
                analysis_data.data_flow_graph.add_mixed_data(origin, stmt.2);
            }

            analysis_data.maybe_add_issue(
                Issue::new(
                    if mixed_with_any {
                        IssueKind::MixedAnyArrayOffset
                    } else {
                        IssueKind::MixedArrayOffset
                    },
                    format!(
                        "Invalid array fetch on {} using offset {}",
                        array_type.get_id(Some(statements_analyzer.get_interner())),
                        offset_type.get_id(Some(statements_analyzer.get_interner()))
                    ),
                    statements_analyzer.get_hpos(stmt.2),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        } else {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::InvalidArrayOffset,
                    format!(
                        "Invalid array fetch on {} using offset {}",
                        array_type.get_id(Some(statements_analyzer.get_interner())),
                        offset_type.get_id(Some(statements_analyzer.get_interner()))
                    ),
                    statements_analyzer.get_hpos(stmt.2),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    // TODO handle if ($offset_type->hasMixed()), and incrementing mixed
    // nonmixed counts, as well as error message handling

    let array_access_type = stmt_type;
    if let Some(array_access_type) = array_access_type {
        array_access_type
    } else {
        // shouldn’t happen, but don’t crash
        get_mixed_any()
    }
}

// Handle array access on vec-list collections
pub(crate) fn handle_array_access_on_vec(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    vec: TAtomic,
    dim_type: TUnion,
    in_assignment: bool,
    has_valid_expected_offset: &mut bool,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let mut union_comparison_result = TypeComparisonResult::new();
    let offset_type_contained_by_expected = union_type_comparator::is_contained_by(
        codebase,
        &dim_type,
        &get_int(),
        false,
        false,
        false,
        &mut union_comparison_result,
    );

    if offset_type_contained_by_expected {
        *has_valid_expected_offset = true;
    }

    if let TAtomic::TVec {
        known_items: Some(known_items),
        type_param,
        ..
    } = vec.clone()
    {
        let type_param = *type_param;
        if let Some(val) = dim_type.get_single_literal_int_value() {
            let index = val as usize;

            if let Some((actual_possibly_undefined, actual_value)) = known_items.get(&index) {
                *has_valid_expected_offset = true;
                // we know exactly which item we are fetching

                if *actual_possibly_undefined
                    && !context.inside_isset
                    && !context.inside_unset
                    && !in_assignment
                {
                    // oh no!
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::PossiblyUndefinedIntArrayOffset,
                            format!(
                                "Fetch on {} using possibly-undefined key {}",
                                vec.get_id(Some(statements_analyzer.get_interner())),
                                val
                            ),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }

                return actual_value.clone();
            }

            if !in_assignment {
                if type_param.is_nothing() {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::UndefinedIntArrayOffset,
                            format!(
                                "Invalid vec fetch on {} using offset {}",
                                vec.get_id(Some(statements_analyzer.get_interner())),
                                index
                            ),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }

                return type_param.clone();
            }
        }

        let mut type_param = type_param;

        for (_, (_, known_item)) in known_items {
            type_param = add_union_type(type_param, &known_item, codebase, false);
        }

        return type_param;
    } else if let TAtomic::TVec { type_param, .. } = vec {
        return *type_param;
    } else if let TAtomic::TKeyset { type_param, .. } = vec {
        return *type_param;
    }

    get_nothing()
}

// Handle array access on dict-like collections
pub(crate) fn handle_array_access_on_dict(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    dict: &TAtomic,
    dim_type: &TUnion,
    in_assignment: bool,
    has_valid_expected_offset: &mut bool,
    allow_possibly_undefined: bool,
    has_possibly_undefined: &mut bool,
    has_matching_dict_key: &mut bool,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let key_param = if in_assignment || context.inside_isset {
        get_arraykey(false)
    } else if let TAtomic::TDict(TDict { params, .. }) = &dict {
        if let Some(params) = params {
            (*params.0).clone()
        } else {
            get_nothing()
        }
    } else {
        panic!()
    };

    let mut union_comparison_result = TypeComparisonResult::new();
    let offset_type_contained_by_expected = union_type_comparator::is_contained_by(
        codebase,
        dim_type,
        &key_param,
        false,
        false,
        false,
        &mut union_comparison_result,
    );

    if offset_type_contained_by_expected {
        *has_valid_expected_offset = true;
    }

    if let TAtomic::TDict(TDict {
        known_items: Some(known_items),
        params,
        ..
    }) = &dict
    {
        if let Some(dict_key) = dim_type.get_single_dict_key() {
            let possible_value = known_items.get(&dict_key).cloned();
            if let Some((actual_possibly_undefined, actual_value)) = possible_value {
                *has_valid_expected_offset = true;
                *has_matching_dict_key = true;
                // we know exactly which item we are fetching

                let expr_type = (*actual_value).clone();

                if actual_possibly_undefined && !in_assignment {
                    if !allow_possibly_undefined {
                        // oh no!
                        analysis_data.maybe_add_issue(
                            Issue::new(
                                match &dict_key {
                                    DictKey::Int(_) => IssueKind::PossiblyUndefinedIntArrayOffset,
                                    _ => IssueKind::PossiblyUndefinedStringArrayOffset,
                                },
                                format!(
                                    "Fetch on {} using possibly-undefined key {}",
                                    dict.get_id(Some(statements_analyzer.get_interner())),
                                    dict_key.to_string(Some(statements_analyzer.get_interner()))
                                ),
                                statements_analyzer.get_hpos(pos),
                                &context.function_context.calling_functionlike_id,
                            ),
                            statements_analyzer.get_config(),
                            statements_analyzer.get_file_path_actual(),
                        );
                    } else {
                        *has_possibly_undefined = true;
                    }
                }

                return expr_type;
            }

            if !in_assignment {
                if let Some(params) = params {
                    return (*params.1).clone();
                }

                if !context.inside_isset {
                    // oh no!
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::UndefinedStringArrayOffset,
                            format!(
                                "Invalid dict fetch on {} using key {}",
                                dict.get_id(Some(statements_analyzer.get_interner())),
                                dict_key.to_string(Some(statements_analyzer.get_interner()))
                            ),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                } else {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::ImpossibleNonnullEntryCheck,
                            format!(
                                "Type {} does not have a nonnull entry for {}",
                                dict.get_id(Some(statements_analyzer.get_interner())),
                                dict_key.to_string(Some(statements_analyzer.get_interner()))
                            ),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }

                // since we're emitting a very specific error
                // we don't want to emit another error afterwards
                *has_valid_expected_offset = true;

                return get_nothing();
            }
        }

        let mut value_param = if let Some(params) = params {
            (*params.1).clone()
        } else {
            get_nothing()
        };

        for (_, known_item) in known_items.values() {
            value_param = add_union_type(value_param, known_item, codebase, false);
        }

        let mut union_comparison_result = TypeComparisonResult::new();

        let array_key = get_arraykey(false);

        let is_contained = union_type_comparator::is_contained_by(
            codebase,
            &key_param,
            if dim_type.is_mixed() {
                &array_key
            } else {
                dim_type
            },
            true,
            value_param.ignore_falsable_issues,
            false,
            &mut union_comparison_result,
        );

        if is_contained {
            *has_valid_expected_offset = true;
        }

        return value_param;
    } else if let TAtomic::TDict(TDict { params, .. }) = dict {
        // TODO Handle Assignments
        // if (context.inside_assignment && replacement_type) {

        // }
        return if let Some(params) = params {
            if let Some(dict_key) = dim_type.get_single_dict_key() {
                if !in_assignment {
                    if !allow_possibly_undefined {
                        // oh no!
                        analysis_data.maybe_add_issue(
                            Issue::new(
                                match &dict_key {
                                    DictKey::Int(_) => IssueKind::PossiblyUndefinedIntArrayOffset,
                                    _ => IssueKind::PossiblyUndefinedStringArrayOffset,
                                },
                                format!(
                                    "Fetch on {} using possibly-undefined key {}",
                                    dict.get_id(Some(statements_analyzer.get_interner())),
                                    dict_key.to_string(Some(statements_analyzer.get_interner()))
                                ),
                                statements_analyzer.get_hpos(pos),
                                &context.function_context.calling_functionlike_id,
                            ),
                            statements_analyzer.get_config(),
                            statements_analyzer.get_file_path_actual(),
                        );
                    } else {
                        *has_possibly_undefined = true;
                    }
                }
            }

            (*params.1).clone()
        } else {
            get_nothing()
        };
    }

    get_nothing()
}

// Handle array access on strings
pub(crate) fn handle_array_access_on_string(
    statements_analyzer: &StatementsAnalyzer,
    string: TAtomic,
    dim_type: TUnion,
    expected_offset_types: &mut Vec<String>,
    has_valid_expected_offset: &mut bool,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();
    let valid_offset_type = if let TAtomic::TLiteralString { value: val, .. } = string {
        if val.is_empty() {
            // empty strings cannt be array accessed
            get_nothing()
        } else if val.len() < 10 {
            let mut valid_offsets = Vec::new();
            let neg = -(val.len() as i64);
            let top = val.len() as i64;
            for n in neg..top {
                valid_offsets.push(TAtomic::TLiteralInt { value: n });
            }

            if valid_offsets.is_empty() {
                // this is weird
            }
            TUnion::new(valid_offsets)
        } else {
            get_int()
        }
    } else {
        get_int()
    };

    if !union_type_comparator::is_contained_by(
        codebase,
        &dim_type,
        &valid_offset_type,
        false,
        false,
        false,
        &mut TypeComparisonResult::new(),
    ) {
        expected_offset_types
            .push(valid_offset_type.get_id(Some(statements_analyzer.get_interner())));

        TUnion::new(vec![TAtomic::TString, TAtomic::TNull])
    } else {
        *has_valid_expected_offset = true;
        get_string()
    }
}

// Handle array access on mixed arrays (not properly typed)
pub(crate) fn handle_array_access_on_mixed(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    mixed: &TAtomic,
    mixed_union: &TUnion,
    stmt_type: Option<TUnion>,
) -> TUnion {
    if !context.inside_isset {
        for origin in &mixed_union.parent_nodes {
            analysis_data.data_flow_graph.add_mixed_data(origin, pos);
        }

        if context.inside_assignment {
            // oh no!
            analysis_data.maybe_add_issue(
                Issue::new(
                    if let TAtomic::TMixedWithFlags(true, ..) = mixed {
                        IssueKind::MixedAnyArrayAssignment
                    } else if let TAtomic::TNothing = mixed {
                        IssueKind::ImpossibleArrayAssignment
                    } else {
                        IssueKind::MixedArrayAssignment
                    },
                    format!(
                        "Unsafe array assignment on value with type {}",
                        mixed.get_id(Some(statements_analyzer.get_interner()))
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        } else {
            // oh no!
            analysis_data.maybe_add_issue(
                Issue::new(
                    if let TAtomic::TMixedWithFlags(true, ..) = mixed {
                        IssueKind::MixedAnyArrayAccess
                    } else {
                        IssueKind::MixedArrayAccess
                    },
                    format!(
                        "Unsafe array access on value with type {}",
                        mixed.get_id(None)
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    if let Some(stmt_var_type) = analysis_data
        .expr_types
        .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
    {
        if !stmt_var_type.parent_nodes.is_empty() {
            let new_parent_node = DataFlowNode::get_for_local_string(
                "mixed-var-array-access".to_string(),
                statements_analyzer.get_hpos(pos),
            );
            analysis_data
                .data_flow_graph
                .add_node(new_parent_node.clone());

            for parent_node in stmt_var_type.parent_nodes.iter() {
                analysis_data.data_flow_graph.add_path(
                    parent_node,
                    &new_parent_node,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
            if let Some(stmt_type) = stmt_type {
                let mut stmt_type_new = stmt_type.clone();
                stmt_type_new.parent_nodes = vec![new_parent_node.clone()];
            }
        }
    }

    if let TAtomic::TNothing = mixed {
        return get_mixed_maybe_from_loop(true);
    }

    get_mixed_any()
}
