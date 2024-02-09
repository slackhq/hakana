use crate::config::Config;
use crate::def_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::functionlike_analyzer::update_analysis_result_with_tast;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::function_context::FunctionContext;
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use hakana_reflection_info::{FileSource, Interner, StrId};
use oxidized::aast;
use rustc_hash::FxHashMap;

pub struct InternalError(pub String, pub HPos);

#[derive(Clone)]
pub struct FileAnalyzer<'a> {
    file_source: FileSource<'a>,
    namespace_name: Option<String>,
    pub resolved_names: &'a FxHashMap<usize, StrId>,
    pub codebase: &'a CodebaseInfo,
    pub interner: &'a Interner,
    pub analysis_config: &'a Config,
}

impl<'a> FileAnalyzer<'a> {
    pub fn new(
        file_source: FileSource<'a>,
        resolved_names: &'a FxHashMap<usize, StrId>,
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
            0,
            None,
        );

        let unnamespaced_file_analyzer = self.clone();
        let type_resolution_context = TypeResolutionContext::new();
        let statements_analyzer = StatementsAnalyzer::new(
            &unnamespaced_file_analyzer,
            &type_resolution_context,
            Vec::new(),
        );

        let mut context = ScopeContext::new(FunctionContext::new());

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
                .get_file_analyzer()
                .get_file_source()
                .file_path,
            false,
        );

        Ok(())
    }

    pub fn get_file_source(&self) -> &FileSource {
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

    fn get_codebase(&self) -> &CodebaseInfo {
        self.codebase
    }

    fn get_interner(&self) -> &Interner {
        self.interner
    }

    fn get_config(&self) -> &Config {
        self.analysis_config
    }
}
