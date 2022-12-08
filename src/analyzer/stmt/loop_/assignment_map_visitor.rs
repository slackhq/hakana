use hakana_reflection_info::StrId;
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

struct Context {
    this_class_name: Option<StrId>,
}

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
            aast::Expr_::Binop(boxed) => match boxed.0 {
                ast_defs::Bop::Eq(_) => {
                    let right_var_id = expression_identifier::get_root_var_id(
                        &boxed.2,
                        c.this_class_name.as_ref(),
                        None,
                    );

                    if let aast::Expr_::List(contents) = &boxed.1 .2 {
                        for list_expr in contents {
                            let left_var_id = expression_identifier::get_root_var_id(
                                &list_expr,
                                c.this_class_name.as_ref(),
                                None,
                            );

                            if let Some(left_var_id) = &left_var_id {
                                if let None = self.first_var_id {
                                    self.first_var_id = Some(left_var_id.clone());
                                }
                                self.assignment_map
                                    .entry(left_var_id.clone())
                                    .or_insert_with(FxHashSet::default)
                                    .insert(right_var_id.clone().unwrap_or("isset".to_string()));
                            }
                        }
                    } else {
                        let left_var_id = expression_identifier::get_root_var_id(
                            &boxed.1,
                            c.this_class_name.as_ref(),
                            None,
                        );

                        if let Some(left_var_id) = &left_var_id {
                            if let None = self.first_var_id {
                                self.first_var_id = Some(left_var_id.clone());
                            }
                            self.assignment_map
                                .entry(left_var_id.clone())
                                .or_insert_with(FxHashSet::default)
                                .insert(right_var_id.clone().unwrap_or("isset".to_string()));
                        }
                    }
                }
                _ => {}
            },
            aast::Expr_::Unop(boxed) => match boxed.0 {
                ast_defs::Uop::Udecr
                | ast_defs::Uop::Uincr
                | ast_defs::Uop::Updecr
                | ast_defs::Uop::Upincr => {
                    let var_id = expression_identifier::get_root_var_id(
                        &boxed.1,
                        c.this_class_name.as_ref(),
                        None,
                    );

                    if let Some(var_id) = &var_id {
                        if let None = self.first_var_id {
                            self.first_var_id = Some(var_id.clone());
                        }
                        self.assignment_map
                            .entry(var_id.clone())
                            .or_insert_with(FxHashSet::default)
                            .insert(var_id.clone());
                    }
                }
                _ => {}
            },
            aast::Expr_::Call(boxed) => {
                for arg_expr in &boxed.2 {
                    if let ParamKind::Pinout(..) = arg_expr.0 {
                        let arg_var_id = expression_identifier::get_root_var_id(
                            &arg_expr.1,
                            c.this_class_name.as_ref(),
                            None,
                        );

                        if let Some(arg_var_id) = &arg_var_id {
                            if let None = self.first_var_id {
                                self.first_var_id = Some(arg_var_id.clone());
                            }
                            self.assignment_map
                                .entry(arg_var_id.clone())
                                .or_insert_with(FxHashSet::default)
                                .insert(arg_var_id.clone());
                        }
                    }
                }

                if let aast::Expr_::Id(_) = &boxed.0 .2 {
                    // do nothing
                } else {
                    match &boxed.0 .2 {
                        aast::Expr_::ObjGet(boxed) => {
                            let (lhs_expr, _, _, prop_or_method) =
                                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

                            match prop_or_method {
                                ast_defs::PropOrMethod::IsMethod => {
                                    let lhs_var_id = expression_identifier::get_root_var_id(
                                        lhs_expr, None, None,
                                    );

                                    if let Some(lhs_var_id) = lhs_var_id {
                                        if let None = self.first_var_id {
                                            self.first_var_id = Some(lhs_var_id.clone());
                                        }
                                        self.assignment_map
                                            .entry(lhs_var_id.clone())
                                            .or_insert_with(FxHashSet::default)
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
    pre_conditions: Vec<&aast::Expr<(), ()>>,
    post_expressions: Vec<&aast::Expr<(), ()>>,
    stmts: &Vec<aast::Stmt<(), ()>>,
    this_class_name: Option<StrId>,
) -> (FxHashMap<String, FxHashSet<String>>, Option<String>) {
    let mut scanner = Scanner::new();
    let mut context = Context { this_class_name };

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
