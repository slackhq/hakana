//! Desugaring of expression tree literals.
//!
//! The contents of an expression tree literal are rewritten into an
//! expression on a visitor class. Given the following expression tree:
//! ```text
//! MyDsl`foo(1) + ${ $x }`;
//! ```
//!
//! First, the splices are extracted and assigned to temporary variables:
//! ```text
//! { $0splice0 = $x; }
//! ```
//!
//! Then the expression is virtualized:
//! ```text
//! MyDsl::symbolType(foo<>)(MyDsl::intType())->__plus( ${ $0splice0 } )
//! ```
//! The virtualized expression is what gives the expression tree its type.
//!
//! Finally, the expression is desugared into its runtime representation:
//! ```text
//! MyDsl::makeTree(
//!   // At runtime, expression tree visitors know the position of the literal.
//!   shape('path' => 'whatever.php', 'start_line' => 123, ...),
//!
//!   // Metadata of the values used inside the visitor, so users can access
//!   // spliced or called values without having to re-run the visit method.
//!   // The virtualized expression is stored here too, under 'type'.
//!   shape(
//!     'splices' => dict['$0splice0' => $0splice0],
//!     'functions' => vec[foo<>],
//!     'static_methods' => vec[],
//!   )
//!
//!   (MyDsl $0v) ==> {
//!     $0v->visitBinop(
//!       // (ignoring ExprPos arguments for brevity)
//!       $0v->visitCall(
//!         $0v->visitGlobalFunction(foo<>),
//!         vec[$0v->visitInt(1)],
//!       ),
//!       '__plus',
//!       $0v->splice('$0splice0', $0splice0),
//!     )
//!   },
//! )
//! ```
//!
//! [`desugar`] returns that runtime expression wrapped in an
//! [`Expr_::ExpressionTree`] node, together with the errors for syntax that
//! expression trees do not support.

use std::collections::BTreeMap;
use std::convert::Infallible;

use bumpalo::Bump;

/// Where the desugared nodes are allocated, with the file's line starts:
/// the desugaring writes positions out as literals, lines and all.
#[derive(Clone, Copy)]
pub(crate) struct Arena<'a> {
    pub(crate) bump: &'a Bump,
    /// The offset at which each line of the file starts.
    pub(crate) line_starts: &'a [u32],
}

impl<'a> Arena<'a> {
    #[inline]
    fn alloc<T>(self, value: T) -> &'a mut T {
        self.bump.alloc(value)
    }

    #[inline]
    fn alloc_slice_copy<T: Copy>(self, items: &[T]) -> &'a mut [T] {
        self.bump.alloc_slice_copy(items)
    }

    #[inline]
    fn alloc_slice_fill_iter<T, I>(self, items: I) -> &'a mut [T]
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        self.bump.alloc_slice_fill_iter(items)
    }

    /// `(line, offset of the start of that line, offset)` for both ends of
    /// a position.
    fn lnum_bol_offset(self, pos: Pos) -> ((usize, usize, usize), (usize, usize, usize)) {
        let at = |offset: usize| {
            let index = self
                .line_starts
                .partition_point(|&start| start as usize <= offset)
                - 1;
            (index + 1, self.line_starts[index] as usize, offset)
        };
        (at(pos.start_offset()), at(pos.end_offset()))
    }
}

use crate::aast::*;
use crate::aast_copy;
use crate::aast_visitor::AstParams;
use crate::aast_visitor::Node;
use crate::aast_visitor::Visitor;

pub(crate) use et::DOLLARDOLLAR_TMP_VAR;

/// Names of the visitor class members the desugared code refers to.
mod et {
    pub const MAKE_TREE: &str = "makeTree";

    pub const INT_TYPE: &str = "intType";
    pub const FLOAT_TYPE: &str = "floatType";
    pub const BOOL_TYPE: &str = "boolType";
    pub const STRING_TYPE: &str = "stringType";
    pub const VOID_TYPE: &str = "voidType";
    pub const SYMBOL_TYPE: &str = "symbolType";
    pub const LAMBDA_TYPE: &str = "lambdaType";
    pub const SHAPE_TYPE: &str = "shapeType";
    pub const VEC_TYPE: &str = "vecType";
    pub const DICT_TYPE: &str = "dictType";
    pub const KEYSET_TYPE: &str = "keysetType";
    pub const OPERATION_TYPE: &str = "operationType";

    pub const VISIT_INT: &str = "visitInt";
    pub const VISIT_FLOAT: &str = "visitFloat";
    pub const VISIT_BOOL: &str = "visitBool";
    pub const VISIT_STRING: &str = "visitString";
    pub const VISIT_NULL: &str = "visitNull";
    pub const VISIT_BINOP: &str = "visitBinop";
    pub const VISIT_UNOP: &str = "visitUnop";
    pub const VISIT_LOCAL: &str = "visitLocal";
    pub const VISIT_LAMBDA: &str = "visitLambda";
    pub const VISIT_GLOBAL_FUNCTION: &str = "visitGlobalFunction";
    pub const VISIT_STATIC_METHOD: &str = "visitStaticMethod";
    pub const VISIT_CALL: &str = "visitCall";
    pub const VISIT_ASSIGN: &str = "visitAssign";
    pub const VISIT_TERNARY: &str = "visitTernary";
    pub const VISIT_IF: &str = "visitIf";
    pub const VISIT_WHILE: &str = "visitWhile";
    pub const VISIT_RETURN: &str = "visitReturn";
    pub const VISIT_FOR: &str = "visitFor";
    pub const VISIT_BREAK: &str = "visitBreak";
    pub const VISIT_CONTINUE: &str = "visitContinue";
    pub const VISIT_PROPERTY_ACCESS: &str = "visitPropertyAccess";
    pub const VISIT_INSTANCE_METHOD: &str = "visitInstanceMethod";
    pub const VISIT_PROPERTY_ACCESS_NULL_SAFE: &str = "visitPropertyAccessNullSafe";
    pub const VISIT_INSTANCE_METHOD_NULL_SAFE: &str = "visitInstanceMethodNullSafe";
    pub const VISIT_SHAPE: &str = "visitShape";
    pub const VISIT_VEC: &str = "visitVec";
    pub const VISIT_DICT: &str = "visitDict";
    pub const VISIT_KEYSET: &str = "visitKeyset";
    pub const VISIT_KEYED_COLLECTION: &str = "visitKeyedCollection";
    pub const VISIT_OPTIONAL_PARAMETER: &str = "visitOptionalParameter";
    pub const VISIT_CLASS_CONSTANT: &str = "visitClassConstant";
    pub const VISIT_SUBSCRIPT: &str = "visitSubscript";
    pub const VISIT_SUBSCRIPT_ASSIGN: &str = "visitSubscriptAssign";
    pub const MAKE_KEYED_COLLECTION_TYPE: &str = "__makeType";

    pub const SPLICE: &str = "splice";

    /// The variable that stands for the `$$` of an enclosing pipe inside
    /// spliced expressions.
    pub(crate) const DOLLARDOLLAR_TMP_VAR: &str = "$0dollardollar";

    pub const BUILTIN_SHAPE_AT: &str = "shapeAt";
    pub const BUILTIN_SHAPE_PUT: &str = "shapePut";
    pub const BUILTIN_SHAPE_IDX: &str = "shapeIdx";

    pub const LIFT: &str = "lift";
}

/// Typechecker pseudo functions, which are left alone inside expression
/// trees.
mod pseudo_functions {
    pub const HH_SHOW: &str = "\\hh_show";
    pub const HH_EXPECT: &str = "\\hh_expect";
    pub const HH_EXPECT_EQUIVALENT: &str = "\\hh_expect_equivalent";
    pub const HH_SHOW_ENV: &str = "\\hh_show_env";
}

mod special_idents {
    pub const DOLLAR_DOLLAR: &str = "$$";
}

mod coeffects {
    pub const DEFAULTS: &str = "defaults";
}

pub(crate) struct Env<'a> {
    pub arena: Arena<'a>,
    /// The enclosing function is `async`.
    pub in_async: bool,
    /// The position of the body of the expression tree literal this one is
    /// spliced into, if any.
    pub enclosing_et_pos: Option<Pos>,
}

pub(crate) struct DesugarResult<'a> {
    pub expr: Expr<'a>,
    pub errors: Vec<(Pos, String)>,
}

#[derive(Clone, Copy)]
struct RewriteResult<'a> {
    virtual_expr: Expr<'a>,
    desugar_expr: Expr<'a>,
}

/// Rewrites the contents `e` of an expression tree literal on the class
/// `visitor` into the expression described in the module documentation.
///
/// `is_nested` says the literal sits inside a splice of another expression
/// tree. Every `$$` inside the spliced expressions of `e` that refers to a
/// pipe around the literal must already have been renamed to
/// [`DOLLARDOLLAR_TMP_VAR`].
pub(crate) fn desugar<'a>(
    visitor: &ClassName<'a>,
    e: Expr<'a>,
    env: &Env<'a>,
    is_nested: bool,
) -> DesugarResult<'a> {
    let arena = env.arena;
    let Id(visitor_pos, visitor_name) = *visitor;
    let et_literal_pos = e.1;

    let free_vars = if is_nested {
        // Only compute the free variables for nested expression trees.
        // Top level ones will just desugar to a virtual expression with
        // free variables and get an error from that
        LiveVars::new_from_expression(arena.alloc(e)).used
    } else {
        BTreeMap::default()
    };

    let mut state = RewriteState {
        arena,
        splices: vec![],
        global_function_pointers: vec![],
        static_method_pointers: vec![],
        errors: vec![],
        contains_spliced_await: false,
    };
    let rewritten_expr = state.rewrite_expr(e, visitor_name);

    let dollardollar_pos = find_dollardollar(&state.splices);

    let splice_count = state.splices.len();
    let function_count = state.global_function_pointers.len();
    let static_method_count = state.static_method_pointers.len();

    // Wrap this in an Efun with appropriate variables for typing.
    // This enables us to report unbound variables correctly.
    let virtualized_expr = {
        let typing_fun_body = FuncBody {
            fb_ast: Block(slice(
                arena,
                &[wrap_return(
                    arena,
                    rewritten_expr.virtual_expr,
                    et_literal_pos,
                )],
            )),
        };
        let mut typing_fun_ = wrap_fun_(false, typing_fun_body, &[], et_literal_pos);
        typing_fun_.ctxs = Some(Contexts(
            et_literal_pos,
            slice(
                arena,
                &[Hint(
                    et_literal_pos,
                    arena.alloc(Hint_::Happly(Id(et_literal_pos, coeffects::DEFAULTS), &[])),
                )],
            ),
        ));
        let mut use_ = vec![];
        let spliced_vars = (0..splice_count)
            .map(|i| CaptureLid((), Lid(visitor_pos, (0, temp_splice_lvar_string(arena, i)))));
        use_.extend(spliced_vars);
        let function_pointer_vars = (0..function_count).map(|i| {
            CaptureLid(
                (),
                Lid(
                    visitor_pos,
                    (0, temp_function_pointer_lvar_string(arena, i)),
                ),
            )
        });
        use_.extend(function_pointer_vars);
        let static_method_vars = (0..static_method_count).map(|i| {
            CaptureLid(
                (),
                Lid(visitor_pos, (0, temp_static_method_lvar_string(arena, i))),
            )
        });
        use_.extend(static_method_vars);
        if is_nested {
            let fvs = free_vars
                .iter()
                .map(|(v, pos)| CaptureLid((), Lid(*pos, *v)));
            use_.extend(fvs);
        }
        Expr(
            (),
            et_literal_pos,
            Expr_::Efun(arena.alloc(Efun {
                fun: typing_fun_,
                use_: slice(arena, &use_),
                closure_class_name: None,
                is_expr_tree_virtual_expr: is_nested,
            })),
        )
    };

    let metadata = maketree_metadata(
        arena,
        visitor_pos,
        &state.splices,
        &state.global_function_pointers,
        &state.static_method_pointers,
        &free_vars,
        &env.enclosing_et_pos,
        virtualized_expr,
    );

    // Make anonymous function of smart constructor calls
    let visitor_expr = wrap_return(arena, rewritten_expr.desugar_expr, et_literal_pos);
    let visitor_body = FuncBody {
        fb_ast: Block(slice(arena, &[visitor_expr])),
    };
    let param = FunParam {
        annotation: (),
        type_hint: TypeHint(
            (),
            Some(Hint(
                visitor_pos,
                arena.alloc(Hint_::Happly(Id(visitor_pos, visitor_name), &[])),
            )),
        ),
        pos: visitor_pos,
        name: visitor_variable(),
        info: FunParamInfo::ParamRequired,
        callconv: ParamKind::Pnormal,
        readonly: None,
        splat: None,
        named: None,
        user_attributes: Default::default(),
        visibility: None,
    };
    let visitor_fun_ = wrap_fun_(false, visitor_body, slice(arena, &[param]), et_literal_pos);
    let visitor_lambda = Expr((), et_literal_pos, mk_lfun(arena, visitor_fun_));

    let spliced_await = env.in_async && state.contains_spliced_await;

    // Create assignment of the extracted expressions to temporary variables
    // `$0splice0 = spliced_expr0;`
    let splice_assignments: Vec<Stmt<'a>> = if spliced_await {
        create_temp_statement_parallel(arena, et_literal_pos, &state.splices, temp_splice_lvar)
    } else {
        create_temp_statements(arena, &state.splices, temp_splice_lvar)
    };
    // `$0fp0 = foo<>;`
    let function_pointer_assignments: Vec<Stmt<'a>> = create_temp_statements(
        arena,
        &state.global_function_pointers,
        temp_function_pointer_lvar,
    );
    // `$0sm0 = Foo::bar<>;`
    let static_method_assignments: Vec<Stmt<'a>> = create_temp_statements(
        arena,
        &state.static_method_pointers,
        temp_static_method_lvar,
    );

    let mut function_pointers = vec![];
    function_pointers.extend(function_pointer_assignments);
    function_pointers.extend(static_method_assignments);
    let make_tree = static_meth_call(
        arena,
        visitor_name,
        et::MAKE_TREE,
        &[exprpos(arena, et_literal_pos), metadata, visitor_lambda],
        visitor_pos,
    );

    let runtime_expr = if splice_assignments.is_empty() && function_pointers.is_empty() {
        make_tree
    } else {
        let body = {
            let mut b = splice_assignments;
            b.extend(function_pointers);
            b.push(wrap_return(arena, make_tree, et_literal_pos));
            b
        };
        let lambda_args = match dollardollar_pos {
            Some(pipe_pos) => vec![(
                (et::DOLLARDOLLAR_TMP_VAR, pipe_pos),
                mk_lvar(arena, pipe_pos, special_idents::DOLLAR_DOLLAR),
            )],
            _ => vec![],
        };

        immediately_invoked_lambda(arena, spliced_await, et_literal_pos, &body, &lambda_args)
    };

    let expr = Expr(
        (),
        et_literal_pos,
        Expr_::ExpressionTree(arena.alloc(ExpressionTree {
            class: Id(visitor_pos, visitor_name),
            runtime_expr,
            free_vars: if is_nested {
                Some(arena.alloc_slice_fill_iter(free_vars.iter().map(|(id, pos)| Lid(*pos, *id))))
            } else {
                None
            },
        })),
    );
    DesugarResult {
        expr,
        errors: state.errors,
    }
}

/// Copy `items` into the arena.
fn slice<'a, T: Copy>(arena: Arena<'a>, items: &[T]) -> &'a [T] {
    arena.alloc_slice_copy(items)
}

/// Build the local variable `n` at `p`.
fn mk_lvar<'a>(arena: Arena<'a>, p: Pos, n: &'a str) -> Expr<'a> {
    Expr((), p, Expr_::Lvar(arena.alloc(Lid(p, (0, n)))))
}

/// Build the lambda `fun_`, with no explicit captures.
fn mk_lfun<'a>(arena: Arena<'a>, fun_: Fun_<'a>) -> Expr_<'a> {
    let captures: &[CaptureLid<'a>] = &[];
    Expr_::Lfun(arena.alloc((fun_, captures)))
}

/// Build a pointer to the function or static method `id`, as the lowering
/// of a call (rather than of a `foo<>` in the source) produces it.
fn mk_function_pointer<'a>(arena: Arena<'a>, id: FunctionPtrId<'a>) -> Expr_<'a> {
    let targs: &[Targ<'a>] = &[];
    Expr_::FunctionPointer(arena.alloc((id, targs, FunctionPointerSource::Lowered)))
}

/// Convert `foo` to `return foo;`.
fn wrap_return<'a>(arena: Arena<'a>, e: Expr<'a>, pos: Pos) -> Stmt<'a> {
    Stmt(pos, Stmt_::Return(arena.alloc(Some(e))))
}

/// Wrap a FuncBody into an anonymous Fun_
fn wrap_fun_<'a>(
    async_: bool,
    body: FuncBody<'a>,
    params: &'a [FunParam<'a>],
    span: Pos,
) -> Fun_<'a> {
    Fun_ {
        span,
        readonly_this: None,
        annotation: (),
        readonly_ret: None,
        tparams: &[],
        ret: TypeHint((), None),
        params,
        body,
        fun_kind: if async_ {
            FunKind::FAsync
        } else {
            FunKind::FSync
        },
        ctxs: None,
        unsafe_ctxs: None,
        user_attributes: Default::default(),
        external: false,
        hidden: false,
        doc_comment: None,
    }
}

/// Finds the first variable standing for the `$$` of a pipe around the
/// expression tree literal.
struct DollarDollarFinder {
    pos: Option<Pos>,
}

impl<'ast> Visitor<'ast> for DollarDollarFinder {
    type Params = AstParams<(), Infallible>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_expr(&mut self, env: &mut (), e: &'ast Expr<'ast>) -> Result<(), Infallible> {
        match &e.2 {
            Expr_::Lvar(l) => {
                if local_id::get_name(&l.1) == et::DOLLARDOLLAR_TMP_VAR && self.pos.is_none() {
                    // Remember the position of the first one
                    self.pos = Some(e.1);
                }
                Ok(())
            }
            // Don't need to recurse into the new scopes of lambdas or expression trees
            Expr_::Lfun(_) | Expr_::Efun(_) | Expr_::ExpressionTree(_) => Ok(()),
            // Only recurse into the left hand side of any pipe as the rhs has new $$
            Expr_::Pipe(p) => {
                let x = &p.1;
                x.accept(env, self.object())
            }
            // Otherwise, recurse completely on the other expressions
            _ => e.recurse(env, self.object()),
        }
    }
}

/// The position of the first use, in the spliced expressions `el`, of the
/// `$$` of a pipe around the expression tree literal. Such uses are
/// [`DOLLARDOLLAR_TMP_VAR`] by the time the literal is desugared.
fn find_dollardollar(el: &[Expr<'_>]) -> Option<Pos> {
    let mut finder = DollarDollarFinder { pos: None };
    for e in el.iter() {
        let Ok(()) = finder.visit_expr(&mut (), e);
    }
    finder.pos
}

struct VoidReturnCheck {
    only_void_return: bool,
}

impl<'ast> Visitor<'ast> for VoidReturnCheck {
    type Params = AstParams<(), Infallible>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_expr(&mut self, env: &mut (), e: &'ast Expr<'ast>) -> Result<(), Infallible> {
        match &e.2 {
            // Don't recurse into splices or LFuns
            Expr_::ETSplice(_) | Expr_::Lfun(_) => Ok(()),
            _ => e.recurse(env, self.object()),
        }
    }

    fn visit_stmt(&mut self, env: &mut (), s: &'ast Stmt<'ast>) -> Result<(), Infallible> {
        match &s.1 {
            Stmt_::Return(e) => {
                if (*e).is_some() {
                    self.only_void_return = false;
                }
                Ok(())
            }
            _ => s.recurse(env, self.object()),
        }
    }
}

fn only_void_return(lfun_body: &[Stmt<'_>]) -> bool {
    let mut checker = VoidReturnCheck {
        only_void_return: true,
    };
    let Ok(()) = lfun_body.accept(&mut (), &mut checker);
    checker.only_void_return
}

fn null_literal<'a>(pos: Pos) -> Expr<'a> {
    Expr((), pos, Expr_::Null)
}

fn string_literal<'a>(pos: Pos, s: &'a str) -> Expr<'a> {
    Expr((), pos, Expr_::String(BStr::new(s)))
}

fn int_literal<'a>(arena: Arena<'a>, pos: Pos, i: usize) -> Expr<'a> {
    Expr(
        (),
        pos,
        Expr_::Int(bumpalo::format!(in arena.bump, "{}", i).into_bump_str()),
    )
}

fn vec_literal<'a>(arena: Arena<'a>, items: &[Expr<'a>]) -> Expr<'a> {
    let positions: Vec<_> = items.iter().map(|x| x.1).collect();
    let position = merge_positions(&positions);
    vec_literal_with_pos(arena, position, items)
}

fn vec_literal_with_pos<'a>(arena: Arena<'a>, pos: Pos, items: &[Expr<'a>]) -> Expr<'a> {
    Expr(
        (),
        pos,
        Expr_::ValCollection(arena.alloc(((pos, VcKind::Vec), None, slice(arena, items)))),
    )
}

fn dict_literal<'a>(
    arena: Arena<'a>,
    pos: Pos,
    key_value_pairs: &[(Expr<'a>, Expr<'a>)],
) -> Expr<'a> {
    let fields: &[Field<'a>] =
        arena.alloc_slice_fill_iter(key_value_pairs.iter().map(|(k, v)| Field(*k, *v)));
    Expr(
        (),
        pos,
        Expr_::KeyValCollection(arena.alloc(((pos, KvcKind::Dict), None, fields))),
    )
}

fn make_id<'a>(pos: Pos, name: &'a str) -> Id<'a> {
    Id(pos, name)
}

fn visitor_variable() -> &'static str {
    "$0v"
}

/// Given a list of arguments, make each a "normal" argument by wrapping it
/// in `Argument::Anormal`
fn build_args<'a>(arena: Arena<'a>, args: &[Expr<'a>]) -> &'a [Argument<'a>] {
    arena.alloc_slice_fill_iter(args.iter().map(|e| Argument::Anormal(*e)))
}

/// Build `$v->meth_name(args)`.
fn v_meth_call<'a>(arena: Arena<'a>, meth_name: &'a str, args: &[Expr<'a>], pos: Pos) -> Expr<'a> {
    let receiver = mk_lvar(arena, pos, visitor_variable());
    let meth = Expr((), pos, Expr_::Id(arena.alloc(Id(pos, meth_name))));

    let c = Expr_::Call(arena.alloc(CallExpr {
        func: Expr(
            (),
            pos,
            Expr_::ObjGet(arena.alloc((
                receiver,
                meth,
                OperatorNullFlavor::Regular,
                PropOrMethod::IsMethod,
            ))),
        ),
        targs: &[],
        args: build_args(arena, args),
        unpacked_arg: None,
    }));
    Expr((), pos, c)
}

fn coalesce<'a>(arena: Arena<'a>, lhs: Expr<'a>, rhs: Expr<'a>, pos: Pos) -> Expr<'a> {
    let c = Expr_::Binop(arena.alloc(Binop {
        bop: Bop::QuestionQuestion,
        lhs,
        rhs,
    }));
    Expr((), pos, c)
}

fn meth_call<'a>(
    arena: Arena<'a>,
    receiver: Expr<'a>,
    meth_name: &'a str,
    args: &[Expr<'a>],
    nullsafe: bool,
    pos: Pos,
) -> Expr<'a> {
    let meth = Expr((), pos, Expr_::Id(arena.alloc(Id(pos, meth_name))));

    let c = Expr_::Call(arena.alloc(CallExpr {
        func: Expr(
            (),
            pos,
            Expr_::ObjGet(arena.alloc((
                receiver,
                meth,
                if nullsafe {
                    OperatorNullFlavor::Nullsafe
                } else {
                    OperatorNullFlavor::Regular
                },
                PropOrMethod::IsMethod,
            ))),
        ),
        targs: &[],
        args: build_args(arena, args),
        unpacked_arg: None,
    }));
    Expr((), pos, c)
}

fn static_meth_call_with_meth_pos<'a>(
    arena: Arena<'a>,
    classname: &'a str,
    meth_name: &'a str,
    meth_pos: Pos,
    args: &[Expr<'a>],
    pos: Pos,
) -> Expr<'a> {
    let callee = Expr(
        (),
        meth_pos,
        Expr_::ClassConst(arena.alloc((
            ClassId(
                (),
                meth_pos,
                ClassId_::CIexpr(Expr(
                    (),
                    meth_pos,
                    Expr_::Id(arena.alloc(Id(meth_pos, classname))),
                )),
            ),
            (meth_pos, meth_name),
        ))),
    );
    Expr(
        (),
        pos,
        Expr_::Call(arena.alloc(CallExpr {
            func: callee,
            targs: &[],
            args: build_args(arena, args),
            unpacked_arg: None,
        })),
    )
}

fn static_meth_call<'a>(
    arena: Arena<'a>,
    classname: &'a str,
    meth_name: &'a str,
    args: &[Expr<'a>],
    pos: Pos,
) -> Expr<'a> {
    static_meth_call_with_meth_pos(arena, classname, meth_name, pos, args, pos)
}

/// Join a slice of positions together into a single, larger position.
fn merge_positions(positions: &[Pos]) -> Pos {
    positions
        .iter()
        .fold(None, |acc, pos| match acc {
            Some(res) => Some(Pos::merge(&res, pos).expect("Positions should be in the same file")),
            None => Some(*pos),
        })
        .unwrap_or(Pos::NONE)
}

fn create_temp_statement_parallel<'a>(
    arena: Arena<'a>,
    pos: Pos,
    exprs: &[Expr<'a>],
    mk_lvar: fn(Arena<'a>, Pos, usize) -> Expr<'a>,
) -> Vec<Stmt<'a>> {
    if exprs.len() < 2 {
        return create_temp_statements(arena, exprs, mk_lvar);
    }
    let lhss = arena.alloc_slice_fill_iter(
        exprs
            .iter()
            .enumerate()
            .map(|(i, expr)| mk_lvar(arena, expr.1, i)),
    );
    // assign a tuple to a list to ensure any awaits can execute concurrently.
    // We don't use a concurrent statement because that requires each rhs to have
    // an await, and that might not be the case here.
    vec![Stmt(
        pos,
        Stmt_::Expr(arena.alloc(Expr(
            (),
            pos,
            Expr_::Assign(arena.alloc((
                Expr((), pos, Expr_::List(lhss)),
                None,
                Expr((), pos, Expr_::Tuple(slice(arena, exprs))),
            ))),
        ))),
    )]
}

fn create_temp_statements<'a>(
    arena: Arena<'a>,
    exprs: &[Expr<'a>],
    mk_lvar: fn(Arena<'a>, Pos, usize) -> Expr<'a>,
) -> Vec<Stmt<'a>> {
    exprs
        .iter()
        .enumerate()
        .map(|(i, expr)| {
            Stmt(
                expr.1,
                Stmt_::Expr(arena.alloc(Expr(
                    (),
                    expr.1,
                    Expr_::Assign(arena.alloc((mk_lvar(arena, expr.1, i), None, *expr))),
                ))),
            )
        })
        .collect()
}

fn temp_lvar_string<'a>(arena: Arena<'a>, name: &str, num: usize) -> &'a str {
    bumpalo::format!(in arena.bump, "$0{}{}", name, num).into_bump_str()
}

fn temp_splice_lvar_string(arena: Arena<'_>, num: usize) -> &str {
    temp_lvar_string(arena, "splice", num)
}

fn temp_splice_lvar<'a>(arena: Arena<'a>, pos: Pos, num: usize) -> Expr<'a> {
    mk_lvar(arena, pos, temp_splice_lvar_string(arena, num))
}

fn temp_function_pointer_lvar_string(arena: Arena<'_>, num: usize) -> &str {
    temp_lvar_string(arena, "fp", num)
}

fn temp_function_pointer_lvar<'a>(arena: Arena<'a>, pos: Pos, num: usize) -> Expr<'a> {
    mk_lvar(arena, pos, temp_function_pointer_lvar_string(arena, num))
}

fn temp_static_method_lvar_string(arena: Arena<'_>, num: usize) -> &str {
    temp_lvar_string(arena, "sm", num)
}

fn temp_static_method_lvar<'a>(arena: Arena<'a>, pos: Pos, num: usize) -> Expr<'a> {
    mk_lvar(arena, pos, temp_static_method_lvar_string(arena, num))
}

/// Given a Pos, returns a shape literal expression representing it.
///
/// ```text
/// shape(
///   'path' => __FILE__,
///   'start_line' => 1,
///   'end_line' => 10,
///   'start_column' => 0,
///   'end_column' => 80,
/// )
/// ```
///
/// If this Pos is `Pos::NONE` or invalid, return a literal null instead.
fn exprpos<'a>(arena: Arena<'a>, pos: Pos) -> Expr<'a> {
    if pos.is_none() || !pos.is_valid() {
        null_literal(pos)
    } else {
        let ((start_lnum, start_bol, start_offset), (end_lnum, end_bol, end_offset)) =
            arena.lnum_bol_offset(pos);

        let fields = [
            (
                "path",
                Expr((), pos, Expr_::Id(arena.alloc(make_id(pos, "__FILE__")))),
            ),
            ("start_line", int_literal(arena, pos, start_lnum)),
            ("end_line", int_literal(arena, pos, end_lnum)),
            (
                "start_column",
                int_literal(arena, pos, start_offset - start_bol),
            ),
            ("end_column", int_literal(arena, pos, end_offset - end_bol)),
        ];

        shape_literal(arena, pos, &fields)
    }
}

fn exprpos_opt<'a>(arena: Arena<'a>, pos: &Option<Pos>) -> Expr<'a> {
    match pos {
        None => null_literal(Pos::NONE),
        Some(pos) => exprpos(arena, *pos),
    }
}

fn shape_literal<'a>(arena: Arena<'a>, pos: Pos, fields: &[(&'a str, Expr<'a>)]) -> Expr<'a> {
    let shape_fields = arena.alloc_slice_fill_iter(fields.iter().map(|(name, value)| {
        let bs = BStr::new(*name);
        let field_name = ShapeFieldName::SFlitStr((pos, bs));
        ShapeExprField::SFField(field_name, *value)
    }));
    Expr((), pos, Expr_::Shape(shape_fields))
}

/// Converts an expression to a boolean by calling the visitor's operationType method
/// followed by the __bool method. This is used for conditional expressions in
/// expression trees to ensure proper type virtualization.
fn boolify<'a>(arena: Arena<'a>, receiver: Expr<'a>, visitor_name: &'a str) -> Expr<'a> {
    let pos = receiver.1;
    let operation_type_expr =
        static_meth_call(arena, visitor_name, et::OPERATION_TYPE, &[receiver], pos);
    meth_call(arena, operation_type_expr, "__bool", &[], false, pos)
}

/// Virtualizes an operation by calling the visitor's operationType method on the operand
/// and then calling the specified operation method. This is used for both binary and
/// unary operations in expression trees to ensure proper type virtualization.
/// The operand2 parameter is optional so this can be used for both unary and
/// binary operations
fn virtualize_operation<'a>(
    arena: Arena<'a>,
    operand1: Expr<'a>,
    operation_method: &'a str,
    operand2: Option<Expr<'a>>,
    visitor_name: &'a str,
    pos: Pos,
) -> Expr<'a> {
    let operation_type_expr =
        static_meth_call(arena, visitor_name, et::OPERATION_TYPE, &[operand1], pos);
    meth_call(
        arena,
        operation_type_expr,
        operation_method,
        operand2.as_slice(),
        false,
        pos,
    )
}

struct RewriteState<'a> {
    arena: Arena<'a>,
    splices: Vec<Expr<'a>>,
    global_function_pointers: Vec<Expr<'a>>,
    static_method_pointers: Vec<Expr<'a>>,
    errors: Vec<(Pos, String)>,
    contains_spliced_await: bool,
}

impl<'a> RewriteState<'a> {
    /// Performs both the virtualization and the desugaring in tandem
    /// Also extracts the expressions that need to be assigned to temporaries
    /// Replaces the extracted splices, function pointers, and static method pointers
    /// with temporary variables
    fn rewrite_expr(&mut self, e: Expr<'a>, visitor_name: &'a str) -> RewriteResult<'a> {
        self.rewrite_expr_impl(e, visitor_name, false)
    }

    /// Rewrite an expression in subscript index position, where a string or
    /// int literal virtualizes as itself rather than as the DSL's arraykey
    /// type. Handling string and int literals in subscripts this way is
    /// necessary to support shape indexing properly.
    fn rewrite_subscript_index(&mut self, e: Expr<'a>, visitor_name: &'a str) -> RewriteResult<'a> {
        self.rewrite_expr_impl(e, visitor_name, true)
    }

    fn rewrite_expr_impl(
        &mut self,
        e: Expr<'a>,
        visitor_name: &'a str,
        subscript_index: bool,
    ) -> RewriteResult<'a> {
        let arena = self.arena;

        // If we can't rewrite the expression (e.g. due to unsupported syntax), return the
        // original syntax unmodified. This is particularly useful during code completion,
        // where an unfinished code fragment might accidentally use unsupported syntax.
        // Both results end up in the same tree, which must not hold an expression
        // twice, so the desugared one is a copy.
        let unchanged_result = || RewriteResult {
            virtual_expr: e,
            desugar_expr: aast_copy::expr(arena.bump, &e),
        };

        let Expr(_, pos, expr_) = e;
        let pos_expr = exprpos(arena, pos);

        match expr_ {
            // Source: MyDsl`1`
            // Virtualized: MyDsl::intType(), or `1` in a subscript index
            // Desugared: $0v->visitInt(new ExprPos(...), 1)
            Expr_::Int(_) => {
                let virtual_expr = if subscript_index {
                    Expr((), pos, expr_)
                } else {
                    static_meth_call(arena, visitor_name, et::INT_TYPE, &[], pos)
                };
                let desugar_expr =
                    v_meth_call(arena, et::VISIT_INT, &[pos_expr, Expr((), pos, expr_)], pos);
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`1.0`
            // Virtualized: MyDsl::floatType()
            // Desugared: $0v->visitFloat(new ExprPos(...), 1.0)
            Expr_::Float(_) => {
                let virtual_expr = static_meth_call(arena, visitor_name, et::FLOAT_TYPE, &[], pos);
                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_FLOAT,
                    &[pos_expr, Expr((), pos, expr_)],
                    pos,
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`'foo'`
            // Virtualized: MyDsl::stringType(), or `'foo'` in a subscript index
            // Desugared: $0v->visitString(new ExprPos(...), 'foo')
            Expr_::String(_) => {
                let virtual_expr = if subscript_index {
                    Expr((), pos, expr_)
                } else {
                    static_meth_call(arena, visitor_name, et::STRING_TYPE, &[], pos)
                };
                let desugar_expr = mk_visit_string(arena, pos, expr_);
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`true`
            // Virtualized: MyDsl::boolType()
            // Desugared: $0v->visitBool(new ExprPos(...), true)
            Expr_::True | Expr_::False => {
                let virtual_expr = static_meth_call(arena, visitor_name, et::BOOL_TYPE, &[], pos);
                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_BOOL,
                    &[pos_expr, Expr((), pos, expr_)],
                    pos,
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`null`
            // Virtualized: null
            // Desugared: $0v->visitNull(new ExprPos(...))
            Expr_::Null => {
                let virtual_expr = null_literal(pos);
                let desugar_expr = v_meth_call(arena, et::VISIT_NULL, &[pos_expr], pos);
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`$x`
            // Virtualized: $x
            // Desugared: $0v->visitLocal(new ExprPos(...), '$x')
            Expr_::Lvar(lid) => {
                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_LOCAL,
                    &[pos_expr, string_literal(lid.0, (lid.1).1)],
                    pos,
                );
                let virtual_expr = Expr((), pos, Expr_::Lvar(lid));
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            Expr_::Binop(binop) => {
                let Binop { bop, lhs, rhs } = *binop;
                let binop_str = match bop {
                    Bop::Plus => "__plus",
                    Bop::Minus => "__minus",
                    Bop::Star => "__star",
                    Bop::Slash => "__slash",
                    Bop::Percent => "__percent",
                    // Convert boolean &&, ||
                    Bop::Ampamp => "__ampamp",
                    Bop::Barbar => "__barbar",
                    // Convert comparison operators, <, <=, >, >=, ===, !==
                    Bop::Lt => "__lessThan",
                    Bop::Lte => "__lessThanEqual",
                    Bop::Gt => "__greaterThan",
                    Bop::Gte => "__greaterThanEqual",
                    Bop::Eqeqeq => "__tripleEquals",
                    Bop::Diff2 => "__notTripleEquals",
                    // Convert string concatenation
                    Bop::Dot => "__dot",
                    // Convert bitwise operators, &, |, ^, <<, >>
                    Bop::Amp => "__amp",
                    Bop::Bar => "__bar",
                    Bop::Xor => "__caret",
                    Bop::Ltlt => "__lessThanLessThan",
                    Bop::Gtgt => "__greaterThanGreaterThan",
                    // Coalesce operator ??
                    Bop::QuestionQuestion => {
                        if matches!(lhs.2, Expr_::ArrayGet(_)) {
                            // Array access expressions cannot be used as the left-hand side operand of the null coalesce operator (??)
                            // because in Hack, ?? has special behavior when applied to array access: it swallows exceptions raised when
                            // the key is not present in the container. To maintain consistency with Hack's behavior, we will re-implement
                            // this special case in the expression tree implementation of the ArrayGet operator.
                            self.errors.push((
                                pos,
                                "Expression trees do not support array access (e.g. `arr[0]`) as the left side of the null coalesce operator (`??`).".into(),
                            ));
                            "__unsupported"
                        } else {
                            "__questionQuestion"
                        }
                    }
                    // Explicit list of unsupported operators and error messages
                    Bop::Starstar => {
                        self.errors.push((
                            pos,
                            "Expression trees do not support the exponent operator `**`.".into(),
                        ));
                        "__unsupported"
                    }
                    Bop::Eqeq | Bop::Diff => {
                        self.errors.push((
                            pos,
                            "Expression trees only support strict equality operators `===` and `!==`".into(),
                        ));
                        "__unsupported"
                    }
                    Bop::Cmp => {
                        self.errors.push((
                            pos,
                            "Expression trees do not support the spaceship operator `<=>`. Try comparison operators like `<` and `>=`".into(),
                        ));
                        "__unsupported"
                    }
                };
                let rewritten_lhs = self.rewrite_expr(lhs, visitor_name);
                let rewritten_rhs = self.rewrite_expr(rhs, visitor_name);

                let virtual_expr = match bop {
                    // Source: MyDsl`... ?? ...`
                    // Virtualized: ... ?? ...
                    // Desugared: $0v->visitBinop(new ExprPos(...), ..., '__questionQuestion', ...)
                    Bop::QuestionQuestion => {
                        // Required to refine the type
                        coalesce(
                            arena,
                            rewritten_lhs.virtual_expr,
                            rewritten_rhs.virtual_expr,
                            pos,
                        )
                    }
                    // Source: MyDsl`... + ...`
                    // Virtualized: MyDsl::operationType(...)->__plus(...)
                    // Desugared: $0v->visitBinop(new ExprPos(...), ..., '__plus', ...)
                    _ => virtualize_operation(
                        arena,
                        rewritten_lhs.virtual_expr,
                        binop_str,
                        Some(rewritten_rhs.virtual_expr),
                        visitor_name,
                        pos,
                    ),
                };
                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_BINOP,
                    &[
                        pos_expr,
                        rewritten_lhs.desugar_expr,
                        string_literal(pos, binop_str),
                        rewritten_rhs.desugar_expr,
                    ],
                    pos,
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            Expr_::Assign(assign) => {
                let (lhs, bop, rhs) = *assign;

                if bop.is_some() {
                    self.errors.push((pos, "Expression trees do not support compound assignments. Try the long form style `$foo = $foo + $bar` instead.".into(),));
                };

                if matches!(&lhs, Expr(_, _, Expr_::ArrayGet(_))) {
                    let (root, chain) = Self::collect_subscript_chain(lhs);

                    // Reject append (None index) at any depth — single-level
                    // (`$arr[] = $val`) and intermediate (`$arr[][$j] = $val`)
                    // both flow through here. Append support may be added at a
                    // later time; empty index at intermediary is always wrong.
                    // Checking every position avoids panicking later in the
                    // key-rewriting loop, which expects all indices Some.
                    if chain.iter().any(|(_, idx)| idx.is_none()) {
                        self.errors.push((
                            pos,
                            "Subscript append is not yet supported in expression trees.".into(),
                        ));
                        return unchanged_result();
                    }

                    let rewritten_root = self.rewrite_expr(root, visitor_name);
                    let rewritten_rhs = self.rewrite_expr(rhs, visitor_name);

                    let (virtual_key_exprs, desugar_key_exprs): (Vec<Expr<'a>>, Vec<Expr<'a>>) =
                        chain
                            .iter()
                            .map(|(_, idx_opt)| {
                                let idx = idx_opt.expect("checked above");
                                let r = self.rewrite_subscript_index(idx, visitor_name);
                                (r.virtual_expr, r.desugar_expr)
                            })
                            .unzip();

                    let virtual_lhs = chain.iter().zip(virtual_key_exprs).fold(
                        rewritten_root.virtual_expr,
                        |acc, ((ag_pos, _), key)| {
                            Expr((), *ag_pos, Expr_::ArrayGet(arena.alloc((acc, Some(key)))))
                        },
                    );

                    let virtual_expr = Expr(
                        (),
                        pos,
                        Expr_::Assign(arena.alloc((virtual_lhs, bop, rewritten_rhs.virtual_expr))),
                    );
                    // Source: MyDsl`$container[$k1]...[$kN] = $value`
                    // Virtualized: $container[$k1]...[$kN] = $value
                    // Desugared: $0v->visitSubscriptAssign(new ExprPos(...), container, vec[k1, ..., kN], value)
                    let desugar_expr = v_meth_call(
                        arena,
                        et::VISIT_SUBSCRIPT_ASSIGN,
                        &[
                            pos_expr,
                            rewritten_root.desugar_expr,
                            vec_literal_with_pos(arena, pos, &desugar_key_exprs),
                            rewritten_rhs.desugar_expr,
                        ],
                        pos,
                    );
                    return RewriteResult {
                        virtual_expr,
                        desugar_expr,
                    };
                }

                let rewritten_lhs = self.rewrite_expr(lhs, visitor_name);
                let rewritten_rhs = self.rewrite_expr(rhs, visitor_name);

                // Source: MyDsl`$x = ...`
                // Virtualized: $x = ...
                // Desugared: $0v->visitAssign(new ExprPos(...), $0v->visitLocal(...), ...)
                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_ASSIGN,
                    &[
                        pos_expr,
                        rewritten_lhs.desugar_expr,
                        rewritten_rhs.desugar_expr,
                    ],
                    pos,
                );
                let virtual_expr = Expr(
                    (),
                    pos,
                    Expr_::Assign(arena.alloc((
                        rewritten_lhs.virtual_expr,
                        bop,
                        rewritten_rhs.virtual_expr,
                    ))),
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`!...`
            // Virtualized: MyDSL::operationType(...)->__exclamationMark(...)
            // Desugared: $0v->visitUnop(new ExprPos(...), ..., '__exclamationMark')
            Expr_::Unop(unop) => {
                let (op, operand) = *unop;

                // Subscript increment/decrement is not yet supported.
                // Both single-level ($arr[$i]++) and nested ($arr[$i][$j]++)
                // are rejected until write support is added.
                if matches!(op, Uop::Upincr | Uop::Updecr)
                    && matches!(&operand, Expr(_, _, Expr_::ArrayGet(_)))
                {
                    self.errors.push((
                        pos,
                        "Subscript increment/decrement is not yet supported in expression trees."
                            .into(),
                    ));
                }

                let rewritten_operand = self.rewrite_expr(operand, visitor_name);

                let op_str = match op {
                    // Allow boolean not operator !$x
                    Uop::Unot => "__exclamationMark",
                    // Allow negation -$x (required for supporting negative literals -123)
                    Uop::Uminus => "__negate",
                    // Allow bitwise complement
                    Uop::Utild => "__tilde",
                    // Currently not allowed operators
                    Uop::Uplus => {
                        self.errors.push((
                            pos,
                            "Expression trees do not support the unary plus operator.".into(),
                        ));
                        "__unsupported"
                    }
                    // Postfix ++
                    Uop::Upincr => "__postfixPlusPlus",
                    // Prefix ++
                    Uop::Uincr => {
                        self.errors.push((
                            pos,
                            "Expression trees only support postfix increment operator `$x++`."
                                .into(),
                        ));
                        "__unsupported"
                    }
                    // Postfix --
                    Uop::Updecr => "__postfixMinusMinus",
                    // Prefix --
                    Uop::Udecr => {
                        self.errors.push((
                            pos,
                            "Expression trees only support postfix decrement operator `$x--`."
                                .into(),
                        ));
                        "__unsupported"
                    }
                    Uop::Usilence => {
                        self.errors.push((
                            pos,
                            "Expression trees do not support the error suppression operator `@`."
                                .into(),
                        ));
                        "__unsupported"
                    }
                };
                let virtual_expr = virtualize_operation(
                    arena,
                    rewritten_operand.virtual_expr,
                    op_str,
                    None,
                    visitor_name,
                    pos,
                );
                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_UNOP,
                    &[
                        pos_expr,
                        rewritten_operand.desugar_expr,
                        string_literal(pos, op_str),
                    ],
                    pos,
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`... ? ... : ...`
            // Virtualized: MyDsl::operationType(...)->__bool() ? ... : ...
            // Desugared: $0v->visitTernary(new ExprPos(...), ..., ..., ...)
            Expr_::Eif(eif) => {
                let (e1, e2o, e3) = *eif;

                let rewritten_e1 = self.rewrite_expr(e1, visitor_name);
                let rewritten_e2 = if let Some(e2) = e2o {
                    self.rewrite_expr(e2, visitor_name)
                } else {
                    self.errors.push((
                        pos,
                        "Unsupported expression tree syntax: Elvis operator".into(),
                    ));
                    // The other two operands of `e` are rewritten below and may
                    // come back as they are, so neither result can be `e` itself.
                    RewriteResult {
                        virtual_expr: aast_copy::expr(arena.bump, &e),
                        desugar_expr: aast_copy::expr(arena.bump, &e),
                    }
                };
                let rewritten_e3 = self.rewrite_expr(e3, visitor_name);

                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_TERNARY,
                    &[
                        pos_expr,
                        rewritten_e1.desugar_expr,
                        rewritten_e2.desugar_expr,
                        rewritten_e3.desugar_expr,
                    ],
                    pos,
                );
                let virtual_expr = Expr(
                    (),
                    pos,
                    Expr_::Eif(arena.alloc((
                        boolify(arena, rewritten_e1.virtual_expr, visitor_name),
                        Some(rewritten_e2.virtual_expr),
                        rewritten_e3.virtual_expr,
                    ))),
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`...()`
            // Virtualized: (...->__unwrap())()
            // Desugared: $0v->visitCall(new ExprPos(...), ..., vec[])
            Expr_::Call(call) => {
                let CallExpr {
                    func: recv,
                    targs,
                    args,
                    unpacked_arg: variadic,
                } = *call;

                if variadic.is_some() {
                    self.errors.push((
                        pos,
                        "Expression trees do not support variadic calls.".into(),
                    ));
                }
                if !targs.is_empty() {
                    self.errors.push((
                        pos,
                        "Expression trees do not support function calls with generics.".into(),
                    ));
                }
                match &recv.2 {
                    // Don't transform calls to `hh_show`.
                    Expr_::Id(sid) if is_typechecker_fun_name(sid.1) => {
                        let call_e = Expr((), pos, Expr_::Call(call));
                        return RewriteResult {
                            desugar_expr: aast_copy::expr(arena.bump, &call_e),
                            virtual_expr: call_e,
                        };
                    }
                    _ => {}
                }

                let mut args_without_inout_and_named = vec![];
                for arg in args {
                    match *arg {
                        Argument::Anormal(e) => args_without_inout_and_named.push(e),
                        Argument::Ainout(_, Expr(_, p, _)) => self.errors.push((
                            p,
                            "Expression trees do not support `inout` function calls.".into(),
                        )),
                        Argument::Anamed(_, Expr(_, p, _)) => self.errors.push((
                            p,
                            "Expression trees do not support named parameters.".into(),
                        )),
                    }
                }

                match recv.2 {
                    // Source: MyDsl`foo()`
                    // Virtualized: (MyDsl::symbolType($0fpXX)->__unwrap())()
                    // Desugared: $0v->visitCall(new ExprPos(...), $0v->visitGlobalFunction(new ExprPos(...), $0fpXX), vec[])
                    Expr_::Id(sid) => {
                        let (virtual_args, desugar_args) =
                            self.rewrite_exprs(&args_without_inout_and_named, visitor_name);
                        let len = self.global_function_pointers.len();
                        self.global_function_pointers
                            .push(global_func_ptr(arena, sid));
                        let temp_variable = temp_function_pointer_lvar(arena, recv.1, len);
                        let desugar_expr = v_meth_call(
                            arena,
                            et::VISIT_CALL,
                            &[
                                pos_expr,
                                v_meth_call(
                                    arena,
                                    et::VISIT_GLOBAL_FUNCTION,
                                    &[exprpos(arena, pos), temp_variable],
                                    pos,
                                ),
                                vec_literal(arena, &desugar_args),
                            ],
                            pos,
                        );
                        let virtual_expr = virtualize_function_pointer(
                            arena,
                            visitor_name,
                            pos,
                            &virtual_args,
                            temp_variable,
                        );
                        RewriteResult {
                            virtual_expr,
                            desugar_expr,
                        }
                    }
                    // Source: MyDsl`Foo::bar()`
                    // Virtualized: (MyDsl::symbolType($0smXX)->__unwrap())()
                    // Desugared: $0v->visitCall(new ExprPos(...), $0v->visitStaticMethod(new ExprPos(...), $0smXX, vec[])
                    Expr_::ClassConst(&(cid, s)) => {
                        let is_supported = match &cid {
                            ClassId(_, _, ClassId_::CIself | ClassId_::CIparent) => false,
                            cid if is_parent(cid) => false,
                            ClassId(_, _, ClassId_::CIreified(_)) => true,
                            ClassId(_, _, ClassId_::CIexpr(Expr(_, _, Expr_::Id(_)))) => true,
                            _ => false,
                        };

                        if !is_supported {
                            self.errors.push((
                                pos,
                                "Expression trees only support static method calls on `static` and named classes.".into(),
                            ));
                        }

                        let len = self.static_method_pointers.len();
                        self.static_method_pointers
                            .push(static_meth_ptr(arena, recv.1, &cid, &s));
                        let temp_variable = temp_static_method_lvar(arena, recv.1, len);
                        let (virtual_expr, desugar_args) = match cid {
                            ClassId(_, _, ClassId_::CIexpr(Expr(_, _, Expr_::Id(Id(_, name)))))
                                if *name == visitor_name && !is_parent(&cid) =>
                            {
                                match s.1 {
                                    // type check any special function calls
                                    et::BUILTIN_SHAPE_AT => handle_shapes_at(
                                        self,
                                        visitor_name,
                                        pos,
                                        &args_without_inout_and_named,
                                    ),
                                    et::BUILTIN_SHAPE_PUT => handle_shapes_put(
                                        self,
                                        visitor_name,
                                        pos,
                                        &args_without_inout_and_named,
                                    ),
                                    et::BUILTIN_SHAPE_IDX => handle_shapes_idx(
                                        self,
                                        visitor_name,
                                        pos,
                                        &args_without_inout_and_named,
                                    ),
                                    _ => handle_class_const(
                                        self,
                                        visitor_name,
                                        pos,
                                        &args_without_inout_and_named,
                                        temp_variable,
                                    ),
                                }
                            }
                            _ => handle_class_const(
                                self,
                                visitor_name,
                                pos,
                                &args_without_inout_and_named,
                                temp_variable,
                            ),
                        };
                        let desugar_expr = v_meth_call(
                            arena,
                            et::VISIT_CALL,
                            &[
                                pos_expr,
                                v_meth_call(
                                    arena,
                                    et::VISIT_STATIC_METHOD,
                                    &[exprpos(arena, pos), temp_variable],
                                    pos,
                                ),
                                vec_literal(arena, &desugar_args),
                            ],
                            pos,
                        );
                        RewriteResult {
                            virtual_expr,
                            desugar_expr,
                        }
                    }
                    // Source: MyDsl`...()`
                    // Virtualized: ...()
                    // Desugared: $0v->visitCall(new ExprPos(...), ..., vec[])
                    _ => {
                        let should_virtualize_call = match recv.2 {
                            // We don't virtualize calls on instance methods
                            Expr_::ObjGet(og) if og.3 == PropOrMethod::IsMethod => false,
                            _ => true,
                        };
                        let (virtual_args, desugar_args) =
                            self.rewrite_exprs(&args_without_inout_and_named, visitor_name);
                        let rewritten_recv =
                            self.rewrite_expr(Expr((), recv.1, recv.2), visitor_name);

                        let desugar_expr = v_meth_call(
                            arena,
                            et::VISIT_CALL,
                            &[
                                pos_expr,
                                rewritten_recv.desugar_expr,
                                vec_literal(arena, &desugar_args),
                            ],
                            pos,
                        );
                        let virtual_expr = Expr(
                            (),
                            pos,
                            Expr_::Call(arena.alloc(CallExpr {
                                func: if should_virtualize_call {
                                    _virtualize_call(arena, rewritten_recv.virtual_expr, false, pos)
                                } else {
                                    rewritten_recv.virtual_expr
                                },
                                targs: &[],
                                args: build_args(arena, &virtual_args),
                                unpacked_arg: None,
                            })),
                        );
                        RewriteResult {
                            virtual_expr,
                            desugar_expr,
                        }
                    }
                }
            }
            // Source: MyDsl`($x) ==> { ... }`
            // Virtualized: ($x) ==> { ...; return MyDsl::voidType(); }
            //   if no `return expr;` statements.
            // Desugared: $0v->visitLambda(new ExprPos(...), vec['$x'], vec[...]).
            Expr_::Lfun(lf) => {
                let mut fun_ = lf.0;

                match &fun_ {
                    Fun_ {
                        // Allow a plain function that isn't async.
                        fun_kind: FunKind::FSync,
                        tparams: _,
                        body: _,
                        span: _,
                        doc_comment: _,
                        ret: _,
                        annotation: (),
                        params: _,
                        user_attributes: _,
                        hidden: _,
                        // The function should not use any of these newer features.
                        readonly_this: None,
                        readonly_ret: None,
                        ctxs: None,
                        unsafe_ctxs: None,
                        external: false,
                    } => {}
                    _ => {
                        self.errors.push((
                            pos,
                            "Expression trees only support simple lambdas, without features like `async`, generators or capabilities."
                                .into(),
                        ));
                    }
                }

                let mut desugar_optional_params = Vec::with_capacity(fun_.params.len());
                let mut param_names = Vec::with_capacity(fun_.params.len());
                // The parameters of the virtualized lambda: the original
                // ones, with their default values virtualized.
                let mut params = Vec::with_capacity(fun_.params.len());
                for param in fun_.params {
                    let mut param = *param;
                    match param.info {
                        FunParamInfo::ParamOptional(Some(expr)) => {
                            let rewritten_lambda = self.rewrite_expr(expr, visitor_name);
                            let desugar_visit_optional_param_expr = v_meth_call(
                                arena,
                                et::VISIT_OPTIONAL_PARAMETER,
                                &[
                                    exprpos(arena, pos),
                                    string_literal(param.pos, param.name),
                                    rewritten_lambda.desugar_expr,
                                ],
                                pos,
                            );
                            desugar_optional_params.push(desugar_visit_optional_param_expr);
                            param.info =
                                FunParamInfo::ParamOptional(Some(rewritten_lambda.virtual_expr));
                        }
                        _ => {
                            param_names.push(string_literal(param.pos, param.name));
                        }
                    }
                    params.push(param);
                }
                fun_.params = slice(arena, &params);

                let body = fun_.body.fb_ast.0;

                let should_append_return = only_void_return(body);

                let (mut virtual_body_stmts, desugar_body) = self.rewrite_stmts(body, visitor_name);
                if should_append_return {
                    virtual_body_stmts.push(Stmt(
                        pos,
                        Stmt_::Return(arena.alloc(Some(static_meth_call(
                            arena,
                            visitor_name,
                            et::VOID_TYPE,
                            &[],
                            pos,
                        )))),
                    ));
                }
                let mut exprs = vec![
                    pos_expr,
                    vec_literal(arena, &param_names),
                    vec_literal(arena, &desugar_body),
                ];
                if !desugar_optional_params.is_empty() {
                    exprs.push(vec_literal(arena, &desugar_optional_params));
                }
                let desugar_expr = v_meth_call(arena, et::VISIT_LAMBDA, &exprs, pos);

                fun_.body.fb_ast = Block(slice(arena, &virtual_body_stmts));

                let virtual_expr = _virtualize_lambda(
                    arena,
                    visitor_name,
                    Expr((), pos, mk_lfun(arena, fun_)),
                    pos,
                );

                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }

            // Splices are lifted out and assigned to temporary variables.
            // Splices can be either normal:
            // Source: MyDsl`${ ... }` where there are no nested free variables in ETs in the splice
            // Lifted to $0splice = ${ ... };
            // Virtualized to ${ $0splice }
            // Desugared to $0v->splice(new ExprPos(...), '$0splice', $0splice)
            //
            // Splices can also be macro splices. In that case, we wrap with a lambda to delay evaluation
            // Source: MyDsl`${ ... }` where there are nested free variables (say $x and $y) in ETs in the splice
            // Lifted to $0splice = ${ () ==> ... };
            // Virtualized to ${ $0splice() } where the splice records $x and $y
            // Desugared to $0v->macro_splice(new ExprPos(...), '$0splice', $0splice, vec!['$x', '$y'])
            // Where there is an await inside of the splice, the lambda created is async.
            // Further, the desugared expression calls async_macro_splice, rather than macro_splice
            Expr_::ETSplice(&EtSplice {
                spliced_expr,
                contains_await,
                extract_client_type: _,
                macro_variables,
                temp_lid: _,
            }) => {
                let len = self.splices.len();
                let expr_pos = spliced_expr.1;
                let temp_variable = temp_splice_lvar(arena, expr_pos, len);
                let temp_variable_string =
                    string_literal(expr_pos, temp_splice_lvar_string(arena, len));
                let mut args = vec![pos_expr, temp_variable_string, temp_variable];
                if let Some(macro_variables) = macro_variables {
                    let macro_var_exprs: Vec<_> = macro_variables
                        .iter()
                        .map(|Lid(pos, (_, s))| string_literal(*pos, s))
                        .collect();
                    args.push(vec_literal(arena, &macro_var_exprs));
                }
                let desugar_expr = v_meth_call(arena, et::SPLICE, &args, pos);
                let virtual_spliced_expr = temp_variable;
                let temp_lid = temp_variable.2.as_lvar().map(|lid| lid.1);
                let virtual_expr = Expr(
                    (),
                    pos,
                    Expr_::ETSplice(arena.alloc(EtSplice {
                        spliced_expr: virtual_spliced_expr,
                        extract_client_type: true,
                        contains_await,
                        macro_variables,
                        temp_lid,
                    })),
                );
                let spliced_expr =
                    static_meth_call(arena, visitor_name, et::LIFT, &[spliced_expr], expr_pos);
                let expr_ = Expr_::ETSplice(arena.alloc(EtSplice {
                    spliced_expr,
                    contains_await,
                    extract_client_type: false,
                    macro_variables,
                    temp_lid,
                }));
                let expr = Expr((), expr_pos, expr_);
                self.splices.push(expr);

                self.contains_spliced_await |= contains_await;
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source:
            //  MyDsl`(...)->foo` or
            //  MyDsl`(...)->foo(...)` or
            //  MyDsl`(...)?->foo` or
            //  MyDsl`(...)?->foo(...)`
            // Virtualized to:
            //   `(...)->foo` or
            //   `(...)->foo(...)` or
            //   `(...)?->foo` or
            //   `(...)?->foo(...)`
            // Desugared to:
            //   `$0v->visitPropertyAccess(new ExprPos(...), ..., 'foo')` or
            //   `$0v->visitInstanceMethod(new ExprPos(...), ..., 'foo')` or
            //   `$0v->visitPropertyAccessNullSafe(new ExprPos(...), ..., 'foo')` or
            //   `$0v->visitInstanceMethodNullSafe(new ExprPos(...), ..., 'foo')`
            Expr_::ObjGet(og) => {
                let (e1, e2, null_flavor, is_prop_call) = *og;
                let rewritten_e1 = self.rewrite_expr(e1, visitor_name);

                let id = if let Expr_::Id(id) = &e2.2 {
                    string_literal(id.0, id.1)
                } else {
                    self.errors.push((
                        pos,
                        "Expression trees only support named property or instance method access."
                            .into(),
                    ));
                    aast_copy::expr(arena.bump, &e2)
                };
                let desugar_expr = v_meth_call(
                    arena,
                    match (is_prop_call, null_flavor) {
                        (PropOrMethod::IsProp, OperatorNullFlavor::Regular) => {
                            et::VISIT_PROPERTY_ACCESS
                        }
                        (PropOrMethod::IsMethod, OperatorNullFlavor::Regular) => {
                            et::VISIT_INSTANCE_METHOD
                        }
                        (PropOrMethod::IsProp, OperatorNullFlavor::Nullsafe) => {
                            et::VISIT_PROPERTY_ACCESS_NULL_SAFE
                        }
                        (PropOrMethod::IsMethod, OperatorNullFlavor::Nullsafe) => {
                            et::VISIT_INSTANCE_METHOD_NULL_SAFE
                        }
                    },
                    &[pos_expr, rewritten_e1.desugar_expr, id],
                    pos,
                );

                let virtual_expr = Expr(
                    (),
                    pos,
                    Expr_::ObjGet(arena.alloc((
                        rewritten_e1.virtual_expr,
                        e2,
                        null_flavor,
                        is_prop_call,
                    ))),
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`ClientMap {...}`
            // Virtualized: ClientMap::__make(...)
            // Desugared: $0v->visitKeyedCollection('ClientMap', ...)
            Expr_::Collection(&(Id(name_pos, name), targ, fields)) => {
                let mut rewritten_fields_virtual = vec![];
                let mut rewritten_fields_desugar = vec![];
                for kv in fields {
                    match *kv {
                        Afield::AFvalue(v) => {
                            self.rewrite_expr(v, visitor_name);
                            self.errors.push((
                                pos,
                                "Collections in expression trees must have `key => value` entries"
                                    .into(),
                            ))
                        }
                        Afield::AFkvalue(k, v) => {
                            let pos = match Pos::merge(&k.1, &v.1) {
                                Some(pos) => pos,
                                _ => k.1,
                            };
                            let k_rewr = self.rewrite_expr(k, visitor_name);
                            let v_rewr = self.rewrite_expr(v, visitor_name);
                            rewritten_fields_virtual.push(Expr(
                                (),
                                pos,
                                Expr_::Tuple(slice(
                                    arena,
                                    &[k_rewr.virtual_expr, v_rewr.virtual_expr],
                                )),
                            ));
                            rewritten_fields_desugar.push(Expr(
                                (),
                                pos,
                                Expr_::Tuple(slice(
                                    arena,
                                    &[k_rewr.desugar_expr, v_rewr.desugar_expr],
                                )),
                            ))
                        }
                    }
                }
                let mut args = vec![pos_expr, string_literal(name_pos, name)];
                args.append(&mut rewritten_fields_desugar);
                let desugar_expr = v_meth_call(arena, et::VISIT_KEYED_COLLECTION, &args, pos);
                let virtual_expr = static_meth_call_with_meth_pos(
                    arena,
                    name,
                    et::MAKE_KEYED_COLLECTION_TYPE,
                    name_pos,
                    &rewritten_fields_virtual,
                    pos,
                );
                match targ {
                    None => {}
                    Some(_) => self.errors.push((
                        pos,
                        "Collections in expression trees must not have explicit type hints".into(),
                    )),
                }
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            // Source: MyDsl`shape('key1' => value1, 'key2' => value2)`
            // Virtualized: shape('key1' => virtualized(value1), 'key2' => virtualized(value2))
            // Desugared: $0v->visitShape(pos, vec[tuple($0->visitString(pos, 'key1'), desugared(value1))])
            Expr_::Shape(fields) => {
                let mut virtual_shape_fields = vec![];
                let mut desugar_shape_fields = vec![];
                for field in fields {
                    match *field {
                        ShapeExprField::SFField(shape_key, shape_value) => {
                            if let ShapeFieldName::SFlitStr((shape_key_pos, shape_key_name)) =
                                shape_key
                            {
                                let value_expr = self.rewrite_expr(shape_value, visitor_name);
                                // we virtualize using the original Hack shape key expression...
                                virtual_shape_fields.push(ShapeExprField::SFField(
                                    shape_key,
                                    value_expr.virtual_expr,
                                ));
                                desugar_shape_fields.push(Expr(
                                    (),
                                    shape_key_pos,
                                    Expr_::Tuple(slice(
                                        arena,
                                        &[
                                            // ...however, we desuger the key into a string expression, to keep the key desugaring
                                            // generic and more akin to other collections.
                                            mk_visit_string(
                                                arena,
                                                shape_key_pos,
                                                Expr_::String(shape_key_name),
                                            ),
                                            value_expr.desugar_expr,
                                        ],
                                    )),
                                ));
                            } else {
                                self.errors.push((
                                    pos,
                                    "Expression trees only support string literal shape field names."
                                        .into(),
                                ));
                            }
                        }
                        ShapeExprField::SFSplat(_) => {
                            self.errors.push((
                                pos,
                                "Splat is not supported in expression tree shapes.".into(),
                            ));
                        }
                    }
                }
                let virtual_expr = static_meth_call(
                    arena,
                    visitor_name,
                    et::SHAPE_TYPE,
                    &[Expr(
                        (),
                        pos,
                        Expr_::Shape(slice(arena, &virtual_shape_fields)),
                    )],
                    pos,
                );
                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_SHAPE,
                    &[pos_expr, vec_literal(arena, &desugar_shape_fields)],
                    pos,
                );
                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            Expr_::Is(_) => {
                self.errors
                    .push((pos, "`is` is not supported in expression trees.".into()));
                unchanged_result()
            }
            // Source: MyDsl`MyClass::FOO`
            // Virtualized: ${MyClass::FOO}
            // Desugared: $0v->visitClassConstant(new ExprPos(...), nameof MyClass, 'FOO', MyClass::FOO)
            Expr_::ClassConst(cc) => {
                let (cid, s) = cc;
                let is_supported = match cid {
                    ClassId(_, _, ClassId_::CIself | ClassId_::CIparent) => false,
                    cid if is_parent(cid) => false,
                    ClassId(_, _, ClassId_::CIreified(_)) => true,
                    ClassId(_, _, ClassId_::CIexpr(Expr(_, _, Expr_::Id(_)))) => true,
                    _ => false,
                };

                if !is_supported {
                    self.errors.push((
                        pos,
                        "Expression trees only support constants on `static` and named classes."
                            .into(),
                    ));
                }

                let class_const = Expr((), pos, Expr_::ClassConst(cc));

                let virtual_expr = Expr(
                    (),
                    pos,
                    Expr_::ETSplice(arena.alloc(EtSplice {
                        spliced_expr: static_meth_call(
                            arena,
                            visitor_name,
                            et::LIFT,
                            &[class_const],
                            pos,
                        ),
                        extract_client_type: true,
                        contains_await: false,
                        macro_variables: None,
                        temp_lid: None,
                    })),
                );

                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_CLASS_CONSTANT,
                    &[
                        pos_expr,
                        Expr(
                            (),
                            pos,
                            Expr_::Nameof(arena.alloc(aast_copy::class_id(arena.bump, cid))),
                        ),
                        string_literal(pos, s.1),
                        aast_copy::expr(arena.bump, &class_const),
                    ],
                    pos,
                );

                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            Expr_::Efun(_) => {
                self.errors.push((
                pos,
                "Expression trees do not support PHP lambdas. Consider using Hack lambdas `() ==> {}` instead.".into(),
            ));
                unchanged_result()
            }
            Expr_::ExpressionTree(_) => {
                self.errors.push((
                    pos,
                    "Expression trees may not be used directly inside of other expression trees."
                        .into(),
                ));
                unchanged_result()
            }
            Expr_::Yield(_) => {
                self.errors
                    .push((pos, "`yield` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::ValCollection(&(kind, targ, values)) => match &kind.1 {
                VcKind::Vector | VcKind::ImmVector => {
                    self.errors.push((
                        pos,
                        "PHP vectors are not supported in expression trees.".into(),
                    ));
                    unchanged_result()
                }
                VcKind::Set | VcKind::ImmSet => {
                    self.errors.push((
                        pos,
                        "PHP sets are not supported in expression trees.".into(),
                    ));
                    unchanged_result()
                }
                VcKind::Keyset => {
                    if targ.is_some() {
                        self.errors.push((
                            pos,
                            "`keyset` in expression trees must not have explicit type hints".into(),
                        ));
                    }

                    // Source: MyDsl`keyset[expr1, expr2, ...]`
                    // Virtualized: MyDsl::keysetType(virtualized(expr1), virtualized(expr2), ...)
                    // Desugared: $0v->visitKeyset(new ExprPos(...), vec[desugared(expr1), desugared(expr2), ...])
                    let mut virtual_values = vec![];
                    let mut desugar_values = vec![];
                    for value in values {
                        let rewritten = self.rewrite_expr(*value, visitor_name);
                        virtual_values.push(rewritten.virtual_expr);
                        desugar_values.push(rewritten.desugar_expr);
                    }
                    let virtual_expr = static_meth_call(
                        arena,
                        visitor_name,
                        et::KEYSET_TYPE,
                        &virtual_values,
                        pos,
                    );
                    let desugar_expr = v_meth_call(
                        arena,
                        et::VISIT_KEYSET,
                        &[pos_expr, vec_literal(arena, &desugar_values)],
                        pos,
                    );
                    RewriteResult {
                        virtual_expr,
                        desugar_expr,
                    }
                }
                VcKind::Vec => {
                    if targ.is_some() {
                        self.errors.push((
                            pos,
                            "`vec` in expression trees must not have explicit type hints".into(),
                        ));
                    }

                    // Source: MyDsl`vec[expr1, expr2, ...]`
                    // Virtualized: MyDsl::vecType(virtualized(expr1), virtualized(expr2), ...)
                    // Desugared: $0v->visitVec(new ExprPos(...), vec[desugared(expr1), desugared(expr2), ...])
                    let mut virtual_values = vec![];
                    let mut desugar_values = vec![];
                    for value in values {
                        let rewritten = self.rewrite_expr(*value, visitor_name);
                        virtual_values.push(rewritten.virtual_expr);
                        desugar_values.push(rewritten.desugar_expr);
                    }
                    let virtual_expr =
                        static_meth_call(arena, visitor_name, et::VEC_TYPE, &virtual_values, pos);
                    let desugar_expr = v_meth_call(
                        arena,
                        et::VISIT_VEC,
                        &[pos_expr, vec_literal(arena, &desugar_values)],
                        pos,
                    );
                    RewriteResult {
                        virtual_expr,
                        desugar_expr,
                    }
                }
            },
            Expr_::KeyValCollection(&(kind, targ, fields)) => match &kind.1 {
                KvcKind::Map | KvcKind::ImmMap => {
                    self.errors.push((
                        pos,
                        "PHP maps are not supported in expression trees.".into(),
                    ));
                    unchanged_result()
                }
                KvcKind::Dict => {
                    if targ.is_some() {
                        self.errors.push((
                            pos,
                            "`dict` in expression trees must not have explicit type hints".into(),
                        ));
                    }

                    // Source: MyDsl`dict['key1' => val1, 'key2' => val2]`
                    // Virtualized: MyDsl::dictType(tuple(virtualized('key1'), virtualized(val1)), ...)
                    // Desugared: $0v->visitDict(new ExprPos(...), vec[tuple(desugared('key1'), desugared(val1)), ...])
                    let mut virtual_fields = vec![];
                    let mut desugar_fields = vec![];
                    for &Field(key, value) in fields {
                        let key_rewr = self.rewrite_expr(key, visitor_name);
                        let val_rewr = self.rewrite_expr(value, visitor_name);
                        let kv_pos =
                            match Pos::merge(&key_rewr.virtual_expr.1, &val_rewr.virtual_expr.1) {
                                Some(p) => p,
                                _ => key_rewr.virtual_expr.1,
                            };
                        virtual_fields.push(Expr(
                            (),
                            pos,
                            Expr_::Tuple(slice(
                                arena,
                                &[key_rewr.virtual_expr, val_rewr.virtual_expr],
                            )),
                        ));
                        desugar_fields.push(Expr(
                            (),
                            kv_pos,
                            Expr_::Tuple(slice(
                                arena,
                                &[key_rewr.desugar_expr, val_rewr.desugar_expr],
                            )),
                        ));
                    }
                    let virtual_expr =
                        static_meth_call(arena, visitor_name, et::DICT_TYPE, &virtual_fields, pos);
                    let desugar_expr = v_meth_call(
                        arena,
                        et::VISIT_DICT,
                        &[pos_expr, vec_literal(arena, &desugar_fields)],
                        pos,
                    );
                    RewriteResult {
                        virtual_expr,
                        desugar_expr,
                    }
                }
            },
            Expr_::This => {
                self.errors
                    .push((pos, "`$this` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Dollardollar(_) => {
                self.errors
                    .push((pos, "`$$` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Clone(_) => {
                self.errors
                    .push((pos, "`clone` is not supported in expression trees.".into()));
                unchanged_result()
            }
            // Source: MyDsl`$container[$index]`
            // Virtualized: $container[$index]
            // Desugared: $0v->visitSubscript(new ExprPos(...), container, index)
            Expr_::ArrayGet(ag) => {
                let (container_expr, index_expr_opt) = *ag;

                let index_expr = match index_expr_opt {
                    Some(idx) => idx,
                    None => {
                        self.errors.push((
                            pos,
                            "`$arr[]` is append syntax and is currently unsupported in expression trees. Use `$arr[$idx]` for element access."
                                .into(),
                        ));
                        return unchanged_result();
                    }
                };

                let rewritten_container = self.rewrite_expr(container_expr, visitor_name);
                let rewritten_index = self.rewrite_subscript_index(index_expr, visitor_name);

                let virtual_expr = Expr(
                    (),
                    pos,
                    Expr_::ArrayGet(arena.alloc((
                        rewritten_container.virtual_expr,
                        Some(rewritten_index.virtual_expr),
                    ))),
                );

                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_SUBSCRIPT,
                    &[
                        pos_expr,
                        rewritten_container.desugar_expr,
                        rewritten_index.desugar_expr,
                    ],
                    pos,
                );

                RewriteResult {
                    virtual_expr,
                    desugar_expr,
                }
            }
            Expr_::FunctionPointer(_) => {
                self.errors.push((
                    pos,
                    "Function pointers are not supported in expression trees.".into(),
                ));
                unchanged_result()
            }
            Expr_::String2(_) => {
                self.errors.push((
                    pos,
                    "String interpolation is not supported in expression trees.".into(),
                ));
                unchanged_result()
            }
            Expr_::Await(_) => {
                self.errors
                    .push((pos, "`await` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Delay(_) => {
                self.errors
                    .push((pos, "`delay` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::ReadonlyExpr(_) => {
                self.errors.push((
                    pos,
                    "`readonly` is not supported in expression trees.".into(),
                ));
                unchanged_result()
            }
            Expr_::Tuple(_) => {
                self.errors
                    .push((pos, "Tuples are not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::List(_) => {
                self.errors
                    .push((pos, "Lists are not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Pair(_) => {
                self.errors
                    .push((pos, "Pairs are not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Cast(_) => {
                self.errors
                    .push((pos, "Casts are not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Pipe(_) => {
                self.errors.push((
                    pos,
                    "The pipe operator `|>` is not supported in expression trees.".into(),
                ));
                unchanged_result()
            }
            Expr_::As(_) => {
                self.errors
                    .push((pos, "`as` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::New(_) => {
                self.errors
                    .push((pos, "`new` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::EnumClassLabel(_) => {
                self.errors.push((
                    pos,
                    "Enum class labels are not supported in expression trees.".into(),
                ));
                unchanged_result()
            }
            Expr_::Nameof(_) => {
                self.errors
                    .push((pos, "`nameof` is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Id(_) => {
                self.errors.push((
                    pos,
                    "Global constants are not supported in expression trees.".into(),
                ));
                unchanged_result()
            }
            Expr_::Xml(_) => {
                self.errors
                    .push((pos, "xhp is not supported in expression trees.".into()));
                unchanged_result()
            }
            Expr_::Omitted
            | Expr_::Invalid(_)
            | Expr_::ClassGet(_)
            | Expr_::PrefixedString(_)
            | Expr_::Upcast(_)
            | Expr_::Lplaceholder(_)
            | Expr_::MethodCaller(_)
            | Expr_::Import(_)
            | Expr_::Hole(_)
            | Expr_::Package(_)
            | Expr_::DestructureShape(_)
            | Expr_::DestructureTuple(_) => {
                self.errors
                    .push((pos, "Unsupported expression tree syntax.".into()));
                unchanged_result()
            }
        }
    }

    fn rewrite_exprs(
        &mut self,
        exprs: &[Expr<'a>],
        visitor_name: &'a str,
    ) -> (Vec<Expr<'a>>, Vec<Expr<'a>>) {
        let mut virtual_results = Vec::with_capacity(exprs.len());
        let mut desugar_results = Vec::with_capacity(exprs.len());
        for expr in exprs {
            let rewritten_expr = self.rewrite_expr(*expr, visitor_name);
            virtual_results.push(rewritten_expr.virtual_expr);
            desugar_results.push(rewritten_expr.desugar_expr);
        }
        (virtual_results, desugar_results)
    }

    fn rewrite_stmts(
        &mut self,
        stmts: &[Stmt<'a>],
        visitor_name: &'a str,
    ) -> (Vec<Stmt<'a>>, Vec<Expr<'a>>) {
        let mut virtual_results = Vec::with_capacity(stmts.len());
        let mut desugar_results = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            let (virtual_stmt, desugared_expr) = self.rewrite_stmt(*stmt, visitor_name);
            virtual_results.push(virtual_stmt);
            if let Some(desugared_expr) = desugared_expr {
                desugar_results.push(desugared_expr);
            }
        }
        (virtual_results, desugar_results)
    }

    fn rewrite_stmt(&mut self, s: Stmt<'a>, visitor_name: &'a str) -> (Stmt<'a>, Option<Expr<'a>>) {
        let arena = self.arena;

        let unchanged_result = (s, None);

        let Stmt(pos, stmt_) = s;
        let pos_expr = exprpos(arena, pos);

        match stmt_ {
            Stmt_::Expr(e) => {
                let result = self.rewrite_expr(*e, visitor_name);
                (
                    Stmt(pos, Stmt_::Expr(arena.alloc(result.virtual_expr))),
                    Some(result.desugar_expr),
                )
            }
            Stmt_::Return(e) => match *e {
                // Source: MyDsl`return ...;`
                // Virtualized: return ...;
                // Desugared: $0v->visitReturn(new ExprPos(...), $0v->...)
                Some(e) => {
                    let result = self.rewrite_expr(e, visitor_name);
                    let desugar_expr = v_meth_call(
                        arena,
                        et::VISIT_RETURN,
                        &[pos_expr, result.desugar_expr],
                        pos,
                    );
                    let virtual_stmt =
                        Stmt(pos, Stmt_::Return(arena.alloc(Some(result.virtual_expr))));
                    (virtual_stmt, Some(desugar_expr))
                }
                // Source: MyDsl`return;`
                // Virtualized: return MyDsl::voidType();
                // Desugared: $0v->visitReturn(new ExprPos(...), null)
                None => {
                    let desugar_expr =
                        v_meth_call(arena, et::VISIT_RETURN, &[pos_expr, null_literal(pos)], pos);

                    let virtual_void_expr =
                        static_meth_call(arena, visitor_name, et::VOID_TYPE, &[], pos);
                    let virtual_stmt =
                        Stmt(pos, Stmt_::Return(arena.alloc(Some(virtual_void_expr))));
                    (virtual_stmt, Some(desugar_expr))
                }
            },
            // Source: MyDsl`if (...) {...} else {...}`
            // Virtualized: if (MyDsl::operationType(...)->__bool())) {...} else {...}
            // Desugared: $0v->visitIf(new ExprPos(...), $0v->..., vec[...], vec[...])
            Stmt_::If(if_stmt) => {
                let (cond_expr, then_block, else_block) = *if_stmt;

                let rewritten_cond = self.rewrite_expr(cond_expr, visitor_name);
                let (virtual_then_stmts, desugar_then) =
                    self.rewrite_stmts(then_block.0, visitor_name);
                let (virtual_else_stmts, desugar_else) =
                    self.rewrite_stmts(else_block.0, visitor_name);

                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_IF,
                    &[
                        pos_expr,
                        rewritten_cond.desugar_expr,
                        vec_literal(arena, &desugar_then),
                        vec_literal(arena, &desugar_else),
                    ],
                    pos,
                );
                let virtual_stmt = Stmt(
                    pos,
                    Stmt_::If(arena.alloc((
                        boolify(arena, rewritten_cond.virtual_expr, visitor_name),
                        Block(slice(arena, &virtual_then_stmts)),
                        Block(slice(arena, &virtual_else_stmts)),
                    ))),
                );
                (virtual_stmt, Some(desugar_expr))
            }
            // Source: MyDsl`while (...) {...}`
            // Virtualized: while (MyDsl::opertationType(...)->__bool()) {...}
            // Desugared: $0v->visitWhile(new ExprPos(...), $0v->..., vec[...])
            Stmt_::While(w) => {
                let (LoopCond(_, _, cond), body) = *w;

                let rewritten_cond = self.rewrite_expr(cond, visitor_name);
                let (virtual_body_stmts, desugar_body) = self.rewrite_stmts(body.0, visitor_name);

                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_WHILE,
                    &[
                        pos_expr,
                        rewritten_cond.desugar_expr,
                        vec_literal(arena, &desugar_body),
                    ],
                    pos,
                );
                let virtual_stmt = Stmt(
                    pos,
                    Stmt_::While(arena.alloc((
                        LoopCond(
                            None,
                            None,
                            boolify(arena, rewritten_cond.virtual_expr, visitor_name),
                        ),
                        Block(slice(arena, &virtual_body_stmts)),
                    ))),
                );
                (virtual_stmt, Some(desugar_expr))
            }
            // Source: MyDsl`for (...; ...; ...) {...}`
            // Virtualized: for (...; MyDsl::operationType(...)->__bool(); ...) {...}
            // Desugared: $0v->visitFor(new ExprPos(...), vec[...], ..., vec[...], vec[...])
            Stmt_::For(w) => {
                let (init, cond, LoopIter(_, _, incr), body) = *w;

                let (virtual_init_exprs, desugar_init_exprs) =
                    self.rewrite_exprs(init, visitor_name);

                let (virtual_cond_option, desugar_cond_expr) = match cond {
                    Some(LoopCond(_, _, cond)) => {
                        let rewritten_cond = self.rewrite_expr(cond, visitor_name);
                        (
                            Some(LoopCond(
                                None,
                                None,
                                boolify(arena, rewritten_cond.virtual_expr, visitor_name),
                            )),
                            rewritten_cond.desugar_expr,
                        )
                    }
                    None => (None, null_literal(pos)),
                };

                let (virtual_incr_exprs, desugar_incr_exprs) =
                    self.rewrite_exprs(incr, visitor_name);

                let (virtual_body_stmts, desugar_body) = self.rewrite_stmts(body.0, visitor_name);

                let desugar_expr = v_meth_call(
                    arena,
                    et::VISIT_FOR,
                    &[
                        pos_expr,
                        vec_literal(arena, &desugar_init_exprs),
                        desugar_cond_expr,
                        vec_literal(arena, &desugar_incr_exprs),
                        vec_literal(arena, &desugar_body),
                    ],
                    pos,
                );
                let virtual_stmt = Stmt(
                    pos,
                    Stmt_::For(arena.alloc((
                        slice(arena, &virtual_init_exprs),
                        virtual_cond_option,
                        LoopIter(None, None, slice(arena, &virtual_incr_exprs)),
                        Block(slice(arena, &virtual_body_stmts)),
                    ))),
                );
                (virtual_stmt, Some(desugar_expr))
            }
            // Source: MyDsl`break;`
            // Virtualized: break;
            // Desugared: $0v->visitBreak(new ExprPos(...))
            Stmt_::Break => {
                let desugar_expr = v_meth_call(arena, et::VISIT_BREAK, &[pos_expr], pos);
                let virtual_stmt = Stmt(pos, Stmt_::Break);
                (virtual_stmt, Some(desugar_expr))
            }
            // Source: MyDsl`continue;`
            // Virtualized: continue;
            // Desugared: $0v->visitContinue(new ExprPos(...))
            Stmt_::Continue => {
                let desugar_expr = v_meth_call(arena, et::VISIT_CONTINUE, &[pos_expr], pos);
                let virtual_stmt = Stmt(pos, Stmt_::Continue);
                (virtual_stmt, Some(desugar_expr))
            }
            Stmt_::Noop => (Stmt(pos, Stmt_::Noop), None),
            // Unsupported operators
            Stmt_::Do(_) => {
                self.errors.push((
                pos,
                "Expression trees do not support `do while` loops. Consider using a `while` loop instead.".into(),
            ));
                unchanged_result
            }
            Stmt_::Switch(_) => {
                self.errors.push((
                pos,
                "Expression trees do not support `switch` statements. Consider using `if`/`else if`/`else` instead.".into(),
            ));
                unchanged_result
            }
            Stmt_::Foreach(_) => {
                self.errors.push((
                pos,
                "Expression trees do not support `foreach` loops. Consider using a `for` loop or a `while` loop instead.".into(),
            ));
                unchanged_result
            }
            _ => {
                self.errors.push((
                    pos,
                    "Expression trees do not support this statement syntax.".into(),
                ));
                unchanged_result
            }
        }
    }

    // Collect a nested ArrayGet chain from an LHS expression into root + vec
    // of (pos, key) pairs. Returns (root, chain) where chain is in
    // root-to-leaf order. Each key is Option<Expr> -- None for append syntax.
    fn collect_subscript_chain(lhs: Expr<'a>) -> (Expr<'a>, Vec<(Pos, Option<Expr<'a>>)>) {
        let mut chain = Vec::new();
        let mut current = lhs;
        while let Expr(_, ag_pos, Expr_::ArrayGet(ag)) = current {
            let (container, index) = *ag;
            chain.push((ag_pos, index));
            current = container;
        }
        chain.reverse();
        (current, chain)
    }
}

fn immediately_invoked_lambda<'a>(
    arena: Arena<'a>,
    async_: bool,
    pos: Pos,
    stmts: &[Stmt<'a>],
    captured_arguments: &[((&'a str, Pos), Expr<'a>)],
) -> Expr<'a> {
    let fun_params =
        arena.alloc_slice_fill_iter(captured_arguments.iter().map(|&((name, pos), _)| FunParam {
            annotation: (),
            type_hint: TypeHint((), None),
            pos,
            name,
            info: FunParamInfo::ParamRequired,
            callconv: ParamKind::Pnormal,
            readonly: None,
            splat: None,
            named: None,
            user_attributes: Default::default(),
            visibility: None,
        }));

    let call_args = arena.alloc_slice_fill_iter(
        captured_arguments
            .iter()
            .map(|(_, e)| Argument::Anormal(*e)),
    );

    let func_body = FuncBody {
        fb_ast: Block(slice(arena, stmts)),
    };
    let fun_ = wrap_fun_(async_, func_body, fun_params, pos);
    let lambda_expr = Expr((), pos, mk_lfun(arena, fun_));

    let call = Expr(
        (),
        pos,
        Expr_::Call(arena.alloc(CallExpr {
            func: lambda_expr,
            targs: &[],
            args: call_args,
            unpacked_arg: None,
        })),
    );
    if async_ {
        Expr((), pos, Expr_::Await(arena.alloc(call)))
    } else {
        call
    }
}

/// Is this `parent`? The tree keeps `parent` on the left of `::` as the
/// identifier `parent` rather than as [`ClassId_::CIparent`].
fn is_parent(cid: &ClassId<'_>) -> bool {
    matches!(
        cid.2,
        ClassId_::CIexpr(Expr(_, _, Expr_::Id(Id(_, "parent"))))
    )
}

/// Is this is a typechecker pseudo function like `hh_show` that
/// shouldn't be desugared?
fn is_typechecker_fun_name(name: &str) -> bool {
    strip_ns(name) == strip_ns(pseudo_functions::HH_SHOW)
        || strip_ns(name) == strip_ns(pseudo_functions::HH_EXPECT)
        || strip_ns(name) == strip_ns(pseudo_functions::HH_EXPECT_EQUIVALENT)
        || strip_ns(name) == strip_ns(pseudo_functions::HH_SHOW_ENV)
}

fn strip_ns(name: &str) -> &str {
    match name.chars().next() {
        Some('\\') => &name[1..],
        _ => name,
    }
}

fn _virtualize_call<'a>(arena: Arena<'a>, e: Expr<'a>, nullsafe: bool, pos: Pos) -> Expr<'a> {
    meth_call(arena, e, "__unwrap", &[], nullsafe, pos)
}

fn _virtualize_lambda<'a>(
    arena: Arena<'a>,
    visitor_name: &'a str,
    e: Expr<'a>,
    pos: Pos,
) -> Expr<'a> {
    static_meth_call(arena, visitor_name, et::LAMBDA_TYPE, &[e], pos)
}

/// Return a shape literal that describes the values inside this
/// expression tree literal. For example, given the expression tree:
///
/// ```text
/// $et = Code`${ $x } + foo() + Bar::baz()`;
/// ```
///
/// The metadata is:
///
/// ```text
/// shape(
///   // Simplified: We actually use a temporary variable whose value is $x.
///   'splices' => dict['$0splice0' => $x],
///
///   'functions' => vec[foo<>],
///   'static_methods' => vec[Bar::baz<>],
///   'variables' => vec[...],
///   'lexically_enclosing_tree' => ...,
///   'type' => ...,
/// )
/// ```
#[allow(clippy::too_many_arguments)]
fn maketree_metadata<'a>(
    arena: Arena<'a>,
    pos: Pos,
    splices: &[Expr<'a>],
    functions: &[Expr<'a>],
    static_methods: &[Expr<'a>],
    freevars: &BTreeMap<LocalId<'a>, Pos>,
    enclosing_et_pos: &Option<Pos>,
    virtual_expr: Expr<'a>,
) -> Expr<'a> {
    let key_value_pairs: Vec<_> = splices
        .iter()
        .enumerate()
        .map(|(i, expr)| {
            let key = Expr(
                (),
                expr.1,
                Expr_::String(BStr::new(temp_splice_lvar_string(arena, i))),
            );
            let value = temp_splice_lvar(arena, expr.1, i);
            (key, value)
        })
        .collect();
    let splices_dict = dict_literal(arena, pos, &key_value_pairs);

    let function_vars: Vec<_> = functions
        .iter()
        .enumerate()
        .map(|(i, expr)| temp_function_pointer_lvar(arena, expr.1, i))
        .collect();
    let functions_vec = vec_literal_with_pos(arena, pos, &function_vars);

    let static_method_vars: Vec<_> = static_methods
        .iter()
        .enumerate()
        .map(|(i, expr)| temp_static_method_lvar(arena, expr.1, i))
        .collect();
    let static_method_vec = vec_literal_with_pos(arena, pos, &static_method_vars);
    let freevars_exprs: Vec<_> = freevars
        .iter()
        .map(|((_, str), pos)| Expr((), *pos, Expr_::String(BStr::new(*str))))
        .collect();
    let freevars_vec = vec_literal_with_pos(arena, pos, &freevars_exprs);
    let fields = [
        ("splices", splices_dict),
        ("functions", functions_vec),
        ("static_methods", static_method_vec),
        ("variables", freevars_vec),
        (
            "lexically_enclosing_tree",
            exprpos_opt(arena, enclosing_et_pos),
        ),
        ("type", virtual_expr),
    ];
    shape_literal(arena, pos, &fields)
}

fn global_func_ptr<'a>(arena: Arena<'a>, sid: &Sid<'a>) -> Expr<'a> {
    let pos = sid.0;
    Expr(
        (),
        pos,
        mk_function_pointer(arena, FunctionPtrId::FPId(*sid)),
    )
}

fn static_meth_ptr<'a>(
    arena: Arena<'a>,
    pos: Pos,
    cid: &ClassId<'a>,
    meth: &Pstring<'a>,
) -> Expr<'a> {
    Expr(
        (),
        pos,
        mk_function_pointer(arena, FunctionPtrId::FPClassConst(*cid, *meth)),
    )
}

fn mk_visit_string<'a>(arena: Arena<'a>, pos: Pos, str: Expr_<'a>) -> Expr<'a> {
    v_meth_call(
        arena,
        et::VISIT_STRING,
        &[exprpos(arena, pos), Expr((), pos, str)],
        pos,
    )
}

// Live variable calculation, only applying to what is allowed in expression trees
struct LiveVars<'a> {
    // The variables that might be used in a block, not following an assignment
    // to that variable in the block
    used: BTreeMap<LocalId<'a>, Pos>,
    // The variables that are definitely assigned in a block
    assigned: BTreeMap<LocalId<'a>, Pos>,
}

impl<'a> LiveVars<'a> {
    // Compute the live variables at the entry to a list of statements,
    // assuming nothing is live after
    fn new_from_statement(stmts: &'a [Stmt<'a>]) -> Self {
        let mut lvs = LiveVars {
            used: BTreeMap::default(),
            assigned: BTreeMap::default(),
        };
        lvs.update_for_stmts(stmts);
        lvs
    }

    // Compute the live variables right before the given expression,
    // assuming that nothing is live after
    fn new_from_expression(expr: &'a Expr<'a>) -> Self {
        let mut lvs = LiveVars {
            used: BTreeMap::default(),
            assigned: BTreeMap::default(),
        };
        let Ok(()) = lvs.visit_expr(&mut (), expr);
        lvs
    }

    // Update self with the live variable information for the stmts preceding
    // it in control-flow order
    fn update_for_stmts(&mut self, stmts: &'a [Stmt<'a>]) {
        for &Stmt(_pos, stmt) in stmts.iter().rev() {
            match stmt {
                Stmt_::Expr(e) | Stmt_::Return(Some(e)) => {
                    let Ok(()) = self.visit_expr(&mut (), e);
                }
                Stmt_::If((cond, Block(then), Block(els))) => {
                    let then_lvs = Self::new_from_statement(then);
                    let els_lvs = Self::new_from_statement(els);
                    let combined_lvs = then_lvs.merge(els_lvs);
                    self.update_for_more_live_vars(combined_lvs);
                    let Ok(()) = self.visit_expr(&mut (), cond);
                }
                Stmt_::While((LoopCond(_, _, test), body)) => {
                    // We don't need to iterate to a fixed point, because we are
                    // only looking at the used variables on entry to the loop,
                    // rather than trying to use them at program points inside
                    // the loop. Any live variable at the start of the loop
                    // either comes from after the loop and it's not definitely
                    // assigned in the body, or it's used in the body, in which
                    // case it's used in the first iteration.

                    let mut body_lvs = Self::new_from_statement(body.0);
                    // The assigned vars don't matter because we might not enter the body
                    body_lvs.assigned = BTreeMap::default();
                    self.update_for_more_live_vars(body_lvs);
                    // We relay on there being no assignments in the expr
                    let Ok(()) = self.visit_expr(&mut (), test);
                }
                Stmt_::For((init, test, inc, body)) => {
                    let mut body_lvs = LiveVars {
                        used: BTreeMap::default(),
                        assigned: BTreeMap::default(),
                    };
                    for e in inc.2.iter().rev() {
                        let Ok(()) = body_lvs.visit_expr(&mut (), e);
                    }
                    body_lvs.update_for_stmts(body.0);
                    body_lvs.assigned = BTreeMap::default();
                    self.update_for_more_live_vars(body_lvs);
                    if let Some(LoopCond(_, _, e)) = test {
                        let Ok(()) = self.visit_expr(&mut (), e);
                    }
                    for e in init.iter().rev() {
                        let Ok(()) = self.visit_expr(&mut (), e);
                    }
                }
                Stmt_::Noop
                | Stmt_::Fallthrough
                | Stmt_::Break
                | Stmt_::Continue
                | Stmt_::Throw(_)
                | Stmt_::Return(_)
                | Stmt_::YieldBreak
                | Stmt_::Awaitall(_)
                | Stmt_::Concurrent(_)
                | Stmt_::Do(_)
                | Stmt_::Using(_)
                | Stmt_::Switch(_)
                | Stmt_::Match(_)
                | Stmt_::Foreach(_)
                | Stmt_::Try(_)
                | Stmt_::Block(_)
                | Stmt_::Markup(_) => {}
            }
        }
    }

    // Update self with the live variable info in update, assuming that self
    // follows update in the control flow
    fn update_for_more_live_vars(&mut self, update: Self) {
        for (v, pos) in update.assigned {
            self.used.remove(&v);
            self.assigned.insert(v, pos);
        }
        for (v, pos) in update.used {
            self.used.insert(v, pos);
        }
    }

    // Compute new live variable information where self and lvs come from two
    // branches of a control-flow split
    fn merge(self, lvs: Self) -> Self {
        let assigned = self
            .assigned
            .into_iter()
            .filter(|(v, _)| lvs.assigned.contains_key(v))
            .collect();
        let mut used = self.used;
        for (v, pos) in lvs.used {
            used.insert(v, pos);
        }
        LiveVars { assigned, used }
    }

    // Add an assignment of Lid to the live variable information. It is removed
    // from used, since the subsequent use will now see the assignment.
    fn add_assign(&mut self, lid: &Lid<'a>) {
        if lid.as_local_id().1 != "$this" {
            let loc = lid.as_local_id();
            self.used.remove(loc);
            self.assigned.insert(*loc, lid.0);
        }
    }

    fn add_use(&mut self, lid: &Lid<'a>) {
        if lid.as_local_id().1 != "$this" {
            self.used.insert(*lid.as_local_id(), lid.0);
        }
    }

    // Update the live variable info in self to reflect the assignment to an
    // lvalue
    fn visit_lvalue(&mut self, env: &mut (), Expr(_, _, e): &'a Expr<'a>) {
        match e {
            Expr_::List(lv) => lv.iter().for_each(|e| self.visit_lvalue(env, e)),
            Expr_::Lvar(lid) => self.add_assign(lid),
            _ => {
                let Ok(()) = self.visit_expr_(env, e);
            }
        }
    }
}

// Visit an expression, updating the live variable information assumed to follow
// the expression in control-flow order
impl<'a> Visitor<'a> for LiveVars<'a> {
    type Params = AstParams<(), Infallible>;

    fn object(&mut self) -> &mut dyn Visitor<'a, Params = Self::Params> {
        self
    }

    // We won't visit lids in lvalues because assignments get treated specially
    // in visit_expr_ (and the other lvalue position (foreach) isn't allowed in
    // ETs)
    fn visit_lid(&mut self, _env: &mut (), lid: &'a Lid<'a>) -> Result<(), Infallible> {
        self.add_use(lid);
        Ok(())
    }

    fn visit_expr_(&mut self, env: &mut (), e: &'a Expr_<'a>) -> Result<(), Infallible> {
        match e {
            Expr_::Assign((lhs, bop, rhs)) => {
                if bop.is_none() {
                    self.visit_lvalue(env, lhs);
                    rhs.recurse(env, self.object())
                } else {
                    lhs.recurse(env, self.object())?;
                    rhs.recurse(env, self.object())?;
                    self.visit_lvalue(env, lhs);
                    Ok(())
                }
            }
            Expr_::ExpressionTree(_) => Ok(()),
            // A splice only uses the variables it records; its expression
            // is evaluated outside the tree.
            Expr_::ETSplice(EtSplice {
                macro_variables, ..
            }) => {
                if let Some(vars) = macro_variables {
                    for v in vars.iter() {
                        self.add_use(v);
                    }
                }
                Ok(())
            }
            Expr_::Lfun((f, _idl)) => {
                // functions create a new scope
                let Block(stmts) = f.body.fb_ast;
                let mut lv = LiveVars::new_from_statement(stmts);
                for p in f.params {
                    lv.used.remove(&(0, p.name));
                }
                for p in f.params {
                    if let FunParamInfo::ParamOptional(Some(e)) = &p.info {
                        let Ok(()) = lv.visit_expr(&mut (), e);
                    }
                }
                for (v, pos) in lv.used {
                    self.used.insert(v, pos);
                }
                Ok(())
            }
            _ => e.recurse(env, self.object()),
        }
    }
}

/// Rewrite the args into virtual and desugared args, treating the second
/// virtual argument (if present) as a shape field and so using the source
/// rather than virtualized expression. Wrap the first virtual argument with
/// an ->unwrap() call.
fn rewrite_and_fixup_for_shape_op<'a>(
    state: &mut RewriteState<'a>,
    visitor_name: &'a str,
    pos: Pos,
    args: &[Expr<'a>],
    nullsafe: bool,
) -> (Vec<Expr<'a>>, Vec<Expr<'a>>) {
    let field_arg = if args.len() > 1 {
        Some(aast_copy::expr(state.arena.bump, &args[1]))
    } else {
        None
    };
    let (mut virtual_args, desugar_args) = state.rewrite_exprs(args, visitor_name);
    if !virtual_args.is_empty() {
        virtual_args[0] = _virtualize_call(state.arena, virtual_args[0], nullsafe, pos);
    }
    if let Some(arg) = field_arg {
        virtual_args[1] = arg;
    }
    (virtual_args, desugar_args)
}

/// Rewrite the args into virtual and desugared args. Create a virtual expression
/// that passes the virtual arguments to a call to symbolType(temp_variable)
fn handle_class_const<'a>(
    state: &mut RewriteState<'a>,
    visitor_name: &'a str,
    pos: Pos,
    args: &[Expr<'a>],
    temp_variable: Expr<'a>,
) -> (Expr<'a>, Vec<Expr<'a>>) {
    let (virtual_args, desugar_args) = state.rewrite_exprs(args, visitor_name);
    let virtual_expr =
        virtualize_function_pointer(state.arena, visitor_name, pos, &virtual_args, temp_variable);
    (virtual_expr, desugar_args)
}

fn virtualize_function_pointer<'a>(
    arena: Arena<'a>,
    visitor_name: &'a str,
    pos: Pos,
    virtual_args: &[Expr<'a>],
    temp_variable: Expr<'a>,
) -> Expr<'a> {
    Expr(
        (),
        pos,
        Expr_::Call(arena.alloc(CallExpr {
            func: _virtualize_call(
                arena,
                static_meth_call(arena, visitor_name, et::SYMBOL_TYPE, &[temp_variable], pos),
                false,
                pos,
            ),
            targs: &[],
            args: build_args(arena, virtual_args),
            unpacked_arg: None,
        })),
    )
}

/// During virtualization, handle_shapes_at unwraps the DSL Shape type into a Hack shape type,
/// and virtualizes the function call into a call from DSL::shapeAt into Shapes::at, to perform the type-checking.
///
/// Note that this isn't used for desugaring, so in the desugared code an actual method call to DSL::shapeAt is emitted.
fn handle_shapes_at<'a>(
    state: &mut RewriteState<'a>,
    visitor_name: &'a str,
    pos: Pos,
    args: &[Expr<'a>],
) -> (Expr<'a>, Vec<Expr<'a>>) {
    let (virtual_args, desugar_args) =
        rewrite_and_fixup_for_shape_op(state, visitor_name, pos, args, false);
    let virtual_expr = static_meth_call(state.arena, "Shapes", "at", &virtual_args, pos);
    (virtual_expr, desugar_args)
}

fn handle_shapes_idx<'a>(
    state: &mut RewriteState<'a>,
    visitor_name: &'a str,
    pos: Pos,
    args: &[Expr<'a>],
) -> (Expr<'a>, Vec<Expr<'a>>) {
    let (mut virtual_args, desugar_args) =
        rewrite_and_fixup_for_shape_op(state, visitor_name, pos, args, true);
    if virtual_args.len() == 2 {
        // We need the null type as additional argument
        virtual_args.push(null_literal(pos));
    }
    let virtual_expr = static_meth_call(state.arena, "Shapes", "idx", &virtual_args, pos);
    (virtual_expr, desugar_args)
}

fn handle_shapes_put<'a>(
    state: &mut RewriteState<'a>,
    visitor_name: &'a str,
    pos: Pos,
    args: &[Expr<'a>],
) -> (Expr<'a>, Vec<Expr<'a>>) {
    let (virtual_args, desugar_args) =
        rewrite_and_fixup_for_shape_op(state, visitor_name, pos, args, false);
    let virtual_expr = static_meth_call(
        state.arena,
        visitor_name,
        et::SHAPE_TYPE,
        &[static_meth_call(
            state.arena,
            "Shapes",
            "put",
            &virtual_args,
            pos,
        )],
        pos,
    );
    (virtual_expr, desugar_args)
}
