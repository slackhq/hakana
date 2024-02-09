use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
    ast_defs::{self, ParamKind},
};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::expr::expression_identifier;

struct Scanner {
    pub assignment_map: FxHashMap<String, FxHashSet<String>>,
    pub first_var_id: Option<String>,
}

struct Context {}

impl Scanner {
    fn new() -> Self {
        Self {
            assignment_map: FxHashMap::default(),
            first_var_id: None,
        }
    }
}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<Context, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_expr(&mut self, c: &mut Context, expr: &aast::Expr<(), ()>) -> Result<(), ()> {
        match &expr.2 {
            aast::Expr_::Binop(boxed) => {
                if let ast_defs::Bop::Eq(_) = boxed.bop {
                    let right_var_id = expression_identifier::get_root_var_id(&boxed.rhs);

                    if let aast::Expr_::List(contents) = &boxed.lhs.2 {
                        for list_expr in contents {
                            let left_var_id = expression_identifier::get_root_var_id(list_expr);

                            if let Some(left_var_id) = &left_var_id {
                                if self.first_var_id.is_none() {
                                    self.first_var_id = Some(left_var_id.clone());
                                }
                                self.assignment_map
                                    .entry(left_var_id.clone())
                                    .or_default()
                                    .insert(right_var_id.clone().unwrap_or("isset".to_string()));
                            }
                        }
                    } else {
                        let left_var_id = expression_identifier::get_root_var_id(&boxed.lhs);

                        if let Some(left_var_id) = &left_var_id {
                            if self.first_var_id.is_none() {
                                self.first_var_id = Some(left_var_id.clone());
                            }
                            self.assignment_map
                                .entry(left_var_id.clone())
                                .or_default()
                                .insert(right_var_id.clone().unwrap_or("isset".to_string()));
                        }
                    }
                }
            }
            aast::Expr_::Unop(boxed) => match boxed.0 {
                ast_defs::Uop::Udecr
                | ast_defs::Uop::Uincr
                | ast_defs::Uop::Updecr
                | ast_defs::Uop::Upincr => {
                    let var_id = expression_identifier::get_root_var_id(&boxed.1);

                    if let Some(var_id) = &var_id {
                        if self.first_var_id.is_none() {
                            self.first_var_id = Some(var_id.clone());
                        }
                        self.assignment_map
                            .entry(var_id.clone())
                            .or_default()
                            .insert(var_id.clone());
                    }
                }
                _ => {}
            },
            aast::Expr_::Call(boxed) => {
                for arg_expr in &boxed.args {
                    if let ParamKind::Pinout(..) = arg_expr.0 {
                        let arg_var_id = expression_identifier::get_root_var_id(&arg_expr.1);

                        if let Some(arg_var_id) = &arg_var_id {
                            if self.first_var_id.is_none() {
                                self.first_var_id = Some(arg_var_id.clone());
                            }
                            self.assignment_map
                                .entry(arg_var_id.clone())
                                .or_default()
                                .insert(arg_var_id.clone());
                        }
                    }
                }

                if let aast::Expr_::Id(_) = &boxed.func.2 {
                    // do nothing
                } else {
                    match &boxed.func.2 {
                        aast::Expr_::ObjGet(boxed) => {
                            let (lhs_expr, _, _, prop_or_method) =
                                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

                            match prop_or_method {
                                ast_defs::PropOrMethod::IsMethod => {
                                    let lhs_var_id =
                                        expression_identifier::get_root_var_id(lhs_expr);

                                    if let Some(lhs_var_id) = lhs_var_id {
                                        if self.first_var_id.is_none() {
                                            self.first_var_id = Some(lhs_var_id.clone());
                                        }
                                        self.assignment_map
                                            .entry(lhs_var_id.clone())
                                            .or_default()
                                            .insert(lhs_var_id);
                                    }
                                }
                                _ => {
                                    // do nothing
                                }
                            }
                        }
                        _ => {
                            // do nothing
                        }
                    }
                };
            }
            aast::Expr_::Lfun(_) | aast::Expr_::Efun(_) => {
                return Result::Ok(());
            }
            _ => {}
        }

        expr.recurse(c, self)
    }
}

pub fn get_assignment_map(
    pre_conditions: &Vec<&aast::Expr<(), ()>>,
    post_expressions: &Vec<&aast::Expr<(), ()>>,
    stmts: &Vec<aast::Stmt<(), ()>>,
) -> (FxHashMap<String, FxHashSet<String>>, Option<String>) {
    let mut scanner = Scanner::new();
    let mut context = Context {};

    for pre_condition in pre_conditions {
        visit(&mut scanner, &mut context, pre_condition).unwrap();
    }

    for stmt in stmts {
        visit(&mut scanner, &mut context, stmt).unwrap();
    }

    for post_expression in post_expressions {
        visit(&mut scanner, &mut context, post_expression).unwrap();
    }

    (scanner.assignment_map, scanner.first_var_id)
}
