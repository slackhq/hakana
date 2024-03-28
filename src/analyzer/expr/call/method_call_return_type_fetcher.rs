use std::rc::Rc;

use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::GenericParent;
use hakana_str::{Interner, StrId};
use oxidized::{aast, ast_defs};
use rustc_hash::FxHashMap;

use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::{DataFlowNode, DataFlowNodeKind};
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_type::{
    get_mixed_any, get_nothing, get_string, template,
    type_expander::{self, TypeExpansionOptions},
};
use oxidized::ast_defs::Pos;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_type::template::{TemplateBound, TemplateResult};

use super::function_call_return_type_fetcher::add_special_param_dataflow;

pub(crate) fn fetch(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
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
    let codebase = statements_analyzer.get_codebase();

    let mut return_type_candidate = if let Some(return_type) =
        get_special_method_return(method_id, statements_analyzer.get_interner())
    {
        return_type
    } else {
        functionlike_storage.return_type.clone().unwrap_or(
            if method_id.1
                == statements_analyzer
                    .get_interner()
                    .get("__toString")
                    .unwrap()
            {
                get_string()
            } else {
                get_mixed_any()
            },
        )
    };

    let method_storage = &functionlike_storage.method_info.as_ref().unwrap();

    let mut template_result = template_result.clone();

    if !functionlike_storage.template_types.is_empty() {
        for (template_name, _) in &functionlike_storage.template_types {
            template_result
                .lower_bounds
                .entry(*template_name)
                .or_insert(FxHashMap::from_iter([(
                    GenericParent::FunctionLike(declaring_method_id.1),
                    vec![TemplateBound::new(get_nothing(), 1, None, None)],
                )]));
        }
    }

    if !template_result.lower_bounds.is_empty() {
        type_expander::expand_union(
            codebase,
            &Some(statements_analyzer.get_interner()),
            &mut return_type_candidate,
            &TypeExpansionOptions {
                self_class: Some(&method_id.0),
                parent_class: classlike_storage.direct_parent_class.as_ref(),
                function_is_final: method_storage.is_final,
                expand_generic: true,
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
        );

        return_type_candidate = template::inferred_type_replacer::replace(
            &return_type_candidate,
            &template_result,
            codebase,
        );
    }

    type_expander::expand_union(
        codebase,
        &Some(statements_analyzer.get_interner()),
        &mut return_type_candidate,
        &TypeExpansionOptions {
            self_class: Some(&method_id.0),
            static_class_type: match lhs_type_part {
                TAtomic::TNamedObject { .. } | TAtomic::TGenericParam { .. } => {
                    type_expander::StaticClassType::Object(lhs_type_part)
                }
                TAtomic::TClassname { as_type } => type_expander::StaticClassType::Object(as_type),
                _ => type_expander::StaticClassType::None,
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
        &mut analysis_data.data_flow_graph,
    );

    add_dataflow(
        statements_analyzer,
        return_type_candidate,
        context,
        call_expr,
        method_id,
        declaring_method_id,
        lhs_var_id,
        lhs_var_pos,
        functionlike_storage,
        analysis_data,
        call_pos,
    )
}

fn get_special_method_return(method_id: &MethodIdentifier, interner: &Interner) -> Option<TUnion> {
    match method_id.0 {
        StrId::DATE_TIME | StrId::DATE_TIME_IMMUTABLE => {
            if interner.lookup(&method_id.1) == "createFromFormat" {
                let mut false_or_datetime = TUnion::new(vec![
                    TAtomic::TNamedObject {
                        name: method_id.0,
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
        }
        StrId::DOMDOCUMENT => {
            if interner.lookup(&method_id.1) == "createElement" {
                let mut false_or_domelement = TUnion::new(vec![
                    TAtomic::TNamedObject {
                        name: interner.get("DOMElement").unwrap(),
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
        }
        StrId::SIMPLE_XML_ELEMENT => match interner.lookup(&method_id.1) {
            "children" | "attributes" | "addChild" => {
                let null_or_simplexmlelement = TUnion::new(vec![
                    TAtomic::TNamedObject {
                        name: interner.get("SimpleXMLElement").unwrap(),
                        type_params: None,
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    },
                    TAtomic::TNull,
                ]);
                return Some(null_or_simplexmlelement);
            }
            _ => {}
        },
        _ => {}
    }

    None
}

fn add_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    mut return_type_candidate: TUnion,
    context: &mut ScopeContext,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    method_id: &MethodIdentifier,
    declaring_method_id: &MethodIdentifier,
    lhs_var_id: Option<&String>,
    lhs_var_pos: Option<&Pos>,
    functionlike_storage: &FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    call_pos: &Pos,
) -> TUnion {
    // todo dispatch AddRemoveTaintsEvent

    let added_taints = vec![];
    let removed_taints = vec![];

    let data_flow_graph = &mut analysis_data.data_flow_graph;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if !context.allow_taints {
            return return_type_candidate;
        }
    }

    let codebase = statements_analyzer.get_codebase();

    let method_call_node;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if method_id != declaring_method_id {
            method_call_node = DataFlowNode::get_for_method_return(
                &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
                statements_analyzer.get_interner(),
                None,
                if functionlike_storage.specialize_call {
                    Some(statements_analyzer.get_hpos(call_pos))
                } else {
                    None
                },
            );

            let declaring_method_call_node = DataFlowNode::get_for_method_return(
                &FunctionLikeIdentifier::Method(declaring_method_id.0, declaring_method_id.1),
                statements_analyzer.get_interner(),
                functionlike_storage.return_type_location,
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
                &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
                statements_analyzer.get_interner(),
                functionlike_storage.return_type_location,
                if functionlike_storage.specialize_call {
                    Some(statements_analyzer.get_hpos(call_pos))
                } else {
                    None
                },
            );

            for classlike_descendant in codebase.get_all_descendants(&method_id.0) {
                let descendant_method_id = codebase.get_declaring_method_id(&MethodIdentifier(
                    classlike_descendant,
                    declaring_method_id.1,
                ));

                let declaring_method_call_node = DataFlowNode::get_for_method_return(
                    &FunctionLikeIdentifier::Method(descendant_method_id.0, descendant_method_id.1),
                    statements_analyzer.get_interner(),
                    functionlike_storage.return_type_location,
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
                    added_taints.clone(),
                    removed_taints.clone(),
                );
            }
        }

        if method_id.1 == StrId::CONSTRUCT {
            if let Some(var_type) = context.vars_in_scope.get_mut("$this") {
                let before_construct_node = DataFlowNode::get_for_this_before_method(
                    method_id,
                    functionlike_storage.return_type_location,
                    Some(statements_analyzer.get_hpos(call_pos)),
                    statements_analyzer.get_interner(),
                );

                for this_parent_node in &var_type.parent_nodes {
                    data_flow_graph.add_path(
                        this_parent_node,
                        &before_construct_node,
                        PathKind::Default,
                        vec![],
                        vec![],
                    )
                }

                data_flow_graph.add_node(before_construct_node);

                let after_construct_node = DataFlowNode::get_for_this_after_method(
                    method_id,
                    functionlike_storage.return_type_location,
                    Some(statements_analyzer.get_hpos(call_pos)),
                    statements_analyzer.get_interner(),
                );

                let mut var_type_inner = (**var_type).clone();

                var_type_inner.parent_nodes = vec![after_construct_node.clone()];

                data_flow_graph.add_node(after_construct_node);

                *var_type = Rc::new(var_type_inner);
            }
        }

        if let (Some(lhs_var_id), Some(lhs_var_pos)) = (lhs_var_id, lhs_var_pos) {
            if functionlike_storage.specialize_call {
                if let Some(context_type) = context.vars_in_scope.get_mut(lhs_var_id) {
                    let var_node = DataFlowNode::get_for_lvar(
                        lhs_var_id.to_owned(),
                        statements_analyzer.get_hpos(lhs_var_pos),
                    );

                    let this_before_method_node = DataFlowNode::get_for_this_before_method(
                        declaring_method_id,
                        functionlike_storage.name_location,
                        Some(statements_analyzer.get_hpos(call_pos)),
                        statements_analyzer.get_interner(),
                    );

                    for parent_node in &context_type.parent_nodes {
                        data_flow_graph.add_path(
                            parent_node,
                            &this_before_method_node,
                            PathKind::Default,
                            vec![],
                            vec![],
                        );

                        data_flow_graph.add_path(
                            parent_node,
                            &var_node,
                            PathKind::Default,
                            vec![],
                            vec![],
                        );
                    }

                    let this_after_method_node = DataFlowNode::get_for_this_after_method(
                        declaring_method_id,
                        functionlike_storage.name_location,
                        Some(statements_analyzer.get_hpos(call_pos)),
                        statements_analyzer.get_interner(),
                    );

                    data_flow_graph.add_path(
                        &this_after_method_node,
                        &var_node,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );

                    let mut context_type_inner = (**context_type).clone();

                    context_type_inner.parent_nodes = vec![var_node.clone()];

                    *context_type = Rc::new(context_type_inner);

                    data_flow_graph.add_node(var_node);
                    data_flow_graph.add_node(this_before_method_node);
                    data_flow_graph.add_node(this_after_method_node);
                }
            }
        }

        if !functionlike_storage.taint_source_types.is_empty() {
            let method_call_node_source = DataFlowNode {
                id: method_call_node.get_id().clone(),
                kind: DataFlowNodeKind::TaintSource {
                    pos: *method_call_node.get_pos(),
                    label: method_call_node.get_label().clone(),
                    types: functionlike_storage.taint_source_types.clone(),
                },
            };
            data_flow_graph.add_node(method_call_node_source);
        }
    } else {
        method_call_node = DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            statements_analyzer.get_interner(),
            functionlike_storage.return_type_location,
            if functionlike_storage.specialize_call {
                Some(statements_analyzer.get_hpos(call_pos))
            } else {
                None
            },
        );
    }

    if method_id.0 == StrId::SHAPES && method_id.1 == StrId::KEY_EXISTS {
        add_special_param_dataflow(
            statements_analyzer,
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            true,
            0,
            statements_analyzer.get_hpos(call_expr.1[0].1.pos()),
            call_pos,
            &FxHashMap::default(),
            data_flow_graph,
            &method_call_node,
            PathKind::Aggregate,
        );
    }

    data_flow_graph.add_node(method_call_node.clone());

    return_type_candidate.parent_nodes = vec![method_call_node.clone()];

    return_type_candidate
}
