use hakana_reflection_info::data_flow::graph::WholeProgramKind;
use hakana_reflection_info::data_flow::node::DataFlowNodeKind;
use hakana_reflection_info::EFFECT_WRITE_LOCAL;
use indexmap::IndexMap;
use rustc_hash::FxHashSet;
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
use crate::formula_generator;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::FunctionAnalysisData;
use hakana_algebra::Clause;
use hakana_reflection_info::assertion::Assertion;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::issue::Issue;
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::taint::SinkType;
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
    is_inout: bool,
) -> Result<(), ()> {
    let (binop, assign_var, assign_value) = (expr.0, expr.1, expr.2);

    let var_id = get_var_id(
        assign_var,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some((
            statements_analyzer.get_codebase(),
            statements_analyzer.get_interner(),
        )),
    );

    if statements_analyzer.get_config().add_fixmes {
        if let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset {
            if current_stmt_offset.line != expr.1.pos().line() {
                current_stmt_offset.line = expr.1.pos().line();
            }

            analysis_data.expr_fixme_positions.insert(
                (expr.1.pos().start_offset(), expr.1.pos().end_offset()),
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

        let analyzed_ok = match binop {
            // this rewrites $a += 4 and $a ??= 4 to $a = $a + 4 and $a = $a ?? 4 respectively
            Bop::Eq(Some(assignment_type)) => {
                let tast_expr_types = analysis_data.expr_types.clone();

                context.inside_assignment_op = true;

                let analyzed_ok = expression_analyzer::analyze(
                    statements_analyzer,
                    &aast::Expr(
                        (),
                        pos.clone(),
                        aast::Expr_::Binop(Box::new((
                            *assignment_type.clone(),
                            assign_var.clone(),
                            assign_value.clone(),
                        ))),
                    ),
                    analysis_data,
                    context,
                    &mut None,
                );

                context.inside_assignment_op = false;

                let new_expr_types = analysis_data.expr_types.clone();
                let expr_type = new_expr_types
                    .get(&(pos.start_offset(), pos.end_offset()))
                    .cloned();

                analysis_data.expr_types = tast_expr_types;

                if let Some(expr_type) = expr_type {
                    analysis_data.expr_types.insert(
                        (assign_value.1.start_offset(), assign_value.1.end_offset()),
                        expr_type,
                    );
                };

                analyzed_ok
            }
            _ => expression_analyzer::analyze(
                statements_analyzer,
                assign_value,
                analysis_data,
                context,
                &mut None,
            ),
        };

        context.inside_general_use = false;

        if !analyzed_ok {
            if let Some(var_id) = &var_id {
                if let Some(existing_type) = context.vars_in_scope.clone().get(var_id) {
                    context.remove_descendants(
                        var_id,
                        existing_type,
                        assign_value_type,
                        None,
                        analysis_data,
                    );
                }

                // if we're not exiting immediately, make everything mixed
                context
                    .vars_in_scope
                    .insert(var_id.clone(), Rc::new(get_mixed_any()));
            }

            return Err(());
        }
    }

    let mut assign_value_type = if let Some(assign_value_type) = assign_value_type {
        assign_value_type.clone()
    } else {
        if let Some(assign_value) = assign_value {
            if let Some(var_type) = analysis_data.get_expr_type(&assign_value.1) {
                let assign_value_type = var_type.clone();

                // todo set from_property flags on union

                assign_value_type
            } else {
                get_mixed_any()
            }
        } else {
            get_mixed_any()
        }
    };

    if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody
        && assign_value_type.parent_nodes.is_empty()
    {
        if let Some(var_id) = &var_id {
            let assignment_node = DataFlowNode::get_for_assignment(
                var_id.clone(),
                statements_analyzer.get_hpos(assign_var.pos()),
            );

            analysis_data
                .data_flow_graph
                .add_node(assignment_node.clone());

            assign_value_type.parent_nodes.insert(assignment_node);

            if !context.inside_assignment_op && !var_id.starts_with("$_") {
                if let Some((start_offset, end_offset)) = context.for_loop_init_bounds {
                    let for_node = DataFlowNode {
                        id: format!("for-init-{}-{}", start_offset, end_offset),
                        kind: DataFlowNodeKind::ForLoopInit {
                            start_offset,
                            end_offset,
                            var_name: var_id.clone(),
                        },
                    };

                    analysis_data.data_flow_graph.add_node(for_node.clone());

                    assign_value_type.parent_nodes.insert(for_node);
                }
            }
        };
    }

    if let (Some(var_id), Some(existing_var_type), Bop::Eq(None)) =
        (&var_id, &existing_var_type, binop)
    {
        if context.inside_loop
            && !context.inside_assignment_op
            && context.for_loop_init_bounds.is_some()
            && var_id != "$_"
        {
            let mut origin_nodes = vec![];

            for parent_node in &existing_var_type.parent_nodes {
                origin_nodes.extend(analysis_data.data_flow_graph.get_origin_nodes(parent_node));
            }

            origin_nodes.retain(|n| match &n.kind {
                DataFlowNodeKind::ForLoopInit {
                    var_name,
                    start_offset,
                    end_offset,
                } => {
                    var_name == var_id
                        && pos.start_offset() > *start_offset
                        && pos.end_offset() < *end_offset
                }
                _ => false,
            });

            if !origin_nodes.is_empty() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::ForLoopInvalidation,
                        format!("{} was previously assigned in a for loop", var_id),
                        statements_analyzer.get_hpos(&pos),
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
        let root_var_id = get_root_var_id(
            assign_var,
            context.function_context.calling_class.as_ref(),
            Some(statements_analyzer.get_file_analyzer().get_file_source()),
        );

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

    if assign_value_type.is_mixed() {
        // we don't really need to know about MixedAssignment, but in Psalm we trigger an issue here
    } else {
        // todo increment non-mixed count
    }

    match &assign_var.2 {
        aast::Expr_::Lvar(_) => analyze_assignment_to_variable(
            statements_analyzer,
            assign_var,
            assign_value,
            assign_value_type,
            var_id.as_ref().unwrap(),
            analysis_data,
            context,
            is_inout,
        ),
        aast::Expr_::ArrayGet(boxed) => {
            array_assignment_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, boxed.1.as_ref(), assign_var.pos()),
                assign_value_type,
                pos,
                analysis_data,
                context,
            );
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
            );
        }
        aast::Expr_::ClassGet(boxed) => {
            let (lhs, rhs, _) = (&boxed.0, &boxed.1, &boxed.2);

            static_property_assignment_analyzer::analyze(
                statements_analyzer,
                (lhs, &rhs),
                if let Some(assign_value) = assign_value {
                    Some(assign_value.pos())
                } else {
                    None
                },
                &assign_value_type,
                &var_id,
                analysis_data,
                context,
            );
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
                    statements_analyzer.get_hpos(&pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    };

    Ok(())
}

fn check_variable_or_property_assignment(
    statements_analyzer: &StatementsAnalyzer,
    var_type: TUnion,
    analysis_data: &mut FunctionAnalysisData,
    assign_var_pos: &Pos,
    var_id: &String,
    context: &ScopeContext,
) -> TUnion {
    if var_type.is_void() {
        // todo (maybe) handle void assignment
    }
    if var_type.is_nothing() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::ImpossibleAssignment,
                "This assignment is impossible".to_string(),
                statements_analyzer.get_hpos(&assign_var_pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }
    let ref mut data_flow_graph = analysis_data.data_flow_graph;

    if !var_type.parent_nodes.is_empty()
        && (matches!(&data_flow_graph.kind, GraphKind::FunctionBody) || context.allow_taints)
    {
        let removed_taints = get_removed_taints_in_comments(statements_analyzer, assign_var_pos);

        // todo create AddRemoveTaintsEvent
        return add_dataflow_to_assignment(
            statements_analyzer,
            var_type,
            data_flow_graph,
            var_id,
            assign_var_pos,
            FxHashSet::default(),
            removed_taints,
        );
    }

    return var_type;
}

fn analyze_list_assignment(
    statements_analyzer: &StatementsAnalyzer,
    expressions: &Vec<aast::Expr<(), ()>>,
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
            statements_analyzer.get_file_analyzer().get_file_source(),
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
                        type_param.clone()
                    }
                } else {
                    type_param.clone()
                }
            } else if let TAtomic::TNamedObject {
                name,
                type_params: Some(type_params),
                ..
            } = assign_value_atomic_type
            {
                if statements_analyzer.get_interner().lookup(name) == "HH\\Vector" {
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
                statements_analyzer.get_file_analyzer().get_file_source(),
                statements_analyzer.get_file_analyzer().resolved_names,
                Some((
                    statements_analyzer.get_codebase(),
                    statements_analyzer.get_interner(),
                )),
            );

            let keyed_array_var_id = if let Some(source_expr_id) = source_expr_id {
                Some(source_expr_id + "['" + offset.to_string().as_str() + "']")
            } else {
                None
            };

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
            false,
        )
        .ok();
    }
}

pub(crate) fn add_dataflow_to_assignment(
    statements_analyzer: &StatementsAnalyzer,
    mut assignment_type: TUnion,
    data_flow_graph: &mut DataFlowGraph,
    var_id: &String,
    var_pos: &Pos,
    added_taints: FxHashSet<SinkType>,
    removed_taints: FxHashSet<SinkType>,
) -> TUnion {
    if let GraphKind::WholeProgram(WholeProgramKind::Taint) = &data_flow_graph.kind {
        if !assignment_type.has_taintable_value() {
            return assignment_type;
        }
    }

    let parent_nodes = &assignment_type.parent_nodes;
    let mut new_parent_nodes = FxHashSet::default();

    let new_parent_node =
        DataFlowNode::get_for_assignment(var_id.clone(), statements_analyzer.get_hpos(var_pos));
    data_flow_graph.add_node(new_parent_node.clone());
    new_parent_nodes.insert(new_parent_node.clone());

    for parent_node in parent_nodes {
        data_flow_graph.add_path(
            parent_node,
            &new_parent_node,
            PathKind::Default,
            if added_taints.is_empty() {
                None
            } else {
                Some(added_taints.clone())
            },
            if removed_taints.is_empty() {
                None
            } else {
                Some(removed_taints.clone())
            },
        );
    }

    assignment_type.parent_nodes = new_parent_nodes;

    assignment_type
}

fn analyze_assignment_to_variable(
    statements_analyzer: &StatementsAnalyzer,
    var_expr: &aast::Expr<(), ()>,
    source_expr: Option<&aast::Expr<(), ()>>,
    assign_value_type: TUnion,
    var_id: &String,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    is_inout: bool,
) {
    if !is_inout {
        if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
            analysis_data
                .data_flow_graph
                .add_node(DataFlowNode::get_for_variable_source(
                    var_id.clone(),
                    statements_analyzer.get_hpos(var_expr.pos()),
                    !context.inside_awaitall
                        && if let Some(source_expr) = source_expr {
                            analysis_data.is_pure(source_expr.pos())
                        } else {
                            false
                        },
                ));
        }
    }

    let assign_value_type = check_variable_or_property_assignment(
        statements_analyzer,
        assign_value_type,
        analysis_data,
        var_expr.pos(),
        var_id,
        &context,
    );

    if assign_value_type.is_bool() {
        if let Some(source_expr) = source_expr {
            if matches!(source_expr.2, aast::Expr_::Binop(..)) {
                // todo support $a = !($b || $c)
                let var_object_id = (var_expr.pos().start_offset(), var_expr.pos().end_offset());
                let cond_object_id = (
                    source_expr.pos().start_offset(),
                    source_expr.pos().end_offset(),
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
                        &var_id,
                        right_clauses.into_iter().map(|v| Rc::new(v)).collect(),
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
                        assignment_clauses.into_iter().map(|v| Rc::new(v)).collect()
                    } else {
                        vec![]
                    };

                    context.clauses.extend(assignment_clauses);
                }
            }
        }
    }

    context
        .vars_in_scope
        .insert(var_id.clone(), Rc::new(assign_value_type));
}

pub(crate) fn analyze_inout_param(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    arg_type: TUnion,
    inout_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) {
    if let Ok(_) = analyze(
        statements_analyzer,
        (&ast_defs::Bop::Eq(None), expr, None),
        expr.pos(),
        Some(inout_type),
        analysis_data,
        context,
        true,
    ) {
        analysis_data.set_expr_type(expr.pos(), arg_type.clone());
    }

    analysis_data.expr_effects.insert(
        (expr.pos().start_offset(), expr.pos().end_offset()),
        EFFECT_WRITE_LOCAL,
    );
}
