use std::{rc::Rc, sync::Arc};

use rustc_hash::FxHashMap;

use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_type::{
    get_mixed_any, get_nothing, get_string, template,
    type_expander::{self, TypeExpansionOptions},
};
use oxidized::ast_defs::Pos;

use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_type::template::{TemplateBound, TemplateResult};

pub(crate) fn fetch(
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    method_id: &MethodIdentifier,
    declaring_method_id: &MethodIdentifier,
    lhs_type_part: &TAtomic,
    lhs_var_id: Option<&String>,
    lhs_var_pos: Option<&Pos>,
    functionlike_storage: &FunctionLikeInfo,
    classlike_storage: &ClassLikeInfo,
    template_result: &TemplateResult,
    call_pos: &Pos,
) -> TUnion {
    let mut return_type_candidate = if let Some(return_type) = get_special_method_return(method_id)
    {
        return_type
    } else {
        functionlike_storage
            .return_type
            .clone()
            .unwrap_or(if method_id.1 == "__toString" {
                get_string()
            } else {
                get_mixed_any()
            })
    };

    let codebase = statements_analyzer.get_codebase();

    let method_storage = &functionlike_storage.method_info.as_ref().unwrap();

    let mut template_result = template_result.clone();

    if !functionlike_storage.template_types.is_empty() {
        let fn_id = Arc::new(format!("fn-{}", method_id.to_string()));
        for (template_name, _) in &functionlike_storage.template_types {
            template_result
                .lower_bounds
                .entry(template_name.clone())
                .or_insert(FxHashMap::from_iter([(
                    fn_id.clone(),
                    vec![TemplateBound::new(get_nothing(), 1, None, None)],
                )]));
        }
    }

    if !template_result.lower_bounds.is_empty() {
        type_expander::expand_union(
            codebase,
            &mut return_type_candidate,
            &TypeExpansionOptions {
                self_class: Some(&method_id.0),
                parent_class: classlike_storage.direct_parent_class.as_ref(),
                function_is_final: method_storage.is_final,
                expand_generic: true,
                ..Default::default()
            },
            &mut tast_info.data_flow_graph,
        );

        return_type_candidate = template::inferred_type_replacer::replace(
            &return_type_candidate,
            &template_result,
            Some(codebase),
        );
    }

    type_expander::expand_union(
        codebase,
        &mut return_type_candidate,
        &TypeExpansionOptions {
            self_class: Some(&method_id.0),
            static_class_type: if let TAtomic::TNamedObject { .. }
            | TAtomic::TTemplateParam { .. } = lhs_type_part
            {
                type_expander::StaticClassType::Object(lhs_type_part)
            } else if let TAtomic::TClassname { as_type } = lhs_type_part {
                type_expander::StaticClassType::Object(as_type)
            } else {
                type_expander::StaticClassType::None
            },
            parent_class: classlike_storage.direct_parent_class.as_ref(),
            function_is_final: method_storage.is_final,
            expand_generic: true,
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

    add_dataflow(
        statements_analyzer,
        return_type_candidate,
        context,
        method_id,
        declaring_method_id,
        lhs_var_id,
        lhs_var_pos,
        functionlike_storage,
        tast_info,
        call_pos,
    )
}

fn get_special_method_return(method_id: &MethodIdentifier) -> Option<TUnion> {
    if (*method_id.0 == "DateTime" || *method_id.0 == "DateTimeImmutable")
        && method_id.1 == "createFromFormat"
    {
        let mut false_or_datetime = TUnion::new(vec![
            TAtomic::TNamedObject {
                name: method_id.0.clone(),
                type_params: None,
                is_this: false,
                extra_types: None,
                remapped_params: false,
            },
            TAtomic::TFalse,
        ]);
        false_or_datetime.ignore_falsable_issues = true;
        return Some(false_or_datetime);
    }

    if *method_id.0 == "DOMDocument" && method_id.1 == "createElement" {
        let mut false_or_domelement = TUnion::new(vec![
            TAtomic::TNamedObject {
                name: Arc::new("DOMElement".to_string()),
                type_params: None,
                is_this: false,
                extra_types: None,
                remapped_params: false,
            },
            TAtomic::TFalse,
        ]);
        false_or_domelement.ignore_falsable_issues = true;
        return Some(false_or_domelement);
    }

    None
}

fn add_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    mut return_type_candidate: TUnion,
    context: &mut ScopeContext,
    method_id: &MethodIdentifier,
    declaring_method_id: &MethodIdentifier,
    lhs_var_id: Option<&String>,
    lhs_var_pos: Option<&Pos>,
    functionlike_storage: &FunctionLikeInfo,
    tast_info: &mut TastInfo,
    call_pos: &Pos,
) -> TUnion {
    // todo dispatch AddRemoveTaintsEvent

    let added_taints = None;
    let removed_taints = None;

    let ref mut data_flow_graph = tast_info.data_flow_graph;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if !context.allow_taints {
            return return_type_candidate;
        }
    }

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        let method_call_node;

        if method_id != declaring_method_id {
            method_call_node = DataFlowNode::get_for_method_return(
                method_id.to_string(),
                None,
                if functionlike_storage.specialize_call {
                    Some(statements_analyzer.get_hpos(call_pos))
                } else {
                    None
                },
            );

            let declaring_method_call_node = DataFlowNode::get_for_method_return(
                declaring_method_id.to_string(),
                functionlike_storage.return_type_location.clone(),
                if functionlike_storage.specialize_call {
                    Some(statements_analyzer.get_hpos(call_pos))
                } else {
                    None
                },
            );

            data_flow_graph.add_node(declaring_method_call_node.clone());
            data_flow_graph.add_path(
                &declaring_method_call_node,
                &method_call_node,
                PathKind::Default,
                added_taints,
                removed_taints,
            );
        } else {
            method_call_node = DataFlowNode::get_for_method_return(
                method_id.to_string(),
                functionlike_storage.return_type_location.clone(),
                if functionlike_storage.specialize_call {
                    Some(statements_analyzer.get_hpos(call_pos))
                } else {
                    None
                },
            );
        }

        if let (Some(lhs_var_id), Some(lhs_var_pos)) = (lhs_var_id, lhs_var_pos) {
            if functionlike_storage.specialize_call {
                if let Some(context_type) = context.vars_in_scope.get_mut(lhs_var_id) {
                    let var_node = DataFlowNode::get_for_assignment(
                        lhs_var_id.to_owned(),
                        statements_analyzer.get_hpos(lhs_var_pos),
                    );

                    let this_before_method_node = DataFlowNode::get_for_this_before_method(
                        &declaring_method_id,
                        functionlike_storage.name_location.clone(),
                        Some(statements_analyzer.get_hpos(call_pos)),
                    );

                    for (_, parent_node) in &context_type.parent_nodes {
                        data_flow_graph.add_path(
                            &parent_node,
                            &this_before_method_node,
                            PathKind::Default,
                            None,
                            None,
                        );

                        data_flow_graph.add_path(
                            &parent_node,
                            &var_node,
                            PathKind::Default,
                            None,
                            None,
                        );
                    }

                    let this_after_method_node = DataFlowNode::get_for_this_after_method(
                        &declaring_method_id,
                        functionlike_storage.name_location.clone(),
                        Some(statements_analyzer.get_hpos(call_pos)),
                    );

                    data_flow_graph.add_path(
                        &this_after_method_node,
                        &var_node,
                        PathKind::Default,
                        None,
                        None,
                    );

                    let mut context_type_inner = (**context_type).clone();

                    context_type_inner.parent_nodes =
                        FxHashMap::from_iter([(var_node.get_id().clone(), var_node.clone())]);

                    *context_type = Rc::new(context_type_inner);

                    data_flow_graph.add_node(var_node);
                    data_flow_graph.add_node(this_before_method_node);
                    data_flow_graph.add_node(this_after_method_node);
                }
            }
        }

        if !functionlike_storage.taint_source_types.is_empty() {
            let method_call_node_source = DataFlowNode::TaintSource {
                id: method_call_node.get_id().clone(),
                label: method_call_node.get_label().clone(),
                pos: method_call_node.get_pos().clone(),
                types: functionlike_storage.taint_source_types.clone(),
            };
            data_flow_graph.add_node(method_call_node_source);
        }

        data_flow_graph.add_node(method_call_node.clone());

        return_type_candidate.parent_nodes =
            FxHashMap::from_iter([(method_call_node.get_id().clone(), method_call_node.clone())]);
    }

    return_type_candidate
}
