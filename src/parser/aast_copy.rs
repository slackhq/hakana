//! Deep copies of expressions of the analysis tree ([`crate::aast`]).
//!
//! Analyses tell the expressions of a tree apart by their address, so no
//! [`Expr`] may be reachable along two paths. An `Expr` is stored inside its
//! parent, which makes its address that of the allocation holding it: code
//! that needs the same expression at two places of a tree must give one of
//! them its own allocations, all the way down. [`expr`] and [`class_id`]
//! make such a copy.
//!
//! Only what holds an expression is duplicated. Names, hints, variables and
//! the like are shared between the original and the copy.

use bumpalo::Bump;

use crate::aast::*;

/// A copy of `e` that shares no expression with it.
pub(crate) fn expr<'a>(arena: &'a Bump, e: &Expr<'a>) -> Expr<'a> {
    Copier { arena }.expr(e)
}

/// A copy of `id` that shares no expression with it.
pub(crate) fn class_id<'a>(arena: &'a Bump, id: &ClassId<'a>) -> ClassId<'a> {
    Copier { arena }.class_id(id)
}

struct Copier<'a> {
    arena: &'a Bump,
}

impl<'a> Copier<'a> {
    /// Copies each of `items` with `f` into a new slice.
    fn slice<T: Copy>(&self, items: &[T], f: impl Fn(&T) -> T) -> &'a [T] {
        if items.is_empty() {
            &[]
        } else {
            self.arena.alloc_slice_fill_iter(items.iter().map(f))
        }
    }

    fn exprs(&self, es: &[Expr<'a>]) -> &'a [Expr<'a>] {
        self.slice(es, |e| self.expr(e))
    }

    fn opt_expr(&self, e: &Option<Expr<'a>>) -> Option<Expr<'a>> {
        e.as_ref().map(|e| self.expr(e))
    }

    fn expr(&self, e: &Expr<'a>) -> Expr<'a> {
        let a = self.arena;
        let expr_ = match e.2 {
            // Nothing below these holds an expression.
            Expr_::Null
            | Expr_::True
            | Expr_::False
            | Expr_::This
            | Expr_::Omitted
            | Expr_::Id(_)
            | Expr_::Lvar(_)
            | Expr_::Dollardollar(_)
            | Expr_::Int(_)
            | Expr_::Float(_)
            | Expr_::String(_)
            | Expr_::DestructureShape(_)
            | Expr_::DestructureTuple(_)
            | Expr_::Lplaceholder(_)
            | Expr_::MethodCaller(_)
            | Expr_::EnumClassLabel(_)
            | Expr_::Package(_) => e.2,
            Expr_::Shape(fields) => Expr_::Shape(self.slice(fields, |f| match f {
                ShapeExprField::SFField(name, value) => {
                    ShapeExprField::SFField(*name, self.expr(value))
                }
                ShapeExprField::SFSplat(value) => ShapeExprField::SFSplat(self.expr(value)),
            })),
            Expr_::ValCollection(x) => Expr_::ValCollection(a.alloc((x.0, x.1, self.exprs(x.2)))),
            Expr_::KeyValCollection(x) => Expr_::KeyValCollection(a.alloc((
                x.0,
                x.1,
                self.slice(x.2, |f| Field(self.expr(&f.0), self.expr(&f.1))),
            ))),
            Expr_::Invalid(x) => Expr_::Invalid(a.alloc(self.opt_expr(x))),
            Expr_::Clone(x) => Expr_::Clone(a.alloc(self.expr(x))),
            Expr_::ArrayGet(x) => Expr_::ArrayGet(a.alloc((self.expr(&x.0), self.opt_expr(&x.1)))),
            Expr_::ObjGet(x) => {
                Expr_::ObjGet(a.alloc((self.expr(&x.0), self.expr(&x.1), x.2, x.3)))
            }
            Expr_::ClassGet(x) => Expr_::ClassGet(a.alloc((self.class_id(&x.0), x.1, x.2))),
            Expr_::ClassConst(x) => Expr_::ClassConst(a.alloc((self.class_id(&x.0), x.1))),
            Expr_::Call(x) => Expr_::Call(a.alloc(CallExpr {
                func: self.expr(&x.func),
                targs: x.targs,
                args: self.arguments(x.args),
                unpacked_arg: self.opt_expr(&x.unpacked_arg),
            })),
            Expr_::FunctionPointer(x) => Expr_::FunctionPointer(a.alloc((
                match &x.0 {
                    FunctionPtrId::FPId(id) => FunctionPtrId::FPId(*id),
                    FunctionPtrId::FPClassConst(class, name) => {
                        FunctionPtrId::FPClassConst(self.class_id(class), *name)
                    }
                },
                x.1,
                x.2,
            ))),
            Expr_::String2(es) => Expr_::String2(self.exprs(es)),
            Expr_::PrefixedString(x) => Expr_::PrefixedString(a.alloc((x.0, self.expr(&x.1)))),
            Expr_::Yield(x) => Expr_::Yield(a.alloc(self.afield(x))),
            Expr_::Await(x) => Expr_::Await(a.alloc(self.expr(x))),
            Expr_::Delay(x) => Expr_::Delay(a.alloc(self.expr(x))),
            Expr_::ReadonlyExpr(x) => Expr_::ReadonlyExpr(a.alloc(self.expr(x))),
            Expr_::Tuple(es) => Expr_::Tuple(self.exprs(es)),
            Expr_::List(es) => Expr_::List(self.exprs(es)),
            Expr_::Cast(x) => Expr_::Cast(a.alloc((x.0, self.expr(&x.1)))),
            Expr_::Unop(x) => Expr_::Unop(a.alloc((x.0, self.expr(&x.1)))),
            Expr_::Binop(x) => Expr_::Binop(a.alloc(Binop {
                bop: x.bop,
                lhs: self.expr(&x.lhs),
                rhs: self.expr(&x.rhs),
            })),
            Expr_::Assign(x) => Expr_::Assign(a.alloc((self.expr(&x.0), x.1, self.expr(&x.2)))),
            Expr_::Pipe(x) => Expr_::Pipe(a.alloc((x.0, self.expr(&x.1), self.expr(&x.2), x.3))),
            Expr_::Eif(x) => {
                Expr_::Eif(a.alloc((self.expr(&x.0), self.opt_expr(&x.1), self.expr(&x.2))))
            }
            Expr_::Is(x) => Expr_::Is(a.alloc((self.expr(&x.0), x.1))),
            Expr_::As(x) => Expr_::As(a.alloc(As_ {
                expr: self.expr(&x.expr),
                ..*x
            })),
            Expr_::Upcast(x) => Expr_::Upcast(a.alloc((self.expr(&x.0), x.1))),
            Expr_::New(x) => Expr_::New(a.alloc((
                self.class_id(&x.0),
                x.1,
                self.arguments(x.2),
                self.opt_expr(&x.3),
                (),
            ))),
            Expr_::Efun(x) => Expr_::Efun(a.alloc(Efun {
                fun: self.fun_(&x.fun),
                ..*x
            })),
            Expr_::Lfun(x) => Expr_::Lfun(a.alloc((self.fun_(&x.0), x.1))),
            Expr_::Xml(x) => Expr_::Xml(a.alloc((
                x.0,
                self.slice(x.1, |attr| match attr {
                    XhpAttribute::XhpSimple(simple) => XhpAttribute::XhpSimple(XhpSimple {
                        expr: self.expr(&simple.expr),
                        ..*simple
                    }),
                    XhpAttribute::XhpSpread(e) => XhpAttribute::XhpSpread(self.expr(e)),
                }),
                self.exprs(x.2),
            ))),
            Expr_::Import(x) => Expr_::Import(a.alloc((x.0, self.expr(&x.1)))),
            Expr_::Collection(x) => {
                Expr_::Collection(a.alloc((x.0, x.1, self.slice(x.2, |f| self.afield(f)))))
            }
            Expr_::ExpressionTree(x) => Expr_::ExpressionTree(a.alloc(ExpressionTree {
                runtime_expr: self.expr(&x.runtime_expr),
                ..*x
            })),
            Expr_::Pair(x) => Expr_::Pair(a.alloc((x.0, self.expr(&x.1), self.expr(&x.2)))),
            Expr_::ETSplice(x) => Expr_::ETSplice(a.alloc(EtSplice {
                spliced_expr: self.expr(&x.spliced_expr),
                ..*x
            })),
            Expr_::Hole(x) => Expr_::Hole(a.alloc((self.expr(&x.0), (), (), x.3))),
            Expr_::Nameof(x) => Expr_::Nameof(a.alloc(self.class_id(x))),
        };
        Expr((), e.1, expr_)
    }

    fn class_id(&self, id: &ClassId<'a>) -> ClassId<'a> {
        let class_id_ = match &id.2 {
            ClassId_::CIparent
            | ClassId_::CIself
            | ClassId_::CIstatic
            | ClassId_::CIreified(_)
            | ClassId_::CI(_) => id.2,
            ClassId_::CIexpr(e) => ClassId_::CIexpr(self.expr(e)),
        };
        ClassId((), id.1, class_id_)
    }

    fn arguments(&self, args: &[Argument<'a>]) -> &'a [Argument<'a>] {
        self.slice(args, |arg| match arg {
            Argument::Ainout(pos, e) => Argument::Ainout(*pos, self.expr(e)),
            Argument::Anormal(e) => Argument::Anormal(self.expr(e)),
            Argument::Anamed(name, e) => Argument::Anamed(*name, self.expr(e)),
        })
    }

    fn afield(&self, field: &Afield<'a>) -> Afield<'a> {
        match field {
            Afield::AFvalue(v) => Afield::AFvalue(self.expr(v)),
            Afield::AFkvalue(k, v) => Afield::AFkvalue(self.expr(k), self.expr(v)),
        }
    }

    fn user_attributes(&self, attrs: &UserAttributes<'a>) -> UserAttributes<'a> {
        UserAttributes(self.slice(attrs.0, |attr| UserAttribute {
            name: attr.name,
            params: self.exprs(attr.params),
        }))
    }

    fn fun_(&self, f: &Fun_<'a>) -> Fun_<'a> {
        Fun_ {
            params: self.slice(f.params, |param| FunParam {
                info: match &param.info {
                    FunParamInfo::ParamOptional(default) => {
                        FunParamInfo::ParamOptional(self.opt_expr(default))
                    }
                    FunParamInfo::ParamRequired | FunParamInfo::ParamVariadic => param.info,
                },
                user_attributes: self.user_attributes(&param.user_attributes),
                ..*param
            }),
            body: FuncBody {
                fb_ast: self.block(&f.body.fb_ast),
            },
            user_attributes: self.user_attributes(&f.user_attributes),
            ..*f
        }
    }

    fn stmts(&self, stmts: &[Stmt<'a>]) -> &'a [Stmt<'a>] {
        self.slice(stmts, |s| self.stmt(s))
    }

    fn block(&self, block: &Block<'a>) -> Block<'a> {
        Block(self.stmts(block.0))
    }

    fn opt_block(&self, block: &Option<Block<'a>>) -> Option<Block<'a>> {
        block.as_ref().map(|block| self.block(block))
    }

    fn loop_cond(&self, cond: &LoopCond<'a>) -> LoopCond<'a> {
        LoopCond(cond.0, self.opt_block(&cond.1), self.expr(&cond.2))
    }

    fn stmt(&self, s: &Stmt<'a>) -> Stmt<'a> {
        let a = self.arena;
        let stmt_ = match s.1 {
            Stmt_::Noop
            | Stmt_::Fallthrough
            | Stmt_::Break
            | Stmt_::Continue
            | Stmt_::YieldBreak
            | Stmt_::Markup(_) => s.1,
            Stmt_::Expr(e) => Stmt_::Expr(a.alloc(self.expr(e))),
            Stmt_::Throw(e) => Stmt_::Throw(a.alloc(self.expr(e))),
            Stmt_::Return(e) => Stmt_::Return(a.alloc(self.opt_expr(e))),
            Stmt_::Awaitall(x) => Stmt_::Awaitall(a.alloc((
                self.slice(x.0, |(lid, e)| (*lid, self.expr(e))),
                self.block(&x.1),
            ))),
            Stmt_::Concurrent(block) => Stmt_::Concurrent(self.block(&block)),
            Stmt_::If(x) => {
                Stmt_::If(a.alloc((self.expr(&x.0), self.block(&x.1), self.block(&x.2))))
            }
            Stmt_::Do(x) => Stmt_::Do(a.alloc((self.block(&x.0), self.loop_cond(&x.1)))),
            Stmt_::While(x) => Stmt_::While(a.alloc((self.loop_cond(&x.0), self.block(&x.1)))),
            Stmt_::Using(x) => Stmt_::Using(a.alloc(UsingStmt {
                exprs: (x.exprs.0, self.exprs(x.exprs.1)),
                block: self.block(&x.block),
                ..*x
            })),
            Stmt_::For(x) => Stmt_::For(a.alloc((
                self.exprs(x.0),
                x.1.as_ref().map(|cond| self.loop_cond(cond)),
                LoopIter(x.2.0, self.opt_block(&x.2.1), self.exprs(x.2.2)),
                self.block(&x.3),
            ))),
            Stmt_::Switch(x) => Stmt_::Switch(
                a.alloc((
                    self.expr(&x.0),
                    self.slice(x.1, |case| Case(self.expr(&case.0), self.block(&case.1))),
                    x.2.as_ref()
                        .map(|default| DefaultCase(default.0, self.block(&default.1))),
                )),
            ),
            Stmt_::Match(x) => Stmt_::Match(a.alloc(StmtMatch {
                expr: self.expr(&x.expr),
                arms: self.slice(x.arms, |arm| StmtMatchArm {
                    pat: arm.pat,
                    body: self.block(&arm.body),
                }),
            })),
            Stmt_::Foreach(x) => Stmt_::Foreach(a.alloc((
                self.expr(&x.0),
                match &x.1 {
                    AsExpr::AsV(v) => AsExpr::AsV(self.expr(v)),
                    AsExpr::AsKv(k, v) => AsExpr::AsKv(self.expr(k), self.expr(v)),
                    AsExpr::AwaitAsV(pos, v) => AsExpr::AwaitAsV(*pos, self.expr(v)),
                    AsExpr::AwaitAsKv(pos, k, v) => {
                        AsExpr::AwaitAsKv(*pos, self.expr(k), self.expr(v))
                    }
                },
                self.block(&x.2),
            ))),
            Stmt_::Try(x) => Stmt_::Try(a.alloc((
                self.block(&x.0),
                self.slice(x.1, |catch| Catch(catch.0, catch.1, self.block(&catch.2))),
                FinallyBlock(self.stmts(x.2.0)),
            ))),
            Stmt_::Block(x) => Stmt_::Block(a.alloc((x.0, self.block(&x.1)))),
        };
        Stmt(s.0, stmt_)
    }
}
