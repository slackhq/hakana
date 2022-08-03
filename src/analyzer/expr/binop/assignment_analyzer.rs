use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;

use crate::expr::assignment::array_assignment_analyzer;
use crate::expr::assignment::instance_property_assignment_analyzer;
use crate::expr::assignment::static_property_assignment_analyzer;
use crate::expr::expression_identifier;
use crate::expr::expression_identifier::get_extended_var_id;
use crate::expr::expression_identifier::get_root_var_id;
use crate::expr::expression_identifier::get_var_id;
use crate::expr::fetch::array_fetch_analyzer;
use crate::expression_analyzer;
use crate::formula_generator;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
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
use hakana_reflection_info::taint::TaintType;
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    is_inout: bool,
) -> Result<(), ()> {
    let (binop, assign_var, assign_value) = (expr.0, expr.1, expr.2);

    let var_id = get_var_id(
        assign_var,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
    );

    let extended_var_id = get_extended_var_id(
        assign_var,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
    );

    //let removed_taints = Vec::new();

    let mut extended_var_type = None;

    if let Some(extended_var_id) = &extended_var_id {
        context.cond_referenced_var_ids.remove(extended_var_id);
        context
            .assigned_var_ids
            .insert(extended_var_id.clone(), assign_var.pos().start_offset());
        context
            .possibly_assigned_var_ids
            .insert(extended_var_id.clone());

        extended_var_type = context.vars_in_scope.get(extended_var_id).cloned();
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
                let tast_expr_types = tast_info.expr_types.clone();

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
                    tast_info,
                    context,
                    &mut None,
                );

                let new_expr_types = tast_info.expr_types.clone();
                let expr_type = new_expr_types
                    .get(&(pos.start_offset(), pos.end_offset()))
                    .cloned();

                tast_info.expr_types = tast_expr_types;

                if let Some(expr_type) = expr_type {
                    tast_info.expr_types.insert(
                        (assign_value.1.start_offset(), assign_value.1.end_offset()),
                        expr_type,
                    );
                };

                analyzed_ok
            }
            _ => expression_analyzer::analyze(
                statements_analyzer,
                assign_value,
                tast_info,
                context,
                &mut None,
            ),
        };

        context.inside_general_use = false;

        if !analyzed_ok {
            if let Some(var_id) = var_id {
                if let Some(extended_var_id) = &extended_var_id {
                    if let Some(existing_type) = context.vars_in_scope.clone().get(extended_var_id)
                    {
                        context.remove_descendants(
                            extended_var_id,
                            existing_type,
                            assign_value_type,
                            None,
                            tast_info,
                        );
                    }
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
            if let Some(var_type) = tast_info.get_expr_type(&assign_value.1) {
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

    if tast_info.data_flow_graph.kind == GraphKind::Variable
        && assign_value_type.parent_nodes.is_empty()
    {
        if let Some(extended_var_id) = &extended_var_id {
            let assignment_node = DataFlowNode::get_for_assignment(
                extended_var_id.clone(),
                statements_analyzer.get_hpos(assign_var.pos()),
                None,
            );

            assign_value_type
                .parent_nodes
                .insert(assignment_node.id.clone(), assignment_node);
        };
    }

    if let Some((extended_var_id, extended_var_type)) =
        if let Some(extended_var_id) = &extended_var_id {
            if let Some(extended_var_type) = extended_var_type {
                Some((extended_var_id.clone(), extended_var_type))
            } else {
                None
            }
        } else {
            None
        }
    {
        context.remove_descendants(
            &extended_var_id,
            &extended_var_type,
            Some(&assign_value_type),
            Some(statements_analyzer),
            tast_info,
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
                    tast_info,
                );
            }
        }
    }

    if assign_value_type.is_mixed() {
        // we don't really need to know about MixedAssignment, but in Psalm we trigger an issue here
    } else {
        // todo increment non-mixed count
    }

    if let Some(var_id) = &var_id {
        if context.protected_var_ids.contains(var_id) && assign_value_type.has_literal_value() {
            // handle loop invalidation
        }
    }

    match &assign_var.2 {
        aast::Expr_::Lvar(_) => analyze_assignment_to_variable(
            statements_analyzer,
            assign_var,
            assign_value,
            assign_value_type,
            &var_id.clone().unwrap(),
            tast_info,
            context,
            is_inout,
        ),
        aast::Expr_::ArrayGet(boxed) => {
            array_assignment_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, boxed.1.as_ref(), assign_var.pos()),
                assign_value_type,
                &pos.clone(),
                tast_info,
                context,
            );
        }
        aast::Expr_::ObjGet(boxed) => {
            let var_id = expression_identifier::get_var_id(
                &assign_var,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                statements_analyzer.get_file_analyzer().resolved_names,
            );
            instance_property_assignment_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1),
                var_id,
                &assign_value_type,
                tast_info,
                context,
            );
        }
        aast::Expr_::ClassGet(boxed) => {
            let (lhs, rhs, _) = (&boxed.0, &boxed.1, &boxed.2);

            static_property_assignment_analyzer::analyze(
                statements_analyzer,
                (lhs, &rhs),
                &assign_value_type,
                tast_info,
                context,
            );
        }
        aast::Expr_::List(expressions) => analyze_list_assignment(
            statements_analyzer,
            expressions,
            assign_value,
            &assign_value_type,
            tast_info,
            context,
        ),
        _ => {
            //println!("{:#?}", expr);
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::UnrecognizedExpression,
                "Unrecognized expression in assignment".to_string(),
                statements_analyzer.get_hpos(&assign_var.1),
            ));
        }
    };

    Ok(())
}

fn check_variable_or_property_assignment(
    statements_analyzer: &StatementsAnalyzer,
    var_type: TUnion,
    tast_info: &mut TastInfo,
    assign_var_pos: &Pos,
    var_id: &String,
) -> TUnion {
    if var_type.is_void() {
        // todo (maybe) handle void assignment
    }
    if var_type.is_nothing() {
        tast_info.maybe_add_issue(Issue::new(
            IssueKind::ImpossibleAssignment,
            "This assignment is impossible".to_string(),
            statements_analyzer.get_hpos(&assign_var_pos),
        ));
    }
    let ref mut data_flow_graph = tast_info.data_flow_graph;

    if !var_type.parent_nodes.is_empty() {
        // todo create AddRemoveTaintsEvent
        return add_dataflow_to_assignment(
            statements_analyzer,
            var_type,
            data_flow_graph,
            var_id,
            assign_var_pos,
            HashSet::new(),
            HashSet::new(),
        );
    }

    return var_type;
}

fn analyze_list_assignment(
    statements_analyzer: &StatementsAnalyzer,
    expressions: &Vec<aast::Expr<(), ()>>,
    source_expr: Option<&aast::Expr<(), ()>>,
    assign_value_type: &TUnion,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) {
    let codebase = statements_analyzer.get_codebase();

    for (offset, assign_var_item) in expressions.iter().enumerate() {
        let list_var_id = expression_identifier::get_extended_var_id(
            assign_var_item,
            context.function_context.calling_class.as_ref(),
            statements_analyzer.get_file_analyzer().get_file_source(),
            statements_analyzer.get_file_analyzer().resolved_names,
        );

        if list_var_id.unwrap_or("".to_string()) == "$_" {
            continue;
        }

        let mut value_type = get_nothing();

        for (_, assign_value_atomic_type) in &assign_value_type.types {
            let atomic_value_type = if let TAtomic::TVec {
                known_items,
                type_param,
                ..
            } = assign_value_atomic_type
            {
                if let Some(known_items) = known_items {
                    if let Some((possibly_undefined, value_type)) = known_items.get(&offset) {
                        if *possibly_undefined {
                            tast_info.maybe_add_issue(Issue::new(
                                IssueKind::PossiblyUndefinedIntArrayOffset,
                                "Possibly undefined offset in list assignment".to_string(),
                                statements_analyzer.get_hpos(&assign_var_item.1),
                            ));
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
                if name == "HH\\Vector" {
                    type_params[0].clone()
                } else {
                    get_nothing()
                }
            } else if let TAtomic::TMixedAny = assign_value_atomic_type {
                get_mixed_any()
            } else if assign_value_atomic_type.is_mixed() {
                get_mixed()
            } else {
                get_nothing()
            };

            value_type = add_union_type(value_type, &atomic_value_type, Some(codebase), false);
        }

        if let Some(source_expr) = source_expr {
            let source_expr_id = expression_identifier::get_extended_var_id(
                source_expr,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                statements_analyzer.get_file_analyzer().resolved_names,
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
                tast_info,
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
            tast_info,
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
    removed_taints: HashSet<TaintType>,
    added_taints: HashSet<TaintType>,
) -> TUnion {
    if data_flow_graph.kind == GraphKind::Taint {
        if !assignment_type.has_taintable_value() {
            return assignment_type;
        }
    }

    let parent_nodes = &assignment_type.parent_nodes;
    let mut new_parent_nodes = HashMap::new();

    let new_parent_node = DataFlowNode::get_for_assignment(
        var_id.clone(),
        statements_analyzer.get_hpos(var_pos),
        None,
    );
    data_flow_graph.add_node(new_parent_node.clone());
    new_parent_nodes.insert(new_parent_node.id.clone(), new_parent_node.clone());

    for (_, parent_node) in parent_nodes {
        data_flow_graph.add_path(
            parent_node,
            &new_parent_node,
            PathKind::Default,
            added_taints.clone(),
            removed_taints.clone(),
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    is_inout: bool,
) {
    let assignment_node = DataFlowNode::get_for_assignment(
        var_id.clone(),
        statements_analyzer.get_hpos(var_expr.pos()),
        None,
    );

    if !is_inout {
        if tast_info.data_flow_graph.kind == GraphKind::Variable {
            tast_info
                .data_flow_graph
                .add_source(assignment_node.clone());
        }
    }

    let assign_value_type = check_variable_or_property_assignment(
        statements_analyzer,
        assign_value_type,
        tast_info,
        var_expr.pos(),
        var_id,
    );

    if assign_value_type.get_id() == "bool" {
        if let Some(source_expr) = source_expr {
            if matches!(source_expr.2, aast::Expr_::Binop(..)) {
                // todo support $a = !($b || $c)
                let var_object_id = (var_expr.pos().start_offset(), var_expr.pos().end_offset());
                let cond_object_id = (
                    source_expr.pos().start_offset(),
                    source_expr.pos().end_offset(),
                );

                let assertion_context = statements_analyzer
                    .get_assertion_context(context.function_context.calling_class.as_ref());

                let right_clauses = formula_generator::get_formula(
                    cond_object_id,
                    cond_object_id,
                    source_expr,
                    &assertion_context,
                    tast_info,
                    true,
                    false,
                );

                if let Ok(right_clauses) = right_clauses {
                    let right_clauses = ScopeContext::filter_clauses(
                        &var_id,
                        right_clauses.into_iter().map(|v| Rc::new(v)).collect(),
                        None,
                        None,
                        tast_info,
                    );

                    let mut possibilities = BTreeMap::new();
                    possibilities.insert(
                        var_id.clone(),
                        BTreeMap::from([(Assertion::Falsy.to_string(), Assertion::Falsy)]),
                    );

                    let assignment_clauses = if let Ok(assignment_clauses) =
                        hakana_algebra::combine_ored_clauses(
                            &vec![Clause::new(
                                possibilities,
                                var_object_id,
                                var_object_id,
                                None,
                                None,
                                None,
                                None,
                            )],
                            &right_clauses.into_iter().map(|v| (*v).clone()).collect(),
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
    arg_type: &TUnion,
    mut inout_type: TUnion,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) {
    inout_type
        .parent_nodes
        .extend(arg_type.parent_nodes.clone());

    if let Ok(_) = analyze(
        statements_analyzer,
        (&ast_defs::Bop::Eq(None), expr, None),
        expr.pos(),
        Some(&inout_type),
        tast_info,
        context,
        true,
    ) {
        tast_info.set_expr_type(expr.pos(), arg_type.clone());
    }
}
