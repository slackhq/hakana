use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
};

struct Context {
}

struct Scanner {

}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<Context, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_stmt(&mut self, c: &mut Context, p: &aast::Stmt<(), ()>) -> Result<(), ()> {
        let result = p.recurse(c, self);

        println!("{}-{}", p.0.to_raw_span().start.line(),p.0.to_raw_span().end.line());

        result
    }
}

pub fn collect_executable_lines(
    program: &aast::Program<(), ()>,
) {
    let mut checker = Scanner {
    };

    let mut context = Context {
    };

    visit(&mut checker, &mut context, program).unwrap();
}
