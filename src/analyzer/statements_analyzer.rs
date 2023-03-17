use crate::config::Config;
use crate::file_analyzer::FileAnalyzer;
use crate::formula_generator::AssertionContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::loop_scope::LoopScope;
use crate::scope_context::ScopeContext;
use crate::stmt_analyzer;
use crate::typed_ast::FunctionAnalysisData;
use hakana_reflection_info::code_location::{FilePath, HPos};
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::symbol_references::ReferenceSource;
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use hakana_reflection_info::{Interner, StrId};
use oxidized::aast;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;

pub struct StatementsAnalyzer<'a> {
    file_analyzer: &'a FileAnalyzer<'a>,
    function_info: Option<&'a FunctionLikeInfo>,
    pub comments: Vec<&'a (Pos, Comment)>,
    type_resolution_context: &'a TypeResolutionContext,
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
        }
    }

    pub fn analyze(
        &self,
        stmts: &Vec<aast::Stmt<(), ()>>,
        analysis_data: &mut FunctionAnalysisData,
        context: &mut ScopeContext,
        loop_scope: &mut Option<LoopScope>,
    ) -> bool {
        for stmt in stmts {
            if context.has_returned
                && self.get_config().allow_issue_kind_in_file(
                    &IssueKind::UnevaluatedCode,
                    self.get_file_path_actual(),
                )
            {
                if self.get_config().find_unused_expressions {
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
            } else {
                if !stmt_analyzer::analyze(self, stmt, analysis_data, context, loop_scope) {
                    return false;
                }
            }
        }

        true
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
        &self.type_resolution_context
    }

    #[inline]
    pub fn get_hpos(&self, pos: &Pos) -> HPos {
        HPos::new(pos, self.file_analyzer.get_file_source().file_path, None)
    }

    #[inline]
    pub(crate) fn get_assertion_context<'b>(
        &'b self,
        this_class_name: Option<&'a StrId>,
        calling_functionlike_id: Option<&'a FunctionLikeIdentifier>,
    ) -> AssertionContext {
        AssertionContext {
            file_source: self.get_file_analyzer().get_file_source(),
            resolved_names: self.get_file_analyzer().resolved_names,
            codebase: Some((self.get_codebase(), self.get_interner())),
            this_class_name,
            type_resolution_context: &self.type_resolution_context,
            reference_source: match calling_functionlike_id {
                Some(functionlike_id) => match functionlike_id {
                    FunctionLikeIdentifier::Function(name) => ReferenceSource::Symbol(false, *name),
                    FunctionLikeIdentifier::Method(a, b) => {
                        ReferenceSource::ClasslikeMember(false, *a, *b)
                    }
                },
                None => ReferenceSource::Symbol(false, self.get_file_path().0),
            },
        }
    }

    pub fn get_file_path(&self) -> &FilePath {
        &self.get_file_analyzer().get_file_source().file_path
    }

    pub fn get_file_path_actual(&self) -> &str {
        &self.get_file_analyzer().get_file_source().file_path_actual
    }
}

impl ScopeAnalyzer for StatementsAnalyzer<'_> {
    fn get_namespace(&self) -> &Option<String> {
        return self.file_analyzer.get_namespace();
    }

    fn get_file_analyzer(&self) -> &FileAnalyzer {
        self.file_analyzer
    }

    fn get_codebase(&self) -> &CodebaseInfo {
        self.file_analyzer.get_codebase()
    }

    fn get_interner(&self) -> &Interner {
        &self.file_analyzer.interner
    }

    fn get_config(&self) -> &Config {
        self.file_analyzer.get_config()
    }
}
