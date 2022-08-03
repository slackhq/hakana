use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
};

use crate::typed_ast::TastInfo;

struct Scanner {}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<TastInfo, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_expr(
        &mut self,
        tast_info: &mut TastInfo,
        expr: &aast::Expr<(), ()>,
    ) -> Result<(), ()> {
        tast_info
            .expr_types
            .remove(&(expr.1.start_offset(), expr.1.end_offset()));

        expr.recurse(tast_info, self)
    }
}

pub(crate) fn clean_nodes(stmts: &Vec<aast::Stmt<(), ()>>, tast_info: &mut TastInfo) {
    let mut scanner = Scanner {};

    for stmt in stmts {
        visit(&mut scanner, tast_info, stmt).unwrap();
    }
}
