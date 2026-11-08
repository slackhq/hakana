//! Converts Hakana's analysis tree (`hakana_parser::aast`) into HHVM's
//! `oxidized::aast`, node for node, so the two parsers' output can be
//! compared with `==`. Only used for testing.

use std::sync::{Arc, OnceLock};

use hakana_parser::aast as n;
use oxidized::aast;
use oxidized::aast::{Expr_, Hint_, Stmt_};
use oxidized::ast_defs::{self, Id};
use oxidized::file_info::Mode;
use oxidized::namespace_env::Env as NamespaceEnv;
use oxidized::pos::Pos;
use oxidized::prim_defs::Comment;
use oxidized::scoured_comments::ScouredComments;
use relative_path::RelativePath;

type Expr = aast::Expr<(), ()>;
type Stmt = aast::Stmt<(), ()>;

fn empty_namespace_env() -> Arc<NamespaceEnv> {
    static ENV: OnceLock<Arc<NamespaceEnv>> = OnceLock::new();
    ENV.get_or_init(|| {
        Arc::new(NamespaceEnv::empty(
            vec![],
            oxidized::namespace_env::Mode::ForTypecheck,
            false,
        ))
    })
    .clone()
}

pub struct Cx<'l> {
    path: Arc<RelativePath>,
    /// The native tree's positions are byte ranges; HHVM's carry lines.
    lines: &'l n::LineIndex,
    ns: Arc<NamespaceEnv>,
    /// Set by `<<file: __PackageOverride('name')>>`.
    package: Option<aast::PackageMembership>,
}

pub fn program(
    p: &n::Program<'_>,
    lines: &n::LineIndex,
    path: Arc<RelativePath>,
) -> aast::Program<(), ()> {
    let mut cx = Cx {
        path,
        lines,
        ns: empty_namespace_env(),
        package: None,
    };
    aast::Program(cx.defs(p.0))
}

pub fn comments(
    c: &n::ScouredComments,
    lines: &n::LineIndex,
    path: Arc<RelativePath>,
) -> ScouredComments {
    let cx = Cx {
        path,
        lines,
        ns: empty_namespace_env(),
        package: None,
    };
    let mut out = ScouredComments::new();
    for (pos, comment) in &c.comments {
        let (start, end) = pos.to_start_and_end_lnum_bol_offset();
        out.comments.push((
            Pos::from_lnum_bol_offset(cx.path.clone(), start, end),
            match comment {
                n::Comment::CmtLine(s) => Comment::CmtLine(s.clone()),
                n::Comment::CmtBlock(s) => Comment::CmtBlock(s.clone()),
            },
        ));
    }
    for (line, codes) in &c.fixmes {
        for (code, pos) in codes {
            out.add_to_fixmes(*line, *code, cx.pos(pos));
        }
    }
    for (line, codes) in &c.ignores {
        for (code, pos) in codes {
            out.add_to_ignores(*line, *code, cx.pos(pos));
        }
    }
    for (line, codes) in &c.misuses {
        for (code, pos) in codes {
            out.add_to_misuses(*line, *code, cx.pos(pos));
        }
    }
    out
}

fn mode(m: n::Mode) -> Mode {
    match m {
        n::Mode::Mhhi => Mode::Mhhi,
        n::Mode::Mstrict => Mode::Mstrict,
    }
}

fn ro(r: Option<n::ReadonlyKind>) -> Option<ast_defs::ReadonlyKind> {
    r.map(|_| ast_defs::ReadonlyKind::Readonly)
}

fn vis(v: n::Visibility) -> ast_defs::Visibility {
    match v {
        n::Visibility::Private => ast_defs::Visibility::Private,
        n::Visibility::Public => ast_defs::Visibility::Public,
        n::Visibility::Protected => ast_defs::Visibility::Protected,
        n::Visibility::Internal => ast_defs::Visibility::Internal,
        n::Visibility::ProtectedInternal => ast_defs::Visibility::ProtectedInternal,
    }
}

fn bop(op: n::Bop) -> ast_defs::Bop {
    use ast_defs::Bop as B;
    match op {
        n::Bop::Plus => B::Plus,
        n::Bop::Minus => B::Minus,
        n::Bop::Star => B::Star,
        n::Bop::Slash => B::Slash,
        n::Bop::Eqeq => B::Eqeq,
        n::Bop::Eqeqeq => B::Eqeqeq,
        n::Bop::Starstar => B::Starstar,
        n::Bop::Diff => B::Diff,
        n::Bop::Diff2 => B::Diff2,
        n::Bop::Ampamp => B::Ampamp,
        n::Bop::Barbar => B::Barbar,
        n::Bop::Lt => B::Lt,
        n::Bop::Lte => B::Lte,
        n::Bop::Gt => B::Gt,
        n::Bop::Gte => B::Gte,
        n::Bop::Dot => B::Dot,
        n::Bop::Amp => B::Amp,
        n::Bop::Bar => B::Bar,
        n::Bop::Ltlt => B::Ltlt,
        n::Bop::Gtgt => B::Gtgt,
        n::Bop::Percent => B::Percent,
        n::Bop::Xor => B::Xor,
        n::Bop::Cmp => B::Cmp,
        n::Bop::QuestionQuestion => B::QuestionQuestion,
    }
}

fn uop(op: n::Uop) -> ast_defs::Uop {
    use ast_defs::Uop as U;
    match op {
        n::Uop::Utild => U::Utild,
        n::Uop::Unot => U::Unot,
        n::Uop::Uplus => U::Uplus,
        n::Uop::Uminus => U::Uminus,
        n::Uop::Uincr => U::Uincr,
        n::Uop::Udecr => U::Udecr,
        n::Uop::Upincr => U::Upincr,
        n::Uop::Updecr => U::Updecr,
        n::Uop::Usilence => U::Usilence,
    }
}

fn flavor(f: n::OperatorNullFlavor) -> ast_defs::OperatorNullFlavor {
    match f {
        n::OperatorNullFlavor::Regular => ast_defs::OperatorNullFlavor::Regular,
        n::OperatorNullFlavor::Nullsafe => ast_defs::OperatorNullFlavor::Nullsafe,
    }
}

fn prop_or_method(p: n::PropOrMethod) -> ast_defs::PropOrMethod {
    match p {
        n::PropOrMethod::IsProp => ast_defs::PropOrMethod::IsProp,
        n::PropOrMethod::IsMethod => ast_defs::PropOrMethod::IsMethod,
    }
}

fn constraint_kind(k: n::ConstraintKind) -> ast_defs::ConstraintKind {
    match k {
        n::ConstraintKind::ConstraintAs => ast_defs::ConstraintKind::ConstraintAs,
        n::ConstraintKind::ConstraintEq => ast_defs::ConstraintKind::ConstraintEq,
        n::ConstraintKind::ConstraintSuper => ast_defs::ConstraintKind::ConstraintSuper,
    }
}

fn fun_kind(k: n::FunKind) -> ast_defs::FunKind {
    match k {
        n::FunKind::FSync => ast_defs::FunKind::FSync,
        n::FunKind::FAsync => ast_defs::FunKind::FAsync,
        n::FunKind::FGenerator => ast_defs::FunKind::FGenerator,
        n::FunKind::FAsyncGenerator => ast_defs::FunKind::FAsyncGenerator,
    }
}

impl Cx<'_> {
    fn pos(&self, p: &n::Pos) -> Pos {
        if p.is_none() {
            return Pos::NONE;
        }
        let (start, end) = p.to_start_and_end_lnum_bol_offset(self.lines);
        Pos::from_lnum_bol_offset(self.path.clone(), start, end)
    }

    fn id(&self, id: &n::Id<'_>) -> Id {
        Id(self.pos(&id.0), id.1.to_string())
    }

    fn pstring(&self, p: &n::Pstring<'_>) -> ast_defs::Pstring {
        (self.pos(&p.0), p.1.to_string())
    }

    fn lid(&self, l: &n::Lid<'_>) -> aast::Lid {
        aast::Lid(self.pos(&l.0), (l.1.0, l.1.1.to_string()))
    }

    fn doc(&self, d: &Option<n::DocComment<'_>>) -> Option<aast::DocComment> {
        d.as_ref().map(|d| self.pstring(d))
    }

    // -- hints ---------------------------------------------------------

    fn hint(&mut self, h: &n::Hint<'_>) -> aast::Hint {
        let pos = self.pos(&h.0);
        let hint_ = self.hint_(h.1);
        aast::Hint::new(pos, hint_)
    }

    fn hints(&mut self, hs: &[n::Hint<'_>]) -> Vec<aast::Hint> {
        hs.iter().map(|h| self.hint(h)).collect()
    }

    fn opt_hint(&mut self, h: &Option<n::Hint<'_>>) -> Option<aast::Hint> {
        h.as_ref().map(|h| self.hint(h))
    }

    fn contexts(&mut self, c: &Option<n::Contexts<'_>>) -> Option<aast::Contexts> {
        c.as_ref()
            .map(|c| aast::Contexts(self.pos(&c.0), self.hints(c.1)))
    }

    fn shape_field_name(&mut self, name: &n::ShapeFieldName<'_>) -> ast_defs::ShapeFieldName {
        match name {
            n::ShapeFieldName::SFlitStr((pos, s)) => {
                ast_defs::ShapeFieldName::SFlitStr((self.pos(pos), (&***s).into()))
            }
            n::ShapeFieldName::SFclassname(id) => {
                ast_defs::ShapeFieldName::SFclassname(self.id(id))
            }
            n::ShapeFieldName::SFclassConst(id, name) => {
                ast_defs::ShapeFieldName::SFclassConst(self.id(id), self.pstring(name))
            }
        }
    }

    fn hint_(&mut self, h: &n::Hint_<'_>) -> Hint_ {
        match h {
            n::Hint_::Happly(id, args) => Hint_::Happly(self.id(id), self.hints(args)),
            n::Hint_::Hoption(h) => Hint_::Hoption(self.hint(h)),
            n::Hint_::Hlike(h) => Hint_::Hlike(self.hint(h)),
            n::Hint_::Hsoft(h) => Hint_::Hsoft(self.hint(h)),
            n::Hint_::Hwildcard => Hint_::Hwildcard,
            n::Hint_::Hvar(n) => Hint_::Hvar(n.to_string()),
            n::Hint_::HfunContext(n) => Hint_::HfunContext(n.to_string()),
            n::Hint_::Hunion(hs) => Hint_::Hunion(self.hints(hs)),
            n::Hint_::Hintersection(hs) => Hint_::Hintersection(self.hints(hs)),
            n::Hint_::HclassPtr(kind, h) => Hint_::HclassPtr(
                match kind {
                    n::ClassPtrKind::CKclass => aast::ClassPtrKind::CKclass,
                    n::ClassPtrKind::CKenum => aast::ClassPtrKind::CKenum,
                },
                self.hint(h),
            ),
            n::Hint_::Haccess(root, names) => {
                Hint_::Haccess(self.hint(root), names.iter().map(|i| self.id(i)).collect())
            }
            n::Hint_::Htuple(t) => Hint_::Htuple(aast::TupleInfo {
                required: self.hints(t.required),
                extra: match &t.extra {
                    n::TupleExtra::Hsplat(h) => aast::TupleExtra::Hsplat(self.hint(h)),
                    n::TupleExtra::Hextra(e) => aast::TupleExtra::Hextra(aast::TupleExtraInfo {
                        optional: self.hints(e.optional),
                        variadic: self.opt_hint(&e.variadic),
                    }),
                },
            }),
            n::Hint_::Hshape(s) => Hint_::Hshape(aast::NastShapeInfo {
                allows_unknown_fields: s.allows_unknown_fields,
                field_map: s
                    .field_map
                    .iter()
                    .map(|f| match f {
                        n::ShapeElement::SEField(f) => {
                            aast::ShapeElement::SEField(aast::ShapeFieldInfo {
                                optional: f.optional,
                                hint: self.hint(&f.hint),
                                name: self.shape_field_name(&f.name),
                            })
                        }
                        n::ShapeElement::SESplat(h) => aast::ShapeElement::SESplat(self.hint(h)),
                    })
                    .collect(),
                unknown_fields_type: self.opt_hint(&s.unknown_fields_type),
            }),
            n::Hint_::Hfun(f) => Hint_::Hfun(aast::HintFun {
                is_readonly: ro(f.is_readonly),
                tparams: self.hint_tparams(f.tparams),
                param_tys: self.hints(f.param_tys),
                param_info: f
                    .param_info
                    .iter()
                    .map(|i| {
                        i.as_ref().map(|i| aast::HfParamInfo {
                            kind: self.param_kind(&i.kind),
                            readonlyness: ro(i.readonlyness),
                            optional: i.optional.map(|_| ast_defs::OptionalKind::Optional),
                            splat: i.splat.map(|_| ast_defs::SplatKind::Splat),
                            named: i.named.map(|s| s.to_string()),
                        })
                    })
                    .collect(),
                variadic_ty: self.opt_hint(&f.variadic_ty),
                named_variadic_ty: self.opt_hint(&f.named_variadic_ty),
                ctxs: self.contexts(&f.ctxs),
                return_ty: self.hint(&f.return_ty),
                is_readonly_return: ro(f.is_readonly_return),
            }),
            n::Hint_::Hrefinement(h, members) => Hint_::Hrefinement(
                self.hint(h),
                members
                    .iter()
                    .map(|m| match m {
                        n::Refinement::Rtype(id, r) => aast::Refinement::Rtype(
                            self.id(id),
                            match r {
                                n::TypeRefinement::TRexact(h) => {
                                    aast::TypeRefinement::TRexact(self.hint(h))
                                }
                                n::TypeRefinement::TRloose(b) => {
                                    aast::TypeRefinement::TRloose(aast::TypeRefinementBounds {
                                        lower: self.hints(b.lower),
                                        upper: self.hints(b.upper),
                                    })
                                }
                            },
                        ),
                        n::Refinement::Rctx(id, r) => aast::Refinement::Rctx(
                            self.id(id),
                            match r {
                                n::CtxRefinement::CRexact(h) => {
                                    aast::CtxRefinement::CRexact(self.hint(h))
                                }
                                n::CtxRefinement::CRloose(b) => {
                                    aast::CtxRefinement::CRloose(aast::CtxRefinementBounds {
                                        lower: self.opt_hint(&b.lower),
                                        upper: self.opt_hint(&b.upper),
                                    })
                                }
                            },
                        ),
                    })
                    .collect(),
            ),
            n::Hint_::Hmixed => Hint_::Hmixed,
            n::Hint_::Hnonnull => Hint_::Hnonnull,
            n::Hint_::Hthis => Hint_::Hthis,
            n::Hint_::Hdynamic => Hint_::Hdynamic,
            n::Hint_::Hnothing => Hint_::Hnothing,
            n::Hint_::Hprim(_) | n::Hint_::Habstr(_) | n::Hint_::HvecOrDict(..) => {
                unreachable!("not produced by lowering")
            }
        }
    }

    fn param_kind(&self, k: &n::ParamKind) -> ast_defs::ParamKind {
        match k {
            n::ParamKind::Pinout(p) => ast_defs::ParamKind::Pinout(self.pos(p)),
            n::ParamKind::Pnormal => ast_defs::ParamKind::Pnormal,
        }
    }

    fn tparams(&mut self, ts: &[n::Tparam<'_>]) -> Vec<aast::Tparam<(), ()>> {
        ts.iter()
            .map(|t| aast::Tparam {
                variance: match t.variance {
                    n::Variance::Covariant => ast_defs::Variance::Covariant,
                    n::Variance::Contravariant => ast_defs::Variance::Contravariant,
                    n::Variance::Invariant => ast_defs::Variance::Invariant,
                },
                name: self.id(&t.name),
                constraints: t
                    .constraints
                    .iter()
                    .map(|(k, h)| (constraint_kind(*k), self.hint(h)))
                    .collect(),
                reified: match t.reified {
                    n::ReifyKind::Erased => aast::ReifyKind::Erased,
                    n::ReifyKind::SoftReified => aast::ReifyKind::SoftReified,
                    n::ReifyKind::Reified => aast::ReifyKind::Reified,
                },
                user_attributes: self.attrs(&t.user_attributes),
            })
            .collect()
    }

    fn hint_tparams(&mut self, ts: &[n::HintTparam<'_>]) -> Vec<aast::HintTparam> {
        ts.iter()
            .map(|t| aast::HintTparam {
                name: self.id(&t.name),
                user_attributes: t.user_attributes.iter().map(|a| self.id(a)).collect(),
                constraints: t
                    .constraints
                    .iter()
                    .map(|(k, h)| (constraint_kind(*k), self.hint(h)))
                    .collect(),
            })
            .collect()
    }

    fn where_constraints(
        &mut self,
        ws: &[n::WhereConstraintHint<'_>],
    ) -> Vec<aast::WhereConstraintHint> {
        ws.iter()
            .map(|w| {
                aast::WhereConstraintHint(self.hint(&w.0), constraint_kind(w.1), self.hint(&w.2))
            })
            .collect()
    }

    fn attrs(&mut self, a: &n::UserAttributes<'_>) -> aast::UserAttributes<(), ()> {
        aast::UserAttributes(
            a.0.iter()
                .map(|a| aast::UserAttribute {
                    name: self.id(&a.name),
                    params: self.exprs(a.params),
                })
                .collect(),
        )
    }

    // -- expressions ---------------------------------------------------

    fn exprs(&mut self, es: &[n::Expr<'_>]) -> Vec<Expr> {
        es.iter().map(|e| self.expr(e)).collect()
    }

    fn opt_expr(&mut self, e: &Option<n::Expr<'_>>) -> Option<Expr> {
        e.as_ref().map(|e| self.expr(e))
    }

    fn targs(&mut self, ts: &[n::Targ<'_>]) -> Vec<aast::Targ<()>> {
        ts.iter().map(|t| aast::Targ((), self.hint(&t.1))).collect()
    }

    fn args(&mut self, args: &[n::Argument<'_>]) -> Vec<aast::Argument<(), ()>> {
        args.iter()
            .map(|a| match a {
                n::Argument::Anormal(e) => aast::Argument::Anormal(self.expr(e)),
                n::Argument::Ainout(p, e) => aast::Argument::Ainout(self.pos(p), self.expr(e)),
                n::Argument::Anamed(id, e) => aast::Argument::Anamed(self.id(id), self.expr(e)),
            })
            .collect()
    }

    fn afield(&mut self, f: &n::Afield<'_>) -> aast::Afield<(), ()> {
        match f {
            n::Afield::AFvalue(v) => aast::Afield::AFvalue(self.expr(v)),
            n::Afield::AFkvalue(k, v) => aast::Afield::AFkvalue(self.expr(k), self.expr(v)),
        }
    }

    fn class_id(&mut self, c: &n::ClassId<'_>) -> aast::ClassId<(), ()> {
        aast::ClassId(
            (),
            self.pos(&c.1),
            match &c.2 {
                n::ClassId_::CIparent => aast::ClassId_::CIparent,
                n::ClassId_::CIself => aast::ClassId_::CIself,
                n::ClassId_::CIstatic => aast::ClassId_::CIstatic,
                n::ClassId_::CIexpr(e) => aast::ClassId_::CIexpr(self.expr(e)),
                n::ClassId_::CIreified(id) => aast::ClassId_::CIreified(self.id(id)),
                n::ClassId_::CI(id) => aast::ClassId_::CI(self.id(id)),
            },
        )
    }

    fn expr(&mut self, e: &n::Expr<'_>) -> Expr {
        stack_limit::maybe_grow(|| {
            let pos = self.pos(&e.1);
            let expr_ = self.expr_(&e.2);
            aast::Expr((), pos, expr_)
        })
    }

    fn expr_(&mut self, e: &n::Expr_<'_>) -> Expr_<(), ()> {
        use n::Expr_ as K;
        match e {
            K::Null => Expr_::Null,
            K::True => Expr_::True,
            K::False => Expr_::False,
            K::Omitted => Expr_::Omitted,
            K::Lvar(l) => Expr_::mk_lvar(self.lid(l)),
            K::Id(id) => Expr_::mk_id(self.id(id)),
            K::Int(s) => Expr_::Int(s.to_string()),
            K::Float(s) => Expr_::Float(s.to_string()),
            K::String(s) => Expr_::String((&***s).into()),
            K::String2(parts) => Expr_::String2(self.exprs(parts)),
            K::PrefixedString(p) => Expr_::mk_prefixed_string(p.0.to_string(), self.expr(&p.1)),
            K::Shape(fields) => Expr_::Shape(
                fields
                    .iter()
                    .map(|f| match f {
                        n::ShapeExprField::SFField(name, e) => {
                            aast::ShapeExprField::SFField(self.shape_field_name(name), self.expr(e))
                        }
                        n::ShapeExprField::SFSplat(e) => {
                            aast::ShapeExprField::SFSplat(self.expr(e))
                        }
                    })
                    .collect(),
            ),
            K::ValCollection(c) => {
                let ((kw, kind), targ, items) = c;
                Expr_::mk_val_collection(
                    (
                        self.pos(kw),
                        match kind {
                            n::VcKind::Vector => aast::VcKind::Vector,
                            n::VcKind::ImmVector => aast::VcKind::ImmVector,
                            n::VcKind::Vec => aast::VcKind::Vec,
                            n::VcKind::Set => aast::VcKind::Set,
                            n::VcKind::ImmSet => aast::VcKind::ImmSet,
                            n::VcKind::Keyset => aast::VcKind::Keyset,
                        },
                    ),
                    targ.as_ref().map(|t| aast::Targ((), self.hint(&t.1))),
                    self.exprs(items),
                )
            }
            K::KeyValCollection(c) => {
                let ((kw, kind), targs, fields) = c;
                Expr_::mk_key_val_collection(
                    (
                        self.pos(kw),
                        match kind {
                            n::KvcKind::Map => aast::KvcKind::Map,
                            n::KvcKind::ImmMap => aast::KvcKind::ImmMap,
                            n::KvcKind::Dict => aast::KvcKind::Dict,
                        },
                    ),
                    targs.as_ref().map(|(k, v)| {
                        (
                            aast::Targ((), self.hint(&k.1)),
                            aast::Targ((), self.hint(&v.1)),
                        )
                    }),
                    fields
                        .iter()
                        .map(|f| aast::Field(self.expr(&f.0), self.expr(&f.1)))
                        .collect(),
                )
            }
            K::Collection(c) => {
                let (name, targs, fields) = c;
                Expr_::mk_collection(
                    self.id(name),
                    targs.as_ref().map(|t| match t {
                        n::CollectionTarg::CollectionTV(v) => {
                            aast::CollectionTarg::CollectionTV(aast::Targ((), self.hint(&v.1)))
                        }
                        n::CollectionTarg::CollectionTKV(k, v) => {
                            aast::CollectionTarg::CollectionTKV(
                                aast::Targ((), self.hint(&k.1)),
                                aast::Targ((), self.hint(&v.1)),
                            )
                        }
                    }),
                    fields.iter().map(|f| self.afield(f)).collect(),
                )
            }
            K::Tuple(items) => Expr_::mk_tuple(self.exprs(items)),
            K::List(items) => Expr_::List(self.exprs(items)),
            K::Clone(e) => Expr_::mk_clone(self.expr(e)),
            K::Await(e) => Expr_::mk_await(self.expr(e)),
            K::ReadonlyExpr(e) => Expr_::mk_readonly_expr(self.expr(e)),
            K::ArrayGet(g) => Expr_::mk_array_get(self.expr(&g.0), self.opt_expr(&g.1)),
            K::ObjGet(g) => Expr_::mk_obj_get(
                self.expr(&g.0),
                self.expr(&g.1),
                flavor(g.2),
                prop_or_method(g.3),
            ),
            K::ClassGet(g) => {
                Expr_::mk_class_get(self.class_id(&g.0), self.pstring(&g.1), prop_or_method(g.2))
            }
            K::ClassConst(c) => Expr_::mk_class_const(self.class_id(&c.0), self.pstring(&c.1)),
            K::Call(c) => Expr_::mk_call(aast::CallExpr {
                func: self.expr(&c.func),
                targs: self.targs(c.targs),
                args: self.args(c.args),
                unpacked_arg: self.opt_expr(&c.unpacked_arg),
            }),
            K::FunctionPointer(fp) => Expr_::mk_function_pointer(
                match &fp.0 {
                    n::FunctionPtrId::FPId(id) => aast::FunctionPtrId::FPId(self.id(id)),
                    n::FunctionPtrId::FPClassConst(c, name) => {
                        aast::FunctionPtrId::FPClassConst(self.class_id(c), self.pstring(name))
                    }
                },
                self.targs(fp.1),
                match fp.2 {
                    n::FunctionPointerSource::Code => aast::FunctionPointerSource::Code,
                    n::FunctionPointerSource::Lowered => aast::FunctionPointerSource::Lowered,
                },
            ),
            K::Yield(f) => Expr_::mk_yield(self.afield(f)),
            K::Cast(c) => Expr_::mk_cast(self.hint(&c.0), self.expr(&c.1)),
            K::Unop(u) => Expr_::mk_unop(uop(u.0), self.expr(&u.1)),
            K::Binop(b) => Expr_::mk_binop(aast::Binop {
                bop: bop(b.bop),
                lhs: self.expr(&b.lhs),
                rhs: self.expr(&b.rhs),
            }),
            K::Assign(a) => Expr_::mk_assign(self.expr(&a.0), a.1.map(bop), self.expr(&a.2)),
            K::Pipe(p) => Expr_::mk_pipe(
                self.lid(&p.0),
                self.expr(&p.1),
                self.expr(&p.2),
                flavor(p.3),
            ),
            K::Eif(c) => Expr_::mk_eif(self.expr(&c.0), self.opt_expr(&c.1), self.expr(&c.2)),
            K::Is(t) => Expr_::mk_is(self.expr(&t.0), self.hint(&t.1)),
            K::As(a) => Expr_::mk_as(aast::As_ {
                expr: self.expr(&a.expr),
                hint: self.hint(&a.hint),
                is_nullable: a.is_nullable,
                enforce_deep: a.enforce_deep,
            }),
            K::Upcast(t) => Expr_::mk_upcast(self.expr(&t.0), self.hint(&t.1)),
            K::New(new) => Expr_::mk_new(
                self.class_id(&new.0),
                self.targs(new.1),
                self.args(new.2),
                self.opt_expr(&new.3),
                (),
            ),
            K::Efun(efun) => Expr_::mk_efun(aast::Efun {
                fun: self.fun_(&efun.fun),
                use_: efun
                    .use_
                    .iter()
                    .map(|c| aast::CaptureLid((), self.lid(&c.1)))
                    .collect(),
                closure_class_name: efun.closure_class_name.map(str::to_string),
                is_expr_tree_virtual_expr: efun.is_expr_tree_virtual_expr,
            }),
            K::Lfun(l) => Expr_::mk_lfun(
                self.fun_(&l.0),
                l.1.iter()
                    .map(|c| aast::CaptureLid((), self.lid(&c.1)))
                    .collect(),
            ),
            K::Xml(x) => {
                let (name, attrs, children) = x;
                Expr_::mk_xml(
                    self.id(name),
                    attrs
                        .iter()
                        .map(|a| match a {
                            n::XhpAttribute::XhpSimple(s) => {
                                aast::XhpAttribute::XhpSimple(aast::XhpSimple {
                                    name: self.pstring(&s.name),
                                    type_: (),
                                    expr: self.expr(&s.expr),
                                })
                            }
                            n::XhpAttribute::XhpSpread(e) => {
                                aast::XhpAttribute::XhpSpread(self.expr(e))
                            }
                        })
                        .collect(),
                    self.exprs(children),
                )
            }
            K::Import(i) => Expr_::mk_import(
                match i.0 {
                    n::ImportFlavor::Include => aast::ImportFlavor::Include,
                    n::ImportFlavor::Require => aast::ImportFlavor::Require,
                    n::ImportFlavor::IncludeOnce => aast::ImportFlavor::IncludeOnce,
                    n::ImportFlavor::RequireOnce => aast::ImportFlavor::RequireOnce,
                },
                self.expr(&i.1),
            ),
            K::EnumClassLabel(l) => {
                Expr_::mk_enum_class_label(l.0.as_ref().map(|c| self.id(c)), l.1.to_string())
            }
            K::Nameof(c) => Expr_::mk_nameof(self.class_id(c)),
            K::This => Expr_::This,
            K::Invalid(e) => Expr_::mk_invalid(self.opt_expr(e)),
            K::Dollardollar(l) => Expr_::mk_dollardollar(self.lid(l)),
            K::Delay(e) => Expr_::mk_delay(self.expr(e)),
            K::DestructureShape(d) => Expr_::mk_destructure_shape(self.destructure_shape(d)),
            K::DestructureTuple(d) => Expr_::mk_destructure_tuple(self.destructure_tuple(d)),
            K::ExpressionTree(et) => Expr_::mk_expression_tree(aast::ExpressionTree {
                class: self.id(&et.class),
                runtime_expr: self.expr(&et.runtime_expr),
                free_vars: et.free_vars.map(|vars| self.lids(vars)),
            }),
            K::Lplaceholder(pos) => Expr_::mk_lplaceholder(self.pos(pos)),
            K::MethodCaller(m) => Expr_::mk_method_caller(self.id(&m.0), self.pstring(&m.1)),
            K::Pair(p) => Expr_::mk_pair(
                p.0.as_ref().map(|(k, v)| {
                    (
                        aast::Targ((), self.hint(&k.1)),
                        aast::Targ((), self.hint(&v.1)),
                    )
                }),
                self.expr(&p.1),
                self.expr(&p.2),
            ),
            K::ETSplice(splice) => Expr_::mk_etsplice(aast::EtSplice {
                extract_client_type: splice.extract_client_type,
                contains_await: splice.contains_await,
                macro_variables: splice.macro_variables.map(|vars| self.lids(vars)),
                temp_lid: splice.temp_lid.map(|(id, name)| (id, name.to_string())),
                spliced_expr: self.expr(&splice.spliced_expr),
            }),
            K::Hole(h) => Expr_::mk_hole(
                self.expr(&h.0),
                (),
                (),
                match &h.3 {
                    n::HoleSource::Typing => aast::HoleSource::Typing,
                    n::HoleSource::UnsafeCast(hs) => aast::HoleSource::UnsafeCast(self.hints(hs)),
                    n::HoleSource::UnsafeNonnullCast => aast::HoleSource::UnsafeNonnullCast,
                    n::HoleSource::EnforcedCast(hs) => {
                        aast::HoleSource::EnforcedCast(self.hints(hs))
                    }
                },
            ),
            K::Package(id) => Expr_::mk_package(self.id(id)),
        }
    }

    fn lids(&self, lids: &[n::Lid<'_>]) -> Vec<aast::Lid> {
        lids.iter().map(|l| self.lid(l)).collect()
    }

    fn destructure_shape(&mut self, d: &n::DestructureShape<'_>) -> aast::DestructureShape<(), ()> {
        aast::DestructureShape {
            pos: self.pos(&d.pos),
            fields: d
                .fields
                .iter()
                .map(|f| aast::DestructureShapeField {
                    optional: f.optional,
                    name: self.shape_field_name(&f.name),
                    target: self.destructure_target(&f.target),
                })
                .collect(),
            ellipsis: d.ellipsis,
        }
    }

    fn destructure_tuple(&mut self, d: &n::DestructureTuple<'_>) -> aast::DestructureTuple<(), ()> {
        aast::DestructureTuple {
            pos: self.pos(&d.pos),
            entries: d
                .entries
                .iter()
                .map(|e| aast::DestructureTupleEntry {
                    optional: e.optional,
                    target: self.destructure_target(&e.target),
                })
                .collect(),
            ellipsis: d.ellipsis,
        }
    }

    fn destructure_target(
        &mut self,
        t: &n::DestructureTarget<'_>,
    ) -> aast::DestructureTarget<(), ()> {
        aast::DestructureTarget(
            (),
            (),
            match &t.2 {
                n::DestructureTarget_::DtLvar(l) => aast::DestructureTarget_::DtLvar(self.lid(l)),
                n::DestructureTarget_::DtWildcard(p) => {
                    aast::DestructureTarget_::DtWildcard(self.pos(p))
                }
                n::DestructureTarget_::DtShape(d) => {
                    aast::DestructureTarget_::DtShape(self.destructure_shape(d))
                }
                n::DestructureTarget_::DtTuple(d) => {
                    aast::DestructureTarget_::DtTuple(self.destructure_tuple(d))
                }
            },
        )
    }

    // -- functions -----------------------------------------------------

    fn param(&mut self, p: &n::FunParam<'_>) -> aast::FunParam<(), ()> {
        aast::FunParam {
            annotation: (),
            type_hint: aast::TypeHint((), self.opt_hint(&p.type_hint.1)),
            pos: self.pos(&p.pos),
            name: p.name.to_string(),
            info: match &p.info {
                n::FunParamInfo::ParamOptional(e) => {
                    aast::FunParamInfo::ParamOptional(self.opt_expr(e))
                }
                n::FunParamInfo::ParamRequired => aast::FunParamInfo::ParamRequired,
                n::FunParamInfo::ParamVariadic => aast::FunParamInfo::ParamVariadic,
            },
            readonly: ro(p.readonly),
            splat: p.splat.map(|_| ast_defs::SplatKind::Splat),
            callconv: self.param_kind(&p.callconv),
            named: p.named.map(|_| ast_defs::ParamNamed::ParamNamed),
            user_attributes: self.attrs(&p.user_attributes),
            visibility: p.visibility.map(vis),
        }
    }

    fn params(&mut self, ps: &[n::FunParam<'_>]) -> Vec<aast::FunParam<(), ()>> {
        ps.iter().map(|p| self.param(p)).collect()
    }

    fn fun_(&mut self, f: &n::Fun_<'_>) -> aast::Fun_<(), ()> {
        aast::Fun_ {
            span: self.pos(&f.span),
            readonly_this: ro(f.readonly_this),
            annotation: (),
            readonly_ret: ro(f.readonly_ret),
            tparams: self.hint_tparams(f.tparams),
            ret: aast::TypeHint((), self.opt_hint(&f.ret.1)),
            params: self.params(f.params),
            ctxs: self.contexts(&f.ctxs),
            unsafe_ctxs: self.contexts(&f.unsafe_ctxs),
            body: aast::FuncBody {
                fb_ast: self.block(&f.body.fb_ast),
            },
            fun_kind: fun_kind(f.fun_kind),
            user_attributes: self.attrs(&f.user_attributes),
            external: f.external,
            hidden: f.hidden,
            doc_comment: self.doc(&f.doc_comment),
        }
    }

    // -- statements ----------------------------------------------------

    fn block(&mut self, b: &n::Block<'_>) -> aast::Block<(), ()> {
        aast::Block(b.0.iter().map(|s| self.stmt(s)).collect())
    }

    fn stmt(&mut self, s: &n::Stmt<'_>) -> Stmt {
        use n::Stmt_ as K;
        let pos = self.pos(&s.0);
        let stmt_ = match &s.1 {
            K::Noop => Stmt_::Noop,
            K::Fallthrough => Stmt_::Fallthrough,
            K::Expr(e) => Stmt_::mk_expr(self.expr(e)),
            K::Break => Stmt_::Break,
            K::Continue => Stmt_::Continue,
            K::Throw(e) => Stmt_::mk_throw(self.expr(e)),
            K::Return(e) => Stmt_::mk_return(self.opt_expr(e)),
            K::YieldBreak => Stmt_::mk_yield_break(),
            K::Concurrent(b) => Stmt_::mk_concurrent(self.block(b)),
            K::If(i) => Stmt_::mk_if(self.expr(&i.0), self.block(&i.1), self.block(&i.2)),
            K::Do(d) => Stmt_::mk_do(
                self.block(&d.0),
                aast::LoopCond(None, None, self.expr(&d.1.2)),
            ),
            K::While(w) => Stmt_::mk_while(
                aast::LoopCond(None, None, self.expr(&w.0.2)),
                self.block(&w.1),
            ),
            K::Using(u) => Stmt_::mk_using(aast::UsingStmt {
                is_block_scoped: u.is_block_scoped,
                has_await: u.has_await,
                exprs: (self.pos(&u.exprs.0), self.exprs(u.exprs.1)),
                block: self.block(&u.block),
            }),
            K::For(f) => Stmt_::mk_for(
                self.exprs(f.0),
                f.1.as_ref()
                    .map(|c| aast::LoopCond(None, None, self.expr(&c.2))),
                aast::LoopIter(None, None, self.exprs(f.2.2)),
                self.block(&f.3),
            ),
            K::Switch(sw) => Stmt_::mk_switch(
                self.expr(&sw.0),
                sw.1.iter()
                    .map(|c| aast::Case(self.expr(&c.0), self.block(&c.1)))
                    .collect(),
                sw.2.as_ref()
                    .map(|d| aast::DefaultCase(self.pos(&d.0), self.block(&d.1))),
            ),
            K::Foreach(f) => Stmt_::mk_foreach(
                self.expr(&f.0),
                match &f.1 {
                    n::AsExpr::AsV(v) => aast::AsExpr::AsV(self.expr(v)),
                    n::AsExpr::AsKv(k, v) => aast::AsExpr::AsKv(self.expr(k), self.expr(v)),
                    n::AsExpr::AwaitAsV(p, v) => aast::AsExpr::AwaitAsV(self.pos(p), self.expr(v)),
                    n::AsExpr::AwaitAsKv(p, k, v) => {
                        aast::AsExpr::AwaitAsKv(self.pos(p), self.expr(k), self.expr(v))
                    }
                },
                self.block(&f.2),
            ),
            K::Try(t) => Stmt_::mk_try(
                self.block(&t.0),
                t.1.iter()
                    .map(|c| aast::Catch(self.id(&c.0), self.lid(&c.1), self.block(&c.2)))
                    .collect(),
                aast::FinallyBlock(t.2.0.iter().map(|s| self.stmt(s)).collect()),
            ),
            K::Block(b) => Stmt_::mk_block(None, self.block(&b.1)),
            K::Markup(m) => Stmt_::mk_markup(self.pstring(m)),
            K::Awaitall(a) => Stmt_::mk_awaitall(
                a.0.iter()
                    .map(|(lid, e)| (self.lid(lid), self.expr(e)))
                    .collect(),
                self.block(&a.1),
            ),
            K::Match(m) => Stmt_::mk_match(aast::StmtMatch {
                expr: self.expr(&m.expr),
                arms: m
                    .arms
                    .iter()
                    .map(|arm| aast::StmtMatchArm {
                        pat: match &arm.pat {
                            n::Pattern::PVar(p) => aast::Pattern::PVar(Box::new(aast::PatVar {
                                pos: self.pos(&p.pos),
                                id: p.id.as_ref().map(|l| self.lid(l)),
                            })),
                            n::Pattern::PRefinement(p) => {
                                aast::Pattern::PRefinement(Box::new(aast::PatRefinement {
                                    pos: self.pos(&p.pos),
                                    id: p.id.as_ref().map(|l| self.lid(l)),
                                    hint: self.hint(&p.hint),
                                }))
                            }
                        },
                        body: self.block(&arm.body),
                    })
                    .collect(),
            }),
        };
        Stmt::new(pos, stmt_)
    }

    // -- definitions ---------------------------------------------------

    fn defs(&mut self, defs: &[n::Def<'_>]) -> Vec<aast::Def<(), ()>> {
        let mut out = Vec::with_capacity(defs.len());
        for def in defs {
            out.push(match def {
                n::Def::Fun(f) => aast::Def::mk_fun(aast::FunDef {
                    namespace: self.ns.clone(),
                    file_attributes: vec![],
                    mode: mode(f.mode),
                    name: self.id(&f.name),
                    fun: self.fun_(&f.fun),
                    internal: f.internal,
                    module: None,
                    tparams: self.tparams(f.tparams),
                    where_constraints: self.where_constraints(f.where_constraints),
                    package: self.package.clone(),
                }),
                n::Def::Class(c) => aast::Def::mk_class(self.class(c)),
                n::Def::Typedef(t) => aast::Def::mk_typedef(aast::Typedef {
                    annotation: (),
                    name: self.id(&t.name),
                    tparams: self.tparams(t.tparams),
                    as_constraint: self.opt_hint(&t.as_constraint),
                    super_constraint: self.opt_hint(&t.super_constraint),
                    assignment: match &t.assignment {
                        n::TypedefAssignment::SimpleTypeDef(s) => {
                            aast::TypedefAssignment::SimpleTypeDef(aast::TypedefVisibilityAndHint {
                                vis: match s.vis {
                                    n::TypedefVisibility::Transparent => {
                                        aast::TypedefVisibility::Transparent
                                    }
                                    n::TypedefVisibility::Opaque => aast::TypedefVisibility::Opaque,
                                    n::TypedefVisibility::OpaqueModule => {
                                        aast::TypedefVisibility::OpaqueModule
                                    }
                                },
                                hint: self.hint(&s.hint),
                            })
                        }
                        n::TypedefAssignment::CaseType(first, rest) => {
                            let mut variant =
                                |v: &n::TypedefCaseTypeVariant<'_>| aast::TypedefCaseTypeVariant {
                                    hint: self.hint(&v.hint),
                                    where_constraints: self.where_constraints(v.where_constraints),
                                };
                            let first = variant(first);
                            aast::TypedefAssignment::CaseType(
                                first,
                                rest.iter().map(variant).collect(),
                            )
                        }
                    },
                    runtime_type: self.hint(&t.runtime_type),
                    user_attributes: self.attrs(&t.user_attributes),
                    file_attributes: vec![],
                    mode: mode(t.mode),
                    namespace: self.ns.clone(),
                    span: self.pos(&t.span),
                    emit_id: None,
                    is_ctx: t.is_ctx,
                    internal: t.internal,
                    module: None,
                    docs_url: t.docs_url.map(|s| s.to_string()),
                    doc_comment: self.doc(&t.doc_comment),
                    package: self.package.clone(),
                }),
                n::Def::Constant(c) => aast::Def::mk_constant(aast::Gconst {
                    annotation: (),
                    mode: mode(c.mode),
                    name: self.id(&c.name),
                    type_: self.opt_hint(&c.type_),
                    value: self.expr(&c.value),
                    namespace: self.ns.clone(),
                    span: self.pos(&c.span),
                    emit_id: None,
                    module: None,
                    package: self.package.clone(),
                }),
                n::Def::Namespace(ns) => {
                    let name = self.id(&ns.0);
                    aast::Def::mk_namespace(name, self.defs(ns.1))
                }
                n::Def::NamespaceUse(uses) => aast::Def::mk_namespace_use(
                    uses.iter()
                        .map(|(kind, name, alias)| {
                            (
                                match kind {
                                    n::NsKind::NSNamespace => aast::NsKind::NSNamespace,
                                    n::NsKind::NSClass => aast::NsKind::NSClass,
                                    n::NsKind::NSClassAndNamespace => {
                                        aast::NsKind::NSClassAndNamespace
                                    }
                                    n::NsKind::NSFun => aast::NsKind::NSFun,
                                    n::NsKind::NSConst => aast::NsKind::NSConst,
                                },
                                self.id(name),
                                self.id(alias),
                            )
                        })
                        .collect(),
                ),
                n::Def::FileAttributes(fa) => {
                    let user_attributes = self.attrs(&fa.user_attributes);
                    for attr in user_attributes.iter() {
                        if attr.name.1 == "__PackageOverride"
                            && let [aast::Expr(_, pos, Expr_::String(name))] =
                                attr.params.as_slice()
                            && let Ok(name) = String::from_utf8(name.to_vec())
                        {
                            self.package =
                                Some(aast::PackageMembership::PackageOverride(pos.clone(), name));
                        }
                    }
                    aast::Def::mk_file_attributes(aast::FileAttribute {
                        user_attributes,
                        namespace: self.ns.clone(),
                    })
                }
                n::Def::SetModule(id) => aast::Def::mk_set_module(self.id(id)),
                n::Def::Stmt(s) => aast::Def::mk_stmt(self.stmt(s)),
                n::Def::Module(m) => aast::Def::mk_module(aast::ModuleDef {
                    annotation: (),
                    name: self.id(&m.name),
                    user_attributes: self.attrs(&m.user_attributes),
                    file_attributes: vec![],
                    span: self.pos(&m.span),
                    mode: mode(m.mode),
                    doc_comment: self.doc(&m.doc_comment),
                }),
                n::Def::ClassAlias(alias) => aast::Def::mk_class_alias(aast::ClassAlias {
                    name: self.id(&alias.name),
                    tparams: self.tparams(alias.tparams),
                    original: self.id(&alias.original),
                    original_tparams: self.tparams(alias.original_tparams),
                }),
            });
        }
        out
    }

    fn class_var(&mut self, v: &n::ClassVar<'_>) -> aast::ClassVar<(), ()> {
        aast::ClassVar {
            final_: v.final_,
            xhp_attr: v.xhp_attr.as_ref().map(|x| aast::XhpAttrInfo {
                like: x.like.as_ref().map(|p| self.pos(p)),
                tag: x.tag.map(xhp_tag),
                enum_values: x
                    .enum_values
                    .iter()
                    .map(|v| match v {
                        n::XhpEnumValue::XEVInt(i) => ast_defs::XhpEnumValue::XEVInt(*i),
                        n::XhpEnumValue::XEVString(s) => {
                            ast_defs::XhpEnumValue::XEVString(s.to_string())
                        }
                    })
                    .collect(),
            }),
            abstract_: v.abstract_,
            readonly: v.readonly,
            visibility: vis(v.visibility),
            type_: aast::TypeHint((), self.opt_hint(&v.type_.1)),
            id: self.id(&v.id),
            expr: self.opt_expr(&v.expr),
            user_attributes: self.attrs(&v.user_attributes),
            doc_comment: self.doc(&v.doc_comment),
            is_static: v.is_static,
            span: self.pos(&v.span),
        }
    }

    fn xhp_child(&mut self, c: &n::XhpChild<'_>) -> aast::XhpChild {
        match c {
            n::XhpChild::ChildName(id) => aast::XhpChild::ChildName(self.id(id)),
            n::XhpChild::ChildList(items) => {
                aast::XhpChild::ChildList(items.iter().map(|c| self.xhp_child(c)).collect())
            }
            n::XhpChild::ChildUnary(c, op) => aast::XhpChild::ChildUnary(
                Box::new(self.xhp_child(c)),
                match op {
                    n::XhpChildOp::ChildStar => aast::XhpChildOp::ChildStar,
                    n::XhpChildOp::ChildPlus => aast::XhpChildOp::ChildPlus,
                    n::XhpChildOp::ChildQuestion => aast::XhpChildOp::ChildQuestion,
                },
            ),
            n::XhpChild::ChildBinary(l, r) => aast::XhpChild::ChildBinary(
                Box::new(self.xhp_child(l)),
                Box::new(self.xhp_child(r)),
            ),
        }
    }

    fn class(&mut self, c: &n::Class_<'_>) -> aast::Class_<(), ()> {
        let abstraction = |a: n::Abstraction| match a {
            n::Abstraction::Concrete => ast_defs::Abstraction::Concrete,
            n::Abstraction::Abstract => ast_defs::Abstraction::Abstract,
        };
        aast::Class_ {
            span: self.pos(&c.span),
            annotation: (),
            mode: mode(c.mode),
            final_: c.final_,
            is_xhp: c.is_xhp,
            has_xhp_keyword: c.has_xhp_keyword,
            kind: match c.kind {
                n::ClassishKind::Cclass(a) => ast_defs::ClassishKind::Cclass(abstraction(a)),
                n::ClassishKind::Cinterface => ast_defs::ClassishKind::Cinterface,
                n::ClassishKind::Ctrait => ast_defs::ClassishKind::Ctrait,
                n::ClassishKind::Cenum => ast_defs::ClassishKind::Cenum,
                n::ClassishKind::CenumClass(a) => {
                    ast_defs::ClassishKind::CenumClass(abstraction(a))
                }
            },
            name: self.id(&c.name),
            tparams: self.tparams(c.tparams),
            extends: self.hints(c.extends),
            uses: self.hints(c.uses),
            xhp_attr_uses: self.hints(c.xhp_attr_uses),
            xhp_category: c
                .xhp_category
                .as_ref()
                .map(|(p, cs)| (self.pos(p), cs.iter().map(|c| self.pstring(c)).collect())),
            reqs: c
                .reqs
                .iter()
                .map(|r| {
                    aast::ClassReq(
                        self.hint(&r.0),
                        match r.1 {
                            n::RequireKind::RequireExtends => aast::RequireKind::RequireExtends,
                            n::RequireKind::RequireImplements => {
                                aast::RequireKind::RequireImplements
                            }
                            n::RequireKind::RequireClass => aast::RequireKind::RequireClass,
                            n::RequireKind::RequireThisAs => aast::RequireKind::RequireThisAs,
                        },
                    )
                })
                .collect(),
            implements: self.hints(c.implements),
            consts: c
                .consts
                .iter()
                .map(|k| aast::ClassConst {
                    user_attributes: self.attrs(&k.user_attributes),
                    type_: self.opt_hint(&k.type_),
                    id: self.id(&k.id),
                    kind: match &k.kind {
                        n::ClassConstKind::CCAbstract(e) => {
                            aast::ClassConstKind::CCAbstract(self.opt_expr(e))
                        }
                        n::ClassConstKind::CCConcrete(e) => {
                            aast::ClassConstKind::CCConcrete(self.expr(e))
                        }
                    },
                    span: self.pos(&k.span),
                    doc_comment: self.doc(&k.doc_comment),
                })
                .collect(),
            typeconsts: c
                .typeconsts
                .iter()
                .map(|t| aast::ClassTypeconstDef {
                    user_attributes: self.attrs(&t.user_attributes),
                    name: self.id(&t.name),
                    kind: match &t.kind {
                        n::ClassTypeconst::TCAbstract(a) => {
                            aast::ClassTypeconst::TCAbstract(aast::ClassAbstractTypeconst {
                                as_constraint: self.opt_hint(&a.as_constraint),
                                super_constraint: self.opt_hint(&a.super_constraint),
                                default: self.opt_hint(&a.default),
                            })
                        }
                        n::ClassTypeconst::TCConcrete(c) => {
                            aast::ClassTypeconst::TCConcrete(aast::ClassConcreteTypeconst {
                                c_tc_type: self.hint(&c.c_tc_type),
                            })
                        }
                    },
                    span: self.pos(&t.span),
                    doc_comment: self.doc(&t.doc_comment),
                    is_ctx: t.is_ctx,
                })
                .collect(),
            vars: c.vars.iter().map(|v| self.class_var(v)).collect(),
            methods: c
                .methods
                .iter()
                .map(|m| aast::Method_ {
                    span: self.pos(&m.span),
                    annotation: (),
                    final_: m.final_,
                    abstract_: m.abstract_,
                    static_: m.static_,
                    readonly_this: m.readonly_this,
                    visibility: vis(m.visibility),
                    name: self.id(&m.name),
                    tparams: self.tparams(m.tparams),
                    where_constraints: self.where_constraints(m.where_constraints),
                    params: self.params(m.params),
                    ctxs: self.contexts(&m.ctxs),
                    unsafe_ctxs: self.contexts(&m.unsafe_ctxs),
                    body: aast::FuncBody {
                        fb_ast: self.block(&m.body.fb_ast),
                    },
                    fun_kind: fun_kind(m.fun_kind),
                    user_attributes: self.attrs(&m.user_attributes),
                    readonly_ret: ro(m.readonly_ret),
                    ret: aast::TypeHint((), self.opt_hint(&m.ret.1)),
                    external: m.external,
                    hidden: m.hidden,
                    doc_comment: self.doc(&m.doc_comment),
                })
                .collect(),
            xhp_children: c
                .xhp_children
                .iter()
                .map(|(p, child)| (self.pos(p), self.xhp_child(child)))
                .collect(),
            xhp_attrs: c
                .xhp_attrs
                .iter()
                .map(|a| {
                    aast::XhpAttr(
                        aast::TypeHint((), self.opt_hint(&a.0.1)),
                        self.class_var(&a.1),
                        a.2.map(xhp_tag),
                        a.3.as_ref().map(|(p, es)| (self.pos(p), self.exprs(es))),
                    )
                })
                .collect(),
            namespace: self.ns.clone(),
            user_attributes: self.attrs(&c.user_attributes),
            file_attributes: vec![],
            docs_url: c.docs_url.map(|s| s.to_string()),
            enum_: c.enum_.as_ref().map(|e| aast::Enum_ {
                base: self.hint(&e.base),
                constraint: self.opt_hint(&e.constraint),
                includes: self.hints(e.includes),
            }),
            doc_comment: self.doc(&c.doc_comment),
            emit_id: None,
            internal: c.internal,
            module: None,
            package: self.package.clone(),
        }
    }
}

fn xhp_tag(t: n::XhpAttrTag) -> aast::XhpAttrTag {
    match t {
        n::XhpAttrTag::Required => aast::XhpAttrTag::Required,
        n::XhpAttrTag::LateInit => aast::XhpAttrTag::LateInit,
    }
}
