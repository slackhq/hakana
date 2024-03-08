use crate::expression_analyzer;
use crate::file_analyzer::FileAnalyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::functionlike_analyzer::FunctionLikeAnalyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::codebase_info::symbols::SymbolKind;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::function_context::FunctionContext;
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
    ) -> Result<(), AnalysisError> {
        let resolved_names = self.file_analyzer.resolved_names.clone();
        let name = if let Some(resolved_name) = resolved_names.get(&(stmt.name.0.start_offset() as u32)) {
            *resolved_name
        } else {
            return Err(AnalysisError::InternalError(
                format!("Cannot resolve class name {}", &stmt.name.1),
                statements_analyzer.get_hpos(stmt.name.pos()),
            ));
        };

        let codebase = self.file_analyzer.get_codebase();

        if self.file_analyzer.analysis_config.ast_diff
            && self.file_analyzer.codebase.safe_symbols.contains(&name)
        {
            return Ok(());
        }

        let classlike_storage = if let Some(storage) = codebase.classlike_infos.get(&name) {
            storage
        } else {
            return Err(AnalysisError::InternalError(
                format!("Cannot get class storage for {}", &stmt.name.1),
                statements_analyzer.get_hpos(&stmt.name.0),
            ));
        };

        for parent_class in &classlike_storage.all_parent_classes {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name, *parent_class, true);
        }

        for parent_interface in &classlike_storage.all_parent_interfaces {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name, *parent_interface, true);
        }

        for trait_name in &classlike_storage.used_traits {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name, *trait_name, true);
        }

        let mut function_context = FunctionContext::new();
        function_context.calling_class = Some(name);
        function_context.calling_class_final = stmt.final_;

        let mut class_context = ScopeContext::new(function_context);

        let mut analysis_data = FunctionAnalysisData::new(
            DataFlowGraph::new(statements_analyzer.get_config().graph_kind),
            statements_analyzer.get_file_analyzer().get_file_source(),
            &statements_analyzer.comments,
            &statements_analyzer.get_config().all_custom_issues,
            None,
            classlike_storage.meta_start.start_offset,
            None,
        );

        for constant in &stmt.consts {
            match &constant.kind {
                aast::ClassConstKind::CCAbstract(Some(expr))
                | aast::ClassConstKind::CCConcrete(expr) => {
                    expression_analyzer::analyze(
                        statements_analyzer,
                        expr,
                        &mut analysis_data,
                        &mut class_context,
                        &mut None,
                    )?;
                }
                _ => {}
            }
        }

        for var in &stmt.vars {
            if let Some(default) = &var.expr {
                expression_analyzer::analyze(
                    statements_analyzer,
                    default,
                    &mut analysis_data,
                    &mut class_context,
                    &mut None,
                )?;
            }
        }

        analysis_result
            .symbol_references
            .extend(analysis_data.symbol_references);

        for method in &stmt.methods {
            if method.abstract_ || matches!(classlike_storage.kind, SymbolKind::Interface) {
                continue;
            }

            let mut method_analyzer = FunctionLikeAnalyzer::new(self.file_analyzer);
            method_analyzer.analyze_method(method, classlike_storage, analysis_result)?;
        }

        Ok(())
    }
}
