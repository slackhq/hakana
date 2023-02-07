use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::node::DataFlowNodeKind;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::issue::Issue;
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::property_info::PropertyKind;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::taint::SinkType;
use hakana_reflection_info::StrId;
use hakana_type::get_named_object;
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
    context: &mut ScopeContext,
    boxed: &Box<(
        ast_defs::Id,
        Vec<aast::XhpAttribute<(), ()>>,
        Vec<aast::Expr<(), ()>>,
    )>,
    pos: &Pos,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    if_body_context: &mut Option<ScopeContext>,
) {
    let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;
    let xhp_class_name = resolved_names.get(&boxed.0 .0.start_offset()).unwrap();

    tast_info.symbol_references.add_reference_to_symbol(
        &context.function_context,
        xhp_class_name.clone(),
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
                    xhp_simple,
                    resolved_names,
                    tast_info,
                    context,
                    &xhp_class_name,
                );

                used_attributes.insert(attribute_name);

                analyze_xhp_attribute_assignment(
                    statements_analyzer,
                    attribute_name,
                    &xhp_class_name,
                    xhp_simple,
                    tast_info,
                    context,
                    if_body_context,
                );
            }
            aast::XhpAttribute::XhpSpread(xhp_expr) => {
                used_attributes.extend(handle_attribute_spread(
                    statements_analyzer,
                    xhp_expr,
                    &xhp_class_name,
                    tast_info,
                    context,
                    if_body_context,
                    codebase,
                ));
            }
        }
    }

    if let Some(classlike_info) = codebase.classlike_infos.get(&xhp_class_name) {
        let mut required_attributes = classlike_info
            .properties
            .iter()
            .filter(|p| matches!(p.1.kind, PropertyKind::XhpAttribute { is_required: true }))
            .map(|p| p.0)
            .collect::<FxHashSet<_>>();

        required_attributes.retain(|attr| !used_attributes.contains(attr));

        if !required_attributes.is_empty() {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::MissingRequiredXhpAttribute,
                    format!(
                        "XHP class {} is missing {}: {}",
                        codebase.interner.lookup(xhp_class_name),
                        if required_attributes.len() == 1 {
                            "a required attribute"
                        } else {
                            "some required attributes"
                        },
                        required_attributes
                            .iter()
                            .map(|attr| codebase.interner.lookup(*attr)[1..].to_string())
                            .join(", ")
                    ),
                    statements_analyzer.get_hpos(&pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    for inner_expr in &boxed.2 {
        expression_analyzer::analyze(
            statements_analyzer,
            inner_expr,
            tast_info,
            context,
            if_body_context,
        );

        let element_name = statements_analyzer
            .get_codebase()
            .interner
            .lookup(xhp_class_name);

        if let Some(expr_type) = tast_info.expr_types.get(&(
            inner_expr.pos().start_offset(),
            inner_expr.pos().end_offset(),
        )) {
            if match element_name {
                "Facebook\\XHP\\HTML\\a" | "Facebook\\XHP\\HTML\\p" => true,
                _ => false,
            } {
                let xml_body_taint = DataFlowNode {
                    id: element_name.to_string(),
                    kind: DataFlowNodeKind::TaintSink {
                        pos: None,
                        label: element_name.to_string(),
                        types: FxHashSet::from_iter([SinkType::Output]),
                    },
                };

                for parent_node in &expr_type.parent_nodes {
                    tast_info.data_flow_graph.add_path(
                        parent_node,
                        &xml_body_taint,
                        PathKind::Default,
                        None,
                        None,
                    );
                }

                tast_info.data_flow_graph.add_node(xml_body_taint);
            }

            // find data leaking to style and script tags
            if match element_name {
                "Facebook\\XHP\\HTML\\style" | "Facebook\\XHP\\HTML\\script" => true,
                _ => false,
            } {
                let xml_body_taint = DataFlowNode {
                    id: element_name.to_string(),
                    kind: DataFlowNodeKind::TaintSink {
                        pos: None,
                        label: element_name.to_string(),
                        types: FxHashSet::from_iter([SinkType::HtmlTag, SinkType::Output]),
                    },
                };

                for parent_node in &expr_type.parent_nodes {
                    tast_info.data_flow_graph.add_path(
                        parent_node,
                        &xml_body_taint,
                        PathKind::Default,
                        None,
                        None,
                    );
                }

                tast_info.data_flow_graph.add_node(xml_body_taint);
            }
        }
    }
    context.inside_general_use = was_inside_general_use;

    tast_info.expr_types.insert(
        (pos.start_offset(), pos.end_offset()),
        Rc::new(get_named_object(*xhp_class_name)),
    );
}

fn handle_attribute_spread(
    statements_analyzer: &StatementsAnalyzer,
    xhp_expr: &aast::Expr<(), ()>,
    element_name: &StrId,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    codebase: &CodebaseInfo,
) -> FxHashSet<StrId> {
    expression_analyzer::analyze(
        statements_analyzer,
        xhp_expr,
        tast_info,
        context,
        if_body_context,
    );

    let mut used_attributes = FxHashSet::default();

    if let Some(expr_type) = tast_info
        .expr_types
        .get(&(xhp_expr.pos().start_offset(), xhp_expr.pos().end_offset()))
        .cloned()
    {
        for expr_type_atomic in &expr_type.types {
            match expr_type_atomic {
                TAtomic::TNamedObject {
                    name: spread_xhp_class,
                    is_this: true,
                    ..
                } => {
                    if let Some(spread_class_info) = codebase.classlike_infos.get(spread_xhp_class)
                    {
                        let all_attributes = spread_class_info
                            .properties
                            .iter()
                            .filter(|p| matches!(p.1.kind, PropertyKind::XhpAttribute { .. }));

                        for spread_attribute in all_attributes {
                            atomic_property_fetch_analyzer::analyze(
                                statements_analyzer,
                                (xhp_expr, xhp_expr),
                                xhp_expr.pos(),
                                tast_info,
                                context,
                                false,
                                expr_type_atomic.clone(),
                                codebase.interner.lookup(spread_attribute.0),
                                &None,
                                &None,
                            );

                            used_attributes.insert(*spread_attribute.0);

                            if let Some(property_fetch_type) = tast_info
                                .expr_types
                                .get(&(xhp_expr.pos().start_offset(), xhp_expr.pos().end_offset()))
                                .cloned()
                            {
                                add_all_dataflow(
                                    tast_info,
                                    statements_analyzer,
                                    (*element_name, *spread_attribute.0),
                                    xhp_expr.pos(),
                                    xhp_expr.pos(),
                                    property_fetch_type,
                                    codebase.interner.lookup(spread_attribute.0),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        tast_info.expr_types.insert(
            (xhp_expr.pos().start_offset(), xhp_expr.pos().end_offset()),
            expr_type,
        );
    }

    used_attributes
}

fn analyze_xhp_attribute_assignment(
    statements_analyzer: &StatementsAnalyzer,
    attribute_name: StrId,
    element_name: &StrId,
    attribute_info: &aast::XhpSimple<(), ()>,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) {
    expression_analyzer::analyze(
        statements_analyzer,
        &attribute_info.expr,
        tast_info,
        context,
        if_body_context,
    );

    let property_id = (*element_name, attribute_name);

    let attribute_value_type = tast_info
        .expr_types
        .get(&(
            attribute_info.expr.pos().start_offset(),
            attribute_info.expr.pos().end_offset(),
        ))
        .cloned();

    let attribute_name_pos = &attribute_info.name.0;
    let codebase = statements_analyzer.get_codebase();

    if let Some(classlike_info) = codebase.classlike_infos.get(element_name) {
        if attribute_name != StrId::data_attribute()
            && attribute_name != StrId::aria_attribute()
            && !classlike_info
                .appearing_property_ids
                .contains_key(&attribute_name)
        {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentXhpAttribute,
                    format!(
                        "XHP attribute {} is not defined on {}",
                        codebase.interner.lookup(&attribute_name),
                        codebase.interner.lookup(element_name)
                    ),
                    statements_analyzer.get_hpos(&attribute_name_pos),
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
            tast_info,
            statements_analyzer,
            property_id,
            attribute_name_pos,
            attribute_value_pos,
            attribute_value_type,
            &attribute_info.name.1,
        );
    }
}

fn add_all_dataflow(
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    property_id: (StrId, StrId),
    attribute_name_pos: &Pos,
    attribute_value_pos: &Pos,
    attribute_value_type: Rc<TUnion>,
    attribute_name: &str,
) {
    if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
        let codebase = statements_analyzer.get_codebase();

        add_unspecialized_property_assignment_dataflow(
            statements_analyzer,
            &property_id,
            attribute_name_pos,
            Some(attribute_value_pos),
            tast_info,
            &attribute_value_type,
            codebase,
            &property_id.0,
            property_id.1,
        );

        add_xml_attribute_dataflow(
            codebase,
            &property_id.0,
            property_id,
            attribute_name,
            tast_info,
        );
    }
}

fn get_attribute_name(
    attribute_info: &oxidized::tast::XhpSimple<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    tast_info: &mut TastInfo,
    context: &ScopeContext,
    element_name: &StrId,
) -> StrId {
    if attribute_info.name.1.starts_with("data-") {
        StrId::data_attribute()
    } else if attribute_info.name.1.starts_with("aria-") {
        StrId::aria_attribute()
    } else {
        let attribute_name = *resolved_names
            .get(&attribute_info.name.0.start_offset())
            .unwrap();

        tast_info.symbol_references.add_reference_to_class_member(
            &context.function_context,
            (*element_name, attribute_name),
            false,
        );

        attribute_name
    }
}

fn add_xml_attribute_dataflow(
    codebase: &CodebaseInfo,
    element_name: &StrId,
    property_id: (StrId, StrId),
    name: &str,
    tast_info: &mut TastInfo,
) {
    if let Some(classlike_storage) = codebase.classlike_infos.get(element_name) {
        let element_name = codebase.interner.lookup(element_name);
        if element_name.starts_with("Facebook\\XHP\\HTML\\")
            || property_id.1 == StrId::data_attribute()
            || property_id.1 == StrId::aria_attribute()
        {
            let label = format!(
                "{}::${}",
                codebase.interner.lookup(&property_id.0),
                codebase.interner.lookup(&property_id.1),
            );

            let mut taints = FxHashSet::from_iter([SinkType::Output]);

            if classlike_storage
                .appearing_property_ids
                .contains_key(&property_id.1)
            {
                // We allow input value attributes to have user-submitted values
                // because that's to be expected
                if element_name == "Facebook\\XHP\\HTML\\label" && name == "for" {
                    // do nothing
                } else if element_name == "Facebook\\XHP\\HTML\\meta" && name == "content" {
                    // do nothing
                } else if name == "id" || name == "class" || name == "lang" {
                    // do nothing
                } else if (element_name == "Facebook\\XHP\\HTML\\input"
                    || element_name == "Facebook\\XHP\\HTML\\option")
                    && (name == "value" || name == "checked")
                {
                    // do nothing
                } else if (element_name == "Facebook\\XHP\\HTML\\a"
                    || element_name == "Facebook\\XHP\\HTML\\area"
                    || element_name == "Facebook\\XHP\\HTML\\base"
                    || element_name == "Facebook\\XHP\\HTML\\link")
                    && name == "href"
                {
                    taints.insert(SinkType::HtmlAttributeUri);
                } else if element_name == "Facebook\\XHP\\HTML\\body" && name == "background" {
                    taints.insert(SinkType::HtmlAttributeUri);
                } else if element_name == "Facebook\\XHP\\HTML\\form" && name == "action" {
                    taints.insert(SinkType::HtmlAttributeUri);
                } else if (element_name == "Facebook\\XHP\\HTML\\button"
                    || element_name == "Facebook\\XHP\\HTML\\input")
                    && name == "formaction"
                {
                    taints.insert(SinkType::HtmlAttributeUri);
                } else if (element_name == "Facebook\\XHP\\HTML\\iframe"
                    || element_name == "Facebook\\XHP\\HTML\\img"
                    || element_name == "Facebook\\XHP\\HTML\\script"
                    || element_name == "Facebook\\XHP\\HTML\\audio"
                    || element_name == "Facebook\\XHP\\HTML\\video"
                    || element_name == "Facebook\\XHP\\HTML\\source")
                    && name == "src"
                {
                    taints.insert(SinkType::HtmlAttributeUri);
                } else if element_name == "Facebook\\XHP\\HTML\\video" && name == "poster" {
                    taints.insert(SinkType::HtmlAttributeUri);
                } else {
                    taints.insert(SinkType::HtmlAttribute);
                }
            }

            let xml_attribute_taint = DataFlowNode {
                id: label.clone(),
                kind: DataFlowNodeKind::TaintSink {
                    pos: None,
                    label,
                    types: taints,
                },
            };

            tast_info.data_flow_graph.add_node(xml_attribute_taint);
        }
    }
}
