use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::data_flow::node::DataFlowNode;
use hakana_code_info::data_flow::node::DataFlowNodeId;
use hakana_code_info::data_flow::node::DataFlowNodeKind;
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::issue::Issue;
use hakana_code_info::issue::IssueKind;
use hakana_code_info::property_info::PropertyKind;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::taint::SinkType;
use hakana_code_info::ttype::get_named_object;
use hakana_code_info::EFFECT_IMPURE;
use hakana_code_info::EFFECT_PURE;
use hakana_str::StrId;
use itertools::Itertools;
use oxidized::aast;
use oxidized::ast_defs;
use oxidized::pos::Pos;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::rc::Rc;

use super::assignment::instance_property_assignment_analyzer::add_unspecialized_property_assignment_dataflow;
use super::fetch::atomic_property_fetch_analyzer;

pub(crate) fn analyze(
    context: &mut BlockContext,
    boxed: &Box<(
        ast_defs::Id,
        Vec<aast::XhpAttribute<(), ()>>,
        Vec<aast::Expr<(), ()>>,
    )>,
    pos: &Pos,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;
    let xhp_class_name =
        if let Some(resolved_name) = resolved_names.get(&(boxed.0 .0.start_offset() as u32)) {
            resolved_name
        } else {
            return Err(AnalysisError::InternalError(
                "could not resolve XML name".to_string(),
                statements_analyzer.get_hpos(pos),
            ));
        };

    analysis_data.symbol_references.add_reference_to_symbol(
        &context.function_context,
        *xhp_class_name,
        false,
    );

    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;

    let mut used_attributes = FxHashSet::default();

    let codebase = statements_analyzer.get_codebase();

    for attribute in &boxed.1 {
        match attribute {
            aast::XhpAttribute::XhpSimple(xhp_simple) => {
                let attribute_name = get_attribute_name(
                    statements_analyzer,
                    xhp_simple,
                    resolved_names,
                    analysis_data,
                    context,
                    xhp_class_name,
                )?;

                used_attributes.insert(attribute_name);

                analyze_xhp_attribute_assignment(
                    statements_analyzer,
                    attribute_name,
                    xhp_class_name,
                    xhp_simple,
                    analysis_data,
                    context,
                )?;
            }
            aast::XhpAttribute::XhpSpread(xhp_expr) => {
                used_attributes.extend(handle_attribute_spread(
                    statements_analyzer,
                    xhp_expr,
                    xhp_class_name,
                    analysis_data,
                    context,
                    codebase,
                )?);
            }
        }
    }

    if let Some(classlike_info) = codebase.classlike_infos.get(xhp_class_name) {
        let mut required_attributes = classlike_info
            .properties
            .iter()
            .filter(|p| matches!(p.1.kind, PropertyKind::XhpAttribute { is_required: true }))
            .map(|p| p.0)
            .collect::<FxHashSet<_>>();

        required_attributes.retain(|attr| !used_attributes.contains(attr));

        if !required_attributes.is_empty() {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::MissingRequiredXhpAttribute,
                    format!(
                        "XHP class {} is missing {}: {}",
                        statements_analyzer.get_interner().lookup(xhp_class_name),
                        if required_attributes.len() == 1 {
                            "a required attribute"
                        } else {
                            "some required attributes"
                        },
                        required_attributes
                            .iter()
                            .map(|attr| statements_analyzer.get_interner().lookup(attr)[1..]
                                .to_string())
                            .join(", ")
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    } else {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentClass,
                format!(
                    "Unknown XHP class {}",
                    statements_analyzer.get_interner().lookup(xhp_class_name)
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
        return Ok(());
    };

    let element_name = statements_analyzer.get_interner().lookup(xhp_class_name);

    analysis_data.expr_effects.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        if element_name.starts_with("Facebook\\XHP\\HTML\\") {
            EFFECT_PURE
        } else {
            EFFECT_IMPURE
        },
    );

    for inner_expr in &boxed.2 {
        expression_analyzer::analyze(statements_analyzer, inner_expr, analysis_data, context)?;

        analysis_data.combine_effects(inner_expr.pos(), pos, pos);

        if let Some(expr_type) = analysis_data.expr_types.get(&(
            inner_expr.pos().start_offset() as u32,
            inner_expr.pos().end_offset() as u32,
        )) {
            if matches!(
                element_name,
                "Facebook\\XHP\\HTML\\a" | "Facebook\\XHP\\HTML\\p"
            ) {
                let xml_body_taint = DataFlowNode {
                    id: DataFlowNodeId::Symbol(*xhp_class_name),
                    kind: DataFlowNodeKind::TaintSink {
                        pos: statements_analyzer.get_hpos(pos),
                        types: vec![SinkType::Output],
                    },
                };

                for parent_node in &expr_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        parent_node,
                        &xml_body_taint,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );
                }

                analysis_data.data_flow_graph.add_node(xml_body_taint);
            }

            // find data leaking to style and script tags
            if matches!(
                element_name,
                "Facebook\\XHP\\HTML\\style" | "Facebook\\XHP\\HTML\\script"
            ) {
                let xml_body_taint = DataFlowNode {
                    id: DataFlowNodeId::Symbol(*xhp_class_name),
                    kind: DataFlowNodeKind::TaintSink {
                        pos: statements_analyzer.get_hpos(pos),
                        types: vec![SinkType::HtmlTag, SinkType::Output],
                    },
                };

                for parent_node in &expr_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        parent_node,
                        &xml_body_taint,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );
                }

                analysis_data.data_flow_graph.add_node(xml_body_taint);
            }
        }
    }
    context.inside_general_use = was_inside_general_use;

    analysis_data.expr_types.insert(
        (pos.start_offset() as u32, pos.end_offset() as u32),
        Rc::new(get_named_object(*xhp_class_name, None)),
    );

    Ok(())
}

fn handle_attribute_spread(
    statements_analyzer: &StatementsAnalyzer,
    xhp_expr: &aast::Expr<(), ()>,
    element_name: &StrId,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    codebase: &CodebaseInfo,
) -> Result<FxHashSet<StrId>, AnalysisError> {
    expression_analyzer::analyze(statements_analyzer, xhp_expr, analysis_data, context)?;

    let mut used_attributes = FxHashSet::default();

    if let Some(expr_type) = analysis_data
        .expr_types
        .get(&(
            xhp_expr.pos().start_offset() as u32,
            xhp_expr.pos().end_offset() as u32,
        ))
        .cloned()
    {
        for expr_type_atomic in &expr_type.types {
            if let TAtomic::TNamedObject {
                name: spread_xhp_class,
                ..
            } = expr_type_atomic
            {
                if let Some(spread_class_info) = codebase.classlike_infos.get(spread_xhp_class) {
                    let all_attributes = spread_class_info
                        .properties
                        .iter()
                        .filter(|p| matches!(p.1.kind, PropertyKind::XhpAttribute { .. }));

                    for spread_attribute in all_attributes {
                        atomic_property_fetch_analyzer::analyze(
                            statements_analyzer,
                            (xhp_expr, xhp_expr),
                            xhp_expr.pos(),
                            analysis_data,
                            context,
                            false,
                            expr_type_atomic.clone(),
                            statements_analyzer
                                .get_interner()
                                .lookup(spread_attribute.0),
                            &None,
                        )?;

                        used_attributes.insert(*spread_attribute.0);

                        if let Some(property_fetch_type) = analysis_data
                            .expr_types
                            .get(&(
                                xhp_expr.pos().start_offset() as u32,
                                xhp_expr.pos().end_offset() as u32,
                            ))
                            .cloned()
                        {
                            add_all_dataflow(
                                analysis_data,
                                statements_analyzer,
                                (*element_name, *spread_attribute.0),
                                xhp_expr.pos(),
                                xhp_expr.pos(),
                                property_fetch_type,
                                statements_analyzer
                                    .get_interner()
                                    .lookup(spread_attribute.0),
                            );
                        }
                    }
                }
            }
        }

        analysis_data.expr_types.insert(
            (
                xhp_expr.pos().start_offset() as u32,
                xhp_expr.pos().end_offset() as u32,
            ),
            expr_type,
        );
    }

    Ok(used_attributes)
}

fn analyze_xhp_attribute_assignment(
    statements_analyzer: &StatementsAnalyzer,
    attribute_name: StrId,
    element_name: &StrId,
    attribute_info: &aast::XhpSimple<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    expression_analyzer::analyze(
        statements_analyzer,
        &attribute_info.expr,
        analysis_data,
        context,
    )?;

    let property_id = (*element_name, attribute_name);

    let attribute_value_type = analysis_data
        .expr_types
        .get(&(
            attribute_info.expr.pos().start_offset() as u32,
            attribute_info.expr.pos().end_offset() as u32,
        ))
        .cloned();

    let attribute_name_pos = &attribute_info.name.0;
    let codebase = statements_analyzer.get_codebase();

    if let Some(classlike_info) = codebase.classlike_infos.get(element_name) {
        if attribute_name != StrId::DATA_ATTRIBUTE
            && attribute_name != StrId::ARIA_ATTRIBUTE
            && !classlike_info
                .appearing_property_ids
                .contains_key(&attribute_name)
        {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentXhpAttribute,
                    format!(
                        "XHP attribute {} is not defined on {}",
                        statements_analyzer.get_interner().lookup(&attribute_name),
                        statements_analyzer.get_interner().lookup(element_name)
                    ),
                    statements_analyzer.get_hpos(attribute_name_pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    let attribute_value_pos = attribute_info.expr.pos();

    if let Some(attribute_value_type) = attribute_value_type {
        add_all_dataflow(
            analysis_data,
            statements_analyzer,
            property_id,
            attribute_name_pos,
            attribute_value_pos,
            attribute_value_type,
            &attribute_info.name.1,
        );
    }

    Ok(())
}

fn add_all_dataflow(
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    property_id: (StrId, StrId),
    attribute_name_pos: &Pos,
    attribute_value_pos: &Pos,
    attribute_value_type: Rc<TUnion>,
    attribute_name: &str,
) {
    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        let codebase = statements_analyzer.get_codebase();

        add_unspecialized_property_assignment_dataflow(
            statements_analyzer,
            &property_id,
            attribute_name_pos,
            Some(attribute_value_pos),
            analysis_data,
            &attribute_value_type,
            codebase,
            &property_id.0,
            property_id.1,
        );

        add_xml_attribute_dataflow(
            statements_analyzer,
            codebase,
            attribute_value_pos,
            &property_id.0,
            property_id,
            attribute_name,
            analysis_data,
        );
    }
}

fn get_attribute_name(
    statements_analyzer: &StatementsAnalyzer,
    attribute_info: &oxidized::tast::XhpSimple<(), ()>,
    resolved_names: &FxHashMap<u32, StrId>,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    element_name: &StrId,
) -> Result<StrId, AnalysisError> {
    if attribute_info.name.1.starts_with("data-") {
        Ok(StrId::DATA_ATTRIBUTE)
    } else if attribute_info.name.1.starts_with("aria-") {
        Ok(StrId::ARIA_ATTRIBUTE)
    } else {
        let attribute_name = if let Some(resolved_name) =
            resolved_names.get(&(attribute_info.name.0.start_offset() as u32))
        {
            *resolved_name
        } else {
            return Err(AnalysisError::InternalError(
                "could not resolve XML name".to_string(),
                statements_analyzer.get_hpos(&attribute_info.name.0),
            ));
        };

        analysis_data
            .symbol_references
            .add_reference_to_class_member(
                &context.function_context,
                (*element_name, attribute_name),
                false,
            );

        Ok(attribute_name)
    }
}

fn add_xml_attribute_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    codebase: &CodebaseInfo,
    attribute_value_pos: &Pos,
    element_name: &StrId,
    property_id: (StrId, StrId),
    name: &str,
    analysis_data: &mut FunctionAnalysisData,
) {
    if let Some(classlike_storage) = codebase.classlike_infos.get(element_name) {
        let element_name = statements_analyzer.get_interner().lookup(element_name);
        if element_name.starts_with("Facebook\\XHP\\HTML\\")
            || property_id.1 == StrId::DATA_ATTRIBUTE
            || property_id.1 == StrId::ARIA_ATTRIBUTE
        {
            let label = DataFlowNodeId::Property(property_id.0, property_id.1);

            let mut taints = vec![SinkType::Output];

            if classlike_storage
                .appearing_property_ids
                .contains_key(&property_id.1)
            {
                match (element_name, name) {
                    // We allow input value attributes to have user-submitted values
                    // because that's to be expected
                    ("Facebook\\XHP\\HTML\\label", "for")
                    | ("Facebook\\XHP\\HTML\\meta", "content")
                    | (_, "id" | "class" | "lang" | "title" | "alt")
                    | (
                        "Facebook\\XHP\\HTML\\input" | "Facebook\\XHP\\HTML\\option",
                        "value" | "checked",
                    ) => {
                        // do nothing
                    }
                    (
                        "Facebook\\XHP\\HTML\\a"
                        | "Facebook\\XHP\\HTML\\area"
                        | "Facebook\\XHP\\HTML\\base"
                        | "Facebook\\XHP\\HTML\\link",
                        "href",
                    ) => {
                        taints.push(SinkType::HtmlAttributeUri);
                    }
                    ("Facebook\\XHP\\HTML\\body", "background")
                    | ("Facebook\\XHP\\HTML\\form", "action")
                    | (
                        "Facebook\\XHP\\HTML\\button" | "Facebook\\XHP\\HTML\\input",
                        "formaction",
                    )
                    | (
                        "Facebook\\XHP\\HTML\\iframe"
                        | "Facebook\\XHP\\HTML\\img"
                        | "Facebook\\XHP\\HTML\\script"
                        | "Facebook\\XHP\\HTML\\audio"
                        | "Facebook\\XHP\\HTML\\video"
                        | "Facebook\\XHP\\HTML\\source",
                        "src",
                    )
                    | ("Facebook\\XHP\\HTML\\video", "poster") => {
                        taints.push(SinkType::HtmlAttributeUri);
                    }
                    _ => {
                        taints.push(SinkType::HtmlAttribute);
                    }
                }
            }

            let xml_attribute_taint = DataFlowNode {
                id: label.clone(),
                kind: DataFlowNodeKind::TaintSink {
                    pos: statements_analyzer.get_hpos(attribute_value_pos),
                    types: taints,
                },
            };

            analysis_data.data_flow_graph.add_node(xml_attribute_taint);
        }
    }
}
