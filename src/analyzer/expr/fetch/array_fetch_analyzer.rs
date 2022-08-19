use std::rc::Rc;

use hakana_reflection_info::{
    data_flow::{
        graph::GraphKind,
        node::DataFlowNode,
        path::{PathExpressionKind, PathKind},
    },
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_type::{
    add_optional_union_type, add_union_type, get_arraykey, get_int, get_mixed_any,
    get_mixed_maybe_from_loop, get_nothing, get_null, get_string,
    type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator},
};
use oxidized::{aast, ast_defs::Pos};
use rustc_hash::FxHashMap;

use crate::{expr::expression_identifier, typed_ast::TastInfo};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&aast::Expr<(), ()>, Option<&aast::Expr<(), ()>>),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    keyed_array_var_id: Option<String>,
) -> bool {
    let extended_var_id = expression_identifier::get_extended_var_id(
        &expr.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
    );

    let mut used_key_type;

    if let Some(dim) = expr.1 {
        let was_inside_general_use = context.inside_general_use;
        context.inside_general_use = true;

        let was_inside_unset = context.inside_unset;
        context.inside_unset = false;

        let analysis_result =
            expression_analyzer::analyze(statements_analyzer, dim, tast_info, context, &mut None);
        context.inside_general_use = was_inside_general_use;
        context.inside_unset = was_inside_unset;

        if !analysis_result {
            return false;
        }

        used_key_type = if let Some(dim_type) = tast_info.get_expr_type(dim.pos()) {
            dim_type.clone()
        } else {
            get_arraykey()
        };
    } else {
        used_key_type = get_int();
    }

    if !expression_analyzer::analyze(statements_analyzer, expr.0, tast_info, context, &mut None) {
        return false;
    }

    if let Some(keyed_array_var_id) = &keyed_array_var_id {
        if context.has_variable(keyed_array_var_id) {
            let mut stmt_type = context.vars_in_scope.remove(keyed_array_var_id).unwrap();

            add_array_fetch_dataflow_rc(
                statements_analyzer,
                expr.0,
                tast_info,
                Some(keyed_array_var_id.clone()),
                &mut stmt_type,
                &mut used_key_type,
            );

            tast_info.set_rc_expr_type(pos, stmt_type.clone());

            context
                .vars_in_scope
                .insert(keyed_array_var_id.clone(), stmt_type.clone());

            tast_info
                .pure_exprs
                .insert((pos.start_offset(), pos.end_offset()));

            return true;
        }
    }

    let stmt_var_type = tast_info.get_expr_type(expr.0.pos()).cloned();

    if let Some(stmt_var_type) = stmt_var_type {
        // maybe todo handle access on null

        let stmt_type = Some(get_array_access_type_given_offset(
            statements_analyzer,
            tast_info,
            (expr.0, expr.1, pos),
            stmt_var_type.clone(),
            &used_key_type,
            false,
            &extended_var_id,
            context,
        ));

        if let Some(mut stmt_type) = stmt_type.clone() {
            if let Some(keyed_array_var_id) = &keyed_array_var_id {
                let can_store_result = context.inside_assignment || !stmt_var_type.is_mixed();

                if !context.inside_isset && can_store_result {
                    context
                        .vars_in_scope
                        .insert(keyed_array_var_id.clone(), Rc::new(stmt_type.clone()));
                }
            }

            add_array_fetch_dataflow(
                statements_analyzer,
                expr.0,
                tast_info,
                keyed_array_var_id.clone(),
                &mut stmt_type,
                &mut used_key_type,
            );

            tast_info.set_expr_type(&pos, stmt_type.clone());
        }
    }

    if tast_info
        .pure_exprs
        .contains(&(expr.0.pos().start_offset(), expr.0.pos().end_offset()))
        && tast_info.pure_exprs.contains(&(
            expr.1.as_ref().unwrap().pos().start_offset(),
            expr.1.as_ref().unwrap().pos().end_offset(),
        ))
    {
        tast_info
            .pure_exprs
            .insert((pos.start_offset(), pos.end_offset()));
    }

    true
}

/**
 * Used to create a path between a variable $foo and $foo["a"]
 */
pub(crate) fn add_array_fetch_dataflow_rc(
    statements_analyzer: &StatementsAnalyzer,
    array_expr: &aast::Expr<(), ()>,
    tast_info: &mut TastInfo,
    keyed_array_var_id: Option<String>,
    value_type: &mut Rc<TUnion>,
    key_type: &mut TUnion,
) {
    let value_type_inner = Rc::make_mut(value_type);
    add_array_fetch_dataflow(
        statements_analyzer,
        array_expr,
        tast_info,
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
    array_expr: &aast::Expr<(), ()>,
    tast_info: &mut TastInfo,
    keyed_array_var_id: Option<String>,
    value_type: &mut TUnion,
    key_type: &mut TUnion,
) {
    if tast_info.data_flow_graph.kind == GraphKind::WholeProgram {
        if !value_type.has_taintable_value() {
            return;
        }
    }

    if let Some(stmt_var_type) = tast_info
        .expr_types
        .get(&(array_expr.1.start_offset(), array_expr.1.end_offset()))
    {
        if !stmt_var_type.parent_nodes.is_empty() {
            // TODO Add events dispatchers

            let node_name = if let Some(keyed_array_var_id) = &keyed_array_var_id {
                keyed_array_var_id.clone()
            } else {
                "arrayvalue-fetch".to_string()
            };
            let new_parent_node = DataFlowNode::get_for_assignment(
                node_name,
                statements_analyzer.get_hpos(array_expr.pos()),
            );
            tast_info.data_flow_graph.add_node(new_parent_node.clone());

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

            if let None = keyed_array_var_id {
                if let None = dim_value {
                    let fetch_node = DataFlowNode::get_for_assignment(
                        "arraykey-fetch".to_string(),
                        statements_analyzer.get_hpos(array_expr.pos()),
                    );
                    tast_info.data_flow_graph.add_node(fetch_node.clone());
                    array_key_node = Some(fetch_node);
                    tast_info.data_flow_graph.add_node(new_parent_node.clone());
                }
            }

            for (_, parent_node) in stmt_var_type.parent_nodes.iter() {
                tast_info.data_flow_graph.add_path(
                    parent_node,
                    &new_parent_node,
                    if let Some(dim_value) = dim_value.clone() {
                        PathKind::ExpressionFetch(
                            PathExpressionKind::ArrayValue,
                            dim_value.to_string(),
                        )
                    } else {
                        PathKind::UnknownExpressionFetch(PathExpressionKind::ArrayValue)
                    },
                    None,
                    None,
                );

                if let Some(array_key_node) = array_key_node.clone() {
                    tast_info.data_flow_graph.add_path(
                        parent_node,
                        &array_key_node,
                        PathKind::UnknownExpressionFetch(PathExpressionKind::ArrayKey),
                        None,
                        None,
                    );
                }
            }

            value_type
                .parent_nodes
                .insert(new_parent_node.get_id().clone(), new_parent_node.clone());

            if let Some(array_key_node) = &array_key_node {
                key_type
                    .parent_nodes
                    .insert(array_key_node.get_id().clone(), array_key_node.clone());
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
    tast_info: &mut TastInfo,
    stmt: (&aast::Expr<(), ()>, Option<&aast::Expr<(), ()>>, &Pos),
    array_type: TUnion,
    offset_type: &TUnion,
    in_assignment: bool,
    extended_var_id: &Option<String>,
    context: &mut ScopeContext,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let mut has_valid_expected_offset = false;

    if offset_type.is_null() {
        // TODO append issue
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NullArrayOffset,
                format!(
                    "Cannot access value on variable {} using null offset",
                    extended_var_id.clone().unwrap_or("".to_string())
                ),
                statements_analyzer.get_hpos(&stmt.2),
            ),
            statements_analyzer.get_config(),
        );
    }

    if offset_type.is_nullable() {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::PossiblyNullArrayOffset,
                format!(
                    "Cannot access value on variable {} using nullable offset",
                    extended_var_id.clone().unwrap_or("".to_string())
                ),
                statements_analyzer.get_hpos(&stmt.2),
            ),
            statements_analyzer.get_config(),
        );
    }

    let array_atomic_types = &array_type.types;

    let mut stmt_type = None;

    for (_, atomic_var_type) in array_atomic_types {
        match atomic_var_type {
            TAtomic::TKeyset { .. } | TAtomic::TVec { .. } => {
                let new_type = handle_array_access_on_vec(
                    statements_analyzer,
                    stmt.2,
                    tast_info,
                    context,
                    atomic_var_type.clone(),
                    offset_type.clone(),
                    in_assignment,
                    &mut has_valid_expected_offset,
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(
                        existing_type,
                        &new_type,
                        Some(codebase),
                        false,
                    ));
                } else {
                    stmt_type = Some(new_type);
                }
            }
            TAtomic::TDict { .. } => {
                let new_type = handle_array_access_on_dict(
                    statements_analyzer,
                    stmt.2,
                    tast_info,
                    context,
                    atomic_var_type,
                    offset_type,
                    in_assignment,
                    &mut has_valid_expected_offset,
                    context.inside_isset,
                    &mut false,
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(
                        existing_type,
                        &new_type,
                        Some(codebase),
                        false,
                    ));
                } else {
                    stmt_type = Some(new_type);
                }
            }
            TAtomic::TString { .. } | TAtomic::TLiteralString { .. } => {
                let new_type = handle_array_access_on_string(
                    statements_analyzer,
                    atomic_var_type.clone(),
                    offset_type.clone(),
                    &mut Vec::new(),
                    &mut has_valid_expected_offset,
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(
                        existing_type,
                        &new_type,
                        Some(codebase),
                        false,
                    ));
                } else {
                    stmt_type = Some(new_type);
                }
            }
            TAtomic::TTemplateParam { .. }
            | TAtomic::TMixed
            | TAtomic::TMixedAny
            | TAtomic::TTruthyMixed
            | TAtomic::TNothing
            | TAtomic::TNonnullMixed => {
                let new_type = handle_array_access_on_mixed(
                    statements_analyzer,
                    stmt.2,
                    tast_info,
                    context,
                    atomic_var_type,
                    &array_type,
                    stmt_type.clone(),
                );

                if let Some(existing_type) = stmt_type {
                    stmt_type = Some(add_union_type(
                        existing_type,
                        &new_type,
                        Some(codebase),
                        false,
                    ));
                } else {
                    stmt_type = Some(new_type);
                }

                has_valid_expected_offset = true;
            }
            TAtomic::TNull => {
                if in_assignment {
                } else {
                    if !context.inside_isset {
                        // error if not nullsafe
                    }

                    stmt_type = Some(add_optional_union_type(
                        get_null(),
                        stmt_type.as_ref(),
                        Some(codebase),
                    ));
                }

                has_valid_expected_offset = true;
            }
            TAtomic::TNamedObject {
                name,
                type_params: Some(type_params),
                ..
            } => {
                if name == "HH\\KeyedContainer" {
                    if let Some(existing_type) = stmt_type {
                        stmt_type = Some(add_union_type(
                            existing_type,
                            &type_params.get(1).unwrap(),
                            Some(codebase),
                            false,
                        ));
                    } else {
                        stmt_type = Some(type_params.get(1).unwrap().clone());
                    }

                    has_valid_expected_offset = true;
                } else if name == "HH\\Container" {
                    if let Some(existing_type) = stmt_type {
                        stmt_type = Some(add_union_type(
                            existing_type,
                            &type_params.get(0).unwrap(),
                            Some(codebase),
                            false,
                        ));
                    } else {
                        stmt_type = Some(type_params.get(0).unwrap().clone());
                    }

                    has_valid_expected_offset = true;
                }
            }
            _ => {
                has_valid_expected_offset = true;
            }
        }
    }

    if !has_valid_expected_offset {
        let mut mixed_with_any = false;
        if offset_type.is_mixed_with_any(&mut mixed_with_any) {
            for (_, origin) in &offset_type.parent_nodes {
                tast_info.data_flow_graph.add_mixed_data(origin, stmt.2);
            }

            tast_info.maybe_add_issue(
                Issue::new(
                    if mixed_with_any {
                        IssueKind::MixedAnyArrayOffset
                    } else {
                        IssueKind::MixedArrayOffset
                    },
                    format!(
                        "Invalid array fetch on {} using offset {}",
                        array_type.get_id(),
                        offset_type.get_id()
                    ),
                    statements_analyzer.get_hpos(&stmt.2),
                ),
                statements_analyzer.get_config(),
            );
        } else {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::InvalidArrayOffset,
                    format!(
                        "Invalid array fetch on {} using offset {}",
                        array_type.get_id(),
                        offset_type.get_id()
                    ),
                    statements_analyzer.get_hpos(&stmt.2),
                ),
                statements_analyzer.get_config(),
            );
        }
    }

    // TODO handle if ($offset_type->hasMixed()), and incrementing mixed
    // nonmixed counts, as well as error message handling

    let array_access_type = stmt_type;
    if let Some(array_access_type) = array_access_type {
        if context.inside_assignment {
            // does not do anything right now
            // array_type.bust_cache();
        }

        return array_access_type;
    } else {
        // shouldn’t happen, but don’t crash
        return get_mixed_any();
    }
}

// Handle array access on vec-list collections
pub(crate) fn handle_array_access_on_vec(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
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
        if let Some(val) = dim_type.get_single_literal_int_value() {
            let index = val as usize;

            if let Some((actual_possibly_undefined, actual_value)) = known_items.get(&index) {
                *has_valid_expected_offset = true;
                // we know exactly which item we are fetching

                if *actual_possibly_undefined && !context.inside_isset && !in_assignment {
                    // oh no!
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::PossiblyUndefinedIntArrayOffset,
                            format!(
                                "Fetch on {} using possibly-undefined key {}",
                                vec.get_id(),
                                val
                            ),
                            statements_analyzer.get_hpos(&pos),
                        ),
                        statements_analyzer.get_config(),
                    );
                }

                return actual_value.clone();
            }

            if !in_assignment {
                if type_param.is_nothing() {
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::UndefinedIntArrayOffset,
                            format!(
                                "Invalid vec fetch on {} using offset {}",
                                vec.get_id(),
                                index.to_string()
                            ),
                            statements_analyzer.get_hpos(&pos),
                        ),
                        statements_analyzer.get_config(),
                    );
                }

                return type_param.clone();
            }
        }

        let mut type_param = type_param;

        for (_, (_, known_item)) in known_items {
            type_param = add_union_type(type_param, &known_item, Some(codebase), false);
        }

        return type_param;
    } else if let TAtomic::TVec { type_param, .. } = vec {
        return type_param;
    } else if let TAtomic::TKeyset { type_param, .. } = vec {
        return type_param;
    }

    return get_nothing();
}

// Handle array access on dict-like collections
pub(crate) fn handle_array_access_on_dict(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    dict: &TAtomic,
    dim_type: &TUnion,
    in_assignment: bool,
    has_valid_expected_offset: &mut bool,
    allow_possibly_undefined: bool,
    has_possibly_undefined: &mut bool,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();

    let key_param = if in_assignment || context.inside_isset {
        get_arraykey()
    } else {
        if let TAtomic::TDict { key_param, .. } = &dict {
            key_param.clone()
        } else {
            panic!()
        }
    };

    let mut union_comparison_result = TypeComparisonResult::new();
    let offset_type_contained_by_expected = union_type_comparator::is_contained_by(
        codebase,
        &dim_type,
        &key_param,
        false,
        false,
        false,
        &mut union_comparison_result,
    );

    if offset_type_contained_by_expected {
        *has_valid_expected_offset = true;
    }

    if let TAtomic::TDict {
        known_items: Some(known_items),
        value_param,
        ..
    } = &dict
    {
        if let Some(val) = dim_type.get_single_literal_string_value() {
            let possible_value = known_items.get(&val).cloned();
            if let Some((actual_possibly_undefined, actual_value)) = possible_value {
                *has_valid_expected_offset = true;
                // we know exactly which item we are fetching

                let expr_type = (*actual_value).clone();

                if actual_possibly_undefined && !in_assignment {
                    if !allow_possibly_undefined {
                        // oh no!
                        tast_info.maybe_add_issue(
                            Issue::new(
                                IssueKind::PossiblyUndefinedStringArrayOffset,
                                format!(
                                    "Fetch on {} using possibly-undefined key '{}'",
                                    dict.get_id(),
                                    val
                                ),
                                statements_analyzer.get_hpos(&pos),
                            ),
                            statements_analyzer.get_config(),
                        );
                    } else {
                        *has_possibly_undefined = true;
                    }
                }

                return expr_type;
            }

            if !in_assignment {
                if value_param.is_nothing() {
                    // oh no!
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::UndefinedStringArrayOffset,
                            format!(
                                "Invalid dict fetch on {} using key '{}'",
                                dict.get_id(),
                                val
                            ),
                            statements_analyzer.get_hpos(&pos),
                        ),
                        statements_analyzer.get_config(),
                    );
                }

                return value_param.clone();
            }
        }

        let mut value_param = value_param.clone();

        for (_, (_, known_item)) in known_items {
            value_param = add_union_type(value_param, &known_item, Some(codebase), false);
        }

        let mut union_comparison_result = TypeComparisonResult::new();

        let array_key = get_arraykey();

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
    } else if let TAtomic::TDict { value_param, .. } = dict {
        // TODO Handle Assignments
        // if (context.inside_assignment && replacement_type) {

        // }
        return value_param.clone();
    }

    return get_nothing();
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
        expected_offset_types.push(valid_offset_type.get_id());
        let mut result = get_string();
        result.add_type(TAtomic::TNull);
        return result;
    } else {
        *has_valid_expected_offset = true;
        return get_string();
    }
}

// Handle array access on mixed arrays (not properly typed)
pub(crate) fn handle_array_access_on_mixed(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    mixed: &TAtomic,
    mixed_union: &TUnion,
    stmt_type: Option<TUnion>,
) -> TUnion {
    if !context.inside_isset {
        for (_, origin) in &mixed_union.parent_nodes {
            tast_info.data_flow_graph.add_mixed_data(origin, pos);
        }

        if context.inside_assignment {
            // oh no!
            tast_info.maybe_add_issue(
                Issue::new(
                    if let TAtomic::TMixedAny = mixed {
                        IssueKind::MixedAnyArrayAssignment
                    } else {
                        IssueKind::MixedArrayAssignment
                    },
                    format!(
                        "Unsafe array assignment on value with type {}",
                        mixed.get_id()
                    ),
                    statements_analyzer.get_hpos(&pos),
                ),
                statements_analyzer.get_config(),
            );
        } else {
            // oh no!
            tast_info.maybe_add_issue(
                Issue::new(
                    if let TAtomic::TMixedAny = mixed {
                        IssueKind::MixedAnyArrayAccess
                    } else {
                        IssueKind::MixedArrayAccess
                    },
                    format!("Unsafe array access on value with type {}", mixed.get_id()),
                    statements_analyzer.get_hpos(&pos),
                ),
                statements_analyzer.get_config(),
            );
        }
    }

    if let Some(stmt_var_type) = tast_info
        .expr_types
        .get(&(pos.start_offset(), pos.end_offset()))
    {
        if !stmt_var_type.parent_nodes.is_empty() {
            let new_parent_node = DataFlowNode::get_for_assignment(
                "mixed-var-array-access".to_string(),
                statements_analyzer.get_hpos(pos),
            );
            tast_info.data_flow_graph.add_node(new_parent_node.clone());

            for (_, parent_node) in stmt_var_type.parent_nodes.iter() {
                tast_info.data_flow_graph.add_path(
                    parent_node,
                    &new_parent_node,
                    PathKind::Default,
                    None,
                    None,
                );
            }
            if let Some(stmt_type) = stmt_type {
                let mut stmt_type_new = stmt_type.clone();
                stmt_type_new.parent_nodes = FxHashMap::from_iter([(
                    new_parent_node.get_id().clone(),
                    new_parent_node.clone(),
                )]);
            }
        }
    }

    if let TAtomic::TNothing = mixed {
        return get_mixed_maybe_from_loop(true);
    }

    return get_mixed_any();
}
