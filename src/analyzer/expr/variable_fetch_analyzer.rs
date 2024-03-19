use crate::{
    function_analysis_data::FunctionAnalysisData, scope_analyzer::ScopeAnalyzer,
    scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};
use hakana_reflection_info::{
    data_flow::{
        graph::GraphKind,
        node::{DataFlowNode, DataFlowNodeKind},
        path::PathKind,
    },
    issue::{Issue, IssueKind},
    t_union::TUnion,
    taint::SourceType,
    EFFECT_READ_GLOBALS,
};
use hakana_type::{get_int, get_mixed_any, get_mixed_dict};
use oxidized::{ast_defs::Pos, tast::Lid};
use rustc_hash::FxHashSet;
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    lid: &Lid,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Result<(), AnalysisError> {
    if !context.has_variable(&lid.1 .1) {
        let superglobal_type = match lid.1 .1.as_str() {
            "$_FILES" | "$_POST" | "$_GET" | "$_ENV" | "$_SERVER" | "$_REQUEST" | "$_COOKIE" => {
                let superglobal_type = Rc::new(get_type_for_superglobal(
                    statements_analyzer,
                    lid.1 .1[1..].to_string(),
                    pos,
                    analysis_data,
                ));

                context
                    .vars_in_scope
                    .insert(lid.1 .1.clone(), superglobal_type.clone());

                superglobal_type
            }
            _ => {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::UndefinedVariable,
                        format!("Cannot find referenced variable {}", &lid.1 .1),
                        statements_analyzer.get_hpos(pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                Rc::new(get_mixed_any())
            }
        };

        analysis_data.set_rc_expr_type(pos, superglobal_type);

        analysis_data.expr_effects.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            EFFECT_READ_GLOBALS,
        );
    } else if let Some(var_type) = context.vars_in_scope.get(&lid.1 .1) {
        let mut var_type = (**var_type).clone();

        var_type = add_dataflow_to_variable(
            statements_analyzer,
            lid,
            pos,
            var_type,
            analysis_data,
            context,
        );

        analysis_data.set_expr_type(pos, var_type);

        if lid.1 .1 == "$$" {
            analysis_data.expr_effects.insert(
                (pos.start_offset() as u32, pos.end_offset() as u32),
                context.pipe_var_effects,
            );
        }
    }

    Ok(())
}

pub(crate) fn get_type_for_superglobal(
    statements_analyzer: &StatementsAnalyzer,
    name: String,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    match name.as_str() {
        "_FILES" | "_SERVER" | "_ENV" => get_mixed_dict(),
        "_GET" | "_REQUEST" | "_POST" | "_COOKIE" => {
            let mut var_type = get_mixed_dict();

            let taint_pos = statements_analyzer.get_hpos(pos);
            let taint_source = DataFlowNode {
                id: format!(
                    "${}:{}:{}",
                    name, taint_pos.file_path.0 .0, taint_pos.start_offset
                ),
                kind: DataFlowNodeKind::TaintSource {
                    pos: None,
                    label: format!("${}", name.clone()),
                    types: if name == "_GET" || name == "_REQUEST" {
                        FxHashSet::from_iter([SourceType::UriRequestHeader])
                    } else {
                        FxHashSet::from_iter([SourceType::NonUriRequestHeader])
                    },
                },
            };

            analysis_data.data_flow_graph.add_node(taint_source.clone());

            var_type.parent_nodes.push(taint_source);

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
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> TUnion {
    let mut stmt_type = stmt_type;

    let data_flow_graph = &mut analysis_data.data_flow_graph;

    if data_flow_graph.kind == GraphKind::FunctionBody && (context.inside_general_use || context.inside_throw || context.inside_isset) {
        let pos = statements_analyzer.get_hpos(pos);

        let assignment_node = DataFlowNode {
            id: lid.1 .1.to_string()
                + "-"
                + &pos.file_path.0 .0.to_string()
                + ":"
                + &pos.start_offset.to_string()
                + "-"
                + &pos.end_offset.to_string(),
            kind: DataFlowNodeKind::VariableUseSink { pos },
        };

        data_flow_graph.add_node(assignment_node.clone());

        let mut parent_nodes = stmt_type.parent_nodes.clone();

        if parent_nodes.is_empty() {
            parent_nodes.push(assignment_node);
        } else {
            for parent_node in &parent_nodes {
                data_flow_graph.add_path(
                    parent_node,
                    &assignment_node,
                    PathKind::Default,
                    None,
                    None,
                );
            }
        }

        stmt_type.parent_nodes = parent_nodes;
    }

    stmt_type
}
