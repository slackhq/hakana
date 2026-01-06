use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::{
    codebase_info::CodebaseInfo, functionlike_info::FunctionLikeInfo, t_union::TUnion,
};
use hakana_str::{Interner, StrId};
use oxidized::{aast, ast_defs::Pos};
use rustc_hash::FxHashMap;

use crate::config::Config;
use crate::{
    config, function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    statements_analyzer::StatementsAnalyzer,
};

pub struct AfterExprAnalysisData<'a> {
    pub context: &'a BlockContext,
    pub expr: &'a aast::Expr<(), ()>,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
    pub already_called: bool,
    pub is_sub_expression: bool,
}

pub struct AfterStmtAnalysisData<'a> {
    pub context: &'a BlockContext,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
    pub stmt: &'a aast::Stmt<(), ()>,
}

pub struct AfterDefAnalysisData<'a> {
    pub context: &'a BlockContext,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
    pub def: &'a aast::Def<(), ()>,
}

pub struct FunctionLikeParamData<'a> {
    pub context: &'a BlockContext,
    pub config: &'a config::Config,
    pub param_type: &'a TUnion,
    pub param_node: &'a aast::FunParam<(), ()>,
    pub codebase: &'a CodebaseInfo,
    pub interner: &'a Interner,
    pub in_migratable_function: bool,
}

pub struct AfterArgAnalysisData<'a> {
    pub arg: &'a aast::Argument<(), ()>,
    pub arg_value_type: &'a TUnion,
    pub argument_offset: usize,
    pub context: &'a BlockContext,
    pub function_name_pos: Option<&'a Pos>,
    pub function_call_pos: &'a Pos,
    pub functionlike_id: &'a FunctionLikeIdentifier,
    pub param_type: &'a TUnion,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
    pub already_called: bool,
}

pub trait InternalHook {
    fn get_migration_name(&self) -> Option<&str> {
        None
    }

    fn get_codegen_name(&self) -> Option<&str> {
        None
    }

    fn run_in_ide(&self) -> bool {
        true
    }

    // This hook is run after analysing every top-level definition (class, function etc)
    #[allow(unused_variables)]
    fn after_def_analysis(
        &self,
        analysis_data: &mut FunctionAnalysisData,
        analysis_result: &mut AnalysisResult,
        after_def_analysis_data: AfterDefAnalysisData,
    ) {
    }

    // This hook is run after analysing every AST statement
    #[allow(unused_variables)]
    fn after_stmt_analysis(
        &self,
        analysis_data: &mut FunctionAnalysisData,
        after_stmt_analysis_data: AfterStmtAnalysisData,
    ) {
    }

    // This hook is run after analysing every AST expression
    #[allow(unused_variables)]
    fn after_expr_analysis(
        &self,
        analysis_data: &mut FunctionAnalysisData,
        after_expr_analysis_data: AfterExprAnalysisData,
    ) {
    }

    // This hook is run when analysing a function or method's parameters
    // This is run before analysing a given function's body statements.
    #[allow(unused_variables)]
    fn handle_functionlike_param(
        &self,
        analysis_data: &mut FunctionAnalysisData,
        functionlike_param_data: FunctionLikeParamData,
    ) {
    }

    // This hook is run after analysing every argument in a given function of method call
    #[allow(unused_variables)]
    fn after_argument_analysis(
        &self,
        analysis_data: &mut FunctionAnalysisData,
        after_arg_analysis_data: AfterArgAnalysisData,
    ) {
    }

    #[allow(unused_variables)]
    fn after_functionlike_analysis(
        &self,
        context: &mut BlockContext,
        functionlike_storage: &FunctionLikeInfo,
        completed_analysis: bool,
        analysis_data: &mut FunctionAnalysisData,
        inferred_return_type: &mut Option<TUnion>,
        codebase: &CodebaseInfo,
        statements_analyzer: &StatementsAnalyzer,
        fb_ast: &[aast::Stmt<(), ()>],
    ) -> bool {
        false
    }

    fn get_custom_issue_names(&self) -> Vec<&str> {
        vec![]
    }

    #[allow(unused_variables)]
    fn get_candidates(
        &self,
        codebase: &CodebaseInfo,
        interner: &Interner,
        analysis_result: &AnalysisResult,
    ) -> Vec<String> {
        vec![]
    }

    #[allow(unused_variables)]
    fn can_codegen_def(
        &self,
        interner: &Interner,
        codebase: &CodebaseInfo,
        resolved_names: &FxHashMap<u32, StrId>,
        def: &aast::Def<(), ()>,
    ) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn after_populate(&self, codebase: &CodebaseInfo, interner: &Interner, config: &Config) {}
}

pub trait CustomHook: InternalHook + Send + Sync + core::fmt::Debug {}
