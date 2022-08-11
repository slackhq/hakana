use function_context::FunctionLikeIdentifier;
use hakana_reflection_info::{
    codebase_info::CodebaseInfo, functionlike_info::FunctionLikeInfo, t_union::TUnion,
};
use oxidized::{
    aast,
    ast_defs::{self, Pos},
};

use crate::{
    config, scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};

pub struct AfterExprAnalysisData<'a> {
    pub context: &'a ScopeContext,
    pub expr: &'a aast::Expr<(), ()>,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
}

pub struct AfterStmtAnalysisData<'a> {
    pub context: &'a ScopeContext,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
    pub stmt: &'a aast::Stmt<(), ()>,
}

pub struct FunctionLikeParamData<'a> {
    pub context: &'a ScopeContext,
    pub config: &'a config::Config,
    pub param_type: &'a TUnion,
    pub param_node: &'a aast::FunParam<(), ()>,
}

pub struct AfterArgAnalysisData<'a> {
    pub arg: (&'a ast_defs::ParamKind, &'a aast::Expr<(), ()>),
    pub arg_value_type: &'a TUnion,
    pub argument_offset: usize,
    pub context: &'a ScopeContext,
    pub function_call_pos: &'a Pos,
    pub functionlike_id: &'a FunctionLikeIdentifier,
    pub param_type: &'a TUnion,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
}

pub trait InternalHook {
    // This hook is run after analysing every AST statement
    #[allow(unused_variables)]
    fn after_stmt_analysis(
        &self,
        tast_info: &mut TastInfo,
        after_stmt_analysis_data: AfterStmtAnalysisData,
    ) {
    }

    // This hook is run after analysing every AST expression
    #[allow(unused_variables)]
    fn after_expr_analysis(
        &self,
        tast_info: &mut TastInfo,
        after_expr_analysis_data: AfterExprAnalysisData,
    ) {
    }

    // This hook is run when analysing a function or method's parameters
    // This is run before analysing a given function's body statements.
    #[allow(unused_variables)]
    fn handle_functionlike_param(
        &self,
        tast_info: &mut TastInfo,
        functionlike_param_data: FunctionLikeParamData,
    ) {
    }

    // This hook is run after analysing every argument in a given function of method call
    #[allow(unused_variables)]
    fn after_argument_analysis(
        &self,
        tast_info: &mut TastInfo,
        after_arg_analysis_data: AfterArgAnalysisData,
    ) {
    }

    #[allow(unused_variables)]
    fn after_functionlike_analysis(
        &self,
        context: &mut ScopeContext,
        functionlike_storage: &FunctionLikeInfo,
        completed_analysis: bool,
        tast_info: &mut TastInfo,
        inferred_return_type: &mut Option<TUnion>,
        codebase: &CodebaseInfo,
        statements_analyzer: &StatementsAnalyzer,
        expected_type_id: String,
    ) -> bool {
        false
    }
}

pub trait CustomHook: InternalHook + Send + Sync {}
