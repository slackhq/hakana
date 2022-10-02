use crate::config::Config;
use crate::def_analyzer;
use crate::functionlike_analyzer::update_analysis_result_with_tast;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_file_info::FileSource;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::function_context::FunctionContext;
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use oxidized::aast;
use rustc_hash::FxHashMap;

#[derive(Clone)]
pub struct FileAnalyzer<'a> {
    file_source: FileSource,
    namespace_name: Option<String>,
    pub resolved_names: &'a FxHashMap<usize, String>,
    pub codebase: &'a CodebaseInfo,
    pub analysis_config: &'a Config,
}

impl<'a> FileAnalyzer<'a> {
    pub fn new(
        file_source: FileSource,
        resolved_names: &'a FxHashMap<usize, String>,
        codebase: &'a CodebaseInfo,
        analysis_config: &'a Config,
    ) -> Self {
        Self {
            file_source,
            namespace_name: None,
            resolved_names,
            codebase,
            analysis_config,
        }
    }

    pub fn analyze(
        &mut self,
        program: &aast::Program<(), ()>,
        analysis_result: &mut AnalysisResult,
    ) {
        let mut context = ScopeContext::new(FunctionContext::new());
        let mut tast_info = TastInfo::new(
            DataFlowGraph::new(self.analysis_config.graph_kind),
            &self.file_source,
            &Vec::from_iter(self.file_source.comments.iter()),
        );

        let unnamespaced_file_analyzer = self.clone();
        let type_resolution_context = TypeResolutionContext::new();
        let statements_analyzer = StatementsAnalyzer::new(
            &unnamespaced_file_analyzer,
            &type_resolution_context,
            Vec::new(),
        );

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
                        &mut tast_info,
                        analysis_result,
                    );
                }

                if namespace_declaration.1.len() > 0 {
                    self.namespace_name = None;
                }
            } else {
                def_analyzer::analyze(
                    self,
                    &statements_analyzer,
                    declaration,
                    &mut context,
                    &mut None,
                    &mut tast_info,
                    analysis_result,
                );
            }
        }

        update_analysis_result_with_tast(
            tast_info,
            analysis_result,
            &statements_analyzer
                .get_file_analyzer()
                .get_file_source()
                .file_path,
            false,
        );
    }

    pub fn get_file_source(&self) -> &FileSource {
        &self.file_source
    }
}

impl ScopeAnalyzer for FileAnalyzer<'_> {
    fn get_namespace(&self) -> &Option<String> {
        return &self.namespace_name;
    }

    fn get_file_analyzer(&self) -> &Self {
        self
    }

    fn get_codebase(&self) -> &CodebaseInfo {
        self.codebase
    }

    fn get_config(&self) -> &Config {
        self.analysis_config
    }
}
