use function_context::{method_identifier::MethodIdentifier, FunctionLikeIdentifier};
use hakana_reflection_info::{
    codebase_info::CodebaseInfo, functionlike_info::FunctionLikeInfo, t_atomic::TAtomic,
    t_union::TUnion,
};
use oxidized::{
    aast,
    ast_defs::{self, Pos},
};

use crate::{
    config, scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};

pub struct ReturnData<'a> {
    pub context: &'a ScopeContext,
    pub expected_return_type_id: &'a Option<String>,
    pub inferred_return_type: &'a TUnion,
    pub return_expr: &'a aast::Expr<(), ()>,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
}

pub struct AfterExprAnalysisData<'a> {
    pub context: &'a ScopeContext,
    pub expr: &'a aast::Expr<(), ()>,
    pub statements_analyzer: &'a StatementsAnalyzer<'a>,
}

pub trait InternalHook {
    #[allow(unused_variables)]
    fn handle_return_expr(&self, tast_info: &mut TastInfo, return_data: ReturnData) {}

    #[allow(unused_variables)]
    fn after_expr_analysis(
        &self,
        tast_info: &mut TastInfo,
        after_expr_analysis_data: AfterExprAnalysisData,
    ) {
        if let aast::Expr_::ArrayGet(boxed) = &after_expr_analysis_data.expr.2 {
            let stmt_var_type = if let Some(expr_type) = tast_info.get_expr_type(&boxed.0 .1) {
                expr_type
            } else {
                return;
            };
        }
    }

    #[allow(unused_variables)]
    fn handle_expanded_param(
        &self,
        context: &ScopeContext,
        config: &config::Config,
        param_type: &TUnion,
        param_node: &aast::FunParam<(), ()>,
        tast_info: &mut TastInfo,
    ) {
    }

    #[allow(unused_variables)]
    fn handle_argument(
        &self,
        functionlike_id: &FunctionLikeIdentifier,
        config: &config::Config,
        context: &ScopeContext,
        arg_value_type: &TUnion,
        tast_info: &mut TastInfo,
        arg: (&ast_defs::ParamKind, &aast::Expr<(), ()>),
        param_type: &TUnion,
        argument_offset: usize,
        function_call_pos: &Pos,
    ) {
    }

    #[allow(unused_variables)]
    fn post_functionlike_analysis(
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

    #[allow(unused_variables)]
    fn handle_method_call_analysis(
        &self,
        method_id: &MethodIdentifier,
        lhs_type_part: &TAtomic,
        codebase: &CodebaseInfo,
        lhs_var_id: Option<&String>,
        tast_info: &mut TastInfo,
        pos: &Pos,
    ) -> Option<TUnion> {
        None
    }
}

pub trait CustomHook: InternalHook + Send + Sync {}
