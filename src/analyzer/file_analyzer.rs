use crate::config::Config;
use crate::def_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::functionlike_analyzer::update_analysis_result_with_tast;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::code_location::HPos;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::data_flow::graph::DataFlowGraph;
use hakana_code_info::function_context::FunctionContext;
use hakana_code_info::type_resolution::TypeResolutionContext;
use hakana_code_info::FileSource;
use hakana_str::{Interner, StrId};
use oxidized::aast;
use rustc_hash::FxHashMap;

pub struct InternalError(pub String, pub HPos);

#[derive(Clone)]
pub struct FileAnalyzer<'a> {
    pub file_source: FileSource<'a>,
    namespace_name: Option<String>,
    pub resolved_names: &'a FxHashMap<u32, StrId>,
    pub codebase: &'a CodebaseInfo,
    pub interner: &'a Interner,
    pub analysis_config: &'a Config,
}

impl<'a> FileAnalyzer<'a> {
    pub fn new(
        file_source: FileSource<'a>,
        resolved_names: &'a FxHashMap<u32, StrId>,
        codebase: &'a CodebaseInfo,
        interner: &'a Interner,
        analysis_config: &'a Config,
    ) -> Self {
        Self {
            file_source,
            namespace_name: None,
            resolved_names,
            codebase,
            interner,
            analysis_config,
        }
    }

    pub fn analyze(
        &mut self,
        program: &aast::Program<(), ()>,
        analysis_result: &mut AnalysisResult,
    ) -> Result<(), InternalError> {
        let mut analysis_data = FunctionAnalysisData::new(
            DataFlowGraph::new(self.analysis_config.graph_kind),
            &self.file_source,
            &Vec::from_iter(self.file_source.comments.iter()),
            &self.get_config().all_custom_issues,
            None,
            None,
            0,
            None,
        );

        if let Some(issue_filter) = &self.get_config().allowed_issues {
            analysis_data.issue_filter = Some(issue_filter.clone());
        }

        let unnamespaced_file_analyzer = self.clone();
        let type_resolution_context = TypeResolutionContext::new();
        let statements_analyzer = StatementsAnalyzer::new(
            &unnamespaced_file_analyzer,
            &type_resolution_context,
            Vec::from_iter(self.file_source.comments.iter()),
        );

        let mut context = BlockContext::new(FunctionContext::new());

        for declaration in program {
            if declaration.is_namespace() {
                let namespace_declaration = declaration.as_namespace().unwrap();
                self.namespace_name = Some(namespace_declaration.0 .1.to_string());

                for namespace_statement in namespace_declaration.1 {
                    def_analyzer::analyze(
                        self,
                        &statements_analyzer,
                        namespace_statement,
                        &mut context,
                        &mut None,
                        &mut analysis_data,
                        analysis_result,
                    )?;
                }

                if !namespace_declaration.1.is_empty() {
                    self.namespace_name = None;
                }
            } else {
                def_analyzer::analyze(
                    self,
                    &statements_analyzer,
                    declaration,
                    &mut context,
                    &mut None,
                    &mut analysis_data,
                    analysis_result,
                )?;
            }
        }

        update_analysis_result_with_tast(
            analysis_data,
            analysis_result,
            &statements_analyzer
                .file_analyzer
                .file_source
                .file_path,
            false,
        );

        Ok(())
    }

    pub fn get_file_source(&self) -> &FileSource<'_> {
        &self.file_source
    }
}

impl ScopeAnalyzer for FileAnalyzer<'_> {
    fn get_namespace(&self) -> &Option<String> {
        &self.namespace_name
    }

    fn get_file_analyzer(&self) -> &Self {
        self
    }

    fn get_config(&self) -> &Config {
        self.analysis_config
    }
}
