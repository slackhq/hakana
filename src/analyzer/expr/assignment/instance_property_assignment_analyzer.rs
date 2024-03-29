use std::rc::Rc;

use hakana_reflection_info::{
    codebase_info::CodebaseInfo,
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    function_context::FunctionLikeIdentifier,
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    t_union::TUnion,
    EFFECT_WRITE_PROPS,
};
use hakana_str::StrId;
use hakana_type::{
    add_optional_union_type, get_mixed_any,
    type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator},
    type_expander::{self, StaticClassType, TypeExpansionOptions},
};
use oxidized::{
    aast::{self, Expr},
    ast_defs::Pos,
};
use rustc_hash::FxHashMap;

use crate::{
    expr::{
        call::argument_analyzer::get_removed_taints_in_comments, expression_identifier,
        fetch::atomic_property_fetch_analyzer::localize_property_type,
    },
    function_analysis_data::FunctionAnalysisData,
    stmt_analyzer::AnalysisError,
};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, &Expr<(), ()>),
    pos: &Pos,
    var_id: Option<String>,
    assign_value_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.get_codebase();
    let stmt_var = expr.0;

    // TODO if ($stmt instanceof PropertyProperty) {

    let assigned_properties = analyze_regular_assignment(
        statements_analyzer,
        expr,
        pos,
        var_id.clone(),
        assign_value_type,
        analysis_data,
        context,
    )?;

    if assigned_properties.is_empty() || assign_value_type.is_mixed() {
        return Ok(());
    }

    for assigned_property in &assigned_properties {
        let class_property_type = &assigned_property.0;
        let assignment_type = &assigned_property.2;

        if class_property_type.is_mixed() {
            continue;
        }

        let mut union_comparison_result = TypeComparisonResult::new();
        let mut invalid_assignment_value_types = FxHashMap::default();

        let type_match_found = union_type_comparator::is_contained_by(
            codebase,
            assignment_type,
            class_property_type,
            true,
            assignment_type.ignore_falsable_issues,
            false,
            &mut union_comparison_result,
        );

        if type_match_found {
            if let Some(union_type) = union_comparison_result.replacement_union_type {
                if let Some(var_id) = var_id.clone() {
                    context.vars_in_scope.insert(var_id, Rc::new(union_type));
                }
            }

            for (name, mut bound) in union_comparison_result.type_variable_lower_bounds {
                if let Some((lower_bounds, _)) = analysis_data.type_variable_bounds.get_mut(&name) {
                    bound.pos = Some(statements_analyzer.get_hpos(pos));
                    lower_bounds.push(bound);
                }
            }

            for (name, mut bound) in union_comparison_result.type_variable_upper_bounds {
                if let Some((_, upper_bounds)) = analysis_data.type_variable_bounds.get_mut(&name) {
                    bound.pos = Some(statements_analyzer.get_hpos(pos));
                    upper_bounds.push(bound);
                }
            }

            if union_comparison_result.upcasted_awaitable {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::UpcastAwaitable,
                        format!(
                            "{} contains Awaitable but was passed into a more general type {}",
                            assignment_type.get_id(Some(statements_analyzer.get_interner())),
                            class_property_type.get_id(Some(statements_analyzer.get_interner())),
                        ),
                        statements_analyzer.get_hpos(&stmt_var.1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        } else {
            if union_comparison_result.type_coerced.unwrap_or(false) {
                if union_comparison_result
                    .type_coerced_from_nested_mixed
                    .unwrap_or(false)
                {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::MixedPropertyTypeCoercion,
                            format!(
                                "{} expects {}, parent type {} provided",
                                var_id.clone().unwrap_or("var".to_string()),
                                class_property_type
                                    .get_id(Some(statements_analyzer.get_interner())),
                                assignment_type.get_id(Some(statements_analyzer.get_interner())),
                            ),
                            statements_analyzer.get_hpos(&stmt_var.1),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                } else {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::PropertyTypeCoercion,
                            format!(
                                "{} expects {}, parent type {} provided",
                                var_id.clone().unwrap_or("var".to_string()),
                                class_property_type
                                    .get_id(Some(statements_analyzer.get_interner())),
                                assignment_type.get_id(Some(statements_analyzer.get_interner())),
                            ),
                            statements_analyzer.get_hpos(&stmt_var.1),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }

            if union_comparison_result.type_coerced.is_none() {
                // if union_type_comparator::is_contained_by(
                //     codebase,
                //     assignment_type,
                //     class_property_type,
                //     true,
                //     true,
                //     false,
                //     &mut union_comparison_result,
                // ) {
                //     has_valid_assignment_value_type = true;
                // }
                invalid_assignment_value_types.insert(
                    &assigned_property.1 .1,
                    class_property_type.get_id(Some(statements_analyzer.get_interner())),
                );
            } else {
                // has_valid_assignment_value_type = true;
            }
        }

        if let Some((property_id, invalid_class_property_type)) =
            invalid_assignment_value_types.iter().next()
        {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::InvalidPropertyAssignmentValue,
                    format!(
                        "Property ${} with declared type {}, cannot be assigned type {}",
                        statements_analyzer.get_interner().lookup(property_id),
                        invalid_class_property_type,
                        assignment_type.get_id(Some(statements_analyzer.get_interner())),
                    ),
                    statements_analyzer.get_hpos(&stmt_var.1),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            return Ok(());
        }
    }
    Ok(())
}

pub(crate) fn analyze_regular_assignment(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, &Expr<(), ()>),
    pos: &Pos,
    var_id: Option<String>,
    assign_value_type: &TUnion,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<Vec<(TUnion, (StrId, StrId), TUnion)>, AnalysisError> {
    let stmt_var = expr.0;

    let mut assigned_properties = Vec::new();
    let mut context_type = None;
    let codebase = statements_analyzer.get_codebase();

    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;

    expression_analyzer::analyze(
        statements_analyzer,
        stmt_var,
        analysis_data,
        context,
        &mut None,
    )?;

    context.inside_general_use = was_inside_general_use;

    analysis_data.expr_effects.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        EFFECT_WRITE_PROPS,
    );

    let lhs_type = analysis_data.get_rc_expr_type(stmt_var.pos()).cloned();

    if lhs_type.is_none() {
        return Ok(assigned_properties);
    }

    let lhs_var_id = expression_identifier::get_var_id(
        stmt_var,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some((
            statements_analyzer.get_codebase(),
            statements_analyzer.get_interner(),
        )),
    );

    // if let Some(var_id) = var_id.clone() {
    //     // TODO: Emit warning
    // }

    if let Some(lhs_type) = lhs_type {
        let mut mixed_with_any = false;
        if lhs_type.is_mixed_with_any(&mut mixed_with_any) {
            if mixed_with_any {
                for origin in &lhs_type.parent_nodes {
                    analysis_data
                        .data_flow_graph
                        .add_mixed_data(origin, expr.1.pos());
                }

                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedAnyPropertyAssignment,
                        lhs_var_id.unwrap_or("data".to_string())
                            + " of type mixed cannot be assigned.",
                        statements_analyzer.get_hpos(&expr.1 .1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            } else {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedPropertyAssignment,
                        lhs_var_id.unwrap_or("data".to_string())
                            + " of type mixed cannot be assigned.",
                        statements_analyzer.get_hpos(&expr.1 .1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }

            return Ok(assigned_properties);
        }

        if lhs_type.is_null() {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NullablePropertyAssignment,
                    lhs_var_id.unwrap_or("data".to_string()) + " of type null cannot be assigned.",
                    statements_analyzer.get_hpos(&expr.1 .1),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
            return Ok(assigned_properties);
        }

        if lhs_type.is_nullable() {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NullablePropertyAssignment,
                    lhs_var_id.clone().unwrap_or("data".to_string())
                        + " with possibly null type cannot be assigned.",
                    statements_analyzer.get_hpos(&expr.1 .1),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }

        for lhs_type_part in &lhs_type.types {
            if let TAtomic::TNull { .. } = lhs_type_part {
                continue;
            }

            let assigned_prop = analyze_atomic_assignment(
                statements_analyzer,
                expr,
                assign_value_type,
                lhs_type_part,
                analysis_data,
                context,
                lhs_type.reference_free,
            );

            if let Some(assigned_prop) = assigned_prop {
                assigned_properties.push(assigned_prop.clone());

                context_type = Some(add_optional_union_type(
                    assigned_prop.2,
                    context_type.as_ref(),
                    codebase,
                ));
            }
        }
    }

    // TODO if ($invalid_assignment_types) {

    if let Some(var_id) = var_id {
        let context_type = Rc::new(context_type.unwrap_or(get_mixed_any()).clone());

        context
            .vars_in_scope
            .insert(var_id.to_owned(), context_type);
    }

    Ok(assigned_properties)
}

pub(crate) fn analyze_atomic_assignment(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, &Expr<(), ()>),
    assign_value_type: &TUnion,
    lhs_type_part: &TAtomic,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    is_lhs_reference_free: bool,
) -> Option<(TUnion, (StrId, StrId), TUnion)> {
    let codebase = statements_analyzer.get_codebase();
    let fq_class_name = match lhs_type_part {
        TAtomic::TNamedObject { name, .. } => *name,
        TAtomic::TReference { name, .. } => {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentClass,
                    format!(
                        "Undefined class {}",
                        statements_analyzer.get_interner().lookup(name)
                    ),
                    statements_analyzer.get_hpos(expr.1.pos()),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            analysis_data.symbol_references.add_reference_to_symbol(
                &context.function_context,
                *name,
                false,
            );

            return None;
        }
        _ => return None,
    };

    let prop_name = if let aast::Expr_::Id(id) = &expr.1 .2 {
        if let Some(prop_name) = statements_analyzer.get_interner().get(&id.1) {
            prop_name
        } else {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentProperty,
                    format!(
                        "Undefined property {}::${}",
                        statements_analyzer.get_interner().lookup(&fq_class_name),
                        &id.1
                    ),
                    statements_analyzer.get_hpos(expr.1.pos()),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            analysis_data.symbol_references.add_reference_to_symbol(
                &context.function_context,
                fq_class_name,
                false,
            );

            return None;
        }
    } else {
        return None;
    };

    let property_id = (fq_class_name, prop_name);

    // TODO self assignments

    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        let var_id = expression_identifier::get_var_id(
            expr.0,
            None,
            statements_analyzer.get_file_analyzer().resolved_names,
            Some((
                statements_analyzer.get_codebase(),
                statements_analyzer.get_interner(),
            )),
        );

        add_instance_property_dataflow(
            statements_analyzer,
            &var_id,
            expr.0.pos(),
            expr.1.pos(),
            analysis_data,
            context,
            assign_value_type,
            &prop_name,
            &fq_class_name,
            &property_id,
        );
    }

    // TODO property does not exist, emit errors

    let declaring_property_class =
        codebase.get_declaring_class_for_property(&fq_class_name, &prop_name);

    if let Some(property_storage) = codebase.get_property_storage(&fq_class_name, &prop_name) {
        if !property_storage.is_promoted {
            analysis_data
                .symbol_references
                .add_reference_to_class_member(
                    &context.function_context,
                    (fq_class_name, prop_name),
                    false,
                );
        }
    } else {
        analysis_data
            .symbol_references
            .add_reference_to_class_member(
                &context.function_context,
                (fq_class_name, prop_name),
                false,
            );
    }

    if let Some(declaring_property_class) = declaring_property_class {
        let declaring_classlike_storage = codebase
            .classlike_infos
            .get(declaring_property_class)
            .unwrap();

        if declaring_classlike_storage.immutable && !is_lhs_reference_free {
            let in_self_constructor =
                if let Some(FunctionLikeIdentifier::Method(context_class, StrId::CONSTRUCT)) =
                    context.function_context.calling_functionlike_id
                {
                    context_class == *declaring_property_class
                } else {
                    false
                };

            if !in_self_constructor {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::ImmutablePropertyWrite,
                        format!(
                            "Property {}::${} is defined on an immutable class",
                            statements_analyzer.get_interner().lookup(&property_id.0),
                            statements_analyzer.get_interner().lookup(&property_id.1),
                        ),
                        statements_analyzer.get_hpos(expr.1.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }

        // TODO trackPropertyImpurity and mutatable/immtable states
        let mut class_property_type =
            if let Some(prop_type) = codebase.get_property_type(&fq_class_name, &prop_name) {
                prop_type
            } else {
                get_mixed_any()
            };

        if let TAtomic::TNamedObject {
            type_params: Some(_),
            ..
        } = lhs_type_part
        {
            class_property_type = localize_property_type(
                codebase,
                class_property_type,
                lhs_type_part,
                codebase.classlike_infos.get(&fq_class_name).unwrap(),
                declaring_classlike_storage,
                analysis_data,
            );
        }

        if !class_property_type.is_mixed() {
            type_expander::expand_union(
                codebase,
                &Some(statements_analyzer.get_interner()),
                &mut class_property_type,
                &TypeExpansionOptions {
                    self_class: Some(&declaring_classlike_storage.name),
                    static_class_type: StaticClassType::Name(&declaring_classlike_storage.name),
                    parent_class: declaring_classlike_storage.direct_parent_class.as_ref(),
                    file_path: Some(
                        &statements_analyzer
                            .get_file_analyzer()
                            .get_file_source()
                            .file_path,
                    ),
                    ..Default::default()
                },
                &mut analysis_data.data_flow_graph,
            );

            // TODO localizeType

            // TODO if (!$class_property_type->hasMixed() && $assignment_value_type->hasMixed()) {

            return Some((
                class_property_type.clone(),
                property_id,
                assign_value_type.clone(),
            ));
        }
    } else {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentProperty,
                format!(
                    "Undefined property {}::${}",
                    statements_analyzer.get_interner().lookup(&property_id.0),
                    statements_analyzer.get_interner().lookup(&property_id.1),
                ),
                statements_analyzer.get_hpos(expr.1.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    None
}

fn add_instance_property_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    lhs_var_id: &Option<String>,
    var_pos: &Pos,
    name_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    assignment_value_type: &TUnion,
    prop_name: &StrId,
    fq_class_name: &StrId,
    property_id: &(StrId, StrId),
) {
    let codebase = statements_analyzer.get_codebase();

    if let Some(classlike_storage) = codebase.classlike_infos.get(fq_class_name) {
        if classlike_storage.specialize_instance {
            if let Some(lhs_var_id) = lhs_var_id.to_owned() {
                add_instance_property_assignment_dataflow(
                    statements_analyzer,
                    analysis_data,
                    lhs_var_id,
                    var_pos,
                    name_pos,
                    property_id,
                    assignment_value_type,
                    context,
                );
            }
        } else {
            add_unspecialized_property_assignment_dataflow(
                statements_analyzer,
                property_id,
                name_pos,
                Some(var_pos),
                analysis_data,
                assignment_value_type,
                codebase,
                fq_class_name,
                *prop_name,
            );
        }
    }
}

fn add_instance_property_assignment_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    lhs_var_id: String,
    var_pos: &Pos,
    name_pos: &Pos,
    property_id: &(StrId, StrId),
    assignment_value_type: &TUnion,
    context: &mut ScopeContext,
) {
    let var_node =
        DataFlowNode::get_for_lvar(lhs_var_id.to_owned(), statements_analyzer.get_hpos(var_pos));
    analysis_data.data_flow_graph.add_node(var_node.clone());
    let property_node = DataFlowNode::get_for_local_property_fetch(
        &lhs_var_id,
        property_id.1,
        statements_analyzer.get_hpos(name_pos),
    );
    analysis_data
        .data_flow_graph
        .add_node(property_node.clone());
    analysis_data.data_flow_graph.add_path(
        &property_node,
        &var_node,
        PathKind::PropertyAssignment(property_id.0, property_id.1),
        vec![],
        vec![],
    );
    for parent_node in assignment_value_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            parent_node,
            &property_node,
            PathKind::Default,
            vec![],
            vec![],
        );
    }
    let stmt_var_type = context.vars_in_scope.get_mut(&lhs_var_id);
    if let Some(stmt_var_type) = stmt_var_type {
        let mut stmt_type_inner = (**stmt_var_type).clone();

        if !stmt_type_inner
            .parent_nodes
            .iter()
            .any(|n| n.id == var_node.id)
        {
            stmt_type_inner.parent_nodes.push(var_node.clone());
        }

        *stmt_var_type = Rc::new(stmt_type_inner);
    }
}

pub(crate) fn add_unspecialized_property_assignment_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    property_id: &(StrId, StrId),
    stmt_name_pos: &Pos,
    var_pos: Option<&Pos>,
    analysis_data: &mut FunctionAnalysisData,
    assignment_value_type: &TUnion,
    codebase: &CodebaseInfo,
    fq_class_name: &StrId,
    prop_name: StrId,
) {
    let localized_property_node = DataFlowNode::get_for_localized_property(
        *property_id,
        statements_analyzer.get_hpos(stmt_name_pos),
    );

    analysis_data
        .data_flow_graph
        .add_node(localized_property_node.clone());

    let removed_taints = if let Some(var_pos) = var_pos {
        get_removed_taints_in_comments(statements_analyzer, var_pos)
    } else {
        vec![]
    };

    let property_node = DataFlowNode::get_for_property(*property_id);

    analysis_data
        .data_flow_graph
        .add_node(property_node.clone());
    analysis_data.data_flow_graph.add_path(
        &localized_property_node,
        &property_node,
        PathKind::PropertyAssignment(property_id.0, property_id.1),
        vec![],
        removed_taints,
    );

    for parent_node in assignment_value_type.parent_nodes.iter() {
        analysis_data.data_flow_graph.add_path(
            parent_node,
            &localized_property_node,
            PathKind::Default,
            vec![],
            vec![],
        );
    }

    let declaring_property_class =
        codebase.get_declaring_class_for_property(fq_class_name, &prop_name);

    if let Some(declaring_property_class) = declaring_property_class {
        if declaring_property_class != fq_class_name {
            let declaring_property_node = DataFlowNode::get_for_property(*property_id);

            analysis_data.data_flow_graph.add_path(
                &property_node,
                &declaring_property_node,
                PathKind::PropertyAssignment(property_id.0, property_id.1),
                vec![],
                vec![],
            );

            analysis_data
                .data_flow_graph
                .add_node(declaring_property_node);
        }
    }
}
