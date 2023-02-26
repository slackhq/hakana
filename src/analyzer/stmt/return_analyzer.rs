use crate::scope_context::ScopeContext;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::StrId;
use hakana_reflection_info::{
    data_flow::{
        graph::{DataFlowGraph, GraphKind},
        node::DataFlowNode,
        path::PathKind,
    },
    functionlike_info::FunctionLikeInfo,
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_type::{
    get_mixed_any, get_null, get_void,
    type_comparator::type_comparison_result::TypeComparisonResult,
    type_expander::{self, TypeExpansionOptions},
    wrap_atomic,
};
use hakana_type::{type_comparator::union_type_comparator, type_expander::StaticClassType};
use oxidized::{aast, aast::Pos};
use rustc_hash::FxHashSet;

use crate::{
    expression_analyzer, scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};

pub(crate) fn analyze(
    stmt: &aast::Stmt<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) {
    let return_expr = stmt.1.as_return().unwrap();

    let interner = &statements_analyzer.get_codebase().interner;

    let mut inferred_return_type = if let Some(return_expr) = return_expr {
        context.inside_return = true;
        expression_analyzer::analyze(
            statements_analyzer,
            return_expr,
            tast_info,
            context,
            &mut None,
        );
        context.inside_return = false;

        if let Some(mut inferred_return_type) = tast_info.get_expr_type(&return_expr.1).cloned() {
            if inferred_return_type.is_nothing() {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::NothingReturn,
                        "This function call evaluates to nothing — likely calling a noreturn function"
                            .to_string(),
                        statements_analyzer.get_hpos(&return_expr.1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(), statements_analyzer.get_file_path_actual()
                );
            }

            if inferred_return_type.is_void() {
                inferred_return_type = get_null();
            }

            inferred_return_type
        } else {
            get_mixed_any()
        }
    } else {
        get_void()
    };

    if let Some(_) = &context.finally_scope {
        // todo handle finally
    }

    context.has_returned = true;

    let functionlike_storage = if let Some(s) = statements_analyzer.get_functionlike_info() {
        s
    } else {
        // should never happen, but some tests have return in the flow
        return;
    };

    handle_inout_at_return(
        functionlike_storage,
        statements_analyzer,
        context,
        tast_info,
        Some(&stmt.0),
    );

    // todo maybe check inout params here, though that's covered by Hack's typechecker
    // examineParamTypes in Psalm's source code

    type_expander::expand_union(
        statements_analyzer.get_codebase(),
        &mut inferred_return_type,
        &TypeExpansionOptions {
            self_class: context.function_context.calling_class.as_ref(),
            static_class_type: if let Some(calling_class) = &context.function_context.calling_class
            {
                StaticClassType::Name(calling_class)
            } else {
                StaticClassType::None
            },
            function_is_final: if let Some(method_info) = &functionlike_storage.method_info {
                method_info.is_final
            } else {
                false
            },
            ..Default::default()
        },
        &mut tast_info.data_flow_graph,
    );

    if functionlike_storage.is_async {
        let parent_nodes = inferred_return_type.parent_nodes.clone();
        inferred_return_type = wrap_atomic(TAtomic::TNamedObject {
            name: statements_analyzer
                .get_codebase()
                .interner
                .get("HH\\Awaitable")
                .unwrap(),
            type_params: Some(vec![inferred_return_type]),
            is_this: false,
            extra_types: None,
            remapped_params: false,
        });
        inferred_return_type.parent_nodes.extend(parent_nodes);
    }

    if let Some(_) = return_expr {
        tast_info
            .inferred_return_types
            .push(inferred_return_type.clone());
    }

    let expected_return_type = if let Some(expected_return_type) = &functionlike_storage.return_type
    {
        let mut expected_type = expected_return_type.clone();

        type_expander::expand_union(
            statements_analyzer.get_codebase(),
            &mut expected_type,
            &TypeExpansionOptions {
                self_class: context.function_context.calling_class.as_ref(),
                static_class_type: if let Some(calling_class) =
                    &context.function_context.calling_class
                {
                    StaticClassType::Name(calling_class)
                } else {
                    StaticClassType::None
                },
                function_is_final: if let Some(method_info) = &functionlike_storage.method_info {
                    method_info.is_final
                } else {
                    false
                },
                file_path: Some(
                    &statements_analyzer
                        .get_file_analyzer()
                        .get_file_source()
                        .file_path,
                ),
                ..Default::default()
            },
            &mut tast_info.data_flow_graph,
        );

        expected_type
    } else {
        get_mixed_any()
    };

    if let Some(return_expr) = return_expr {
        handle_dataflow(
            statements_analyzer,
            context,
            return_expr,
            &inferred_return_type,
            &mut tast_info.data_flow_graph,
            &if let Some(closure_id) = context.calling_closure_id {
                FunctionLikeIdentifier::Function(closure_id)
            } else {
                context.function_context.calling_functionlike_id.unwrap()
            },
            functionlike_storage,
        );

        if !expected_return_type.is_mixed() {
            if expected_return_type.is_generator(interner) && functionlike_storage.has_yield {
                return;
            }

            let mut mixed_with_any = false;

            if expected_return_type.is_mixed() {
                return;
            }

            if inferred_return_type.is_mixed_with_any(&mut mixed_with_any) {
                if expected_return_type.is_void() {
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::InvalidReturnStatement,
                            format!(
                                "No return values are expected for {}",
                                context
                                    .function_context
                                    .calling_functionlike_id
                                    .as_ref()
                                    .unwrap()
                                    .to_string(interner)
                            ),
                            statements_analyzer.get_hpos(&return_expr.1),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );

                    return;
                }

                for origin in &inferred_return_type.parent_nodes {
                    tast_info.data_flow_graph.add_mixed_data(origin, &stmt.0);
                }

                // todo increment mixed count

                tast_info.maybe_add_issue(
                    Issue::new(
                        if mixed_with_any {
                            IssueKind::MixedAnyReturnStatement
                        } else {
                            IssueKind::MixedReturnStatement
                        },
                        format!(
                            "Could not infer a proper return type — saw {}",
                            inferred_return_type.get_id(Some(interner))
                        ),
                        statements_analyzer.get_hpos(&return_expr.1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                return;
            }

            // todo increment non-mixed count

            if expected_return_type.is_void() {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidReturnStatement,
                        format!(
                            "No return values are expected for {}",
                            context
                                .function_context
                                .calling_functionlike_id
                                .as_ref()
                                .unwrap()
                                .to_string(interner)
                        ),
                        statements_analyzer.get_hpos(&return_expr.1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                return;
            }

            let mut union_comparison_result = TypeComparisonResult::new();

            let is_contained_by = union_type_comparator::is_contained_by(
                statements_analyzer.get_codebase(),
                &inferred_return_type,
                &expected_return_type,
                true,
                true,
                false,
                &mut union_comparison_result,
            );

            if !is_contained_by {
                if union_comparison_result.type_coerced.unwrap_or(false) {
                    if union_comparison_result
                        .type_coerced_from_nested_any
                        .unwrap_or(false)
                    {
                        tast_info.maybe_add_issue(
                            Issue::new(
                            IssueKind::LessSpecificNestedAnyReturnStatement,
                            format!(
                                "The type {} is more general than the declared return type {} for {}",
                                inferred_return_type.get_id(Some(interner)),
                                expected_return_type.get_id(Some(interner)),
                                context.function_context.calling_functionlike_id.as_ref().unwrap().to_string(interner)
                            ),
                            statements_analyzer.get_hpos(&return_expr.1),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual());
                    } else if union_comparison_result
                        .type_coerced_from_nested_mixed
                        .unwrap_or(false)
                    {
                        if !union_comparison_result
                            .type_coerced_from_as_mixed
                            .unwrap_or(false)
                        {
                            tast_info.maybe_add_issue(
                                Issue::new(
                                    IssueKind::LessSpecificNestedReturnStatement,
                                    format!(
                                        "The type {} is more general than the declared return type {} for {}",
                                        inferred_return_type.get_id(Some(interner)),
                                        expected_return_type.get_id(Some(interner)),
                                        context.function_context.calling_functionlike_id.as_ref().unwrap().to_string(interner)
                                    ),
                                    statements_analyzer.get_hpos(&return_expr.1),
                                    &context.function_context.calling_functionlike_id,
                                ),
                                statements_analyzer.get_config(),
                                statements_analyzer.get_file_path_actual()
                            );
                        }
                    } else {
                        if !union_comparison_result
                            .type_coerced_from_as_mixed
                            .unwrap_or(false)
                        {
                            tast_info.maybe_add_issue(Issue::new(
                                IssueKind::LessSpecificReturnStatement,
                                format!(
                                    "The type {} is more general than the declared return type {} for {}",
                                    inferred_return_type.get_id(Some(interner)),
                                    expected_return_type.get_id(Some(interner)),
                                    context.function_context.calling_functionlike_id.as_ref().unwrap().to_string(interner)
                                ),
                                statements_analyzer.get_hpos(&return_expr.1),
                                &context.function_context.calling_functionlike_id,
                            ),
                            statements_analyzer.get_config(),
                            statements_analyzer.get_file_path_actual()
                        );
                        }
                    }
                } else {
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::InvalidReturnStatement,
                            format!(
                                "The type {} does not match the declared return type {} for {}",
                                inferred_return_type.get_id(Some(interner)),
                                expected_return_type.get_id(Some(interner)),
                                context
                                    .function_context
                                    .calling_functionlike_id
                                    .as_ref()
                                    .unwrap()
                                    .to_string(interner)
                            ),
                            statements_analyzer.get_hpos(&return_expr.1),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }

            if inferred_return_type.is_nullable()
                && !expected_return_type.is_nullable()
                && !expected_return_type.has_template()
            {
                tast_info.maybe_add_issue(Issue::new(
                    IssueKind::NullableReturnStatement,
                    format!(
                        "The declared return type {} for {} is not nullable, but the function returns {}",
                        expected_return_type.get_id(Some(interner)),
                        context.function_context.calling_functionlike_id.as_ref().unwrap().to_string(interner),
                        inferred_return_type.get_id(Some(interner)),
                    ),
                    statements_analyzer.get_hpos(&return_expr.1),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual());
            }

            // todo at some point in the future all notions of falsability can be removed
            if inferred_return_type.is_falsable()
                && !expected_return_type.is_falsable()
                && !expected_return_type.has_template()
                && !inferred_return_type.ignore_falsable_issues
            {
                tast_info.maybe_add_issue(Issue::new(
                    IssueKind::FalsableReturnStatement,
                    format!(
                        "The declared return type {} for {} is not falsable, but the function returns {}",
                        expected_return_type.get_id(Some(interner)),
                        context.function_context.calling_functionlike_id.as_ref().unwrap().to_string(interner),
                        inferred_return_type.get_id(Some(interner)),
                    ),
                    statements_analyzer.get_hpos(&return_expr.1),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual());
            }
        }
    } else if !expected_return_type.is_void()
        && !functionlike_storage.has_yield
        && !functionlike_storage.is_async
        && statements_analyzer
            .get_codebase()
            .interner
            .lookup(&functionlike_storage.name)
            != "__construct"
    {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::InvalidReturnStatement,
                format!(
                    "Empty return statement not expected in {}",
                    context
                        .function_context
                        .calling_functionlike_id
                        .as_ref()
                        .unwrap()
                        .to_string(interner)
                ),
                statements_analyzer.get_hpos(&stmt.0),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }
}

pub(crate) fn handle_inout_at_return(
    functionlike_storage: &FunctionLikeInfo,
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    tast_info: &mut TastInfo,
    _return_pos: Option<&Pos>,
) {
    for (i, param) in functionlike_storage.params.iter().enumerate() {
        if param.is_inout {
            if let Some(context_type) = context.vars_in_scope.get(&param.name) {
                if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {}
                let new_parent_node =
                    if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
                        DataFlowNode::get_for_method_argument_out(
                            context
                                .function_context
                                .calling_functionlike_id
                                .clone()
                                .unwrap()
                                .to_string(&statements_analyzer.get_codebase().interner),
                            i,
                            Some(param.name_location.clone()),
                            None,
                        )
                    } else {
                        DataFlowNode::get_for_variable_sink(
                            "out ".to_string() + param.name.as_str(),
                            param.name_location.clone(),
                        )
                    };

                tast_info.data_flow_graph.add_node(new_parent_node.clone());

                for parent_node in &context_type.parent_nodes {
                    tast_info.data_flow_graph.add_path(
                        parent_node,
                        &new_parent_node,
                        PathKind::Default,
                        None,
                        None,
                    );
                }
            }
        }
    }
}

fn handle_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    context: &ScopeContext,
    return_expr: &aast::Expr<(), ()>,
    inferred_type: &TUnion,
    data_flow_graph: &mut DataFlowGraph,
    functionlike_id: &FunctionLikeIdentifier,
    functionlike_storage: &FunctionLikeInfo,
) {
    if data_flow_graph.kind == GraphKind::FunctionBody {
        let return_node = DataFlowNode::get_for_variable_sink(
            "return".to_string(),
            statements_analyzer.get_hpos(return_expr.pos()),
        );

        for parent_node in &inferred_type.parent_nodes {
            data_flow_graph.add_path(&parent_node, &return_node, PathKind::Default, None, None);
        }
        data_flow_graph.add_node(return_node);
    } else {
        if !inferred_type.has_taintable_value() {
            return;
        }

        if !context.allow_taints {
            return;
        }

        let codebase = statements_analyzer.get_codebase();

        for at in &inferred_type.types {
            if let Some(shape_name) = at.get_shape_name() {
                if let Some(t) = codebase.type_definitions.get(&shape_name) {
                    if t.shape_field_taints.is_some() {
                        return;
                    }
                }
            }
        }

        let return_expr_node = DataFlowNode::get_for_assignment(
            "return".to_string(),
            statements_analyzer.get_hpos(&return_expr.pos()),
        );

        for parent_node in &inferred_type.parent_nodes {
            data_flow_graph.add_path(
                &parent_node,
                &return_expr_node,
                PathKind::Default,
                functionlike_storage.added_taints.clone(),
                functionlike_storage.removed_taints.clone(),
            );
        }

        let method_node = DataFlowNode::get_for_method_return(
            functionlike_id.to_string(&codebase.interner),
            functionlike_storage.return_type_location.clone(),
            None,
        );

        data_flow_graph.add_path(
            &return_expr_node,
            &method_node,
            PathKind::Default,
            None,
            None,
        );

        if let FunctionLikeIdentifier::Method(classlike_name, method_name) = functionlike_id {
            if let Some(classlike_info) = codebase.classlike_infos.get(&classlike_name) {
                if *method_name != StrId::construct() {
                    let mut all_parents = classlike_info
                        .all_parent_classes
                        .iter()
                        .collect::<FxHashSet<_>>();
                    all_parents.extend(classlike_info.all_parent_interfaces.iter());

                    for parent_classlike in all_parents {
                        if codebase.declaring_method_exists(&parent_classlike, &method_name) {
                            let new_sink = DataFlowNode::get_for_method_return(
                                codebase.interner.lookup(parent_classlike).to_string()
                                    + "::"
                                    + codebase.interner.lookup(method_name),
                                None,
                                None,
                            );

                            data_flow_graph.add_node(new_sink.clone());

                            data_flow_graph.add_path(
                                &method_node,
                                &new_sink,
                                PathKind::Default,
                                None,
                                None,
                            );
                        }
                    }
                }
            }
        }

        data_flow_graph.add_node(return_expr_node);
        data_flow_graph.add_node(method_node);
    }
}
