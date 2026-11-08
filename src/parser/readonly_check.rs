//! The readonly analysis HHVM runs over the AST of files that use `readonly`.
//!
//! The pass follows, function body by function body, which locals hold
//! readonly values, and reports the places where such a value is modified or
//! ends up somewhere that has to be mutable. Hakana has to report exactly
//! what HHVM reports, in the same order, so this is HHVM's
//! `hphp/hack/src/parser/readonly_check.rs` function for function, under the
//! same names and with its quirks intact. Keep it that way: the two files are
//! meant to be read side by side whenever HHVM's changes.
//!
//! One thing cannot be carried over as written. HHVM's pass rewrites the tree
//! while it checks it: an expression it finds readonly in certain positions
//! (an argument, the callee, either side of an assignment into a collection)
//! gets wrapped in `ReadonlyExpr`. Nothing downstream needs that here, and
//! this tree is immutable, but the pass itself goes on to look at what it
//! rewrote. Loop bodies are analysed more than once and an expression is
//! classified after its operands were visited, so a wrapped expression counts
//! as readonly from then on, even once the variable in it has been assigned
//! something mutable. The [`Checker`] therefore remembers which expressions
//! HHVM would have wrapped, and [`Checker::shape`] answers with what HHVM's
//! tree would hold in an expression's place.

// HHVM's phrasing is kept where clippy would put it differently.
#![allow(clippy::single_match, clippy::match_like_matches_macro)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::Infallible;

use crate::SyntaxError;
use crate::aast::Afield;
use crate::aast::Argument;
use crate::aast::AsExpr;
use crate::aast::Block;
use crate::aast::Bop;
use crate::aast::DestructureTarget;
use crate::aast::DestructureTarget_;
use crate::aast::Expr;
use crate::aast::Expr_;
use crate::aast::Fun_;
use crate::aast::FunDef;
use crate::aast::FunParamInfo;
use crate::aast::Hint_;
use crate::aast::Method_;
use crate::aast::ParamKind;
use crate::aast::Pos;
use crate::aast::Program;
use crate::aast::ReadonlyKind;
use crate::aast::ShapeExprField;
use crate::aast::Stmt;
use crate::aast::Stmt_;
use crate::aast::Uop;
use crate::aast::XhpSimple;
use crate::aast::local_id;
use crate::aast_visitor::AstParams;
use crate::aast_visitor::Node;
use crate::aast_visitor::Visitor;
use crate::aast_visitor::visit;

// The messages, as in HHVM's `syntax_error.rs`.
const ASSIGNMENT_TO_READONLY: &str = "This expression is readonly, its members cannot be modified";
const ASSIGN_READONLY_TO_MUTABLE_COLLECTION: &str = "This expression is readonly, but the collection you're modifying is mutable, which isn't allowed.";
const INOUT_READONLY_ASSIGNMENT: &str = "Cannot write a readonly value to an inout parameter";
const INOUT_READONLY_ARGUMENT: &str = "This expression is readonly. We currently do not support passing readonly values to an inout parameter.";
const YIELD_READONLY: &str =
    "This expression is readonly. We currently do not support yielding readonly values.";
const THROW_READONLY_EXCEPTION: &str =
    "This exception is readonly; throwing readonly exceptions is not currently supported.";
const READONLY_ON_XHP: &str = "Readonly values cannot be written to xhp attributes at this time.";

fn invalid_readonly(r1: &str, r2: &str, reason: &str) -> String {
    format!("This expression is {r1}, but we expected a {r2} value because {reason}")
}

/// The type names HHVM's `typehints::PRIMITIVE_TYPEHINTS` lists.
const PRIMITIVE_TYPEHINTS: [&str; 9] = [
    "null", "void", "int", "bool", "float", "resource", "num", "arraykey", "noreturn",
];

/// HHVM's `user_attributes::ignore_readonly_local_errors`.
fn ignore_readonly_local_errors(name: &str) -> bool {
    name == "__IgnoreReadonlyLocalErrors"
}

/// What `++` and `--` assign, as far as this analysis is concerned: any
/// expression that is not readonly.
static IMPLICIT_RHS: Expr<'static> = Expr((), Pos::NONE, Expr_::Omitted);

// Local environment which keeps track of how many readonly values it has
#[derive(Clone, Default)]
struct Lenv<'a> {
    lenv: HashMap<&'a str, Rty>,
    num_readonly: u32,
}

impl<'a> Lenv<'a> {
    fn insert(&mut self, var_name: &'a str, rty: Rty) {
        let result = self.lenv.insert(var_name, rty);
        match (rty, result) {
            (Rty::Readonly, Some(Rty::Mutable)) => {
                self.num_readonly += 1;
            }
            (Rty::Readonly, None) => {
                self.num_readonly += 1;
            }
            (Rty::Mutable, Some(Rty::Readonly)) => {
                self.num_readonly -= 1;
            }
            _ => {} // otherwise number of readonly values does not change
        }
    }

    fn get(&self, var_name: &str) -> Option<&Rty> {
        self.lenv.get(var_name)
    }
}

#[derive(PartialEq, Copy, Clone)]
enum Rty {
    Readonly,
    Mutable,
}

struct Context<'a> {
    locals: Lenv<'a>,
    readonly_return: Rty,
    this_ty: Rty,
    inout_params: HashSet<&'a str>,
}

impl<'a> Context<'a> {
    fn new(readonly_ret: Rty, this_ty: Rty) -> Self {
        Self {
            locals: Lenv::default(),
            readonly_return: readonly_ret,
            this_ty,
            inout_params: HashSet::default(),
        }
    }

    fn add_local(&mut self, var_name: &'a str, rty: Rty) {
        self.locals.insert(var_name, rty);
    }

    fn add_param(&mut self, var_name: &'a str, rty: Rty, inout_param: bool) {
        self.locals.insert(var_name, rty);
        if inout_param {
            self.inout_params.insert(var_name);
        }
    }

    fn get_rty(&self, var_name: &str) -> Rty {
        match self.locals.get(var_name) {
            Some(&x) => x,
            None => Rty::Mutable,
        }
    }
}

fn ro_expr_list<'a>(
    context: &mut Context<'a>,
    checker: &Checker<'a>,
    exprs: &'a [Expr<'a>],
) -> Rty {
    if exprs
        .iter()
        .any(|e| rty_expr(context, checker, e) == Rty::Readonly)
    {
        Rty::Readonly
    } else {
        Rty::Mutable
    }
}

fn ro_kind_to_rty(ro: Option<ReadonlyKind>) -> Rty {
    match ro {
        Some(ReadonlyKind::Readonly) => Rty::Readonly,
        _ => Rty::Mutable,
    }
}

fn rty_expr<'a>(context: &mut Context<'a>, checker: &Checker<'a>, expr: &'a Expr<'a>) -> Rty {
    use Expr_::*;
    match checker.shape(expr) {
        ReadonlyExpr(_) => Rty::Readonly,
        ObjGet(og) => {
            let (obj, _member_name, _null_flavor, _reffiness) = og;
            rty_expr(context, checker, obj)
        }
        Lvar(id_orig) => {
            let var_name = local_id::get_name(&id_orig.1);
            let is_this = var_name == "$this";
            if is_this {
                context.this_ty
            } else {
                context.get_rty(var_name)
            }
        }
        Shape(fields) => {
            if fields.iter().any(|f| match f {
                ShapeExprField::SFField(_, e) => rty_expr(context, checker, e) == Rty::Readonly,
                ShapeExprField::SFSplat(e) => rty_expr(context, checker, e) == Rty::Readonly,
            }) {
                Rty::Readonly
            } else {
                Rty::Mutable
            }
        }
        ValCollection(v) => {
            let (_, _, exprs) = v;
            ro_expr_list(context, checker, exprs)
        }
        KeyValCollection(kv) => {
            let (_, _, fields) = kv;
            if fields
                .iter()
                .any(|f| rty_expr(context, checker, &f.1) == Rty::Readonly)
            {
                Rty::Readonly
            } else {
                Rty::Mutable
            }
        }
        Collection(c) => {
            let (_, _, fields) = c;
            if fields.iter().any(|f| match f {
                Afield::AFvalue(e) => rty_expr(context, checker, e) == Rty::Readonly,
                Afield::AFkvalue(_, e) => rty_expr(context, checker, e) == Rty::Readonly,
            }) {
                Rty::Readonly
            } else {
                Rty::Mutable
            }
        }
        Xml(_) | Efun(_) | Lfun(_) => Rty::Mutable,
        Tuple(t) => ro_expr_list(context, checker, t),
        // Only list destructuring
        List(_) => Rty::Mutable,
        // Boolean statement always mutable
        Is(_) => Rty::Mutable,
        As(a) => {
            // Readonlyness of inner expression
            match a.hint.1 {
                // Primitives are always mutable. The tree has no primitive
                // hints of its own at this stage, so look at the name.
                Hint_::Happly(cn, _)
                    if PRIMITIVE_TYPEHINTS.contains(&cn.name()) || cn.name() == "string" =>
                {
                    Rty::Mutable
                }
                _ => rty_expr(context, checker, &a.expr),
            }
        }
        Upcast(a) => {
            // Readonlyness of inner expression
            let (exp, _) = a;
            rty_expr(context, checker, exp)
        }
        Eif(e) => {
            // $x ? a : b is readonly if either a or b are readonly
            let (cond, exp1_opt, exp2) = e;
            if let Some(exp1) = exp1_opt {
                match (
                    rty_expr(context, checker, exp1),
                    rty_expr(context, checker, exp2),
                ) {
                    (_, Rty::Readonly) | (Rty::Readonly, _) => Rty::Readonly,
                    (Rty::Mutable, Rty::Mutable) => Rty::Mutable,
                }
            } else {
                // Elvis operator: cond ?: exp2
                // Result is cond if truthy, exp2 if falsy
                // So readonly if either cond or exp2 is readonly
                match (
                    rty_expr(context, checker, cond),
                    rty_expr(context, checker, exp2),
                ) {
                    (_, Rty::Readonly) | (Rty::Readonly, _) => Rty::Readonly,
                    (Rty::Mutable, Rty::Mutable) => Rty::Mutable,
                }
            }
        }
        Pair(p) => {
            let (_, exp1, exp2) = p;
            match (
                rty_expr(context, checker, exp1),
                rty_expr(context, checker, exp2),
            ) {
                (_, Rty::Readonly) | (Rty::Readonly, _) => Rty::Readonly,
                (Rty::Mutable, Rty::Mutable) => Rty::Mutable,
            }
        }
        Hole(h) => {
            let (expr, _, _, _) = h;
            rty_expr(context, checker, expr)
        }
        Invalid(_e) => Rty::Mutable,
        Cast(_) => Rty::Mutable, // Casts are only valid on primitive types, so its always mutable
        New(_) => Rty::Mutable,
        This => context.this_ty,
        ArrayGet(ag) => {
            let (expr, _) = ag;
            rty_expr(context, checker, expr)
        }
        Await(expr) => rty_expr(context, checker, expr),
        Delay(expr) => rty_expr(context, checker, expr),
        // Primitive types are mutable
        Null | True | False | Omitted => Rty::Mutable,
        Int(_) | Float(_) | String(_) | String2(_) | PrefixedString(_) | Nameof(_) => Rty::Mutable,
        Id(_) => Rty::Mutable,
        Dollardollar(lid) => {
            // The lowering does not produce this node (`$$` is an `Lvar`
            // named `$$`, which is what a pipe records its left-hand side
            // as); this is what HHVM does with it all the same.
            let (id, dollardollar) = lid.1;
            let var_name = format!("{dollardollar}{id}");
            context.get_rty(&var_name)
        }
        Clone(e) => {
            // Clone only clones shallowly, so we need to respect readonly even if you clone it
            rty_expr(context, checker, e)
        }
        Call(c) => {
            if let Id(i) = checker.shape(&c.func) {
                if is_special_builtin(i.1) && !c.args.is_empty() {
                    // Take first argument
                    let expr = c.args[0].to_expr_ref();
                    rty_expr(context, checker, expr)
                } else {
                    Rty::Mutable
                }
            } else {
                Rty::Mutable
            }
        }
        // Mutable unless wrapped in a readonly expression
        ClassGet(_) | ClassConst(_) => Rty::Mutable,
        FunctionPointer(_) => Rty::Mutable,
        // This is really just a statement, does not have a value
        Yield(_) => Rty::Mutable,
        Binop(b) => {
            match b.bop {
                Bop::QuestionQuestion => {
                    match (
                        rty_expr(context, checker, &b.lhs),
                        rty_expr(context, checker, &b.rhs),
                    ) {
                        (Rty::Readonly, _) | (_, Rty::Readonly) => Rty::Readonly,
                        (Rty::Mutable, Rty::Mutable) => Rty::Mutable,
                    }
                }
                // All other operators are all primitive in result
                _ => Rty::Mutable,
            }
        }
        Assign(_) => Rty::Mutable,
        // Unary operators are all primitive in result
        Unop(_) => Rty::Mutable,
        Pipe(p) => {
            let (lid, left, right, _) = p;
            // The only time the id number matters is for Dollardollar
            let (_, dollardollar) = lid.1;
            let left_rty = rty_expr(context, checker, left);
            context.add_local(dollardollar, left_rty);
            rty_expr(context, checker, right)
        }
        ExpressionTree(_) | EnumClassLabel(_) | ETSplice(_) => Rty::Mutable,
        Import(_) | Lplaceholder(_) => Rty::Mutable,
        // More function values which are always mutable
        MethodCaller(_) => Rty::Mutable,
        Package(_) => Rty::Mutable,
        DestructureShape(_) | DestructureTuple(_) => Rty::Mutable,
    }
}

fn strip_ns(name: &str) -> &str {
    match name.chars().next() {
        Some('\\') => &name[1..],
        _ => name,
    }
}

// Special builtins that can take readonly values and return the same readonlyness back
// These are represented as bytecodes, so do not go through regular call enforcement
fn is_special_builtin(f_name: &str) -> bool {
    // HHVM qualifies the name with `HH\` first if it is one of the functions
    // it imports from that namespace into every file, and then compares it
    // with a list. It builds the qualified name from the name as written
    // though, so only the bare `vec` gets there, not `\vec`; and nothing
    // else on the list is such a function.
    match f_name {
        "dict" | "varray" | "darray" | "vec" => true,
        _ => matches!(
            strip_ns(f_name),
            "HH\\dict"
                | "HH\\varray"
                | "HH\\darray"
                | "HH\\vec"
                | "hphp_array_idx"
                | "HH\\FIXME\\UNSAFE_NONNULL_CAST"
                | "HH\\FIXME\\UNSAFE_CAST"
        ),
    }
}

// For assignments to nonlocals, i.e.
// $x->prop[0] = new Foo();
fn check_assignment_nonlocal<'a>(
    context: &mut Context<'a>,
    checker: &mut Checker<'a>,
    pos: &Pos,
    lhs: &'a Expr<'a>,
    rhs: &'a Expr<'a>,
) {
    match checker.shape(lhs) {
        Expr_::ObjGet(o) => {
            let (obj, _get, _, _) = o;
            // If obj is readonly, throw error
            match rty_expr(context, checker, obj) {
                Rty::Readonly => {
                    checker.add_error(pos, ASSIGNMENT_TO_READONLY);
                }
                Rty::Mutable => {
                    match rty_expr(context, checker, rhs) {
                        Rty::Readonly => {
                            // make the readonly expression explicit, since if it is
                            // readonly we need to make sure the property is a readonly prop
                            checker.explicit_readonly(rhs);
                        }
                        // Mutable case does not require special checks
                        Rty::Mutable => {}
                    }
                }
            }
        }

        // Support self::$x["x"] assignments
        // Here, if the rhs is readonly we need to be explicit
        Expr_::ClassGet(_) => match rty_expr(context, checker, rhs) {
            Rty::Readonly => {
                checker.explicit_readonly(rhs);
            }
            _ => {}
        },
        // On an array get <expr>[0] = <rhs>, recurse and check compatibility of
        // inner <expr> with <rhs>
        Expr_::ArrayGet(ag) => {
            let (array, _) = ag;
            check_assignment_nonlocal(context, checker, pos, array, rhs);
        }
        _ => {
            // Base case: here we just check whether the lhs expression is readonly
            // compared to the rhs
            match (
                rty_expr(context, checker, lhs),
                rty_expr(context, checker, rhs),
            ) {
                (Rty::Mutable, Rty::Readonly) => {
                    // error, can't assign a readonly value to a mutable collection
                    checker.add_error(
                        &rhs.1, // Position of readonly expression
                        ASSIGN_READONLY_TO_MUTABLE_COLLECTION,
                    )
                }
                (Rty::Readonly, Rty::Readonly) => {
                    // make rhs explicit (to make sure we are not writing a readonly
                    // value to a mutable one)
                    checker.explicit_readonly(rhs);
                    // make lhs readonly explicitly, to check that it's a readonly
                    // copy on write array here the lefthandside is either a local
                    // variable or a class_get.
                    checker.explicit_readonly(lhs);
                }
                (Rty::Readonly, Rty::Mutable) => {
                    checker.explicit_readonly(lhs);
                }
                // Assigning to a mutable value always succeeds, so no explicit checks
                // are needed
                (Rty::Mutable, Rty::Mutable) => {}
            }
        }
    }
}

// Merges two local environments together. Note that
// because of conservativeness, the resulting Lenv's num_readonly is at
// least the max of lenv1.num_readonly, lenv2.num_readonly
fn merge_lenvs<'a>(lenv1: &Lenv<'a>, lenv2: &Lenv<'a>) -> Lenv<'a> {
    let mut new_lenv = lenv1.clone();
    for (&key, value) in lenv2.lenv.iter() {
        match (value, new_lenv.get(key)) {
            (_, Some(Rty::Readonly)) => {}
            (rty, _) => {
                new_lenv.insert(key, *rty);
            }
        }
    }
    new_lenv
}

// Check readonly validity for a destructure target (DtLvar, DtWildcard, DtShape, DtTuple)
fn check_destructure_target_assignment<'a>(
    context: &mut Context<'a>,
    checker: &mut Checker<'a>,
    target: &'a DestructureTarget<'a>,
    rhs: &'a Expr<'a>,
) {
    match &target.2 {
        DestructureTarget_::DtLvar(id_orig) => {
            let var_name = local_id::get_name(&id_orig.1);
            let rhs_rty = rty_expr(context, checker, rhs);
            if context.inout_params.contains(var_name) && rhs_rty == Rty::Readonly {
                checker.add_error(&id_orig.0, INOUT_READONLY_ASSIGNMENT);
            }
            context.add_local(var_name, rhs_rty);
        }
        DestructureTarget_::DtWildcard(_) => {}
        DestructureTarget_::DtShape(ds) => {
            for dsf in ds.fields.iter() {
                check_destructure_target_assignment(context, checker, &dsf.target, rhs);
            }
        }
        DestructureTarget_::DtTuple(dt) => {
            for dte in dt.entries.iter() {
                check_destructure_target_assignment(context, checker, &dte.target, rhs);
            }
        }
    }
}

// Toplevel assignment check
fn check_assignment_validity<'a>(
    context: &mut Context<'a>,
    checker: &mut Checker<'a>,
    pos: &Pos,
    lhs: &'a Expr<'a>,
    rhs: &'a Expr<'a>,
) {
    match checker.shape(lhs) {
        Expr_::Lvar(id_orig) => {
            let var_name = local_id::get_name(&id_orig.1);
            let rhs_rty = rty_expr(context, checker, rhs);
            if context.inout_params.contains(var_name) && rhs_rty == Rty::Readonly {
                checker.add_error(pos, INOUT_READONLY_ASSIGNMENT);
            }
            context.add_local(var_name, rhs_rty);
        }
        // list assignment
        Expr_::List(l) | Expr_::Tuple(l) => {
            for e in l.iter() {
                check_assignment_validity(context, checker, &e.1, e, rhs);
            }
        }
        // shape assignment
        Expr_::Shape(l) => {
            for field in l.iter() {
                match field {
                    ShapeExprField::SFField(_field_name, e) => {
                        check_assignment_validity(context, checker, &e.1, e, rhs);
                    }
                    ShapeExprField::SFSplat(_) => {}
                }
            }
        }
        // shape destructuring pattern
        Expr_::DestructureShape(ds) => {
            for dsf in ds.fields.iter() {
                check_destructure_target_assignment(context, checker, &dsf.target, rhs);
            }
        }
        // tuple destructuring pattern
        Expr_::DestructureTuple(dt) => {
            for dte in dt.entries.iter() {
                check_destructure_target_assignment(context, checker, &dte.target, rhs);
            }
        }
        // directly assigning to a class static is always valid (locally) as long as
        // the rhs is explicit on its readonlyness
        Expr_::ClassGet(_) => {
            let rhs_rty = rty_expr(context, checker, rhs);
            match rhs_rty {
                Rty::Readonly => checker.explicit_readonly(rhs),
                _ => {}
            }
        }
        _ => {
            check_assignment_nonlocal(context, checker, pos, lhs, rhs);
        }
    }
}

struct Checker<'a> {
    errors: Vec<SyntaxError>,
    /// Check as HHVM does when it parses for its typechecker (see
    /// [`check_program`]).
    is_typechecker: bool,
    /// The expressions HHVM's pass would have wrapped in `ReadonlyExpr` by
    /// now, by address: within a method, a function or the rest of a class,
    /// every expression is a node of its own in the arena.
    made_readonly: HashSet<*const Expr<'a>>,
}

impl<'a> Checker<'a> {
    fn new(typechecker: bool) -> Self {
        Self {
            errors: vec![],
            is_typechecker: typechecker,
            made_readonly: HashSet::default(),
        }
    }

    fn add_error(&mut self, pos: &Pos, msg: impl Into<Cow<'static, str>>) {
        self.errors.push(SyntaxError {
            start: pos.start_offset() as u32,
            end: pos.end_offset() as u32,
            message: msg.into(),
        });
    }

    /// What HHVM's `explicit_readonly` does to the tree: from here on the
    /// pass finds `readonly <expr>` where `expr` was.
    fn explicit_readonly(&mut self, expr: &'a Expr<'a>) {
        match expr.2 {
            Expr_::ReadonlyExpr(_) => {}
            _ => {
                self.made_readonly.insert(expr);
            }
        }
    }

    /// The expression HHVM's pass finds in the place of `expr` at this
    /// point: `expr` itself, or `readonly <expr>` once
    /// [`explicit_readonly`](Self::explicit_readonly) has been applied to
    /// it. Everything that inspects an expression HHVM may have wrapped goes
    /// through this.
    fn shape(&self, expr: &'a Expr<'a>) -> Expr_<'a> {
        if self.made_readonly.contains(&std::ptr::from_ref(expr)) {
            Expr_::ReadonlyExpr(expr)
        } else {
            expr.2
        }
    }

    fn subtype(&mut self, pos: &Pos, r_sub: &Rty, r_sup: &Rty, reason: &str) {
        use Rty::*;
        match (r_sub, r_sup) {
            (Readonly, Mutable) => {
                self.add_error(pos, invalid_readonly("readonly", "mutable", reason))
            }
            _ => {}
        }
    }

    fn handle_single_block(
        &mut self,
        context: &mut Context<'a>,
        lenv: Lenv<'a>,
        b: &'a Block<'a>,
    ) -> Lenv<'a> {
        context.locals = lenv;
        let _ = b.recurse(context, self.object());
        context.locals.clone()
    }

    // Handles analysis for a given loop
    // Will run b.recurse() up to X times, where X is the number
    // of readonly values in the local environment
    // on each loop, we check if the number of readonly values has changed
    // since each loop iteration will monotonically increase the number
    // of readonly values, the loop must end within that fixed number of iterations.
    fn handle_loop(
        &mut self,
        context: &mut Context<'a>,
        lenv: Lenv<'a>,
        b: &'a Block<'a>,
        as_expr_var_info: Option<(&'a str, Rty)>,
    ) -> Lenv<'a> {
        let mut new_lenv = lenv.clone();
        // Reassign the as_expr_var_info on every iteration of the loop
        if let Some((var_name, rty)) = as_expr_var_info {
            new_lenv.insert(var_name, rty);
        }
        // run the block once and merge the environment
        let new_lenv = merge_lenvs(&lenv, &self.handle_single_block(context, new_lenv, b));
        if new_lenv.num_readonly > lenv.num_readonly {
            self.handle_loop(context, new_lenv, b, as_expr_var_info)
        } else {
            new_lenv
        }
    }
}

impl<'a> Visitor<'a> for Checker<'a> {
    type Params = AstParams<Context<'a>, Infallible>;

    fn object(&mut self) -> &mut dyn Visitor<'a, Params = Self::Params> {
        self
    }

    fn visit_method_(
        &mut self,
        _context: &mut Context<'a>,
        m: &'a Method_<'a>,
    ) -> Result<(), Infallible> {
        // Nothing HHVM has wrapped so far is looked at again from here on (a
        // method is not part of anything that is analysed twice), and it has
        // to be forgotten: unlike in HHVM's tree, the attributes of a
        // promoted constructor parameter are the very nodes the property
        // made from it has, and that was visited before the methods.
        self.made_readonly.clear();
        if m.user_attributes
            .iter()
            .any(|ua| ignore_readonly_local_errors(ua.name.1))
        {
            return Ok(());
        }
        let readonly_return = ro_kind_to_rty(m.readonly_ret);
        let readonly_this = if m.readonly_this {
            Rty::Readonly
        } else {
            Rty::Mutable
        };
        let mut context = Context::new(readonly_return, readonly_this);

        for p in m.params.iter() {
            let is_inout = match p.callconv {
                ParamKind::Pinout(_) => true,
                _ => false,
            };
            match &p.info {
                FunParamInfo::ParamOptional(Some(rhs)) => {
                    let ro_rhs = rty_expr(&mut context, self, rhs);
                    self.subtype(
                        &rhs.1,
                        &ro_rhs,
                        &ro_kind_to_rty(p.readonly),
                        "this parameter is not marked readonly",
                    );
                }
                _ => {}
            }

            if p.readonly.is_some() {
                context.add_param(p.name, Rty::Readonly, is_inout)
            } else {
                context.add_param(p.name, Rty::Mutable, is_inout)
            }
        }
        m.recurse(&mut context, self.object())
    }

    fn visit_fun_(&mut self, context: &mut Context<'a>, f: &'a Fun_<'a>) -> Result<(), Infallible> {
        // Is run on every function definition and closure definition
        if f.user_attributes
            .iter()
            .any(|ua| ignore_readonly_local_errors(ua.name.1))
        {
            return Ok(());
        }
        let readonly_return = ro_kind_to_rty(f.readonly_ret);
        let readonly_this = ro_kind_to_rty(f.readonly_this);
        let mut new_context = Context::new(readonly_return, readonly_this);
        // Add the old context's stuff into the new context, as readonly if needed
        for (&local, rty) in context.locals.lenv.iter() {
            if readonly_this == Rty::Readonly {
                new_context.add_local(local, Rty::Readonly);
            } else {
                new_context.add_local(local, *rty);
            }
        }
        for p in f.params.iter() {
            let is_inout = match p.callconv {
                ParamKind::Pinout(_) => true,
                _ => false,
            };
            match &p.info {
                FunParamInfo::ParamOptional(Some(rhs)) => {
                    let ro_rhs = rty_expr(&mut new_context, self, rhs);
                    self.subtype(
                        &rhs.1,
                        &ro_rhs,
                        &ro_kind_to_rty(p.readonly),
                        "this parameter is not marked readonly",
                    );
                }
                _ => {}
            }
            if p.readonly.is_some() {
                new_context.add_param(p.name, Rty::Readonly, is_inout)
            } else {
                new_context.add_param(p.name, Rty::Mutable, is_inout)
            }
        }
        f.recurse(&mut new_context, self.object())
    }

    fn visit_fun_def(
        &mut self,
        _context: &mut Context<'a>,
        f: &'a FunDef<'a>,
    ) -> Result<(), Infallible> {
        // Clear the context completely on a fun_def
        let mut context = Context::new(Rty::Mutable, Rty::Mutable);
        f.recurse(&mut context, self.object())
    }

    fn visit_expr(&mut self, context: &mut Context<'a>, p: &'a Expr<'a>) -> Result<(), Infallible> {
        // This looks at `p` itself rather than at its `shape`: when HHVM
        // visits an expression it has wrapped, it steps through the wrapper,
        // which gets no treatment of its own, and visits the expression as
        // it would have without it.
        //
        // Pipe expressions have their own recursion method due to their weird evaluation rules
        if !matches!(p.2, Expr_::Pipe(_)) {
            // recurse on inner members first, then assign to value
            p.recurse(context, self.object())?;
        }
        match p.2 {
            Expr_::Assign(x) => {
                let (e_lhs, _, e_rhs) = x;
                check_assignment_validity(context, self, &p.1, e_lhs, e_rhs);
            }
            Expr_::Unop(x) => {
                let (operator, operand) = x;
                if let Uop::Upincr | Uop::Updecr | Uop::Uincr | Uop::Udecr = operator {
                    // These unops are treated as assignments of some
                    // non-readonly value
                    check_assignment_validity(context, self, &p.1, operand, &IMPLICIT_RHS);
                }
            }
            Expr_::Call(x) => {
                match rty_expr(context, self, &x.func) {
                    Rty::Readonly => self.explicit_readonly(&x.func),
                    Rty::Mutable => {}
                };
                for arg in x.args.iter() {
                    match arg {
                        Argument::Ainout(_, param) => match rty_expr(context, self, param) {
                            Rty::Readonly => self.add_error(param.pos(), INOUT_READONLY_ARGUMENT),
                            Rty::Mutable => {}
                        },
                        Argument::Anormal(param) | Argument::Anamed(_, param) => {
                            match rty_expr(context, self, param) {
                                Rty::Readonly => self.explicit_readonly(param),
                                Rty::Mutable => {}
                            }
                        }
                    }
                }
            }
            Expr_::New(n) => {
                let (_, _targs, args, _variadic, _) = n;
                for arg in args.iter() {
                    let e = match arg {
                        Argument::Ainout(_, e) => e,
                        Argument::Anormal(e) => e,
                        Argument::Anamed(_, e) => e,
                    };
                    match rty_expr(context, self, e) {
                        Rty::Readonly => self.explicit_readonly(e),
                        Rty::Mutable => {}
                    }
                }
            }
            Expr_::Pipe(p) => {
                let (lid, left, right, _) = p;
                // The only time the id number matters is for Dollardollar
                let (_, dollardollar) = lid.1;
                // Go left first, get the readonlyness, then go right
                self.visit_expr(context, left)?;
                let left_rty = rty_expr(context, self, left);
                context.add_local(dollardollar, left_rty);
                self.visit_expr(context, right)?;
            }
            Expr_::Yield(y) => {
                // HHVM only checks this when parsing for the typechecker
                if self.is_typechecker {
                    let mut expect_mutable =
                        |expr: &'a Expr<'a>| match rty_expr(context, self, expr) {
                            Rty::Readonly => {
                                self.add_error(expr.pos(), YIELD_READONLY);
                            }
                            Rty::Mutable => (),
                        };

                    match y {
                        Afield::AFkvalue(k, v) => {
                            expect_mutable(k);
                            expect_mutable(v);
                        }
                        Afield::AFvalue(v) => {
                            expect_mutable(v);
                        }
                    };
                }
            }

            _ => {}
        }
        Ok(())
    }

    fn visit_xhp_simple(
        &mut self,
        context: &mut Context<'a>,
        p: &'a XhpSimple<'a>,
    ) -> Result<(), Infallible> {
        if let Rty::Readonly = rty_expr(context, self, &p.expr) {
            self.add_error(&p.expr.1, READONLY_ON_XHP);
        }
        p.recurse(context, self.object())
    }

    fn visit_stmt(&mut self, context: &mut Context<'a>, s: &'a Stmt<'a>) -> Result<(), Infallible> {
        match s.1 {
            Stmt_::Return(r) => {
                if let Some(expr) = r {
                    self.subtype(
                        &expr.1,
                        &rty_expr(context, self, expr),
                        &context.readonly_return,
                        "this function does not return readonly. Please mark it to return readonly if needed.",
                    )
                }
                s.recurse(context, self.object())
            }
            Stmt_::If(i) => {
                let (condition, if_, else_) = i;
                let old_lenv = context.locals.clone();
                self.visit_expr(context, condition)?;
                let if_lenv = self.handle_single_block(context, old_lenv.clone(), if_);
                let else_lenv = self.handle_single_block(context, old_lenv, else_);
                let new_lenv = merge_lenvs(&if_lenv, &else_lenv);
                context.locals = new_lenv;
                Ok(())
            }
            Stmt_::Try(t) => {
                let (try_, catches, finally_) = t;
                let old_lenv = context.locals.clone();
                try_.recurse(context, self.object())?;
                // Each catch should run with no assumptions about how much of the try ran,
                // i.e. with old_lenv. Start with the lenv right after the try block and merge
                let result_lenv =
                    catches
                        .iter()
                        .fold(context.locals.clone(), |result_lenv, catch| {
                            let catch_lenv =
                                self.handle_single_block(context, old_lenv.clone(), &catch.2);
                            merge_lenvs(&result_lenv, &catch_lenv)
                        });
                // Update the lenv from the old lenv with the result of
                context.locals = result_lenv;
                finally_.recurse(context, self.object())
            }
            Stmt_::Switch(s) => {
                let (condition, cases, default) = s;
                self.visit_expr(context, condition)?;
                let old_lenv = context.locals.clone();
                let result_lenv = context.locals.clone();
                let result_lenv = cases.iter().fold(result_lenv, |result_lenv, case| {
                    let _ = self.visit_expr(context, &case.0);
                    let case_lenv = self.handle_single_block(context, old_lenv.clone(), &case.1);
                    merge_lenvs(&result_lenv, &case_lenv)
                });
                let result_lenv = default.iter().fold(result_lenv, |result_lenv, case| {
                    let case_lenv = self.handle_single_block(context, old_lenv.clone(), &case.1);
                    merge_lenvs(&result_lenv, &case_lenv)
                });
                context.locals = result_lenv;
                Ok(())
            }
            Stmt_::Throw(t) => {
                match rty_expr(context, self, t) {
                    Rty::Readonly => {
                        self.add_error(&t.1, THROW_READONLY_EXCEPTION);
                    }
                    Rty::Mutable => {}
                }
                self.visit_expr(context, t)
            }
            Stmt_::Foreach(f) => {
                let (e, as_expr, b) = f;
                // Tracks what variable is being assigned each iteration of the loop
                let mut as_expr_var_info = None;
                match as_expr {
                    AsExpr::AsV(Expr(_, _, Expr_::Lvar(id))) => {
                        let var_name = local_id::get_name(&id.1);
                        let rty = rty_expr(context, self, e);
                        as_expr_var_info = Some((var_name, rty));
                    }
                    AsExpr::AsKv(
                        _, // key is arraykey and does not need to be readonly
                        Expr(_, _, Expr_::Lvar(value_id)),
                    ) => {
                        let var_name = local_id::get_name(&value_id.1);
                        let rty = rty_expr(context, self, e);
                        as_expr_var_info = Some((var_name, rty));
                    }
                    AsExpr::AwaitAsV(_, Expr(_, _, Expr_::Lvar(id))) => {
                        let var_name = local_id::get_name(&id.1);
                        let rty = rty_expr(context, self, e);
                        as_expr_var_info = Some((var_name, rty));
                    }
                    AsExpr::AwaitAsKv(
                        _,
                        _, // key is arraykey and does not need to be readonly
                        Expr(_, _, Expr_::Lvar(value_id)),
                    ) => {
                        let var_name = local_id::get_name(&value_id.1);
                        let rty = rty_expr(context, self, e);
                        as_expr_var_info = Some((var_name, rty));
                    }
                    // Any other Foreach here would mean no Lvar
                    // where an Lvar is needed. In those cases
                    // we will parse error, but the parse tree might still exist
                    // so we just ignore those cases and continue our analysis
                    // knowing that a parse error will show up
                    _ => {}
                };
                self.visit_expr(context, e)?;
                as_expr.recurse(context, self.object())?;
                let old_lenv = context.locals.clone();
                let new_lenv = self.handle_loop(context, old_lenv, b, as_expr_var_info);
                context.locals = new_lenv;
                Ok(())
            }
            Stmt_::Do(d) => {
                let (b, cond) = d;
                // loop runs at least once
                let new_lenv = self.handle_single_block(context, context.locals.clone(), b);
                let block_lenv = self.handle_loop(context, new_lenv, b, None);
                context.locals = block_lenv;
                self.visit_expr(context, &cond.2)
            }
            Stmt_::While(w) => {
                let (cond, b) = w;
                self.visit_expr(context, &cond.2)?;
                let old_lenv = context.locals.clone();
                let new_lenv = self.handle_loop(context, old_lenv, b, None);
                context.locals = new_lenv;
                Ok(())
            }
            Stmt_::For(f) => {
                let (initializers, term, increment, b) = f;
                for i in initializers.iter() {
                    self.visit_expr(context, i)?;
                }
                match term {
                    Some(t) => {
                        self.visit_expr(context, &t.2)?;
                    }
                    None => {}
                }
                for inc in increment.2 {
                    self.visit_expr(context, inc)?;
                }
                let old_lenv = context.locals.clone();
                // Technically, for loops can have an as_expr_var_info as well, but it's
                // very rare to write one where this occurs, so we will just be conservative
                let new_lenv = self.handle_loop(context, old_lenv, b, None);
                context.locals = new_lenv;
                Ok(())
            }
            Stmt_::Awaitall(a) => {
                let (assignments, block) = a;
                for i in assignments.iter() {
                    // if there's a temp local
                    let lid = &i.0;
                    let var_name = local_id::get_name(&lid.1);
                    let rhs_rty = rty_expr(context, self, &i.1);
                    context.add_local(var_name, rhs_rty);
                    i.recurse(context, self.object())?;
                }
                block.recurse(context, self.object())
            }
            _ => s.recurse(context, self.object()),
        }
    }
}

/// Runs the analysis over a file's lowered tree and returns what HHVM's
/// pass reports for it, in HHVM's order. `is_typechecker` is whether HHVM
/// would be parsing for its typechecker rather than for its compiler, which
/// additionally rejects yielding readonly values.
pub fn check_program(program: &Program<'_>, is_typechecker: bool) -> Vec<SyntaxError> {
    let mut checker = Checker::new(is_typechecker);
    let mut context = Context::new(Rty::Mutable, Rty::Mutable);
    let Ok(()) = visit(&mut checker, &mut context, program);
    checker.errors
}
