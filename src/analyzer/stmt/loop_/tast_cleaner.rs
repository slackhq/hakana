use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
};

use crate::function_analysis_data::FunctionAnalysisData;

struct Scanner {}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<FunctionAnalysisData, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_expr(
        &mut self,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
    ) -> Result<(), ()> {
        analysis_data
            .expr_types
            .remove(&(expr.1.start_offset() as u32, expr.1.end_offset() as u32));

        expr.recurse(analysis_data, self)
    }
}

pub(crate) fn clean_nodes(
    stmts: &Vec<aast::Stmt<(), ()>>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut scanner = Scanner {};

    for stmt in stmts {
        visit(&mut scanner, analysis_data, stmt).unwrap();
    }
}
