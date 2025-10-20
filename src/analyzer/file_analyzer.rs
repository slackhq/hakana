use crate::config::Config;
use crate::def_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::functionlike_analyzer::update_analysis_result_with_tast;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_code_info::FileSource;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::code_location::HPos;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::data_flow::graph::DataFlowGraph;
use hakana_code_info::function_context::FunctionContext;
use hakana_code_info::type_resolution::TypeResolutionContext;
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

        let type_resolution_context = TypeResolutionContext::new();
        let mut context = BlockContext::new(FunctionContext::new());
        let comments = Vec::from_iter(self.file_source.comments.iter());

        for declaration in program {
            let namespace_declaration = declaration.as_namespace();
            self.namespace_name = namespace_declaration.map(|(id, _)| id.1.to_string());

            let statements_analyzer =
                StatementsAnalyzer::new(&self, &type_resolution_context, &comments);

            if namespace_declaration.is_some() {
                let (_, statements) = namespace_declaration.unwrap();

                for namespace_statement in statements {
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

            if namespace_declaration.map_or(false, |(_, statements)| !statements.is_empty()) {
                self.namespace_name = None;
            }
        }

        update_analysis_result_with_tast(
            analysis_data,
            analysis_result,
            &self.file_source.file_path,
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
