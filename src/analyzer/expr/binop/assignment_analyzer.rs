use hakana_reflection_info::data_flow::graph::WholeProgramKind;
use hakana_reflection_info::data_flow::node::DataFlowNodeId;
use hakana_reflection_info::data_flow::node::DataFlowNodeKind;
use hakana_reflection_info::VarId;
use hakana_reflection_info::EFFECT_WRITE_LOCAL;
use hakana_str::StrId;
use indexmap::IndexMap;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::expr::assignment::array_assignment_analyzer;
use crate::expr::assignment::instance_property_assignment_analyzer;
use crate::expr::assignment::static_property_assignment_analyzer;
use crate::expr::call::argument_analyzer::get_removed_taints_in_comments;
use crate::expr::expression_identifier;
use crate::expr::expression_identifier::get_root_var_id;
use crate::expr::expression_identifier::get_var_id;
use crate::expr::fetch::array_fetch_analyzer;
use crate::expression_analyzer;
use crate::expression_analyzer::expr_has_logic;
use crate::expression_analyzer::find_expr_logic_issues;
use crate::formula_generator;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_algebra::Clause;
use hakana_reflection_info::assertion::Assertion;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::issue::Issue;
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_type::add_union_type;
use hakana_type::get_literal_int;
use hakana_type::get_mixed;
use hakana_type::get_mixed_any;
use hakana_type::get_nothing;
use oxidized::ast::Bop;
use oxidized::ast_defs;
use oxidized::pos::Pos;
use oxidized::{aast, ast};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ast::Bop, &aast::Expr<(), ()>, Option<&aast::Expr<(), ()>>),
    pos: &Pos,
    assign_value_type: Option<&TUnion>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    inout_node: Option<DataFlowNode>,
) -> Result<(), AnalysisError> {
    let (binop, assign_var, assign_value) = (expr.0, expr.1, expr.2);

    let var_id = get_var_id(
        assign_var,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some((
            statements_analyzer.get_codebase(),
            statements_analyzer.get_interner(),
        )),
    );

    if statements_analyzer.get_config().add_fixmes {
        if let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset {
            if current_stmt_offset.line != expr.1.pos().line() as u32 {
                current_stmt_offset.line = expr.1.pos().line() as u32;
            }

            analysis_data.expr_fixme_positions.insert(
                (
                    expr.1.pos().start_offset() as u32,
                    expr.1.pos().end_offset() as u32,
                ),
                *current_stmt_offset,
            );
        }
    }

    //let removed_taints = Vec::new();

    let mut existing_var_type = None;

    if let Some(var_id) = &var_id {
        context.cond_referenced_var_ids.remove(var_id);
        context
            .assigned_var_ids
            .insert(var_id.clone(), assign_var.pos().start_offset());
        context.possibly_assigned_var_ids.insert(var_id.clone());

        existing_var_type = context.vars_in_scope.get(var_id).cloned();
    }

    if let Some(assign_value) = assign_value {
        let mut root_expr = assign_var;
        while let aast::Expr_::ArrayGet(boxed) = &root_expr.2 {
            root_expr = &boxed.0;
        }

        // if we don't know where this data is going, treat as a dead-end usage

        if !matches!(root_expr.2, aast::Expr_::Lvar(..)) {
            context.inside_general_use = true;
        }

        match binop {
            // this rewrites $a += 4 and $a ??= 4 to $a = $a + 4 and $a = $a ?? 4 respectively
            Bop::Eq(Some(assignment_type)) => {
                let tast_expr_types = analysis_data.expr_types.clone();

                context.inside_assignment_op = true;

                expression_analyzer::analyze(
                    statements_analyzer,
                    &aast::Expr(
                        (),
                        pos.clone(),
                        aast::Expr_::Binop(Box::new(oxidized::aast::Binop {
                            bop: *assignment_type.clone(),
                            lhs: assign_var.clone(),
                            rhs: assign_value.clone(),
                        })),
                    ),
                    analysis_data,
                    context,
                    &mut None,
                )?;

                context.inside_assignment_op = false;

                let new_expr_types = analysis_data.expr_types.clone();
                let expr_type = new_expr_types
                    .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
                    .cloned();

                analysis_data.expr_types = tast_expr_types;

                if let Some(expr_type) = expr_type {
                    analysis_data.expr_types.insert(
                        (
                            assign_value.1.start_offset() as u32,
                            assign_value.1.end_offset() as u32,
                        ),
                        expr_type,
                    );
                };
            }
            _ => {
                expression_analyzer::analyze(
                    statements_analyzer,
                    assign_value,
                    analysis_data,
                    context,
                    &mut None,
                )?;
            }
        };

        if expr_has_logic(assign_value) {
            find_expr_logic_issues(statements_analyzer, context, assign_value, analysis_data);
        }

        context.inside_general_use = false;
    }

    let assign_value_type = if let Some(assign_value_type) = assign_value_type {
        assign_value_type.clone()
    } else if let Some(assign_value) = assign_value {
        if let Some(var_type) = analysis_data.get_expr_type(&assign_value.1) {
            // todo set from_property flags on union

            var_type.clone()
        } else {
            get_mixed_any()
        }
    } else {
        get_mixed_any()
    };

    if let (Some(var_id), Some(existing_var_type), Bop::Eq(None)) =
        (&var_id, &existing_var_type, binop)
    {
        if context.inside_loop
            && !context.inside_assignment_op
            && context.for_loop_init_bounds.0 > 0
            && var_id != "$_"
        {
            let mut origin_node_ids = vec![];

            for parent_node in &existing_var_type.parent_nodes {
                origin_node_ids.extend(
                    analysis_data
                        .data_flow_graph
                        .get_origin_node_ids(&parent_node.id, vec![]),
                );
            }

            origin_node_ids.retain(|id| {
                let node = &analysis_data.data_flow_graph.get_node(id).unwrap();

                match &node.kind {
                    DataFlowNodeKind::ForLoopInit {
                        var_name,
                        start_offset,
                        end_offset,
                    } => {
                        var_name == var_id
                            && (pos.start_offset() as u32) > *start_offset
                            && (pos.end_offset() as u32) < *end_offset
                    }
                    _ => false,
                }
            });

            if !origin_node_ids.is_empty() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::ForLoopInvalidation,
                        format!("{} was previously assigned in a for loop", var_id),
                        statements_analyzer.get_hpos(pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                )
            }
        }
    }

    if let (Some(var_id), Some(existing_var_type)) = (&var_id, &existing_var_type) {
        context.remove_descendants(
            var_id,
            existing_var_type,
            Some(&assign_value_type),
            Some(statements_analyzer),
            analysis_data,
        );
    } else {
        let root_var_id = get_root_var_id(assign_var);

        if let Some(root_var_id) = root_var_id {
            if let Some(existing_root_type) = context.vars_in_scope.get(&root_var_id).cloned() {
                context.remove_var_from_conflicting_clauses(
                    &root_var_id,
                    Some(&existing_root_type),
                    Some(statements_analyzer),
                    analysis_data,
                );
            }
        }
    }

    analysis_data.expr_effects.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        EFFECT_WRITE_LOCAL,
    );

    match &assign_var.2 {
        aast::Expr_::Lvar(_) => analyze_assignment_to_variable(
            statements_analyzer,
            assign_var,
            assign_value,
            assign_value_type,
            var_id.as_ref().unwrap(),
            analysis_data,
            context,
            inout_node,
        ),
        aast::Expr_::ArrayGet(boxed) => {
            array_assignment_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, boxed.1.as_ref(), assign_var.pos()),
                assign_value_type,
                pos,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::ObjGet(boxed) => {
            instance_property_assignment_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1),
                pos,
                var_id,
                &assign_value_type,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::ClassGet(boxed) => {
            let (lhs, rhs, _) = (&boxed.0, &boxed.1, &boxed.2);

            static_property_assignment_analyzer::analyze(
                statements_analyzer,
                (lhs, rhs),
                if let Some(assign_value) = assign_value {
                    Some(assign_value.pos())
                } else {
                    None
                },
                &assign_value_type,
                &var_id,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::List(expressions) => analyze_list_assignment(
            statements_analyzer,
            expressions,
            assign_value,
            &assign_value_type,
            analysis_data,
            context,
        ),
        aast::Expr_::Omitted => {
            // do nothing
        }
        _ => {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::UnrecognizedExpression,
                    "Unrecognized expression in assignment".to_string(),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    };

    Ok(())
}

fn analyze_list_assignment(
    statements_analyzer: &StatementsAnalyzer,
    expressions: &[aast::Expr<(), ()>],
    source_expr: Option<&aast::Expr<(), ()>>,
    assign_value_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) {
    let codebase = statements_analyzer.get_codebase();

    for (offset, assign_var_item) in expressions.iter().enumerate() {
        let list_var_id = expression_identifier::get_var_id(
            assign_var_item,
            context.function_context.calling_class.as_ref(),
            statements_analyzer.get_file_analyzer().resolved_names,
            Some((
                statements_analyzer.get_codebase(),
                statements_analyzer.get_interner(),
            )),
        );

        if list_var_id.unwrap_or("".to_string()) == "$_" {
            continue;
        }

        let mut value_type = get_nothing();

        for assign_value_atomic_type in &assign_value_type.types {
            let atomic_value_type = if let TAtomic::TVec {
                known_items,
                type_param,
                ..
            } = assign_value_atomic_type
            {
                if let Some(known_items) = known_items {
                    if let Some((possibly_undefined, value_type)) = known_items.get(&offset) {
                        if *possibly_undefined {
                            analysis_data.maybe_add_issue(
                                Issue::new(
                                    IssueKind::PossiblyUndefinedIntArrayOffset,
                                    "Possibly undefined offset in list assignment".to_string(),
                                    statements_analyzer.get_hpos(&assign_var_item.1),
                                    &context.function_context.calling_functionlike_id,
                                ),
                                statements_analyzer.get_config(),
                                statements_analyzer.get_file_path_actual(),
                            );
                        }

                        value_type.clone()
                    } else {
                        (**type_param).clone()
                    }
                } else {
                    (**type_param).clone()
                }
            } else if let TAtomic::TNamedObject {
                name,
                type_params: Some(type_params),
                ..
            } = assign_value_atomic_type
            {
                if *name == StrId::VECTOR {
                    type_params[0].clone()
                } else {
                    get_nothing()
                }
            } else if let TAtomic::TMixedWithFlags(true, ..) = assign_value_atomic_type {
                get_mixed_any()
            } else if assign_value_atomic_type.is_mixed() {
                get_mixed()
            } else {
                get_nothing()
            };

            value_type = add_union_type(value_type, &atomic_value_type, codebase, false);
        }

        if let Some(source_expr) = source_expr {
            let source_expr_id = expression_identifier::get_var_id(
                source_expr,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().resolved_names,
                Some((
                    statements_analyzer.get_codebase(),
                    statements_analyzer.get_interner(),
                )),
            );

            let keyed_array_var_id = source_expr_id
                .map(|source_expr_id| source_expr_id + "['" + offset.to_string().as_str() + "']");

            let mut value_type_rc = Rc::new(value_type);

            array_fetch_analyzer::add_array_fetch_dataflow_rc(
                statements_analyzer,
                source_expr,
                analysis_data,
                keyed_array_var_id,
                &mut value_type_rc,
                &mut get_literal_int(offset as i64),
            );

            value_type = (*value_type_rc).clone();
        }

        analyze(
            statements_analyzer,
            (&ast_defs::Bop::Eq(None), assign_var_item, None),
            assign_var_item.pos(),
            Some(&value_type),
            analysis_data,
            context,
            None,
        )
        .ok();
    }
}

fn analyze_assignment_to_variable(
    statements_analyzer: &StatementsAnalyzer,
    var_expr: &aast::Expr<(), ()>,
    source_expr: Option<&aast::Expr<(), ()>>,
    mut assign_value_type: TUnion,
    var_id: &String,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    inout_node: Option<DataFlowNode>,
) {
    // if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody && !is_inout {
    //     analysis_data
    //         .data_flow_graph
    //         .add_node(DataFlowNode::get_for_variable_source(
    //             var_id.clone(),
    //             statements_analyzer.get_hpos(var_expr.pos()),
    //             !context.inside_awaitall
    //                 && if let Some(source_expr) = source_expr {
    //                     analysis_data.is_pure(source_expr.pos())
    //                 } else {
    //                     false
    //                 },
    //             assign_value_type.has_awaitable_types(),
    //         ));
    // }

    let assign_var_pos = var_expr.pos();

    if assign_value_type.is_nothing() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::ImpossibleAssignment,
                "This assignment is impossible".to_string(),
                statements_analyzer.get_hpos(assign_var_pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let has_parent_nodes = !assign_value_type.parent_nodes.is_empty();

    let can_taint = has_parent_nodes
        && match analysis_data.data_flow_graph.kind {
            GraphKind::FunctionBody => inout_node.is_none(),
            GraphKind::WholeProgram(kind) => {
                context.allow_taints
                    && (kind != WholeProgramKind::Taint || assign_value_type.has_taintable_value())
            }
        };

    let assignment_node = if let Some(inout_node) = inout_node {
        inout_node
    } else if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
        && matches!(var_expr.2, aast::Expr_::Lvar(_))
    {
        DataFlowNode::get_for_variable_source(
            VarId(statements_analyzer.get_interner().get(var_id).unwrap()),
            statements_analyzer.get_hpos(var_expr.pos()),
            !context.inside_awaitall
                && if let Some(source_expr) = source_expr {
                    analysis_data.is_pure(source_expr.pos())
                } else {
                    false
                },
            has_parent_nodes,
            assign_value_type.has_awaitable_types(),
        )
    } else {
        DataFlowNode::get_for_lvar(
            VarId(statements_analyzer.get_interner().get(var_id).unwrap()),
            statements_analyzer.get_hpos(var_expr.pos()),
        )
    };

    analysis_data
        .data_flow_graph
        .add_node(assignment_node.clone());

    if can_taint {
        let removed_taints = get_removed_taints_in_comments(statements_analyzer, assign_var_pos);

        for parent_node in &assign_value_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                parent_node,
                &assignment_node,
                PathKind::Default,
                vec![],
                removed_taints.clone(),
            );
        }
    }

    assign_value_type.parent_nodes = vec![assignment_node];

    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
        && !has_parent_nodes
        && !context.inside_assignment_op
        && !var_id.starts_with("$_")
    {
        let (start_offset, end_offset) = context.for_loop_init_bounds;
        if start_offset != 0 {
            let for_node = DataFlowNode {
                id: DataFlowNodeId::ForInit(start_offset, end_offset),
                kind: DataFlowNodeKind::ForLoopInit {
                    start_offset,
                    end_offset,
                    var_name: var_id.clone(),
                },
            };

            analysis_data.data_flow_graph.add_node(for_node.clone());
            assign_value_type.parent_nodes.push(for_node);
        }
    }

    if assign_value_type.is_bool() {
        if let Some(source_expr) = source_expr {
            if matches!(source_expr.2, aast::Expr_::Binop(..)) {
                handle_assignment_with_boolean_logic(
                    var_expr,
                    source_expr,
                    statements_analyzer,
                    context,
                    analysis_data,
                    var_id,
                );
            }
        }
    }

    context
        .vars_in_scope
        .insert(var_id.clone(), Rc::new(assign_value_type));
}

fn handle_assignment_with_boolean_logic(
    var_expr: &aast::Expr<(), ()>,
    source_expr: &aast::Expr<(), ()>,
    statements_analyzer: &StatementsAnalyzer<'_>,
    context: &mut ScopeContext,
    analysis_data: &mut FunctionAnalysisData,
    var_id: &String,
) {
    // todo support $a = !($b || $c)
    let var_object_id = (
        var_expr.pos().start_offset() as u32,
        var_expr.pos().end_offset() as u32,
    );
    let cond_object_id = (
        source_expr.pos().start_offset() as u32,
        source_expr.pos().end_offset() as u32,
    );

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );

    let right_clauses = formula_generator::get_formula(
        cond_object_id,
        cond_object_id,
        source_expr,
        &assertion_context,
        analysis_data,
        true,
        false,
    );

    if let Ok(right_clauses) = right_clauses {
        let right_clauses = ScopeContext::filter_clauses(
            var_id,
            right_clauses.into_iter().map(Rc::new).collect(),
            None,
            None,
            analysis_data,
        );

        let mut possibilities = BTreeMap::new();
        possibilities.insert(
            var_id.clone(),
            IndexMap::from([(Assertion::Falsy.to_hash(), Assertion::Falsy)]),
        );

        let assignment_clauses = if let Ok(assignment_clauses) =
            hakana_algebra::combine_ored_clauses(
                vec![Clause::new(
                    possibilities,
                    var_object_id,
                    var_object_id,
                    None,
                    None,
                    None,
                )],
                right_clauses.into_iter().map(|v| (*v).clone()).collect(),
                cond_object_id,
            ) {
            assignment_clauses.into_iter().map(Rc::new).collect()
        } else {
            vec![]
        };

        context.clauses.extend(assignment_clauses);
    }
}

pub(crate) fn analyze_inout_param(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    arg_type: TUnion,
    inout_type: &TUnion,
    assignment_node: DataFlowNode,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    analyze(
        statements_analyzer,
        (&ast_defs::Bop::Eq(None), expr, None),
        expr.pos(),
        Some(inout_type),
        analysis_data,
        context,
        Some(assignment_node),
    )?;

    analysis_data.set_expr_type(expr.pos(), arg_type.clone());

    analysis_data.expr_effects.insert(
        (
            expr.pos().start_offset() as u32,
            expr.pos().end_offset() as u32,
        ),
        EFFECT_WRITE_LOCAL,
    );

    Ok(())
}
