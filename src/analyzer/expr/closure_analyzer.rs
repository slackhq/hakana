use std::rc::Rc;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{functionlike_analyzer::FunctionLikeAnalyzer, scope_analyzer::ScopeAnalyzer};
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::data_flow::node::DataFlowNode;
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_parameter::FnParameter;
use hakana_code_info::symbol_references::SymbolReferences;
use hakana_code_info::t_atomic::{TAtomic, TClosure};
use hakana_code_info::ttype::type_expander;
use hakana_code_info::ttype::type_expander::TypeExpansionOptions;
use hakana_code_info::ttype::wrap_atomic;
use oxidized::aast;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    fun: &aast::Fun_<(), ()>,
    expr: &aast::Expr<(), ()>,
) -> Result<(), AnalysisError> {
    let mut function_analyzer = FunctionLikeAnalyzer::new(statements_analyzer.file_analyzer);
    let mut analysis_result =
        AnalysisResult::new(analysis_data.data_flow_graph.kind, SymbolReferences::new());
    let mut lambda_storage = if let Ok(lambda_storage) = function_analyzer.analyze_lambda(
        fun,
        context.clone(),
        analysis_data,
        &mut analysis_result,
        expr.pos(),
    ) {
        lambda_storage
    } else {
        return Err(AnalysisError::UserError);
    };

    for param in lambda_storage.params.iter_mut() {
        if let Some(ref mut param_type) = param.signature_type {
            type_expander::expand_union(
                statements_analyzer.codebase,
                &Some(&analysis_data.scoped_interner),
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

    let mut closure_type = wrap_atomic(TAtomic::TClosure(Box::new(TClosure {
        params: lambda_storage
            .params
            .into_iter()
            .map(|param| FnParameter {
                signature_type: param.signature_type.map(Box::new),
                is_inout: param.is_inout,
                is_variadic: param.is_variadic,
                is_optional: param.is_optional,
            })
            .collect(),
        return_type: lambda_storage.return_type,
        effects: lambda_storage.effects.to_u8(),
        closure_id: (
            *statements_analyzer.get_file_path(),
            fun.span.start_offset() as u32,
        ),
    })));

    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        let application_node = DataFlowNode::get_for_method_reference(
            &FunctionLikeIdentifier::Closure(
                *statements_analyzer.get_file_path(),
                fun.span.start_offset() as u32,
            ),
            Some(statements_analyzer.get_hpos(expr.pos())),
        );

        let closure_return_node = DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Closure(
                *statements_analyzer.get_file_path(),
                fun.span.start_offset() as u32,
            ),
            Some(statements_analyzer.get_hpos(expr.pos())),
            None,
        );

        analysis_data.data_flow_graph.add_path(
            &closure_return_node,
            &application_node,
            PathKind::Default,
            vec![],
            vec![],
        );

        analysis_data
            .data_flow_graph
            .add_node(application_node.clone());

        closure_type.parent_nodes = vec![application_node];
    }

    analysis_data.expr_types.insert(
        (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
        Rc::new(closure_type),
    );

    Ok(())
}
