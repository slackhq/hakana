use crate::{
    scope_analyzer::ScopeAnalyzer, scope_context::ScopeContext,
    statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo,
};
use hakana_reflection_info::{
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    issue::{Issue, IssueKind},
    t_union::TUnion,
    taint::SourceType,
};
use hakana_type::{get_int, get_mixed_any, get_mixed_dict};
use oxidized::{ast_defs::Pos, tast::Lid};
use rustc_hash::FxHashSet;
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    lid: &Lid,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    tast_info
        .pure_exprs
        .insert((pos.start_offset(), pos.end_offset()));

    if !context.has_variable(&lid.1 .1) {
        let superglobal_type = match lid.1 .1.as_str() {
            "$_FILES" | "$_POST" | "$_GET" | "$_ENV" | "$_SERVER" | "$_REQUEST" | "$_COOKIE" => {
                let superglobal_type = Rc::new(get_type_for_superglobal(
                    statements_analyzer,
                    lid.1 .1[1..].to_string(),
                    pos,
                    tast_info,
                ));

                context
                    .vars_in_scope
                    .insert(lid.1 .1.clone(), superglobal_type.clone());

                superglobal_type
            }
            _ => {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::UndefinedVariable,
                        format!("Cannot find referenced variable {}", &lid.1 .1),
                        statements_analyzer.get_hpos(&pos),
                    ),
                    statements_analyzer.get_config(),
                );

                Rc::new(get_mixed_any())
            }
        };
        tast_info.set_rc_expr_type(&pos, superglobal_type);
    } else if let Some(var_type) = context.vars_in_scope.get(&lid.1 .1) {
        let mut var_type = (**var_type).clone();

        var_type =
            add_dataflow_to_variable(statements_analyzer, lid, pos, var_type, tast_info, context);

        tast_info.set_expr_type(&pos, var_type);
    }

    true
}

pub(crate) fn get_type_for_superglobal(
    statements_analyzer: &StatementsAnalyzer,
    name: String,
    pos: &Pos,
    tast_info: &mut TastInfo,
) -> TUnion {
    match name.as_str() {
        "_FILES" | "_SERVER" | "_ENV" => get_mixed_dict(),
        "_GET" | "_REQUEST" | "_POST" | "_COOKIE" => {
            let mut var_type = get_mixed_dict();

            let taint_pos = statements_analyzer.get_hpos(pos);
            let taint_source = DataFlowNode::TaintSource {
                id: format!(
                    "${}:{}:{}",
                    name, taint_pos.file_path, taint_pos.start_offset
                ),
                label: format!("${}", name.clone()),
                pos: None,
                types: if name == "_GET" || name == "_REQUEST" {
                    FxHashSet::from_iter([SourceType::UriRequestHeader])
                } else {
                    FxHashSet::from_iter([SourceType::NonUriRequestHeader])
                },
            };

            tast_info.data_flow_graph.add_node(taint_source.clone());

            var_type
                .parent_nodes
                .insert(taint_source.get_id().clone(), taint_source);

            var_type
        }
        "argv" => get_mixed_any(),
        "argc" => get_int(),
        _ => get_mixed_any(),
    }
}

fn add_dataflow_to_variable(
    statements_analyzer: &StatementsAnalyzer,
    lid: &Lid,
    pos: &Pos,
    stmt_type: TUnion,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> TUnion {
    let mut stmt_type = stmt_type;

    let ref mut data_flow_graph = tast_info.data_flow_graph;

    if data_flow_graph.kind == GraphKind::FunctionBody {
        if context.inside_general_use || context.inside_throw || context.inside_isset {
            let assignment_node = DataFlowNode::VariableUseSink {
                id: lid.1 .1.to_string(),
                pos: statements_analyzer.get_hpos(pos),
            };

            data_flow_graph.add_node(assignment_node.clone());

            let mut parent_nodes = stmt_type.parent_nodes.clone();

            if parent_nodes.is_empty() {
                parent_nodes.insert(assignment_node.get_id().clone(), assignment_node);
            } else {
                for (_, parent_node) in &parent_nodes {
                    data_flow_graph.add_path(
                        &parent_node,
                        &assignment_node,
                        PathKind::Default,
                        None,
                        None,
                    );
                }
            }

            stmt_type.parent_nodes = parent_nodes;
        }
    }

    stmt_type
}
