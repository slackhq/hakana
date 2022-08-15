use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::taint::SinkType;
use hakana_type::get_named_object;
use oxidized::aast;
use oxidized::ast_defs;
use rustc_hash::FxHashSet;
use std::rc::Rc;

use super::assignment::instance_property_assignment_analyzer::add_unspecialized_property_assignment_dataflow;

pub(crate) fn analyze(
    context: &mut ScopeContext,
    boxed: &Box<(
        ast_defs::Id,
        Vec<aast::XhpAttribute<(), ()>>,
        Vec<aast::Expr<(), ()>>,
    )>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    if_body_context: &mut Option<ScopeContext>,
    expr: &aast::Expr<(), ()>,
) {
    let mut name_string = boxed.0 .1.clone();
    if name_string.starts_with(":") {
        name_string = name_string[1..].to_string();
    }
    let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;
    if let Some(fq_name) = resolved_names.get(&boxed.0 .0.start_offset()) {
        name_string = fq_name.clone();
    }

    tast_info
        .symbol_references
        .add_reference_to_symbol(&context.function_context, name_string.clone());

    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    for attribute in &boxed.1 {
        match attribute {
            aast::XhpAttribute::XhpSimple(xhp_simple) => {
                analyze_xhp_attribute_assignment(
                    statements_analyzer,
                    &name_string,
                    xhp_simple,
                    tast_info,
                    context,
                    if_body_context,
                );
            }
            aast::XhpAttribute::XhpSpread(xhp_expr) => {
                expression_analyzer::analyze(
                    statements_analyzer,
                    xhp_expr,
                    tast_info,
                    context,
                    if_body_context,
                );
            }
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
    }
    context.inside_general_use = was_inside_general_use;

    tast_info.expr_types.insert(
        (expr.1.start_offset(), expr.1.end_offset()),
        Rc::new(get_named_object(name_string)),
    );
}

fn analyze_xhp_attribute_assignment(
    statements_analyzer: &StatementsAnalyzer,
    element_name: &String,
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

    let codebase = statements_analyzer.get_codebase();

    let property_id = (element_name.clone(), format!(":{}", attribute_info.name.1));

    let attribute_type = tast_info
        .expr_types
        .get(&(
            attribute_info.expr.pos().start_offset(),
            attribute_info.expr.pos().end_offset(),
        ))
        .cloned();

    if let Some(attribute_type) = attribute_type {
        if tast_info.data_flow_graph.kind == GraphKind::Taint {
            add_unspecialized_property_assignment_dataflow(
                statements_analyzer,
                &property_id,
                &attribute_info.name.0,
                Some(attribute_info.expr.pos()),
                tast_info,
                &attribute_type,
                codebase,
                element_name,
                &property_id.1,
            );

            if let Some(classlike_storage) = codebase.classlike_infos.get(element_name) {
                if element_name.starts_with("Facebook\\XHP\\HTML\\") {
                    let label = format!("{}::${}", property_id.0, property_id.1);

                    let mut taints = FxHashSet::from_iter([SinkType::Logging]);

                    if classlike_storage
                        .appearing_property_ids
                        .contains_key(&property_id.1)
                    {
                        // We allow input value attributes to have user-submitted values
                        // because that's to be expected
                        if element_name == "Facebook\\XHP\\HTML\\label"
                            && attribute_info.name.1 == "for"
                        {
                            // do nothing
                        } else if element_name == "Facebook\\XHP\\HTML\\meta"
                            && attribute_info.name.1 == "content"
                        {
                            // do nothing
                        } else if attribute_info.name.1 == "id"
                            || attribute_info.name.1 == "class"
                            || attribute_info.name.1 == "lang"
                        {
                            // do nothing
                        } else if (element_name == "Facebook\\XHP\\HTML\\input"
                            || element_name == "Facebook\\XHP\\HTML\\option")
                            && (attribute_info.name.1 == "value"
                                || attribute_info.name.1 == "checked")
                        {
                            // do nothing
                        } else if (element_name == "Facebook\\XHP\\HTML\\a"
                            || element_name == "Facebook\\XHP\\HTML\\area"
                            || element_name == "Facebook\\XHP\\HTML\\base"
                            || element_name == "Facebook\\XHP\\HTML\\link")
                            && attribute_info.name.1 == "href"
                        {
                            taints.insert(SinkType::HtmlAttributeUri);
                        } else if element_name == "Facebook\\XHP\\HTML\\body"
                            && attribute_info.name.1 == "background"
                        {
                            taints.insert(SinkType::HtmlAttributeUri);
                        } else if element_name == "Facebook\\XHP\\HTML\\form"
                            && attribute_info.name.1 == "action"
                        {
                            taints.insert(SinkType::HtmlAttributeUri);
                        } else if (element_name == "Facebook\\XHP\\HTML\\button"
                            || element_name == "Facebook\\XHP\\HTML\\input")
                            && attribute_info.name.1 == "formaction"
                        {
                            taints.insert(SinkType::HtmlAttributeUri);
                        } else if (element_name == "Facebook\\XHP\\HTML\\iframe"
                            || element_name == "Facebook\\XHP\\HTML\\img"
                            || element_name == "Facebook\\XHP\\HTML\\script"
                            || element_name == "Facebook\\XHP\\HTML\\audio"
                            || element_name == "Facebook\\XHP\\HTML\\video"
                            || element_name == "Facebook\\XHP\\HTML\\source")
                            && attribute_info.name.1 == "src"
                        {
                            taints.insert(SinkType::HtmlAttributeUri);
                        } else if element_name == "Facebook\\XHP\\HTML\\video"
                            && attribute_info.name.1 == "poster"
                        {
                            taints.insert(SinkType::HtmlAttributeUri);
                        } else {
                            taints.insert(SinkType::HtmlAttribute);
                        }
                    }

                    let xml_attribute_taint = DataFlowNode::TaintSink {
                        id: label.clone(),
                        label,
                        pos: None,
                        types: taints,
                    };

                    tast_info.data_flow_graph.add_node(xml_attribute_taint);
                }
            }
        }
    }
}
