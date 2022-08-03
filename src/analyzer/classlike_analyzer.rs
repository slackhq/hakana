use crate::expression_analyzer;
use crate::file_analyzer::FileAnalyzer;
use crate::functionlike_analyzer::FunctionLikeAnalyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use function_context::FunctionContext;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::codebase_info::symbols::SymbolKind;
use hakana_reflection_info::data_flow::graph::{DataFlowGraph, GraphKind};
use oxidized::aast;

pub(crate) struct ClassLikeAnalyzer<'a> {
    file_analyzer: &'a FileAnalyzer<'a>,
}

impl<'a> ClassLikeAnalyzer<'a> {
    pub fn new(file_analyzer: &'a FileAnalyzer) -> Self {
        Self { file_analyzer }
    }

    pub fn analyze(
        &mut self,
        stmt: &aast::Class_<(), ()>,
        statements_analyzer: &StatementsAnalyzer,
        analysis_result: &mut AnalysisResult,
    ) {
        let resolved_names = self.file_analyzer.resolved_names.clone();
        let resolved_name = resolved_names.get(&stmt.name.0.start_offset());

        let name = match resolved_name {
            Some(resolved_name) => resolved_name.clone(),
            None => stmt.name.1.clone(),
        };

        let codebase = self.file_analyzer.get_codebase();
        let classlike_storage = codebase.classlike_infos.get(&name).unwrap();

        for parent_class in &classlike_storage.all_parent_classes {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name.clone(), parent_class.clone());
        }

        for parent_interface in &classlike_storage.all_parent_interfaces {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name.clone(), parent_interface.clone());
        }

        for trait_name in &classlike_storage.used_traits {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name.clone(), trait_name.clone());
        }

        let mut class_context = ScopeContext::new(FunctionContext::new());

        class_context.function_context.calling_class = Some(name.clone());

        let mut tast_info = TastInfo::new(
            DataFlowGraph::new(GraphKind::Variable),
            statements_analyzer.get_file_analyzer().get_file_source(),
        );

        for constant in &stmt.consts {
            match &constant.kind {
                aast::ClassConstKind::CCAbstract(Some(expr))
                | aast::ClassConstKind::CCConcrete(expr) => {
                    expression_analyzer::analyze(
                        statements_analyzer,
                        expr,
                        &mut tast_info,
                        &mut class_context,
                        &mut None,
                    );
                }
                _ => {}
            }
        }

        for var in &stmt.vars {
            if let Some(default) = &var.expr {
                expression_analyzer::analyze(
                    statements_analyzer,
                    default,
                    &mut tast_info,
                    &mut class_context,
                    &mut None,
                );
            }
        }

        analysis_result
            .symbol_references
            .extend(tast_info.symbol_references);

        for method in &stmt.methods {
            if method.abstract_ || matches!(classlike_storage.kind, SymbolKind::Interface) {
                continue;
            }

            let mut method_analyzer = FunctionLikeAnalyzer::new(self.file_analyzer);
            let mut context = ScopeContext::new(FunctionContext::new());
            method_analyzer.analyze_method(
                method,
                classlike_storage,
                &mut context,
                analysis_result,
            );
        }
    }
}
