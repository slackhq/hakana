//! A visitor over the analysis tree ([`crate::aast`]).
//!
//! Implement [`Visitor`] and override the `visit_*` methods for the nodes of
//! interest; call `node.recurse(context, self.object())` to continue into a
//! node's children. Children are visited in source order.

use std::marker::PhantomData;

use crate::aast::*;

/// The context and error types a visitor works with.
pub trait Params {
    type Context;
    type Error;
}

pub struct AstParams<Context, Error>(PhantomData<(Context, Error)>);

impl<C, E> Params for AstParams<C, E> {
    type Context = C;
    type Error = E;
}

type Ctx<P> = <P as Params>::Context;
type Res<P> = Result<(), <P as Params>::Error>;

/// A node that can be visited.
pub trait Node<'a, P: Params> {
    /// Dispatches to the visitor method for this node type.
    fn accept(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P>;
    /// Visits the children of this node.
    fn recurse(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P>;
}

pub fn visit<'a, P: Params>(
    v: &mut impl Visitor<'a, Params = P>,
    c: &mut P::Context,
    node: &'a impl Node<'a, P>,
) -> Result<(), P::Error> {
    node.accept(c, v.object())
}

macro_rules! visitor {
    ($( $method:ident : $ty:ty ),* $(,)?) => {
        pub trait Visitor<'a> {
            type Params: Params;

            fn object(&mut self) -> &mut dyn Visitor<'a, Params = Self::Params>;

            $(
                fn $method(&mut self, c: &mut Ctx<Self::Params>, p: &'a $ty) -> Res<Self::Params> {
                    p.recurse(c, self.object())
                }
            )*
        }

        $(
            impl<'a, P: Params> NodeAccept<'a, P> for $ty {
                #[inline]
                fn accept_impl(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
                    v.$method(c, self)
                }
            }
        )*
    };
}

/// Per-type dispatch, generated alongside the [`Visitor`] trait.
#[doc(hidden)]
pub trait NodeAccept<'a, P: Params> {
    fn accept_impl(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P>;
}

visitor! {
    visit_program: Program<'a>,
    visit_def: Def<'a>,
    visit_fun_def: FunDef<'a>,
    visit_fun_: Fun_<'a>,
    visit_fun_param: FunParam<'a>,
    visit_func_body: FuncBody<'a>,
    visit_class_: Class_<'a>,
    visit_class_const: ClassConst<'a>,
    visit_class_typeconst_def: ClassTypeconstDef<'a>,
    visit_class_var: ClassVar<'a>,
    visit_method_: Method_<'a>,
    visit_typedef: Typedef<'a>,
    visit_gconst: Gconst<'a>,
    visit_block: Block<'a>,
    visit_finally_block: FinallyBlock<'a>,
    visit_stmt: Stmt<'a>,
    visit_stmt_: Stmt_<'a>,
    visit_using_stmt: UsingStmt<'a>,
    visit_as_expr: AsExpr<'a>,
    visit_case: Case<'a>,
    visit_default_case: DefaultCase<'a>,
    visit_catch: Catch<'a>,
    visit_expr: Expr<'a>,
    visit_expr_: Expr_<'a>,
    visit_class_id: ClassId<'a>,
    visit_function_ptr_id: FunctionPtrId<'a>,
    visit_call_expr: CallExpr<'a>,
    visit_argument: Argument<'a>,
    visit_afield: Afield<'a>,
    visit_field: Field<'a>,
    visit_shape_field_name: ShapeFieldName<'a>,
    visit_shape_field_info: ShapeFieldInfo<'a>,
    visit_xhp_attribute: XhpAttribute<'a>,
    visit_xhp_simple: XhpSimple<'a>,
    visit_efun: Efun<'a>,
    visit_targ: Targ<'a>,
    visit_type_hint: TypeHint<'a>,
    visit_hint: Hint<'a>,
    visit_hint_: Hint_<'a>,
    visit_hint_fun: HintFun<'a>,
    visit_contexts: Contexts<'a>,
    visit_tparam: Tparam<'a>,
    visit_where_constraint_hint: WhereConstraintHint<'a>,
    visit_user_attribute: UserAttribute<'a>,
    visit_user_attributes: UserAttributes<'a>,
    visit_id: Id<'a>,
    visit_lid: Lid<'a>,
}

// Shorthands used by the `recurse` implementations below.
macro_rules! go {
    ($c:ident, $v:ident; $($e:expr),* $(,)?) => {{ $( $e.accept($c, $v)?; )* }};
}

macro_rules! node {
    ($ty:ty, |$s:ident, $c:ident, $v:ident| $body:block) => {
        impl<'a, P: Params> Node<'a, P> for $ty {
            #[inline]
            fn accept(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
                self.accept_impl(c, v)
            }
            fn recurse(&'a self, $c: &mut Ctx<P>, $v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
                let $s = self;
                $body
                Ok(())
            }
        }
    };
}

// Containers simply visit their elements.
impl<'a, P: Params, T: Node<'a, P>> Node<'a, P> for [T] {
    fn accept(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
        for item in self {
            item.accept(c, v)?;
        }
        Ok(())
    }
    fn recurse(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
        self.accept(c, v)
    }
}

impl<'a, P: Params, T: Node<'a, P>> Node<'a, P> for Option<T> {
    fn accept(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
        match self {
            Some(item) => item.accept(c, v),
            None => Ok(()),
        }
    }
    fn recurse(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
        self.accept(c, v)
    }
}

node!(Program<'a>, |s, c, v| { go!(c, v; s.0) });

node!(Def<'a>, |s, c, v| {
    match s {
        Def::Fun(f) => go!(c, v; f),
        Def::Class(x) => go!(c, v; x),
        Def::Stmt(x) => go!(c, v; x),
        Def::Typedef(x) => go!(c, v; x),
        Def::Constant(x) => go!(c, v; x),
        Def::Namespace(ns) => go!(c, v; ns.0, ns.1),
        Def::NamespaceUse(uses) => {
            for (_, name, alias) in uses.iter() {
                go!(c, v; name, alias);
            }
        }
        Def::FileAttributes(fa) => go!(c, v; fa.user_attributes),
        Def::SetModule(id) => go!(c, v; id),
        Def::ClassAlias(alias) => {
            go!(c, v; alias.name, alias.tparams, alias.original, alias.original_tparams)
        }
        Def::Module(m) => go!(c, v; m.name, m.user_attributes),
    }
});

node!(FunDef<'a>, |s, c, v| {
    go!(c, v; s.name, s.fun, s.tparams, s.where_constraints)
});

node!(Fun_<'a>, |s, c, v| {
    hint_tparams(s.tparams, c, v)?;
    go!(c, v; s.ret, s.params, s.ctxs, s.unsafe_ctxs, s.body, s.user_attributes)
});

/// The type parameters of a lambda or function type.
fn hint_tparams<'a, P: Params>(
    tparams: &'a [HintTparam<'a>],
    c: &mut Ctx<P>,
    v: &mut dyn Visitor<'a, Params = P>,
) -> Res<P> {
    for tparam in tparams {
        go!(c, v; tparam.name, tparam.user_attributes);
        for (_, hint) in tparam.constraints {
            go!(c, v; hint);
        }
    }
    Ok(())
}

fn xhp_child<'a, P: Params>(
    child: &'a XhpChild<'a>,
    c: &mut Ctx<P>,
    v: &mut dyn Visitor<'a, Params = P>,
) -> Res<P> {
    match child {
        XhpChild::ChildName(id) => go!(c, v; id),
        XhpChild::ChildList(children) => {
            for child in children.iter() {
                xhp_child(child, c, v)?;
            }
        }
        XhpChild::ChildUnary(child, _) => xhp_child(child, c, v)?,
        XhpChild::ChildBinary(a, b) => {
            xhp_child(a, c, v)?;
            xhp_child(b, c, v)?;
        }
    }
    Ok(())
}

fn destructure_target<'a, P: Params>(
    target: &'a DestructureTarget<'a>,
    c: &mut Ctx<P>,
    v: &mut dyn Visitor<'a, Params = P>,
) -> Res<P> {
    match &target.2 {
        DestructureTarget_::DtLvar(lid) => go!(c, v; lid),
        DestructureTarget_::DtWildcard(_) => {}
        DestructureTarget_::DtShape(shape) => destructure_shape(shape, c, v)?,
        DestructureTarget_::DtTuple(tuple) => destructure_tuple(tuple, c, v)?,
    }
    Ok(())
}

fn destructure_shape<'a, P: Params>(
    shape: &'a DestructureShape<'a>,
    c: &mut Ctx<P>,
    v: &mut dyn Visitor<'a, Params = P>,
) -> Res<P> {
    for field in shape.fields {
        go!(c, v; field.name);
        destructure_target(&field.target, c, v)?;
    }
    Ok(())
}

fn destructure_tuple<'a, P: Params>(
    tuple: &'a DestructureTuple<'a>,
    c: &mut Ctx<P>,
    v: &mut dyn Visitor<'a, Params = P>,
) -> Res<P> {
    for entry in tuple.entries {
        destructure_target(&entry.target, c, v)?;
    }
    Ok(())
}

node!(FunParam<'a>, |s, c, v| {
    go!(c, v; s.type_hint);
    if let FunParamInfo::ParamOptional(e) = &s.info {
        go!(c, v; e);
    }
    go!(c, v; s.user_attributes);
});

node!(FuncBody<'a>, |s, c, v| { go!(c, v; s.fb_ast) });

node!(Class_<'a>, |s, c, v| {
    go!(c, v; s.name, s.tparams, s.extends, s.uses, s.xhp_attr_uses);
    for req in s.reqs {
        go!(c, v; req.0);
    }
    go!(c, v; s.implements, s.consts, s.typeconsts, s.vars, s.methods);
    for (_, child) in s.xhp_children {
        xhp_child(child, c, v)?;
    }
    for attr in s.xhp_attrs {
        go!(c, v; attr.0, attr.1);
        if let Some((_, exprs)) = &attr.3 {
            go!(c, v; exprs);
        }
    }
    go!(c, v; s.user_attributes);
    if let Some(e) = &s.enum_ {
        go!(c, v; e.base, e.constraint, e.includes);
    }
});

node!(ClassConst<'a>, |s, c, v| {
    go!(c, v; s.user_attributes, s.type_, s.id);
    match &s.kind {
        ClassConstKind::CCAbstract(e) => go!(c, v; e),
        ClassConstKind::CCConcrete(e) => go!(c, v; e),
    }
});

node!(ClassTypeconstDef<'a>, |s, c, v| {
    go!(c, v; s.user_attributes, s.name);
    match &s.kind {
        ClassTypeconst::TCAbstract(a) => go!(c, v; a.as_constraint, a.super_constraint, a.default),
        ClassTypeconst::TCConcrete(t) => go!(c, v; t.c_tc_type),
    }
});

node!(ClassVar<'a>, |s, c, v| {
    go!(c, v; s.type_, s.id, s.expr, s.user_attributes)
});

node!(Method_<'a>, |s, c, v| {
    go!(c, v; s.name, s.tparams, s.where_constraints, s.params, s.ctxs, s.unsafe_ctxs, s.body, s.user_attributes, s.ret)
});

node!(Typedef<'a>, |s, c, v| {
    go!(c, v; s.name, s.tparams, s.as_constraint, s.super_constraint);
    match &s.assignment {
        TypedefAssignment::SimpleTypeDef(t) => go!(c, v; t.hint),
        TypedefAssignment::CaseType(first, rest) => {
            go!(c, v; first.hint, first.where_constraints);
            for variant in rest.iter() {
                go!(c, v; variant.hint, variant.where_constraints);
            }
        }
    }
    go!(c, v; s.runtime_type, s.user_attributes);
});

node!(Gconst<'a>, |s, c, v| {
    go!(c, v; s.name, s.type_, s.value)
});

node!(Block<'a>, |s, c, v| { go!(c, v; s.0) });
node!(FinallyBlock<'a>, |s, c, v| { go!(c, v; s.0) });
node!(Stmt<'a>, |s, c, v| { go!(c, v; s.1) });

node!(Stmt_<'a>, |s, c, v| {
    match s {
        Stmt_::Noop
        | Stmt_::Fallthrough
        | Stmt_::Break
        | Stmt_::Continue
        | Stmt_::YieldBreak
        | Stmt_::Markup(_) => {}
        Stmt_::Expr(e) | Stmt_::Throw(e) => go!(c, v; e),
        Stmt_::Return(e) => go!(c, v; e),
        Stmt_::Awaitall(x) => {
            for (lid, e) in x.0.iter() {
                go!(c, v; lid, e);
            }
            go!(c, v; x.1);
        }
        Stmt_::Concurrent(b) => go!(c, v; b),
        Stmt_::If(x) => go!(c, v; x.0, x.1, x.2),
        Stmt_::Do(x) => go!(c, v; x.0, x.1.0, x.1.1, x.1.2),
        Stmt_::While(x) => go!(c, v; x.0.0, x.0.1, x.0.2, x.1),
        Stmt_::Using(u) => go!(c, v; u),
        Stmt_::For(x) => {
            go!(c, v; x.0);
            if let Some(cond) = &x.1 {
                go!(c, v; cond.0, cond.1, cond.2);
            }
            go!(c, v; x.2.0, x.2.1, x.2.2, x.3);
        }
        Stmt_::Switch(x) => go!(c, v; x.0, x.1, x.2),
        Stmt_::Match(m) => {
            go!(c, v; m.expr);
            for arm in m.arms {
                match &arm.pat {
                    Pattern::PVar(var) => go!(c, v; var.id),
                    Pattern::PRefinement(r) => go!(c, v; r.id, r.hint),
                }
                go!(c, v; arm.body);
            }
        }
        Stmt_::Foreach(x) => go!(c, v; x.0, x.1, x.2),
        Stmt_::Try(x) => go!(c, v; x.0, x.1, x.2),
        Stmt_::Block(x) => go!(c, v; x.0, x.1),
    }
});

node!(UsingStmt<'a>, |s, c, v| { go!(c, v; s.exprs.1, s.block) });

node!(AsExpr<'a>, |s, c, v| {
    match s {
        AsExpr::AsV(e) | AsExpr::AwaitAsV(_, e) => go!(c, v; e),
        AsExpr::AsKv(k, e) | AsExpr::AwaitAsKv(_, k, e) => go!(c, v; k, e),
    }
});

node!(Case<'a>, |s, c, v| { go!(c, v; s.0, s.1) });
node!(DefaultCase<'a>, |s, c, v| { go!(c, v; s.1) });
node!(Catch<'a>, |s, c, v| { go!(c, v; s.0, s.1, s.2) });
node!(Expr<'a>, |s, c, v| { go!(c, v; s.2) });

node!(Expr_<'a>, |s, c, v| {
    match s {
        Expr_::Null
        | Expr_::True
        | Expr_::False
        | Expr_::This
        | Expr_::Omitted
        | Expr_::Int(_)
        | Expr_::Float(_)
        | Expr_::String(_)
        | Expr_::Lplaceholder(_) => {}
        Expr_::EnumClassLabel(x) => go!(c, v; x.0),
        Expr_::Shape(fields) => {
            for f in fields.iter() {
                match f {
                    ShapeExprField::SFField(name, e) => go!(c, v; name, e),
                    ShapeExprField::SFSplat(e) => go!(c, v; e),
                }
            }
        }
        Expr_::ValCollection(x) => go!(c, v; x.1, x.2),
        Expr_::KeyValCollection(x) => {
            if let Some((k, val)) = &x.1 {
                go!(c, v; k, val);
            }
            go!(c, v; x.2);
        }
        Expr_::Invalid(e) => go!(c, v; e),
        Expr_::Id(id) | Expr_::Package(id) => go!(c, v; id),
        Expr_::Lvar(l) | Expr_::Dollardollar(l) => go!(c, v; l),
        Expr_::Clone(e) | Expr_::Await(e) | Expr_::Delay(e) | Expr_::ReadonlyExpr(e) => {
            go!(c, v; e)
        }
        Expr_::ArrayGet(x) => go!(c, v; x.0, x.1),
        Expr_::ObjGet(x) => go!(c, v; x.0, x.1),
        Expr_::ClassGet(x) => go!(c, v; x.0),
        Expr_::ClassConst(x) => go!(c, v; x.0),
        Expr_::Call(x) => go!(c, v; x),
        Expr_::FunctionPointer(x) => go!(c, v; x.0, x.1),
        Expr_::String2(es) | Expr_::Tuple(es) | Expr_::List(es) => go!(c, v; es),
        Expr_::PrefixedString(x) => go!(c, v; x.1),
        Expr_::Yield(f) => go!(c, v; f),
        Expr_::DestructureShape(shape) => destructure_shape(shape, c, v)?,
        Expr_::DestructureTuple(tuple) => destructure_tuple(tuple, c, v)?,
        Expr_::Cast(x) => go!(c, v; x.0, x.1),
        Expr_::Unop(x) => go!(c, v; x.1),
        Expr_::Binop(b) => go!(c, v; b.lhs, b.rhs),
        Expr_::Assign(x) => go!(c, v; x.0, x.2),
        Expr_::Pipe(x) => go!(c, v; x.0, x.1, x.2),
        Expr_::Eif(x) => go!(c, v; x.0, x.1, x.2),
        Expr_::Is(x) | Expr_::Upcast(x) => go!(c, v; x.0, x.1),
        Expr_::As(a) => go!(c, v; a.expr, a.hint),
        Expr_::New(x) => go!(c, v; x.0, x.1, x.2, x.3),
        Expr_::Efun(e) => go!(c, v; e),
        Expr_::Lfun(x) => {
            go!(c, v; x.0);
            for capture in x.1.iter() {
                go!(c, v; capture.1);
            }
        }
        Expr_::Xml(x) => go!(c, v; x.0, x.1, x.2),
        Expr_::Import(x) => go!(c, v; x.1),
        Expr_::Collection(x) => {
            go!(c, v; x.0);
            match &x.1 {
                Some(CollectionTarg::CollectionTV(t)) => go!(c, v; t),
                Some(CollectionTarg::CollectionTKV(k, t)) => go!(c, v; k, t),
                None => {}
            }
            go!(c, v; x.2);
        }
        Expr_::ExpressionTree(et) => go!(c, v; et.class, et.runtime_expr, et.free_vars),
        Expr_::MethodCaller(x) => go!(c, v; x.0),
        Expr_::Pair(x) => {
            if let Some((k, t)) = &x.0 {
                go!(c, v; k, t);
            }
            go!(c, v; x.1, x.2);
        }
        Expr_::ETSplice(x) => go!(c, v; x.macro_variables, x.spliced_expr),
        Expr_::Hole(x) => go!(c, v; x.0),
        Expr_::Nameof(cid) => go!(c, v; cid),
    }
});

node!(ClassId<'a>, |s, c, v| {
    match &s.2 {
        ClassId_::CIparent | ClassId_::CIself | ClassId_::CIstatic => {}
        ClassId_::CIexpr(e) => go!(c, v; e),
        ClassId_::CIreified(id) | ClassId_::CI(id) => go!(c, v; id),
    }
});

node!(FunctionPtrId<'a>, |s, c, v| {
    match s {
        FunctionPtrId::FPId(id) => go!(c, v; id),
        FunctionPtrId::FPClassConst(cid, _) => go!(c, v; cid),
    }
});

node!(CallExpr<'a>, |s, c, v| {
    go!(c, v; s.func, s.targs, s.args, s.unpacked_arg)
});

node!(Argument<'a>, |s, c, v| {
    match s {
        Argument::Ainout(_, e) | Argument::Anormal(e) => go!(c, v; e),
        Argument::Anamed(id, e) => go!(c, v; id, e),
    }
});

node!(Afield<'a>, |s, c, v| {
    match s {
        Afield::AFvalue(e) => go!(c, v; e),
        Afield::AFkvalue(k, e) => go!(c, v; k, e),
    }
});

node!(Field<'a>, |s, c, v| { go!(c, v; s.0, s.1) });

node!(ShapeFieldName<'a>, |s, c, v| {
    match s {
        ShapeFieldName::SFlitStr(_) => {}
        ShapeFieldName::SFclassname(id) => go!(c, v; id),
        ShapeFieldName::SFclassConst(id, _) => go!(c, v; id),
    }
});

node!(ShapeFieldInfo<'a>, |s, c, v| { go!(c, v; s.hint, s.name) });

node!(XhpAttribute<'a>, |s, c, v| {
    match s {
        XhpAttribute::XhpSimple(x) => go!(c, v; x),
        XhpAttribute::XhpSpread(e) => go!(c, v; e),
    }
});

node!(XhpSimple<'a>, |s, c, v| { go!(c, v; s.expr) });

node!(Efun<'a>, |s, c, v| {
    go!(c, v; s.fun);
    for capture in s.use_ {
        go!(c, v; capture.1);
    }
});

node!(Targ<'a>, |s, c, v| { go!(c, v; s.1) });
node!(TypeHint<'a>, |s, c, v| { go!(c, v; s.1) });
node!(Hint<'a>, |s, c, v| { go!(c, v; *s.1) });

node!(Hint_<'a>, |s, c, v| {
    match s {
        Hint_::Hprim(_)
        | Hint_::Hmixed
        | Hint_::Hwildcard
        | Hint_::Hnonnull
        | Hint_::Habstr(_)
        | Hint_::Hthis
        | Hint_::Hdynamic
        | Hint_::Hnothing
        | Hint_::HfunContext(_)
        | Hint_::Hvar(_) => {}
        Hint_::Happly(id, args) => go!(c, v; id, args),
        Hint_::Hoption(h) | Hint_::Hlike(h) | Hint_::Hsoft(h) | Hint_::HclassPtr(_, h) => {
            go!(c, v; h)
        }
        Hint_::Hfun(f) => go!(c, v; f),
        Hint_::Htuple(t) => {
            go!(c, v; t.required);
            match &t.extra {
                TupleExtra::Hextra(e) => go!(c, v; e.optional, e.variadic),
                TupleExtra::Hsplat(h) => go!(c, v; h),
            }
        }
        Hint_::Hshape(shape) => {
            for f in shape.field_map {
                match f {
                    ShapeElement::SEField(f) => go!(c, v; f),
                    ShapeElement::SESplat(h) => go!(c, v; h),
                }
            }
            go!(c, v; shape.unknown_fields_type);
        }
        Hint_::Haccess(h, ids) => go!(c, v; h, ids),
        Hint_::Hrefinement(h, members) => {
            go!(c, v; h);
            for m in members.iter() {
                match m {
                    Refinement::Rctx(id, r) => {
                        go!(c, v; id);
                        match r {
                            CtxRefinement::CRexact(h) => go!(c, v; h),
                            CtxRefinement::CRloose(b) => go!(c, v; b.lower, b.upper),
                        }
                    }
                    Refinement::Rtype(id, r) => {
                        go!(c, v; id);
                        match r {
                            TypeRefinement::TRexact(h) => go!(c, v; h),
                            TypeRefinement::TRloose(b) => go!(c, v; b.lower, b.upper),
                        }
                    }
                }
            }
        }
        Hint_::HvecOrDict(k, h) => go!(c, v; k, h),
        Hint_::Hunion(hs) | Hint_::Hintersection(hs) => go!(c, v; hs),
    }
});

node!(HintFun<'a>, |s, c, v| {
    hint_tparams(s.tparams, c, v)?;
    go!(c, v; s.param_tys, s.variadic_ty, s.named_variadic_ty, s.ctxs, s.return_ty)
});

node!(Contexts<'a>, |s, c, v| { go!(c, v; s.1) });

node!(Tparam<'a>, |s, c, v| {
    go!(c, v; s.name);
    for (_, hint) in s.constraints {
        go!(c, v; hint);
    }
    go!(c, v; s.user_attributes);
});

node!(WhereConstraintHint<'a>, |s, c, v| { go!(c, v; s.0, s.2) });
node!(UserAttribute<'a>, |s, c, v| { go!(c, v; s.name, s.params) });
node!(UserAttributes<'a>, |s, c, v| { go!(c, v; s.0) });
node!(Id<'a>, |_s, _c, _v| {});
node!(Lid<'a>, |_s, _c, _v| {});

// References forward to the node they point at.
impl<'a, P: Params, T: Node<'a, P> + ?Sized> Node<'a, P> for &'a T {
    #[inline]
    fn accept(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
        (**self).accept(c, v)
    }
    #[inline]
    fn recurse(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
        (**self).recurse(c, v)
    }
}

// Tuples of nodes (the payloads of some statement and expression variants)
// visit their elements in order.
macro_rules! tuple_node {
    ($($t:ident $i:tt),+) => {
        impl<'a, P: Params, $($t: Node<'a, P>),+> Node<'a, P> for ($($t,)+) {
            fn accept(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
                $( self.$i.accept(c, v)?; )+
                Ok(())
            }
            fn recurse(&'a self, c: &mut Ctx<P>, v: &mut dyn Visitor<'a, Params = P>) -> Res<P> {
                self.accept(c, v)
            }
        }
    };
}

tuple_node!(A 0, B 1);
tuple_node!(A 0, B 1, C 2);
tuple_node!(A 0, B 1, C 2, D 3);
