use crate::config::Config;
use crate::file_analyzer::FileAnalyzer;
use crate::formula_generator::AssertionContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::loop_scope::LoopScope;
use crate::scope_context::ScopeContext;
use crate::stmt_analyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::codebase_info::symbols::Symbol;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use hakana_reflection_info::StrId;
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
        tast_info: &mut TastInfo,
        context: &mut ScopeContext,
        loop_scope: &mut Option<LoopScope>,
    ) -> bool {
        for stmt in stmts {
            if context.has_returned {
                if self.get_config().find_unused_expressions {
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::UnevaluatedCode,
                            "Unused code after return/throw/continue".to_string(),
                            self.get_hpos(&stmt.0),
                        ),
                        self.get_config(),
                        self.get_file_path_actual()
                    );
                }
            } else {
                if !stmt_analyzer::analyze(self, stmt, tast_info, context, loop_scope) {
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
        HPos::new(pos, self.file_analyzer.get_file_source().file_path)
    }

    #[inline]
    pub(crate) fn get_assertion_context<'b>(
        &'b self,
        this_class_name: Option<&'a Symbol>,
    ) -> AssertionContext {
        AssertionContext {
            file_source: self.get_file_analyzer().get_file_source(),
            resolved_names: self.get_file_analyzer().resolved_names,
            codebase: Some(self.get_codebase()),
            this_class_name,
            type_resolution_context: &self.type_resolution_context,
        }
    }

    pub fn get_file_path(&self) -> &StrId {
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

    fn get_config(&self) -> &Config {
        self.file_analyzer.get_config()
    }
}
