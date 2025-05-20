use super::{control_analyzer::BreakContext, loop_analyzer};
use crate::{
    expr::{
        binop::assignment_analyzer, expression_identifier,
        fetch::array_fetch_analyzer::add_array_fetch_dataflow,
    },
    expression_analyzer,
    function_analysis_data::FunctionAnalysisData,
    scope::{loop_scope::LoopScope, BlockContext},
    scope_analyzer::ScopeAnalyzer,
    statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};
use hakana_code_info::ttype::{
    add_optional_union_type, add_union_type, combine_optional_union_types, get_arraykey, get_int,
    get_literal_int, get_literal_string, get_mixed_any, get_nothing,
};
use hakana_code_info::{
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    issue::{Issue, IssueKind},
    t_atomic::{DictKey, TAtomic, TDict},
    t_union::TUnion,
    var_name::VarName,
};
use hakana_str::StrId;
use itertools::Itertools;
use oxidized::{aast, ast_defs};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &aast::AsExpr<(), ()>,
        &aast::Block<(), ()>,
    ),
    pos: &ast_defs::Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
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

    expression_analyzer::analyze(statements_analyzer, stmt.0, analysis_data, context)?;

    context.inside_general_use = was_inside_general_use;

    let mut key_type = None;
    let mut value_type = None;
    let mut always_non_empty_array = true;

    let var_id = expression_identifier::get_var_id(
        stmt.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.file_analyzer.resolved_names,
        Some((statements_analyzer.codebase, &statements_analyzer.interner)),
    );

    let iterator_type = if let Some(stmt_expr_type) = analysis_data.get_expr_type(stmt.0.pos()) {
        Some(stmt_expr_type.clone())
    } else if let Some(var_id) = &var_id {
        let var_id = VarName::new(var_id.clone());
        if context.has_variable(&var_id) {
            context.locals.get(&var_id).map(|t| (**t).clone())
        } else {
            None
        }
    } else {
        None
    };

    if let Some(iterator_type) = iterator_type {
        let result = check_iterator_type(
            statements_analyzer,
            analysis_data,
            stmt.0,
            stmt.0.pos(),
            &iterator_type,
            value_is_async,
            context,
        );

        key_type = Some(result.0.unwrap_or(get_arraykey(true)));
        value_type = Some(result.1.unwrap_or(get_mixed_any()));
        always_non_empty_array = result.2;
    }

    let mut foreach_context = context.clone();

    foreach_context.inside_loop_exprs = true;

    foreach_context.inside_loop = true;
    foreach_context.break_types.push(BreakContext::Loop);

    match stmt.1 {
        aast::AsExpr::AsKv(key_expr, _) | aast::AsExpr::AwaitAsKv(_, key_expr, _) => {
            let key_type = key_type.unwrap_or(get_arraykey(true));

            assignment_analyzer::analyze(
                statements_analyzer,
                (key_expr, None, None),
                stmt.0.pos(),
                Some(&key_type),
                analysis_data,
                &mut foreach_context,
                None,
            )
            .ok();
        }
        _ => {}
    }

    let value_type = value_type.unwrap_or(get_mixed_any());

    foreach_context.for_loop_init_bounds = (
        value_expr.pos().end_offset() as u32,
        pos.end_offset() as u32,
    );

    assignment_analyzer::analyze(
        statements_analyzer,
        (value_expr, None, None),
        stmt.0.pos(),
        Some(&value_type),
        analysis_data,
        &mut foreach_context,
        None,
    )?;

    foreach_context.for_loop_init_bounds = (0, 0);
    foreach_context.inside_loop_exprs = false;

    let prev_loop_bounds = foreach_context.loop_bounds;
    foreach_context.loop_bounds = (pos.start_offset() as u32, pos.end_offset() as u32);

    loop_analyzer::analyze(
        statements_analyzer,
        &stmt.2 .0,
        vec![],
        vec![],
        &mut LoopScope::new(context.locals.clone()),
        &mut foreach_context,
        context,
        analysis_data,
        false,
        always_non_empty_array,
    )?;

    foreach_context.loop_bounds = prev_loop_bounds;

    // todo do we need to remove the loop scope from analysis_data here? unsure

    Ok(())
}

fn check_iterator_type(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    expr: &aast::Expr<(), ()>,
    pos: &ast_defs::Pos,
    iterator_type: &TUnion,
    is_async: bool,
    context: &mut BlockContext,
) -> (Option<TUnion>, Option<TUnion>, bool) {
    let mut always_non_empty_array = true;

    if iterator_type.is_null() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NullIterator,
                "Cannot iterate over null".to_string(),
                statements_analyzer.get_hpos(expr.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return (None, None, false);
    }

    if iterator_type.is_nullable() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NullIterator,
                "Cannot iterate over null".to_string(),
                statements_analyzer.get_hpos(expr.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return (None, None, false);
    }

    let mut has_valid_iterator = false;
    //let mut invalid_iterator_types = vec![];
    //let mut raw_object_types = vec![];

    let mut iterator_atomic_types = iterator_type.types.iter().collect_vec();

    let mut key_type = None;
    let mut value_type = None;

    let codebase = statements_analyzer.codebase;

    while let Some(mut iterator_atomic_type) = iterator_atomic_types.pop() {
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. } = iterator_atomic_type
        {
            iterator_atomic_types.extend(&as_type.types);
            continue;
        }

        if let TAtomic::TTypeAlias {
            as_type: Some(as_type),
            ..
        } = iterator_atomic_type
        {
            iterator_atomic_type = as_type.get_single();
        }

        match &iterator_atomic_type {
            TAtomic::TVec {
                type_param,
                known_items: None,
                ..
            } => {
                if type_param.is_nothing() {
                    always_non_empty_array = false;
                    has_valid_iterator = true;
                    continue;
                }
            }
            TAtomic::TKeyset { type_param, .. } => {
                if type_param.is_nothing() {
                    always_non_empty_array = false;
                    has_valid_iterator = true;
                    continue;
                }
            }
            TAtomic::TDict(TDict {
                params,
                known_items: None,
                ..
            }) => {
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
            TAtomic::TDict(TDict {
                known_items: None,
                non_empty: false,
                ..
            }) => {
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
            TAtomic::TDict(TDict { .. }) | TAtomic::TVec { .. } | TAtomic::TKeyset { .. } => {
                let (key_param, value_param) = match iterator_atomic_type {
                    TAtomic::TDict(TDict {
                        known_items,
                        params,
                        ..
                    }) => {
                        let mut key_param;
                        let mut value_param;

                        if let Some(params) = params {
                            key_param = (*params.0).clone();
                            value_param = (*params.1).clone();
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
                                            &get_literal_int(*var_id as i64),
                                            codebase,
                                            false,
                                        );
                                        value_param = add_union_type(
                                            value_param,
                                            known_item,
                                            codebase,
                                            false,
                                        );
                                    }
                                    DictKey::String(var_id) => {
                                        key_param = add_union_type(
                                            key_param,
                                            &get_literal_string(var_id.clone()),
                                            codebase,
                                            false,
                                        );
                                        value_param = add_union_type(
                                            value_param,
                                            known_item,
                                            codebase,
                                            false,
                                        );
                                    }
                                    DictKey::Enum(enum_name, member_name) => {
                                        if let Some(literal_value) = statements_analyzer
                                            .codebase
                                            .get_classconst_literal_value(enum_name, member_name)
                                        {
                                            if let Some(value) =
                                                literal_value.get_literal_string_value()
                                            {
                                                key_param = add_union_type(
                                                    key_param,
                                                    &get_literal_string(value),
                                                    codebase,
                                                    false,
                                                );
                                                value_param = add_union_type(
                                                    value_param,
                                                    known_item,
                                                    codebase,
                                                    false,
                                                );
                                            } else if let Some(value) =
                                                literal_value.get_literal_int_value()
                                            {
                                                key_param = add_union_type(
                                                    key_param,
                                                    &get_literal_int(value),
                                                    codebase,
                                                    false,
                                                );
                                                value_param = add_union_type(
                                                    value_param,
                                                    known_item,
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
                        let mut value_param = (**type_param).clone();

                        if let Some(known_items) = known_items {
                            for (offset, (_, known_item)) in known_items {
                                key_param = add_union_type(
                                    key_param,
                                    &get_literal_int(*offset as i64),
                                    codebase,
                                    false,
                                );
                                value_param =
                                    add_union_type(value_param, known_item, codebase, false);
                            }
                        }

                        (key_param, value_param)
                    }
                    TAtomic::TKeyset { type_param, .. } => {
                        ((**type_param).clone(), (**type_param).clone())
                    }
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
        } else if let TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } = iterator_atomic_type
        {
            match *name {
                StrId::KEYED_CONTAINER | StrId::KEYED_ITERATOR | StrId::KEYED_TRAVERSABLE => {
                    has_valid_iterator = true;
                    key_type = Some(combine_optional_union_types(
                        key_type.as_ref(),
                        Some(type_params.first().unwrap()),
                        codebase,
                    ));
                    value_type = Some(combine_optional_union_types(
                        value_type.as_ref(),
                        Some(type_params.get(1).unwrap()),
                        codebase,
                    ));
                }
                StrId::CONTAINER | StrId::ITERATOR | StrId::TRAVERSABLE => {
                    has_valid_iterator = true;
                    key_type = Some(combine_optional_union_types(
                        key_type.as_ref(),
                        Some(&get_arraykey(true)),
                        codebase,
                    ));
                    value_type = Some(combine_optional_union_types(
                        value_type.as_ref(),
                        Some(type_params.first().unwrap()),
                        codebase,
                    ));
                }
                StrId::ASYNC_KEYED_ITERATOR => {
                    if is_async {
                        has_valid_iterator = true;
                        key_type = Some(combine_optional_union_types(
                            key_type.as_ref(),
                            Some(type_params.first().unwrap()),
                            codebase,
                        ));
                        value_type = Some(combine_optional_union_types(
                            value_type.as_ref(),
                            Some(type_params.get(1).unwrap()),
                            codebase,
                        ));
                    }
                }
                StrId::ASYNC_ITERATOR => {
                    if is_async {
                        has_valid_iterator = true;
                        key_type = Some(combine_optional_union_types(
                            key_type.as_ref(),
                            Some(&get_arraykey(true)),
                            codebase,
                        ));
                        value_type = Some(combine_optional_union_types(
                            value_type.as_ref(),
                            Some(type_params.first().unwrap()),
                            codebase,
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    if has_valid_iterator {
        if let Some(ref mut key_type) = key_type {
            if let Some(ref mut value_type) = value_type {
                add_array_fetch_dataflow(
                    statements_analyzer,
                    expr.pos(),
                    analysis_data,
                    None,
                    value_type,
                    key_type,
                )
            }
        }
    }

    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
        let foreach_node = DataFlowNode::get_for_unlabelled_sink(statements_analyzer.get_hpos(pos));

        for parent_node in &iterator_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                parent_node,
                &foreach_node,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
        analysis_data.data_flow_graph.add_node(foreach_node);
    }

    (key_type, value_type, always_non_empty_array)
}
