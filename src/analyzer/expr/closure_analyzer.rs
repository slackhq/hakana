use std::rc::Rc;

use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use crate::{functionlike_analyzer::FunctionLikeAnalyzer, scope_analyzer::ScopeAnalyzer};
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::functionlike_parameter::FnParameter;
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::type_expander;
use hakana_type::wrap_atomic;
use oxidized::aast;
use rustc_hash::FxHashSet;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut ScopeContext,
    tast_info: &mut TastInfo,
    fun: &aast::Fun_<(), ()>,
    expr: &aast::Expr<(), ()>,
) -> bool {
    let mut function_analyzer = FunctionLikeAnalyzer::new(statements_analyzer.get_file_analyzer());
    let mut lambda_context = context.clone();
    let mut analysis_result =
        AnalysisResult::new(tast_info.data_flow_graph.kind, SymbolReferences::new());
    let mut lambda_storage = function_analyzer.analyze_lambda(
        fun,
        &mut lambda_context,
        tast_info,
        &mut analysis_result,
        expr.pos(),
    );

    for param in lambda_storage.params.iter_mut() {
        if let Some(ref mut param_type) = param.signature_type {
            type_expander::expand_union(
                statements_analyzer.get_codebase(),
                param_type,
                &TypeExpansionOptions {
                    evaluate_conditional_types: true,
                    expand_generic: true,
                    ..Default::default()
                },
                &mut tast_info.data_flow_graph,
            )
        }
    }

    let issues = analysis_result.emitted_issues.into_iter().next();

    if let Some(issues) = issues {
        for issue in issues.1 {
            tast_info.maybe_add_issue(
                issue,
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    let replacements = analysis_result.replacements.into_iter().next();

    if let Some((_, replacements)) = replacements {
        tast_info.replacements.extend(replacements);
    }

    let closure_id = format!("{}:{}", fun.span.filename(), fun.span.start_offset());

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
        closure_id: statements_analyzer
            .get_codebase()
            .interner
            .get(&closure_id)
            .unwrap(),
    });

    if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
        let application_node = DataFlowNode::get_for_method_reference(
            closure_id.clone(),
            statements_analyzer.get_hpos(expr.pos()),
        );

        let closure_return_node = DataFlowNode::get_for_method_return(
            closure_id.clone(),
            Some(statements_analyzer.get_hpos(expr.pos())),
            None,
        );

        tast_info.data_flow_graph.add_path(
            &closure_return_node,
            &application_node,
            PathKind::Default,
            None,
            None,
        );

        tast_info.data_flow_graph.add_node(application_node.clone());

        closure_type.parent_nodes = FxHashSet::from_iter([application_node]);
    }

    tast_info.expr_types.insert(
        (expr.1.start_offset(), expr.1.end_offset()),
        Rc::new(closure_type),
    );

    true
}
