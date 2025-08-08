use crate::config::Config;
use crate::file_analyzer::FileAnalyzer;
use crate::formula_generator::AssertionContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::loop_scope::LoopScope;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::stmt_analyzer::{self, AnalysisError};
use hakana_code_info::code_location::{FilePath, HPos};
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::symbol_references::ReferenceSource;
use hakana_code_info::type_resolution::TypeResolutionContext;
use hakana_str::{Interner, StrId};
use oxidized::aast;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;

pub struct StatementsAnalyzer<'a> {
    pub file_analyzer: &'a FileAnalyzer<'a>,
    function_info: Option<&'a FunctionLikeInfo>,
    pub comments: Vec<&'a (Pos, Comment)>,
    type_resolution_context: &'a TypeResolutionContext,
    pub in_migratable_function: bool,
    pub interner: &'a Interner,
    pub codebase: &'a CodebaseInfo,
}

impl<'a> StatementsAnalyzer<'a> {
    pub fn new(
        file_analyzer: &'a FileAnalyzer,
        type_resolution_context: &'a TypeResolutionContext,
        comments: Vec<&'a (Pos, Comment)>,
    ) -> Self {
        Self {
            file_analyzer,
            function_info: None,
            comments,
            type_resolution_context,
            in_migratable_function: false,
            interner: file_analyzer.interner,
            codebase: &file_analyzer.codebase,
        }
    }

    pub(crate) fn analyze(
        &self,
        stmts: &Vec<aast::Stmt<(), ()>>,
        analysis_data: &mut FunctionAnalysisData,
        context: &mut BlockContext,
        loop_scope: &mut Option<LoopScope>,
    ) -> Result<(), AnalysisError> {
        for stmt in stmts {
            if context.has_returned {
                if self.get_config().find_unused_expressions {
                    let is_harmless = match &stmt.1 {
                        aast::Stmt_::Break => true,
                        aast::Stmt_::Continue => true,
                        aast::Stmt_::Return(boxed) => boxed.is_none(),
                        _ => false,
                    };

                    if stmt.0.line() > 0 {
                        if is_harmless {
                            analysis_data.maybe_add_issue(
                                Issue::new(
                                    IssueKind::UselessControlFlow,
                                    "This control flow is unnecessary".to_string(),
                                    self.get_hpos(&stmt.0),
                                    &context.function_context.calling_functionlike_id,
                                ),
                                self.get_config(),
                                self.get_file_path_actual(),
                            );
                        } else {
                            analysis_data.maybe_add_issue(
                                Issue::new(
                                    IssueKind::UnevaluatedCode,
                                    "Unused code after return/throw/continue".to_string(),
                                    self.get_hpos(&stmt.0),
                                    &context.function_context.calling_functionlike_id,
                                ),
                                self.get_config(),
                                self.get_file_path_actual(),
                            );
                        }
                    }
                }
            } else {
                stmt_analyzer::analyze(self, stmt, analysis_data, context, loop_scope)?;
            }
        }

        Ok(())
    }

    pub fn set_function_info(&mut self, function_info: &'a FunctionLikeInfo) {
        self.function_info = Some(function_info);
    }

    #[inline]
    pub fn get_functionlike_info(&self) -> Option<&FunctionLikeInfo> {
        self.function_info
    }

    #[inline]
    pub fn get_type_resolution_context(&self) -> &TypeResolutionContext {
        self.type_resolution_context
    }

    #[inline]
    pub fn get_hpos(&self, pos: &Pos) -> HPos {
        HPos::new(pos, self.file_analyzer.file_source.file_path)
    }

    #[inline]
    pub(crate) fn get_assertion_context(
        &self,
        this_class_name: Option<StrId>,
        calling_functionlike_id: Option<FunctionLikeIdentifier>,
    ) -> AssertionContext<'a> {
        AssertionContext {
            file_source: &self.file_analyzer.file_source,
            resolved_names: self.file_analyzer.resolved_names,
            codebase: Some((self.codebase, self.interner)),
            this_class_name,
            type_resolution_context: self.type_resolution_context,
            reference_source: match calling_functionlike_id {
                Some(functionlike_id) => match functionlike_id {
                    FunctionLikeIdentifier::Function(name) => ReferenceSource::Symbol(false, name),
                    FunctionLikeIdentifier::Method(a, b) => {
                        ReferenceSource::ClasslikeMember(false, a, b)
                    }
                    _ => {
                        panic!()
                    }
                },
                None => ReferenceSource::Symbol(false, self.get_file_path().0),
            },
            config: self.file_analyzer.analysis_config,
        }
    }

    pub fn get_file_path(&self) -> &FilePath {
        &self.file_analyzer.file_source.file_path
    }

    pub fn get_file_path_actual(&self) -> &str {
        &self.file_analyzer.file_source.file_path_actual
    }
}

impl ScopeAnalyzer for StatementsAnalyzer<'_> {
    fn get_namespace(&self) -> &Option<String> {
        return self.file_analyzer.get_namespace();
    }

    fn get_file_analyzer(&self) -> &FileAnalyzer {
        self.file_analyzer
    }

    fn get_config(&self) -> &Config {
        self.file_analyzer.get_config()
    }
}
