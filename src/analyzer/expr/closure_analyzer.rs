use std::rc::Rc;

use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::{functionlike_analyzer::FunctionLikeAnalyzer, scope_analyzer::ScopeAnalyzer};
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::functionlike_parameter::FnParameter;
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::type_expander;
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::wrap_atomic;
use oxidized::aast;
use rustc_hash::FxHashSet;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    analysis_data: &mut FunctionAnalysisData,
    fun: &aast::Fun_<(), ()>,
    expr: &aast::Expr<(), ()>,
) -> bool {
    let mut function_analyzer = FunctionLikeAnalyzer::new(statements_analyzer.get_file_analyzer());
    let mut analysis_result =
        AnalysisResult::new(analysis_data.data_flow_graph.kind, SymbolReferences::new());
    let mut lambda_storage = function_analyzer.analyze_lambda(
        fun,
        context.clone(),
        analysis_data,
        &mut analysis_result,
        expr.pos(),
    );

    for param in lambda_storage.params.iter_mut() {
        if let Some(ref mut param_type) = param.signature_type {
            type_expander::expand_union(
                statements_analyzer.get_codebase(),
                &Some(statements_analyzer.get_interner()),
                param_type,
                &TypeExpansionOptions {
                    evaluate_conditional_types: true,
                    expand_generic: true,
                    ..Default::default()
                },
                &mut analysis_data.data_flow_graph,
            )
        }
    }

    let issues = analysis_result.emitted_issues.into_iter().next();

    if let Some(issues) = issues {
        for issue in issues.1 {
            analysis_data.maybe_add_issue(
                issue,
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    let replacements = analysis_result.replacements.into_iter().next();

    if let Some((_, replacements)) = replacements {
        analysis_data.replacements.extend(replacements);
    }

    let closure_id = format!(
        "{}:{}",
        statements_analyzer.get_file_path().0 .0,
        fun.span.start_offset()
    );

    let mut closure_type = wrap_atomic(TAtomic::TClosure {
        params: lambda_storage
            .params
            .into_iter()
            .map(|param| FnParameter {
                signature_type: param.signature_type,
                is_inout: param.is_inout,
                is_variadic: param.is_variadic,
                is_optional: param.is_optional,
            })
            .collect(),
        return_type: lambda_storage.return_type,
        effects: lambda_storage.effects.to_u8(),
        closure_id: statements_analyzer.get_interner().get(&closure_id).unwrap(),
    });

    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        let application_node = DataFlowNode::get_for_method_reference(
            closure_id.clone(),
            Some(statements_analyzer.get_hpos(expr.pos())),
        );

        let closure_return_node = DataFlowNode::get_for_method_return(
            closure_id.clone(),
            Some(statements_analyzer.get_hpos(expr.pos())),
            None,
        );

        analysis_data.data_flow_graph.add_path(
            &closure_return_node,
            &application_node,
            PathKind::Default,
            None,
            None,
        );

        analysis_data
            .data_flow_graph
            .add_node(application_node.clone());

        closure_type.parent_nodes = FxHashSet::from_iter([application_node]);
    }

    analysis_data.expr_types.insert(
        (expr.1.start_offset(), expr.1.end_offset()),
        Rc::new(closure_type),
    );

    true
}
