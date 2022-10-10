use super::{control_analyzer::BreakContext, loop_analyzer};
use crate::{
    expr::{
        binop::assignment_analyzer, expression_identifier,
        fetch::array_fetch_analyzer::add_array_fetch_dataflow,
    },
    expression_analyzer,
    scope_analyzer::ScopeAnalyzer,
    scope_context::{loop_scope::LoopScope, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};
use hakana_reflection_info::{
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    issue::{Issue, IssueKind},
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_type::{
    add_optional_union_type, add_union_type, combine_optional_union_types, get_arraykey, get_int,
    get_literal_int, get_literal_string, get_mixed_any, get_nothing,
};
use oxidized::{aast, ast_defs};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &aast::AsExpr<(), ()>,
        &aast::Block<(), ()>,
    ),
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let mut value_is_async = false;

    let value_expr = match stmt.1 {
        aast::AsExpr::AsV(value_expr) | aast::AsExpr::AsKv(_, value_expr) => value_expr,
        aast::AsExpr::AwaitAsV(_, value_expr) | aast::AsExpr::AwaitAsKv(_, _, value_expr) => {
            value_is_async = true;
            value_expr
        }
    };

    // todo add foreach var location maybe

    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;

    if expression_analyzer::analyze(statements_analyzer, stmt.0, tast_info, context, &mut None)
        == false
    {
        context.inside_general_use = was_inside_general_use;

        return false;
    }

    context.inside_general_use = was_inside_general_use;

    let mut key_type = None;
    let mut value_type = None;
    let mut always_non_empty_array = true;

    let var_id = expression_identifier::get_var_id(
        stmt.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some(statements_analyzer.get_codebase()),
    );

    let iterator_type = if let Some(stmt_expr_type) = tast_info.get_expr_type(stmt.0.pos()) {
        Some(stmt_expr_type.clone())
    } else if let Some(var_id) = &var_id {
        if context.has_variable(&var_id) {
            if let Some(t) = context.vars_in_scope.get(var_id) {
                Some((**t).clone())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(iterator_type) = iterator_type {
        let result = check_iterator_type(
            statements_analyzer,
            tast_info,
            stmt.0,
            stmt.0.pos(),
            &iterator_type,
            value_is_async,
            context,
        );

        if !result.0 {
            return false;
        }

        key_type = Some(result.1.unwrap_or(get_arraykey(true)));
        value_type = Some(result.2.unwrap_or(get_mixed_any()));
        always_non_empty_array = result.3;
    }

    let mut foreach_context = context.clone();

    foreach_context.inside_loop = true;
    foreach_context.break_types.push(BreakContext::Loop);

    match stmt.1 {
        aast::AsExpr::AsKv(key_expr, _) | aast::AsExpr::AwaitAsKv(_, key_expr, _) => {
            let key_type = key_type.unwrap_or(get_arraykey(true));

            assignment_analyzer::analyze(
                statements_analyzer,
                (&ast_defs::Bop::Eq(None), key_expr, None),
                stmt.0.pos(),
                Some(&key_type),
                tast_info,
                &mut foreach_context,
                false,
            )
            .ok();
        }
        _ => {}
    }

    let value_type = value_type.unwrap_or(get_mixed_any());

    assignment_analyzer::analyze(
        statements_analyzer,
        (&ast_defs::Bop::Eq(None), value_expr, None),
        stmt.0.pos(),
        Some(&value_type),
        tast_info,
        &mut foreach_context,
        false,
    )
    .ok();

    let mut loop_scope = LoopScope::new(context.vars_in_scope.clone());

    loop_scope.protected_var_ids = context.protected_var_ids.clone();

    let (analysis_result, _) = loop_analyzer::analyze(
        statements_analyzer,
        stmt.2,
        vec![],
        vec![],
        &mut Some(loop_scope),
        &mut foreach_context,
        context,
        tast_info,
        false,
        always_non_empty_array,
    );

    if !analysis_result {
        return false;
    }

    // todo do we need to remove the loop scope from tast_info here? unsure

    return true;
}

fn check_iterator_type(
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    expr: &aast::Expr<(), ()>,
    pos: &ast_defs::Pos,
    iterator_type: &TUnion,
    is_async: bool,
    context: &mut ScopeContext,
) -> (bool, Option<TUnion>, Option<TUnion>, bool) {
    let mut always_non_empty_array = true;

    if iterator_type.is_null() {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NullIterator,
                "Cannot iterate over null".to_string(),
                statements_analyzer.get_hpos(&expr.pos()),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return (true, None, None, false);
    }

    if iterator_type.is_nullable() {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NullIterator,
                "Cannot iterate over null".to_string(),
                statements_analyzer.get_hpos(&expr.pos()),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return (true, None, None, false);
    }

    let mut has_valid_iterator = false;
    //let mut invalid_iterator_types = vec![];
    //let mut raw_object_types = vec![];

    let mut iterator_atomic_types = iterator_type.types.clone();

    let mut key_type = None;
    let mut value_type = None;

    let codebase = statements_analyzer.get_codebase();

    while let Some(mut iterator_atomic_type) = iterator_atomic_types.pop() {
        if let TAtomic::TTemplateParam { as_type, .. } = iterator_atomic_type {
            iterator_atomic_types.extend(as_type.types);
            continue;
        }

        if let TAtomic::TTypeAlias {
            as_type: Some(as_type),
            ..
        } = iterator_atomic_type
        {
            iterator_atomic_type = (*as_type).clone();
        }

        match &iterator_atomic_type {
            TAtomic::TVec {
                type_param,
                known_items: None,
                ..
            }
            | TAtomic::TKeyset { type_param, .. } => {
                if type_param.is_nothing() {
                    always_non_empty_array = false;
                    has_valid_iterator = true;
                    continue;
                }
            }
            TAtomic::TDict {
                params,
                known_items: None,
                ..
            } => {
                if params.is_none() {
                    always_non_empty_array = false;
                    has_valid_iterator = true;
                    continue;
                }
            }
            _ => {}
        }

        if let TAtomic::TNull { .. } | TAtomic::TFalse { .. } = iterator_atomic_type {
            always_non_empty_array = false;
            continue;
        }

        match iterator_atomic_type {
            TAtomic::TDict {
                known_items: None,
                non_empty: false,
                ..
            } => {
                always_non_empty_array = false;
            }
            TAtomic::TVec {
                known_items: None,
                non_empty: false,
                ..
            } => {
                always_non_empty_array = false;
            }
            TAtomic::TKeyset { .. } => {
                always_non_empty_array = false;
            }
            _ => (),
        }

        match iterator_atomic_type {
            TAtomic::TDict { .. } | TAtomic::TVec { .. } | TAtomic::TKeyset { .. } => {
                let (key_param, value_param) = match iterator_atomic_type {
                    TAtomic::TDict {
                        known_items,
                        params,
                        ..
                    } => {
                        let mut key_param;
                        let mut value_param;

                        if let Some(params) = params {
                            key_param = params.0;
                            value_param = params.1;
                        } else {
                            key_param = get_nothing();
                            value_param = get_nothing();
                        }

                        if let Some(known_items) = known_items {
                            for (var_id, (_, known_item)) in known_items {
                                match var_id {
                                    DictKey::Int(var_id) => {
                                        key_param = add_union_type(
                                            key_param,
                                            &get_literal_int(var_id as i64),
                                            codebase,
                                            false,
                                        );
                                        value_param = add_union_type(
                                            value_param,
                                            &known_item,
                                            codebase,
                                            false,
                                        );
                                    }
                                    DictKey::String(var_id) => {
                                        key_param = add_union_type(
                                            key_param,
                                            &get_literal_string(var_id),
                                            codebase,
                                            false,
                                        );
                                        value_param = add_union_type(
                                            value_param,
                                            &known_item,
                                            codebase,
                                            false,
                                        );
                                    }
                                    DictKey::Enum(enum_name, member_name) => {
                                        if let Some(literal_value) = statements_analyzer
                                            .get_codebase()
                                            .get_classconst_literal_value(&enum_name, &member_name)
                                        {
                                            if let Some(value) = literal_value
                                                .get_single_literal_string_value(
                                                    &statements_analyzer.get_codebase().interner,
                                                )
                                            {
                                                key_param = add_union_type(
                                                    key_param,
                                                    &get_literal_string(value),
                                                    codebase,
                                                    false,
                                                );
                                                value_param = add_union_type(
                                                    value_param,
                                                    &known_item,
                                                    codebase,
                                                    false,
                                                );
                                            } else if let Some(value) =
                                                literal_value.get_single_literal_int_value()
                                            {
                                                key_param = add_union_type(
                                                    key_param,
                                                    &get_literal_int(value),
                                                    codebase,
                                                    false,
                                                );
                                                value_param = add_union_type(
                                                    value_param,
                                                    &known_item,
                                                    codebase,
                                                    false,
                                                );
                                            } else {
                                                panic!()
                                            }
                                        } else {
                                            panic!();
                                        }
                                    }
                                }
                            }
                        }

                        (key_param, value_param)
                    }
                    TAtomic::TVec {
                        known_items,
                        type_param,
                        ..
                    } => {
                        let mut key_param = if type_param.is_nothing() {
                            get_nothing()
                        } else {
                            get_int()
                        };
                        let mut value_param = type_param;

                        if let Some(known_items) = known_items {
                            for (offset, (_, known_item)) in known_items {
                                key_param = add_union_type(
                                    key_param,
                                    &get_literal_int(offset as i64),
                                    codebase,
                                    false,
                                );
                                value_param =
                                    add_union_type(value_param, &known_item, codebase, false);
                            }
                        }

                        (key_param, value_param)
                    }
                    TAtomic::TKeyset { type_param, .. } => (type_param.clone(), type_param),
                    _ => panic!(),
                };

                key_type = Some(add_optional_union_type(
                    key_param,
                    key_type.as_ref(),
                    codebase,
                ));

                value_type = Some(add_optional_union_type(
                    value_param,
                    value_type.as_ref(),
                    codebase,
                ));

                has_valid_iterator = true;
                continue;
            }
            _ => (),
        }

        always_non_empty_array = false;

        if iterator_atomic_type.is_mixed() {
            has_valid_iterator = true;
            key_type = Some(add_optional_union_type(
                get_arraykey(true),
                key_type.as_ref(),
                codebase,
            ));

            value_type = Some(add_optional_union_type(
                get_mixed_any(),
                value_type.as_ref(),
                codebase,
            ));

            if !context.function_context.pure {
                // do purity stuff
            } else {
                // todo maybe emit impure method call issue?
            }
        } else if let TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } = iterator_atomic_type
        {
            match codebase.interner.lookup(name) {
                "HH\\KeyedContainer" | "HH\\KeyedIterator" => {
                    has_valid_iterator = true;
                    key_type = Some(combine_optional_union_types(
                        key_type.as_ref(),
                        Some(type_params.get(0).unwrap()),
                        codebase,
                    ));
                    value_type = Some(combine_optional_union_types(
                        value_type.as_ref(),
                        Some(type_params.get(1).unwrap()),
                        codebase,
                    ));
                }
                "HH\\Container" | "HH\\Iterator" | "HH\\Traversable" => {
                    has_valid_iterator = true;
                    key_type = Some(combine_optional_union_types(
                        key_type.as_ref(),
                        Some(&get_arraykey(true)),
                        codebase,
                    ));
                    value_type = Some(combine_optional_union_types(
                        value_type.as_ref(),
                        Some(type_params.get(0).unwrap()),
                        codebase,
                    ));
                }
                "HH\\AsyncKeyedIterator" => {
                    if is_async {
                        has_valid_iterator = true;
                        key_type = Some(combine_optional_union_types(
                            key_type.as_ref(),
                            Some(type_params.get(0).unwrap()),
                            codebase,
                        ));
                        value_type = Some(combine_optional_union_types(
                            value_type.as_ref(),
                            Some(type_params.get(1).unwrap()),
                            codebase,
                        ));
                    }
                }
                "HH\\AsyncIterator" => {
                    if is_async {
                        has_valid_iterator = true;
                        key_type = Some(combine_optional_union_types(
                            key_type.as_ref(),
                            Some(&get_arraykey(true)),
                            codebase,
                        ));
                        value_type = Some(combine_optional_union_types(
                            value_type.as_ref(),
                            Some(type_params.get(0).unwrap()),
                            codebase,
                        ));
                    }
                }
                _ => {}
            }

            if !context.function_context.pure {
                // do purity stuff
            } else {
                // todo maybe emit impure method call issue?
            }
        }
    }

    if has_valid_iterator {
        if let Some(ref mut key_type) = key_type {
            if let Some(ref mut value_type) = value_type {
                add_array_fetch_dataflow(
                    statements_analyzer,
                    expr,
                    tast_info,
                    None,
                    value_type,
                    key_type,
                )
            }
        }
    }

    if tast_info.data_flow_graph.kind == GraphKind::FunctionBody {
        let foreach_node = DataFlowNode::get_for_variable_sink(
            "foreach".to_string(),
            statements_analyzer.get_hpos(pos),
        );

        for (_, parent_node) in &iterator_type.parent_nodes {
            tast_info.data_flow_graph.add_path(
                &parent_node,
                &foreach_node,
                PathKind::Default,
                None,
                None,
            );
        }
        tast_info.data_flow_graph.add_node(foreach_node);
    }

    (true, key_type, value_type, always_non_empty_array)
}
