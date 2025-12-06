use std::rc::Rc;

use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::taint::SinkType;
use hakana_code_info::ttype::get_mixed;
use hakana_code_info::var_name::VarName;
use hakana_code_info::{ExprId, GenericParent, VarId};
use hakana_str::{Interner, StrId};
use oxidized::aast;
use rustc_hash::FxHashMap;

use hakana_code_info::classlike_info::ClassLikeInfo;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::data_flow::node::{DataFlowNode, DataFlowNodeKind};
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::method_identifier::MethodIdentifier;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::{
    get_mixed_any, get_nothing, get_string, template,
    type_expander::{self, TypeExpansionOptions},
};
use oxidized::ast_defs::Pos;

use crate::expr::expression_identifier::get_expr_id;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_code_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_code_info::ttype::template::{TemplateBound, TemplateResult};

use super::function_call_return_type_fetcher::add_special_param_dataflow;

pub(crate) fn fetch(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    lhs_expr: Option<&aast::Expr<(), ()>>,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    method_id: &MethodIdentifier,
    declaring_method_id: &MethodIdentifier,
    lhs_type_part: &TAtomic,
    lhs_var_id: Option<&String>,
    functionlike_storage: &FunctionLikeInfo,
    classlike_storage: &ClassLikeInfo,
    template_result: &TemplateResult,
    call_pos: &Pos,
) -> TUnion {
    let codebase = statements_analyzer.codebase;

    // Get the calling function's where constraints to apply to the return type
    let calling_where_constraints = context
        .function_context
        .get_functionlike_info(codebase)
        .map(|info| &info.where_constraints)
        .filter(|c| !c.is_empty());

    let mut return_type_candidate = if let Some(return_type) =
        get_special_method_return(method_id, statements_analyzer.interner)
    {
        return_type
    } else {
        functionlike_storage.return_type.clone().unwrap_or(
            if method_id.1 == statements_analyzer.interner.get("__toString").unwrap() {
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
            &Some(statements_analyzer.interner),
            statements_analyzer.get_file_path(),
            &mut return_type_candidate,
            &TypeExpansionOptions {
                self_class: Some(method_id.0),
                parent_class: classlike_storage.direct_parent_class,
                function_is_final: method_storage.is_final,
                expand_generic: true,
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
            &mut 0,
        );

        return_type_candidate = template::inferred_type_replacer::replace(
            &return_type_candidate,
            &template_result,
            codebase,
        );
    }

    type_expander::expand_union(
        codebase,
        &Some(statements_analyzer.interner),
        &statements_analyzer.file_analyzer.file_source.file_path,
        &mut return_type_candidate,
        &TypeExpansionOptions {
            self_class: Some(method_id.0),
            static_class_type: match lhs_type_part {
                TAtomic::TNamedObject { .. } | TAtomic::TGenericParam { .. } => {
                    type_expander::StaticClassType::Object(lhs_type_part)
                }
                TAtomic::TGenericClassname { as_type, .. }
                | TAtomic::TClassname { as_type }
                | TAtomic::TGenericClassPtr { as_type, .. }
                | TAtomic::TClassPtr { as_type } => type_expander::StaticClassType::Object(as_type),
                _ => type_expander::StaticClassType::None,
            },
            parent_class: classlike_storage.direct_parent_class,
            function_is_final: method_storage.is_final,
            expand_generic: true,
            // Apply calling function's where constraints to narrow class template params
            where_constraints: calling_where_constraints,
            ..Default::default()
        },
        &mut analysis_data.data_flow_graph,
        &mut 0,
    );

    if return_type_candidate.is_nothing() && context.function_context.ignore_noreturn_calls {
        return_type_candidate = get_mixed();
    }

    add_dataflow(
        statements_analyzer,
        return_type_candidate,
        context,
        lhs_expr,
        call_expr,
        method_id,
        declaring_method_id,
        lhs_var_id,
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
        StrId::MESSAGE_FORMATTER => match method_id.1 {
            StrId::FORMAT_MESSAGE => {
                let mut u = TUnion::new(vec![TAtomic::TString, TAtomic::TFalse]);
                u.ignore_falsable_issues = true;
                return Some(u);
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
    context: &mut BlockContext,
    lhs_expr: Option<&aast::Expr<(), ()>>,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    method_id: &MethodIdentifier,
    declaring_method_id: &MethodIdentifier,
    lhs_var_id: Option<&String>,
    functionlike_storage: &FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    call_pos: &Pos,
) -> TUnion {
    // todo dispatch AddRemoveTaintsEvent

    let added_taints = vec![];
    let removed_taints = vec![];

    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        if !context.allow_taints {
            return return_type_candidate;
        }
    }

    let codebase = statements_analyzer.codebase;

    let method_call_node;

    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        method_call_node = get_tainted_method_node(
            statements_analyzer,
            context,
            lhs_expr,
            method_id,
            declaring_method_id,
            lhs_var_id,
            functionlike_storage,
            analysis_data,
            call_pos,
            added_taints,
            removed_taints,
            codebase,
        );
    } else {
        method_call_node = DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            functionlike_storage.return_type_location,
            if functionlike_storage.specialize_call {
                Some(statements_analyzer.get_hpos(call_pos))
            } else {
                None
            },
        );

        if let Some(lhs_expr) = lhs_expr {
            if let Some(expr_type) = analysis_data.expr_types.get(&(
                lhs_expr.pos().start_offset() as u32,
                lhs_expr.pos().end_offset() as u32,
            )) {
                let lhs_expr_parent_nodes = &expr_type.parent_nodes;

                if matches!(functionlike_storage.effects, FnEffect::Pure) {
                    for parent_node in lhs_expr_parent_nodes {
                        analysis_data.data_flow_graph.add_path(
                            &parent_node.id,
                            &method_call_node.id,
                            PathKind::Default,
                            vec![],
                            vec![],
                        );
                    }
                } else {
                    let sink_node = DataFlowNode::get_for_unlabelled_sink(
                        statements_analyzer.get_hpos(lhs_expr.pos()),
                    );

                    for parent_node in lhs_expr_parent_nodes {
                        analysis_data.data_flow_graph.add_path(
                            &parent_node.id,
                            &sink_node.id,
                            PathKind::Default,
                            vec![],
                            vec![],
                        );
                    }

                    analysis_data.data_flow_graph.add_node(sink_node);
                }
            }
        }
    }

    if method_id.0 == StrId::SHAPES && method_id.1 == StrId::KEY_EXISTS {
        add_special_param_dataflow(
            statements_analyzer,
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            true,
            0,
            statements_analyzer.get_hpos(call_expr.1[0].to_expr_ref().pos()),
            call_pos,
            &FxHashMap::default(),
            &mut analysis_data.data_flow_graph,
            &method_call_node,
            PathKind::Aggregate,
        );
    } else if method_id.0 == StrId::MESSAGE_FORMATTER && method_id.1 == StrId::FORMAT_MESSAGE {
        add_special_param_dataflow(
            statements_analyzer,
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            true,
            0,
            statements_analyzer.get_hpos(call_expr.1[0].to_expr_ref().pos()),
            call_pos,
            &FxHashMap::default(),
            &mut analysis_data.data_flow_graph,
            &method_call_node,
            PathKind::Aggregate,
        );
        add_special_param_dataflow(
            statements_analyzer,
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            true,
            1,
            statements_analyzer.get_hpos(call_expr.1[1].to_expr_ref().pos()),
            call_pos,
            &FxHashMap::default(),
            &mut analysis_data.data_flow_graph,
            &method_call_node,
            PathKind::Default,
        );
        add_special_param_dataflow(
            statements_analyzer,
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            true,
            2,
            statements_analyzer.get_hpos(call_expr.1[2].to_expr_ref().pos()),
            call_pos,
            &FxHashMap::default(),
            &mut analysis_data.data_flow_graph,
            &method_call_node,
            PathKind::UnknownArrayFetch(
                hakana_code_info::data_flow::path::ArrayDataKind::ArrayValue,
            ),
        );
    }

    analysis_data
        .data_flow_graph
        .add_node(method_call_node.clone());

    return_type_candidate.parent_nodes = vec![method_call_node.clone()];

    return_type_candidate
}

fn get_tainted_method_node(
    statements_analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    lhs_expr: Option<&aast::Expr<(), ()>>,
    method_id: &MethodIdentifier,
    declaring_method_id: &MethodIdentifier,
    lhs_var_id: Option<&String>,
    functionlike_storage: &FunctionLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    call_pos: &Pos,
    added_taints: Vec<SinkType>,
    removed_taints: Vec<SinkType>,
    codebase: &CodebaseInfo,
) -> DataFlowNode {
    let method_call_node;

    let data_flow_graph = &mut analysis_data.data_flow_graph;
    if method_id != declaring_method_id {
        method_call_node = DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            None,
            if functionlike_storage.specialize_call {
                Some(statements_analyzer.get_hpos(call_pos))
            } else {
                None
            },
        );

        let declaring_method_call_node = DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Method(declaring_method_id.0, declaring_method_id.1),
            functionlike_storage.return_type_location,
            if functionlike_storage.specialize_call {
                Some(statements_analyzer.get_hpos(call_pos))
            } else {
                None
            },
        );

        data_flow_graph.add_node(declaring_method_call_node.clone());
        data_flow_graph.add_path(
            &declaring_method_call_node.id,
            &method_call_node.id,
            PathKind::Default,
            added_taints,
            removed_taints,
        );
    } else {
        method_call_node = DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
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
                functionlike_storage.return_type_location,
                if functionlike_storage.specialize_call {
                    Some(statements_analyzer.get_hpos(call_pos))
                } else {
                    None
                },
            );

            data_flow_graph.add_node(declaring_method_call_node.clone());
            data_flow_graph.add_path(
                &declaring_method_call_node.id,
                &method_call_node.id,
                PathKind::Default,
                added_taints.clone(),
                removed_taints.clone(),
            );
        }
    }

    if method_id.1 == StrId::CONSTRUCT {
        if let Some(var_type) = context.locals.get_mut("$this") {
            let before_construct_node = DataFlowNode::get_for_this_before_method(
                method_id,
                functionlike_storage.return_type_location,
                Some(statements_analyzer.get_hpos(call_pos)),
            );

            for this_parent_node in &var_type.parent_nodes {
                data_flow_graph.add_path(
                    &this_parent_node.id,
                    &before_construct_node.id,
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
            );

            let mut var_type_inner = (**var_type).clone();

            var_type_inner.parent_nodes = vec![after_construct_node.clone()];

            data_flow_graph.add_node(after_construct_node);

            *var_type = Rc::new(var_type_inner);
        }
    }

    if let (Some(lhs_expr), Some(lhs_var_id)) = (lhs_expr, lhs_var_id) {
        if functionlike_storage.specialize_call {
            if let Some(context_type) = &analysis_data
                .expr_types
                .get(&(
                    lhs_expr.pos().start_offset() as u32,
                    lhs_expr.pos().end_offset() as u32,
                ))
                .cloned()
            {
                let lhs_var_expr_id = get_expr_id(lhs_expr, statements_analyzer);

                let var_node = match lhs_var_expr_id {
                    Some(ExprId::Var(id)) => DataFlowNode::get_for_lvar(
                        VarId(id),
                        statements_analyzer.get_hpos(lhs_expr.pos()),
                    ),
                    Some(ExprId::InstanceProperty(lhs_expr, name_pos, rhs_expr)) => {
                        DataFlowNode::get_for_local_property_fetch(lhs_expr, rhs_expr, name_pos)
                    }
                    None => DataFlowNode::get_for_instance_method_call(
                        statements_analyzer.get_hpos(lhs_expr.pos()),
                    ),
                };

                let this_before_method_node = DataFlowNode::get_for_this_before_method(
                    declaring_method_id,
                    functionlike_storage.name_location,
                    Some(statements_analyzer.get_hpos(call_pos)),
                );

                for parent_node in &context_type.parent_nodes {
                    data_flow_graph.add_path(
                        &parent_node.id,
                        &this_before_method_node.id,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );
                }

                let this_after_method_node = DataFlowNode::get_for_this_after_method(
                    declaring_method_id,
                    functionlike_storage.name_location,
                    Some(statements_analyzer.get_hpos(call_pos)),
                );

                data_flow_graph.add_path(
                    &this_after_method_node.id,
                    &var_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );

                let mut context_type_inner = (**context_type).clone();

                context_type_inner.parent_nodes = vec![var_node.clone()];

                context.locals.insert(
                    VarName::new(lhs_var_id.clone()),
                    Rc::new(context_type_inner),
                );

                data_flow_graph.add_node(var_node);
                data_flow_graph.add_node(this_before_method_node);
                data_flow_graph.add_node(this_after_method_node);
            }
        }
    }

    if !functionlike_storage.taint_source_types.is_empty() {
        let method_call_node_source = DataFlowNode {
            id: method_call_node.id.clone(),
            kind: DataFlowNodeKind::TaintSource {
                pos: method_call_node.get_pos(),
                types: functionlike_storage.taint_source_types.clone(),
            },
        };
        data_flow_graph.add_node(method_call_node_source);
    }

    method_call_node
}
