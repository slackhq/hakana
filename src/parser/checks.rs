//! Errors for code that parses but is not valid Hack.
//!
//! HHVM reports these from several passes that run after its parser (its
//! syntax-error pass over the concrete tree, the lowerer, and a few checks
//! over the lowered tree). Here the ones that only need the tokens at hand
//! are reported by the parser as it goes; the rest are reported by walking
//! each top-level definition once it has been parsed. Messages and positions
//! match HHVM's.

/// Language features that have to be switched on per file with
/// `<<file:__EnableUnstableFeatures('name')>>` before the syntax they guard
/// may be used. Features HHVM considers released are not listed: using them
/// is never an error. The exception is `XhpTypeConstants`, which HHVM asks
/// for by name wherever it matters.
// The names are HHVM's.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum Feature {
    UnionIntersectionTypeHints,
    ExpressionTrees,
    Readonly,
    ModuleReferences,
    ContextAliasDeclaration,
    ContextAliasDeclarationShort,
    ClassConstDefault,
    UpcastExpression,
    NewtypeSuperBounds,
    CaseTypeWhereClauses,
    RecursiveCaseTypes,
    MatchStatements,
    StrictSwitch,
    FunctionReferences,
    LikeTypeHints,
    ShapeAndTupleDestructuring,
    SimpliHack,
    NamedParameters,
    CapturePipeVariables,
    AllowExtendedAwaitSyntax,
    AllowConditionalAwaitSyntax,
    ShapeFieldPunning,
    RepresentableAs,
    ClassAliasesEverywhere,
    TestFeature,
    WithRefinementAlias,
    ShapeSplatConcrete,
    ShapeSplatTypeParameters,
    ShapeSplatExpression,
    NamedParametersMethCaller,
    XhpTypeConstants,
}

impl Feature {
    const GATED: &'static [(Feature, &'static str)] = &[
        (
            Feature::UnionIntersectionTypeHints,
            "union_intersection_type_hints",
        ),
        (Feature::ExpressionTrees, "expression_trees"),
        (Feature::Readonly, "readonly"),
        (Feature::ModuleReferences, "module_references"),
        (
            Feature::ContextAliasDeclaration,
            "context_alias_declaration",
        ),
        (
            Feature::ContextAliasDeclarationShort,
            "context_alias_declaration_short",
        ),
        (Feature::ClassConstDefault, "class_const_default"),
        (Feature::UpcastExpression, "upcast_expression"),
        (Feature::NewtypeSuperBounds, "newtype_super_bounds"),
        (Feature::CaseTypeWhereClauses, "case_type_where_clauses"),
        (Feature::RecursiveCaseTypes, "recursive_case_types"),
        (Feature::MatchStatements, "match_statements"),
        (Feature::StrictSwitch, "strict_switch"),
        (Feature::FunctionReferences, "function_references"),
        (Feature::LikeTypeHints, "like_type_hints"),
        (
            Feature::ShapeAndTupleDestructuring,
            "shape_and_tuple_destructuring",
        ),
        (Feature::SimpliHack, "simpli_hack"),
        (Feature::NamedParameters, "named_parameters"),
        (Feature::CapturePipeVariables, "capture_pipe_variables"),
        (
            Feature::AllowExtendedAwaitSyntax,
            "allow_extended_await_syntax",
        ),
        (
            Feature::AllowConditionalAwaitSyntax,
            "allow_conditional_await_syntax",
        ),
        (Feature::ShapeFieldPunning, "shape_field_punning"),
        (Feature::RepresentableAs, "representable_as"),
        (Feature::ClassAliasesEverywhere, "class_aliases_everywhere"),
        (Feature::TestFeature, "test_feature"),
        (Feature::WithRefinementAlias, "with_refinement_alias"),
        (Feature::ShapeSplatConcrete, "shape_splat_concrete"),
        (
            Feature::ShapeSplatTypeParameters,
            "shape_splat_type_parameters",
        ),
        (Feature::ShapeSplatExpression, "shape_splat_expression"),
        (
            Feature::NamedParametersMethCaller,
            "named_parameters_meth_caller",
        ),
        (Feature::XhpTypeConstants, "xhp_type_constants"),
    ];

    /// Every feature name HHVM knows, in its order. The ones missing from
    /// [`Feature`] are always enabled.
    pub(crate) const ALL_NAMES: &'static [&'static str] = &[
        "union_intersection_type_hints",
        "expression_trees",
        "readonly",
        "module_references",
        "context_alias_declaration",
        "context_alias_declaration_short",
        "type_const_multiple_bounds",
        "type_const_super_bound",
        "class_const_default",
        "upcast_expression",
        "require_constraint",
        "newtype_super_bounds",
        "case_types",
        "case_type_where_clauses",
        "recursive_case_types",
        "module_level_traits",
        "module_level_traits_extensions",
        "pipe_await",
        "match_statements",
        "strict_switch",
        "class_type",
        "function_references",
        "function_type_optional_params",
        "await_in_splice",
        "open_tuples",
        "type_splat",
        "expression_tree_nested_bindings",
        "like_type_hints",
        "shape_and_tuple_destructuring",
        "expression_tree_shape_creation",
        "no_disjoint_union",
        "simpli_hack",
        "polymorphic_function_hints",
        "polymorphic_lambda",
        "expression_tree_coalesce_operator",
        "expression_tree_nullsafe_obj_get",
        "named_parameters",
        "xhp_type_constants",
        "capture_pipe_variables",
        "allow_extended_await_syntax",
        "allow_conditional_await_syntax",
        "expression_tree_hack_arrays",
        "expression_tree_subscript",
        "shape_field_punning",
        "representable_as",
        "class_aliases_everywhere",
        "test_feature",
        "with_refinement_alias",
        "shape_splat_concrete",
        "shape_splat_type_parameters",
        "shape_splat_expression",
        "named_parameters_meth_caller",
    ];

    pub(crate) fn name(self) -> &'static str {
        Self::GATED[self as usize].1
    }

    pub(crate) fn from_name(name: &str) -> Option<Feature> {
        Self::GATED
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(f, _)| *f)
    }

    #[inline]
    pub(crate) fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

use crate::Span;
use crate::ast::*;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// The source text of a literal with any surrounding quotes removed.
    fn unquoted_text(&self, e: &Expr<'a>) -> &'a str {
        let text = e.span.text(self.src);
        let b = text.as_bytes();
        if b.len() >= 2 && (b[0] == b'\'' || b[0] == b'"') && b[b.len() - 1] == b[0] {
            &text[1..text.len() - 1]
        } else {
            text
        }
    }

    /// Handles the attributes of `<<file: ...>>` that the parser itself
    /// acts on. `__EnableUnstableFeatures('name', ...)` switches features on
    /// for the rest of the file.
    pub(crate) fn check_file_attribute(&mut self, attr: &Attribute<'a>) {
        let (start, end) = (attr.name.span.start, self.prev_end);
        match attr.name.name {
            "__EnableUnstableFeatures" => {
                if attr.args.is_empty() {
                    self.report(
                        start,
                        end,
                        format!(
                            "This is an invalid use of `__EnableUnstableFeatures` because you didn't select a feature. Available features are:\n\t{}",
                            Feature::ALL_NAMES.join("\n\t")
                        ),
                    );
                }
                for arg in attr.args {
                    let is_literal = matches!(
                        arg.kind,
                        ExprKind::String(_)
                            | ExprKind::String2(_)
                            | ExprKind::Int(_)
                            | ExprKind::Float(_)
                            | ExprKind::True
                            | ExprKind::False
                            | ExprKind::Null
                    );
                    if !is_literal {
                        self.report(
                            arg.span.start,
                            arg.span.end,
                            "This is an invalid use of `__EnableUnstableFeatures` because this is not a literal string expression",
                        );
                        continue;
                    }
                    let name = self.unquoted_text(arg);
                    if let Some(feature) = Feature::from_name(name) {
                        self.features |= feature.bit();
                    } else if !Feature::ALL_NAMES.contains(&name) {
                        self.report(
                            arg.span.start,
                            arg.span.end,
                            format!(
                                "This is an invalid use of `__EnableUnstableFeatures` because there is no feature named {}.\nAvailable features are:\n\t{}",
                                name,
                                Feature::ALL_NAMES.join("\n\t")
                            ),
                        );
                    }
                }
            }
            "__PackageOverride" => {
                if attr.args.len() != 1 {
                    self.report(
                        start,
                        end,
                        "Package override must specify exactly one package name.",
                    );
                }
            }
            _ => {}
        }
    }
}

use crate::parser::decl::{MAX_MODIFIERS, Modifiers};
use crate::token::T;

fn modifier_text(kind: T) -> &'static str {
    match kind {
        T::Abstract => "abstract",
        T::Final => "final",
        T::Static => "static",
        T::Async => "async",
        T::Readonly => "readonly",
        T::Public => "public",
        T::Private => "private",
        T::Protected => "protected",
        T::Internal => "internal",
        T::Optional => "optional",
        T::Inout => "inout",
        T::Named => "named",
        _ => "",
    }
}

fn is_visibility(kind: T) -> bool {
    matches!(kind, T::Public | T::Private | T::Protected | T::Internal)
}

impl Modifiers {
    /// The modifier tokens in source order.
    pub(crate) fn tokens(&self) -> &[(T, u32)] {
        &self.toks[..(self.count as usize).min(MAX_MODIFIERS)]
    }

    /// The extent of the first modifier of the given kind.
    pub(crate) fn first(&self, kind: T) -> Option<(u32, u32)> {
        self.tokens()
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|&(k, start)| (start, start + modifier_text(k).len() as u32))
    }

    /// The extent of the whole modifier list.
    pub(crate) fn span(&self) -> Option<(u32, u32)> {
        let toks = self.tokens();
        let (last, last_start) = *toks.last()?;
        Some((toks[0].1, last_start + modifier_text(last).len() as u32))
    }
}

/// The class, interface or trait whose members are being parsed.
#[derive(Clone, Copy)]
pub(crate) struct ClassInfo<'a> {
    pub kind: ClassKind,
    pub name: &'a str,
    /// One of the class's type parameters is `reify`.
    pub has_reified: bool,
    /// The class's type parameters.
    pub tparams: &'a [Tparam<'a>],
    /// The class has the `<<__EnumClass>>` attribute, which lets its
    /// constants be initialized with `new`.
    pub enum_class_attr: bool,
}

/// What the checks on a parameter need to know about the function it
/// belongs to.
#[derive(Clone, Copy, Default)]
pub(crate) struct ParamCtx {
    pub is_async: bool,
    /// A lambda or anonymous function rather than a declaration.
    pub is_lambda: bool,
    pub is_construct: bool,
    /// The function has `<<__Memoize>>` or `<<__MemoizeLSB>>`.
    pub memoized: bool,
}

/// A parameter modifier keyword as written: kind, start and end.
pub(crate) type ParamModifier = (T, u32, u32);

pub(crate) fn param_modifier_rank(kind: T) -> u8 {
    match kind {
        T::Optional => 0,
        T::Inout => 1,
        T::Named => 2,
        _ => 3,
    }
}

/// Names HHVM reserves: they cannot name a class, enum or type alias.
fn is_reserved_type_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "callable", "parent", "self", "arraykey", "bool", "dynamic", "float", "int", "mixed",
        "nonnull", "noreturn", "nothing", "null", "num", "resource", "this", "void", "_", "darray",
        "false", "static", "true", "varray",
    ];
    name.len() <= 8 && RESERVED.iter().any(|r| name.eq_ignore_ascii_case(r))
}

pub(crate) fn has_attr(attrs: &[Attribute<'_>], name: &str) -> bool {
    attrs.iter().any(|a| a.name.name == name)
}

impl<'a> Parser<'a> {
    /// The position HHVM gives to something that is absent from the source:
    /// just past the previous token and the rest of its line.
    pub(crate) fn missing_pos(&self) -> u32 {
        self.leading_trivia_start(self.prev_end) as u32
    }

    /// Checks the modifier keywords of a declaration: each has to be one the
    /// kind of declaration allows (`ok`), none may be repeated, and at most
    /// one visibility may be given (`protected internal` aside). `internal`,
    /// and `public` on a top-level declaration, need the file to be in a
    /// module.
    pub(crate) fn check_modifiers(
        &mut self,
        m: &Modifiers,
        decl: &'static str,
        toplevel: bool,
        ok: impl Fn(T) -> bool,
    ) {
        let toks = m.tokens();
        if toks.is_empty() {
            return;
        }
        let mut visibilities = 0;
        let mut last_visibility = (0, 0);
        for &(kind, start) in toks {
            let end = start + modifier_text(kind).len() as u32;
            if kind == T::Internal {
                if !self.in_module {
                    self.report(
                        start,
                        end,
                        "`internal` cannot be used outside of a module. Please define a module for this file.",
                    );
                }
            } else if kind == T::Public && toplevel && !self.in_module {
                self.report(
                    start,
                    end,
                    "`public` on toplevel entities cannot be used outside of a module. Please define a module for this file.",
                );
            }
            if !ok(kind) {
                self.report(
                    start,
                    end,
                    format!("{} cannot be declared `{}`", decl, modifier_text(kind)),
                );
            }
            if is_visibility(kind) {
                visibilities += 1;
                last_visibility = (start, end);
            }
        }
        if toks.len() > 1 {
            // Scanning from the end, the first keyword that repeats a later
            // one makes that later one the duplicate.
            'dup: for i in (0..toks.len() - 1).rev() {
                for &(kind, start) in &toks[i + 1..] {
                    if kind == toks[i].0 {
                        self.report(
                            start,
                            start + modifier_text(kind).len() as u32,
                            format!("{} cannot have duplicate modifiers", decl),
                        );
                        break 'dup;
                    }
                }
            }
            if visibilities > 1 && !(visibilities == 2 && m.has_internal && m.has_protected) {
                self.report(
                    last_visibility.0,
                    last_visibility.1,
                    format!(
                        "{} cannot have multiple visibility modifiers except for `protected internal`.",
                        decl
                    ),
                );
            }
        }
    }

    /// Reports a class, enum or alias named like a built-in type.
    pub(crate) fn check_type_name(&mut self, name: Id<'a>) {
        if is_reserved_type_name(name.name) {
            self.report(
                name.span.start,
                name.span.end,
                format!(
                    "Cannot use `{}` as a type name as it is reserved",
                    name.name
                ),
            );
        }
    }

    /// Checks the `optional` / `inout` / `named` / `readonly` keywords of a
    /// parameter, given in the order written. `param` is the extent of the
    /// whole parameter and `name` that of its name (or `...`); `in_type` is
    /// set for the parameters of a function type.
    pub(crate) fn check_param_modifiers(
        &mut self,
        mods: &[ParamModifier],
        param: (u32, u32),
        name: (u32, u32),
        in_type: bool,
    ) {
        let mut seen = [false; 4];
        for &(kind, start, end) in mods {
            let rank = param_modifier_rank(kind) as usize;
            if seen[rank] {
                self.report(
                    start,
                    end,
                    format!("Duplicate parameter modifier `{}`.", modifier_text(kind)),
                );
                return;
            }
            seen[rank] = true;
        }
        let find = |kind: T| mods.iter().find(|m| m.0 == kind).copied();
        let (optional, inout, named, readonly) = (
            find(T::Optional),
            find(T::Inout),
            find(T::Named),
            find(T::Readonly),
        );
        let mut incompatible = false;
        if let (Some(_), Some(inout)) = (optional, inout) {
            let at = if in_type { (inout.1, inout.2) } else { param };
            self.report(
                at.0,
                at.1,
                "A parameter cannot be both `optional` and `inout`.",
            );
            incompatible = true;
        }
        if inout.is_some() && readonly.is_some() && !in_type {
            self.report(
                name.0,
                name.1,
                "A parameter cannot be both `inout` and `readonly`.",
            );
            incompatible = true;
        }
        if let (Some(_), Some(named)) = (inout, named) {
            self.report(
                named.1,
                named.2,
                "A parameter cannot be both `inout` and `named`.",
            );
            incompatible = true;
        }
        if incompatible {
            return;
        }
        if let Some(w) = mods
            .windows(2)
            .find(|w| param_modifier_rank(w[0].0) >= param_modifier_rank(w[1].0))
        {
            let mut order = String::new();
            for kind in [T::Optional, T::Inout, T::Named, T::Readonly] {
                if find(kind).is_some() {
                    if !order.is_empty() {
                        order.push(' ');
                    }
                    order.push_str(modifier_text(kind));
                }
            }
            self.report(
                w[1].1,
                w[1].2,
                format!(
                    "Invalid parameter modifier order. The correct order is `{}`.",
                    order
                ),
            );
        }
    }

    /// Checks that apply to a parameter list as a whole: where variadic
    /// (`T ...$x`) and splat (`...T $x`) parameters may appear and what they
    /// may be combined with. `spans` holds the extent of each parameter and
    /// `trailing_comma` the position of a comma after the last one. In a
    /// function type (`in_type`) a `named` parameter with `...` does not
    /// count as variadic.
    pub(crate) fn check_param_list(
        &mut self,
        params: &[Param<'a>],
        spans: &[(u32, u32)],
        trailing_comma: Option<u32>,
        in_type: bool,
    ) {
        use param_flags::*;
        let n = params.len();
        let variadic =
            |p: &Param<'a>| p.flags & VARIADIC != 0 && !(in_type && p.flags & NAMED != 0);
        if let (Some(comma), Some(last)) = (trailing_comma, params.last())
            && variadic(last)
        {
            self.report(
                comma,
                comma + 1,
                "A variadic parameter `...` may not be followed by a comma.",
            );
        }
        // A variadic parameter may only be followed by named parameters.
        for i in 0..n {
            if variadic(&params[i]) && params[i + 1..].iter().any(|p| p.flags & NAMED == 0) {
                self.report(
                    spans[i].0,
                    spans[i].1,
                    "A variadic parameter `...` may only appear at the end of a parameter list.",
                );
                break;
            }
        }
        if let Some(i) = params[..n - 1].iter().position(|p| p.flags & SPLAT != 0) {
            self.report(
                spans[i].0,
                spans[i].1,
                "A splat parameter `...` may only appear at the end of a parameter list.",
            );
        }
        for p in params {
            if p.flags & (NAMED | VARIADIC) == NAMED | VARIADIC && p.name.name != "..." {
                self.report(
                    p.name.span.start,
                    p.name.span.end,
                    "A variadic `named` parameter cannot have a name, since nothing can refer to the arguments it collects. For example: `named int...`.",
                );
                break;
            }
        }
        if let Some(i) = params
            .iter()
            .enumerate()
            .filter(|(_, p)| p.flags & (NAMED | VARIADIC) == NAMED | VARIADIC)
            .map(|(i, _)| i)
            .nth(1)
        {
            self.report(
                spans[i].0,
                spans[i].1,
                "A parameter list may have at most one variadic `named` parameter. The first one already collects every named argument that matches no named parameter.",
            );
        }
        // The first variadic and the first splat parameter, which may be
        // one and the same; what is wrong with them comes in HHVM's order.
        let first = |i: Option<usize>| i.map(|i| (&params[i], spans[i]));
        let first_variadic = first(params.iter().position(variadic));
        let first_splat = first(params.iter().position(|p| p.flags & SPLAT != 0));
        type Found<'p, 'a> = (&'p Param<'a>, (u32, u32));
        let has_default = |(p, _): &Found<'_, 'a>| p.default.is_some();
        let has_inout = |(p, _): &Found<'_, 'a>| p.inout.is_some();
        let has = |flag: u8| move |(p, _): &Found<'_, 'a>| p.flags & flag != 0;
        let found = [
            (
                first_variadic.filter(has_default),
                "A variadic parameter `...` must not have a default value.",
            ),
            (
                first_splat.filter(has_default),
                "A splat parameter `...` must not have a default value.",
            ),
            (
                first_variadic.filter(has_inout),
                "A variadic parameter `...` cannot have a modifier that changes the calling convention, like `inout`.",
            ),
            (
                first_splat.filter(has_inout),
                "A splat parameter `...` cannot have a modifier that changes the calling convention, like `inout`.",
            ),
            (
                first_splat.filter(|(p, _)| variadic(p)),
                "A splat parameter cannot also be variadic.",
            ),
            (
                first_splat.filter(has(OPTIONAL)),
                "A splat parameter cannot also be optional.",
            ),
            (
                first_variadic.filter(has(READONLY)),
                "Variadic parameters cannot be marked readonly",
            ),
            (
                first_splat.filter(has(READONLY)),
                "Splat parameters cannot be marked readonly",
            ),
        ];
        for (param, message) in found {
            if let Some((_, (start, end))) = param {
                self.report(start, end, message);
            }
        }
    }
}

const NOT_CONSTANT: &str = "Expected constant expression for initializer";

const INVALID_CONCURRENT: &str = "`concurrent` block must contain a compound statement of two or more expression statements, IE concurrent `{ <expr>; <expr>; }`.";

/// Functions that may be called in a constant initializer; HHVM only
/// recognises them fully qualified.
fn is_constant_function(name: &str) -> bool {
    matches!(
        name.strip_prefix('\\').unwrap_or(name),
        "HH\\array_mark_legacy"
            | "HH\\array_unmark_legacy"
            | "HH\\FIXME\\UNSAFE_CAST"
            | "HH\\FIXME\\UNSAFE_NONNULL_CAST"
    )
}

impl<'a> Parser<'a> {
    #[cold]
    fn not_constant(&mut self, e: &Expr<'a>) {
        self.report(e.span.start, e.span.end, NOT_CONSTANT);
    }

    /// Checks that an initializer (of a constant, property, parameter
    /// default, ...) only uses what HHVM can evaluate without running code:
    /// literals, constants, collections of those and a few operators.
    /// Anything else is reported at the offending sub-expression.
    /// `static_allowed` says whether `static::X` counts as constant.
    pub(crate) fn check_constant_expr(&mut self, e: &Expr<'a>, static_allowed: bool) {
        match e.kind {
            ExprKind::Null
            | ExprKind::True
            | ExprKind::False
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_)
            | ExprKind::Omitted => {}
            ExprKind::Id(name) => {
                // A qualified name is always fine; a single token has to be
                // a plain name.
                // A token that is no name at all, taken for the expression
                // that should have been there (see `repair.rs`), is none
                // either.
                let punctuation = !name
                    .bytes()
                    .next()
                    .is_some_and(|c| crate::lexer::is_name_start(c) || c == b'\\');
                if punctuation
                    || !name.contains('\\')
                        && (matches!(name, "self" | "parent" | "static")
                            || name.eq_ignore_ascii_case("__FUNCTION_CREDENTIAL__"))
                {
                    self.not_constant(e);
                }
            }
            ExprKind::PrefixedString(p) => self.check_constant_expr(&p.expr, static_allowed),
            ExprKind::Unop(Uop::Not | Uop::Plus | Uop::Minus | Uop::Tilde, operand) => {
                self.check_constant_expr(operand, static_allowed)
            }
            ExprKind::Binop(b) if b.op != Bop::QuestionQuestion => {
                self.check_constant_expr(&b.lhs, static_allowed);
                self.check_constant_expr(&b.rhs, static_allowed);
            }
            ExprKind::Eif(c) => {
                self.check_constant_expr(&c.cond, static_allowed);
                if let Some(then) = &c.then {
                    self.check_constant_expr(then, static_allowed);
                }
                self.check_constant_expr(&c.otherwise, static_allowed);
            }
            ExprKind::Upcast(t) => self.check_constant_expr(&t.expr, static_allowed),
            ExprKind::Collection(c) if !c.name.name.contains('\\') => {
                for field in c.fields {
                    match field {
                        Afield::Value(v) => self.check_constant_expr(v, static_allowed),
                        Afield::KeyValue(k, v) => {
                            self.check_constant_expr(k, static_allowed);
                            self.check_constant_expr(v, static_allowed);
                        }
                    }
                }
            }
            ExprKind::Tuple(items, _) => {
                for item in items {
                    self.check_constant_expr(item, static_allowed);
                }
            }
            ExprKind::ValCollection(c) => {
                for item in c.items {
                    self.check_constant_expr(item, static_allowed);
                }
            }
            ExprKind::KeyValCollection(c) => {
                for (k, v) in c.fields {
                    self.check_constant_expr(k, static_allowed);
                    self.check_constant_expr(v, static_allowed);
                }
            }
            ExprKind::Shape(fields, _) => {
                for field in fields {
                    self.check_constant_expr(field.value(), static_allowed);
                }
            }
            ExprKind::ClassConst(access) => {
                let ok = match access.class.kind {
                    ExprKind::Id(class) => {
                        (static_allowed || class != "static")
                            && !(class == "parent" && access.name.name == "class")
                    }
                    _ => false,
                };
                if !ok {
                    self.not_constant(e);
                }
            }
            ExprKind::Nameof(target) => {
                let ok = match target.kind {
                    ExprKind::Id(name) => {
                        name.contains('\\')
                            || !(name == "parent" || (!static_allowed && name == "static"))
                    }
                    _ => false,
                };
                if !ok {
                    self.not_constant(e);
                }
            }
            ExprKind::Call(call) => {
                let ok = matches!(call.func.kind, ExprKind::Id(name) if is_constant_function(name))
                    && call.unpacked.is_none();
                if ok {
                    for arg in call.args {
                        self.check_constant_expr(arg, static_allowed);
                    }
                } else {
                    self.not_constant(e);
                }
            }
            ExprKind::New(_) if matches!(self.class, Some(c) if c.enum_class_attr) => {}
            _ => self.not_constant(e),
        }
    }

    /// Checks one parameter of a function declaration, lambda or anonymous
    /// function. `param` is the extent of the whole parameter.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_param(
        &mut self,
        ctx: ParamCtx,
        param: (u32, u32),
        name: Id<'a>,
        inout: Option<crate::span::Span>,
        flags: u8,
        default: Option<&Expr<'a>>,
        attrs: &[Attribute<'a>],
    ) {
        if name.name == "$this" {
            self.report(name.span.start, name.span.end, "Cannot re-assign `$this`");
        }
        if let Some(inout) = inout {
            if ctx.is_async {
                self.report(
                    inout.start,
                    inout.end,
                    "Parameters may not be marked `inout` on `async` functions",
                );
            }
            if ctx.is_construct && !ctx.is_lambda {
                self.report(
                    inout.start,
                    inout.end,
                    "Parameters may not be marked `inout` on constructors",
                );
            }
            if ctx.memoized && !ctx.is_lambda {
                self.report(
                    inout.start,
                    inout.end,
                    "`<<__Memoize>>` cannot be used on functions with `inout` parameters",
                );
            }
        }
        if flags & param_flags::NAMED != 0 {
            self.require_feature(param.0, param.1, Feature::NamedParameters);
        }
        if let Some(default) = default {
            // Only declarations may refer to `static::` in a default
            // value. HHVM checks that of a lambda twice, the second time
            // with `static::` allowed, and reports the rest again.
            if ctx.is_lambda {
                self.check_constant_expr(default, false);
            }
            self.check_constant_expr(default, true);
            if flags & param_flags::OPTIONAL != 0 {
                self.report_lowering(
                    param.0,
                    param.1,
                    "Optional keyword not required for initializer",
                );
            }
        }
        if flags & param_flags::VARIADIC != 0 {
            if !attrs.is_empty() {
                self.report_lowering(
                    param.0,
                    param.1,
                    "Attributes on variadic parameters are not allowed",
                );
            }
            if flags & param_flags::OPTIONAL != 0 {
                self.report_lowering(
                    param.0,
                    param.1,
                    "Cannot use `optional` on variadic or splat parameters",
                );
            }
        }
    }
}

/// The pieces of a function or method declaration its checks look at.
pub(crate) struct FunHeader<'p, 'a> {
    /// The whole declaration, attributes included.
    pub span: (u32, u32),
    /// From the first modifier (or `function`) to the end of the signature.
    pub header: (u32, u32),
    /// Where the modifier list starts, or would be if there were one.
    pub modifiers_pos: u32,
    /// The function's `fun_flags`.
    pub flags: u8,
    pub m: &'p Modifiers,
    pub attrs: &'a [Attribute<'a>],
    pub name: Id<'a>,
    /// Extent of `<...>` after the name; empty when there is none.
    pub tparams_span: (u32, u32),
    pub params: &'a [Param<'a>],
    pub params_span: (u32, u32),
    pub ret: Option<Hint<'a>>,
    /// Position of the `;` of a declaration without a body.
    pub semicolon: Option<u32>,
    pub is_method: bool,
}

/// Magic methods HHVM no longer supports.
fn is_unsupported_magic_method(name: &str) -> bool {
    matches!(
        name,
        "__call" | "__callStatic" | "__get" | "__isset" | "__set" | "__unset"
    )
}

/// Constructors and magic methods; these cannot be `async` (and cannot
/// `yield`). `__disposeAsync` and `__invoke` are exempt from one each.
pub(crate) fn is_magic_method(name: &str) -> bool {
    matches!(
        name,
        "__construct"
            | "__destruct"
            | "__call"
            | "__callStatic"
            | "__clone"
            | "__debugInfo"
            | "__dispose"
            | "__disposeAsync"
            | "__get"
            | "__invoke"
            | "__isset"
            | "__set"
            | "__set_state"
            | "__sleep"
            | "__toString"
            | "__unset"
            | "__wakeup"
    )
}

impl<'a> Parser<'a> {
    /// Checks a function or method declaration once all of it is parsed.
    pub(crate) fn check_function(&mut self, h: &FunHeader<'_, 'a>) {
        let name = h.name.name;
        let is_construct = name == "__construct";
        let promoted = h.params.iter().any(|p| p.visibility.is_some());
        if let Some(ret) = &h.ret {
            self.check_async_return(h.flags, ret);
        }
        if is_construct {
            if let Some(ret) = &h.ret
                && !matches!(ret.kind, HintKind::Name("void"))
            {
                self.report(
                    ret.span.start,
                    ret.span.end,
                    "A constructor cannot have a non-`void` type annotation.",
                );
            }
            if h.tparams_span.0 != h.tparams_span.1 {
                self.report(
                    h.tparams_span.0,
                    h.tparams_span.1,
                    "Generic type parameters are not allowed on constructors. Consider adding a type parameter to the class",
                );
            }
            if promoted && h.m.is_abstract {
                self.report(
                    h.params_span.0,
                    h.params_span.1,
                    "Abstract constructors cannot have parameters with visibility modifiers",
                );
            }
        } else if promoted {
            self.report(
                h.params_span.0,
                h.params_span.1,
                "Parameters cannot have visibility modifiers (except in parameter lists of constructors).",
            );
        }
        let class = self.class;
        let in_interface = matches!(class, Some(c) if c.kind == ClassKind::Interface);
        if promoted && matches!(class, Some(c) if c.kind != ClassKind::Class) {
            self.report(
                h.params_span.0,
                h.params_span.1,
                "Traits or interfaces cannot have parameters with visibility modifiers",
            );
        }
        let m = h.m;
        let native = !h.attrs.is_empty() && has_attr(h.attrs, "__Native");
        if self.nested_function {
            self.report(
                h.header.0,
                h.header.1,
                "Declarations are not supported outside global scope",
            );
        } else if !h.is_method {
            self.check_redeclared_function(h);
            self.check_declared_value_name(h.name, true);
        }
        if !h.is_method {
            self.check_modifiers(m, "Top-level functions", true, |k| {
                matches!(k, T::Async | T::Internal | T::Public)
            });
            if let Some(semi) = h.semicolon
                && !self.hhi
                && !native
            {
                self.report(semi, semi + 1, "Non-native functions must have a body");
            }
            if !h.attrs.is_empty() && has_attr(h.attrs, "__MemoizeLSB") {
                self.report(
                    h.span.0,
                    h.span.1,
                    "`<<__MemoizeLSB>>` can only be applied to methods",
                );
            }
            return;
        }

        let class_name = class.map_or("", |c| c.name);
        if class.is_some() {
            self.class_members.push(MemberName {
                name,
                kind: MemberName::METHOD,
                span: h.header,
            });
            if m.is_abstract {
                self.class_has_abstract_method = true;
            }
            if is_construct && promoted {
                self.ctor_promotion = Some((h.params_span, h.params));
            }
        }
        // Errors about the modifiers as a whole sit on the modifier list,
        // or where it would be when there are no modifiers.
        let modifiers = m.span().unwrap_or((h.modifiers_pos, h.modifiers_pos));
        if in_interface && !h.attrs.is_empty() && has_attr(h.attrs, "__Memoize") {
            self.report(
                h.header.0,
                h.header.1,
                "`__Memoize` is not allowed on interface methods",
            );
        }
        if name.starts_with("__") {
            if is_construct && m.is_static {
                self.report(
                    modifiers.0,
                    modifiers.1,
                    format!("Constructor `{}::{}()` cannot be static", class_name, name),
                );
            }
            if is_unsupported_magic_method(name) {
                self.report(
                    h.header.0,
                    h.header.1,
                    format!("Magic `{}` methods are no longer supported", name),
                );
            }
            if m.is_async && name != "__disposeAsync" && is_magic_method(name) {
                self.report(
                    modifiers.0,
                    modifiers.1,
                    format!(
                        "Cannot declare constructors and magic methods like `{}` as `async`",
                        name
                    ),
                );
            }
            if name == "__clone" {
                if !h.params.is_empty() {
                    self.report(
                        modifiers.0,
                        modifiers.1,
                        format!(
                            "Method `{}::{}` cannot accept any arguments",
                            class_name, name
                        ),
                    );
                }
                if m.is_static {
                    self.report(
                        modifiers.0,
                        modifiers.1,
                        format!("Clone method `{}::{}()` cannot be static", class_name, name),
                    );
                }
            }
        }
        self.check_modifiers(m, "Methods", false, |_| true);
        if in_interface {
            self.check_modifiers(m, "Interface methods", false, |k| {
                k != T::Final && k != T::Abstract
            });
        }
        if let Some(semi) = h.semicolon
            && !m.is_abstract
            && !in_interface
            && !native
            && !self.hhi
        {
            self.report(
                semi,
                semi + 1,
                "A method must have a body or be marked `abstract`.",
            );
        }
        if m.is_abstract {
            if m.has_private {
                self.report(
                    modifiers.0,
                    modifiers.1,
                    format!(
                        "Cannot declare abstract method `{}::{}` `private`",
                        class_name, name
                    ),
                );
            }
            if let Some((start, end)) = m.first(T::Final) {
                self.report(
                    start,
                    end,
                    format!(
                        "Cannot declare abstract method `{}::{}` `final`",
                        class_name, name
                    ),
                );
            }
        }
        if m.is_readonly && m.is_static {
            self.report(
                h.span.0,
                h.span.1,
                "Static methods do not need to be marked readonly",
            );
        }
        if !m.is_static && !h.attrs.is_empty() && has_attr(h.attrs, "__MemoizeLSB") {
            self.report(
                h.span.0,
                h.span.1,
                "`<<__MemoizeLSB>>` can only be applied to static methods",
            );
        }
        if let Some((start, end)) = m.first(T::Async) {
            if in_interface {
                self.report(
                    start,
                    end,
                    "`async` cannot be used on a method in an interface. Use an `Awaitable<...>` return type instead.",
                );
            }
            if m.is_abstract {
                self.report(
                    start,
                    end,
                    "`async` cannot be used on an `abstract` method. Use an `Awaitable<...>` return type instead.",
                );
            }
            if m.tokens().last().is_some_and(|t| t.0 != T::Async) {
                self.report(
                    modifiers.0,
                    modifiers.1,
                    "The `async` modifier must be directly before the `function` keyword.",
                );
            }
        }
    }
}

/// A name declared in the class being parsed, with the extent HHVM reports
/// a redeclaration at.
#[derive(Clone, Copy)]
pub(crate) struct MemberName<'a> {
    pub name: &'a str,
    pub kind: u8,
    pub span: (u32, u32),
}

impl MemberName<'_> {
    pub const PROPERTY: u8 = 0;
    /// A constant, type constant or context constant; the extent is the
    /// whole member.
    pub const CONST: u8 = 1;
    pub const XHP_ATTRIBUTE: u8 = 2;
    pub const METHOD: u8 = 3;
    /// One `NAME = value` of a constant declaration.
    pub const CONST_ITEM: u8 = 4;
    /// A method whose body is `= Trait::method;` (see
    /// `parse_trait_resolution`), outside any namespace.
    pub const TRAIT_RESOLUTION: u8 = 5;
}

/// The pieces of a class, interface or trait declaration its checks need.
pub(crate) struct ClassDecl<'p, 'a> {
    pub span: (u32, u32),
    pub m: &'p Modifiers,
    pub has_xhp_keyword: bool,
    pub kind: ClassKind,
    pub name: Id<'a>,
    pub attrs: &'a [Attribute<'a>],
    /// Extent of the `<<...>>` in front of the declaration.
    pub attrs_span: (u32, u32),
    pub extends_kw: Option<(u32, u32)>,
    /// Extent of the list after `extends`.
    pub extends_span: (u32, u32),
    pub extends_count: usize,
    pub has_implements: bool,
    pub members: &'a [Member<'a>],
}

/// `n1\n2`, the way HHVM joins a namespace and a name.
fn combine_names(n1: &str, n2: &str) -> String {
    match (n2.starts_with('\\'), n1.ends_with('\\')) {
        (true, true) => format!("{}{}", n1, &n2[1..]),
        (false, false) => format!("{}\\{}", n1, n2),
        _ => format!("{}{}", n1, n2),
    }
}

impl<'a> Parser<'a> {
    /// Members of an interface cannot be `private`.
    pub(crate) fn check_member_modifiers(&mut self, m: &Modifiers) {
        if m.has_private && matches!(self.class, Some(c) if c.kind == ClassKind::Interface) {
            for &(kind, start) in m.tokens() {
                if kind == T::Private {
                    self.report(
                        start,
                        start + 7,
                        "Interface methods must be `public` or `protected`",
                    );
                }
            }
        }
    }

    /// Checks a property declaration; `start` is where it begins (attributes
    /// included) and the last consumed token is its `;`.
    pub(crate) fn check_property(
        &mut self,
        start: u32,
        attrs: &'a [Attribute<'a>],
        m: &Modifiers,
        items: &[PropItem<'a>],
    ) {
        let end = self.prev_end;
        self.check_member_modifiers(m);
        self.check_modifiers(m, "Properties", false, |k| {
            matches!(
                k,
                T::Static | T::Private | T::Protected | T::Internal | T::Public | T::Readonly
            )
        });
        if m.visibility.is_none() {
            self.report(
                start,
                end,
                "Property declarations require a visibility modifier such as `public`, `private` or `protected`.",
            );
        }
        if m.is_abstract && m.has_private {
            self.report(start, end, "Cannot declare abstract properties `private`.");
        }
        if m.is_static && matches!(self.class, Some(c) if c.has_reified) {
            self.report(
                start,
                end,
                "You may not use static properties in a class with reified type parameters",
            );
        }
        if !attrs.is_empty() && has_attr(attrs, "__Const") && has_attr(attrs, "__LateInit") {
            self.report(
                start,
                end,
                "`__Const` properties may not also be `__LateInit`",
            );
        }
        if self.class.is_some() {
            for item in items {
                self.class_members.push(MemberName {
                    name: item.name.name,
                    kind: MemberName::PROPERTY,
                    span: (start, end),
                });
            }
        }
    }

    /// Checks a class, interface or trait once its body has been parsed.
    pub(crate) fn check_class(&mut self, c: &ClassDecl<'_, 'a>) {
        let (start, end) = c.span;
        let m = c.m;
        let is_class = c.kind == ClassKind::Class;
        if !is_class && let Some((s, e)) = m.first(T::Abstract) {
            self.report(s, e, "Only classes may be declared `abstract`.");
        }
        let mut sealed = false;
        let mut is_const = false;
        for attr in c.attrs {
            match attr.name.name {
                "__Sealed" => {
                    sealed = true;
                    for arg in attr.args {
                        if let ExprKind::ClassConst(access) = arg.kind
                            && matches!(
                                access.class.kind,
                                ExprKind::Id("self" | "parent" | "static")
                            )
                        {
                            self.report(
                                access.class.span.start,
                                access.class.span.end,
                                "`__Sealed` can only be used with named types, e.g. `Foo::class`",
                            );
                        }
                    }
                }
                "__Const" => is_const = true,
                _ => {}
            }
        }
        // (No type at all after `extends` is another error.)
        if is_class && c.extends_kw.is_some() && c.extends_count > 1 {
            self.report(
                c.extends_span.0,
                c.extends_span.1,
                "A class may extend at most **one** other class.",
            );
        }
        self.check_modifiers(m, "Classes, interfaces, and traits", true, |_| true);
        if sealed {
            let bad_arg = c.attrs.iter().any(|attr| {
                attr.name.name == "__Sealed"
                    && attr.args.iter().any(|arg| {
                        !matches!(arg.kind, ExprKind::ClassConst(a) if a.name.name == "class")
                    })
            });
            if bad_arg {
                self.report(
                    c.attrs_span.0,
                    c.attrs_span.1,
                    "Values in sealed allowlist must be classname constants.",
                );
            }
        }
        if let (ClassKind::Trait, Some((s, e))) = (c.kind, c.extends_kw) {
            self.report(
                s,
                e,
                "Only interfaces and classes may extend other interfaces and classes.",
            );
        }
        if sealed && m.is_final {
            self.report(
                c.attrs_span.0,
                c.attrs_span.1,
                "Classes cannot be both `final` and `sealed`.",
            );
        }
        self.check_type_name(c.name);
        if c.kind == ClassKind::Interface && c.has_implements {
            self.report(
                start,
                end,
                "Interfaces may not implement other interfaces or classes",
            );
        }
        if is_const {
            if is_class && m.is_abstract && m.is_final {
                // HHVM makes this check in two places.
                for _ in 0..2 {
                    self.report(
                        start,
                        end,
                        "Cannot apply `__Const` attribute to an abstract final class",
                    );
                }
            } else {
                self.report(
                    start,
                    end,
                    "Classes, interfaces, traits and enums may not be declared `__Const`",
                );
            }
        }
        if m.is_final && !is_class {
            self.report(
                start,
                end,
                if c.kind == ClassKind::Interface {
                    "Interfaces cannot be declared `final`."
                } else {
                    "Traits cannot be declared `final`."
                },
            );
        }
        if c.has_xhp_keyword && !is_class {
            self.report(
                start,
                end,
                if c.kind == ClassKind::Interface {
                    "Interfaces are not valid xhp classes."
                } else {
                    "Traits are not valid xhp classes."
                },
            );
        }
        if matches!(self.class, Some(info) if info.has_reified) && !is_class {
            self.report(
                start,
                end,
                if c.kind == ClassKind::Interface {
                    "Invalid to use a reified type within an interface's type parameters"
                } else {
                    "Invalid to use a reified type within a trait's type parameters"
                },
            );
        }
        self.check_redeclared_members(c);
        if is_class && !m.is_abstract && self.class_has_abstract_method {
            self.report(
                c.name.span.start,
                c.name.span.end,
                format!(
                    "Class `{}` contains an abstract method and must therefore be declared `abstract`",
                    c.name.name
                ),
            );
        }
        if let Some((params_span, params)) = self.ctor_promotion {
            // A promoted constructor parameter declares a property, which
            // must not also be declared in the class body.
            let clash = params.iter().filter(|p| p.visibility.is_some()).find(|p| {
                c.members.iter().any(|member| {
                    matches!(member, Member::Property(decl)
                        if decl.items.iter().any(|item| item.name.name == p.name.name))
                })
            });
            if let Some(p) = clash {
                self.report(
                    params_span.0,
                    params_span.1,
                    format!("Cannot redeclare `{}::{}`", c.name.name, p.name.name),
                );
            }
        }
        // Only the last of several `category` / `children` declarations is
        // reported.
        let mut categories = 0;
        let mut children = 0;
        let mut last_category = (0, 0);
        let mut last_children = (0, 0);
        for member in c.members {
            match member {
                Member::XhpCategory(decl) => {
                    categories += 1;
                    last_category = (decl.span.start, decl.span.end);
                }
                Member::XhpChildren(decl) => {
                    children += 1;
                    last_children = (decl.span.start, decl.span.end);
                }
                _ => {}
            }
        }
        if categories > 1 {
            self.report(
                last_category.0,
                last_category.1,
                "An XHP class cannot have multiple category declarations",
            );
        }
        if children > 1 {
            self.report(
                last_children.0,
                last_children.1,
                "An XHP class cannot have multiple children declarations",
            );
        }
    }

    /// Reports properties, constants, XHP attributes and methods declared
    /// more than once in the class just parsed.
    fn check_redeclared_members(&mut self, c: &ClassDecl<'_, 'a>) {
        let n = self.class_members.len();
        if n < 2 {
            return;
        }
        // Find, for each name, whether an earlier member of the same kind
        // has it. Small classes are compared pairwise; larger ones go
        // through a set.
        let mut repeats: SmallVec<[usize; 4]> = SmallVec::new();
        // HHVM keeps one list of names for methods and for what it takes
        // for functions (see `MemberName::TRAIT_RESOLUTION`), and compares
        // a new one with the last of its name only.
        let callable =
            |kind: u8| kind == MemberName::METHOD || kind == MemberName::TRAIT_RESOLUTION;
        let resolutions = self
            .class_members
            .iter()
            .any(|member| member.kind == MemberName::TRAIT_RESOLUTION);
        if n <= 24 || resolutions {
            for i in 1..n {
                let a = self.class_members[i];
                // A member without a name is another error.
                if a.name.is_empty() {
                    continue;
                }
                let earlier = &self.class_members[..i];
                let repeated = if callable(a.kind) {
                    earlier
                        .iter()
                        .rev()
                        .find(|b| callable(b.kind) && b.name == a.name)
                        .is_some_and(|b| b.kind == a.kind)
                } else {
                    earlier.iter().any(|b| b.kind == a.kind && b.name == a.name)
                };
                if repeated {
                    repeats.push(i);
                }
            }
        } else {
            let mut seen = std::collections::HashSet::with_capacity_and_hasher(
                n,
                std::hash::BuildHasherDefault::<NameHasher>::default(),
            );
            for (i, member) in self.class_members.iter().enumerate() {
                if !member.name.is_empty() && !seen.insert((member.kind, member.name)) {
                    repeats.push(i);
                }
            }
        }
        if repeats.is_empty() {
            return;
        }
        let full_name = combine_names(&self.ns_name, c.name.name);
        let full_name = full_name.strip_prefix('\\').unwrap_or(&full_name);
        for i in repeats {
            let member = self.class_members[i];
            let message = match member.kind {
                MemberName::METHOD | MemberName::TRAIT_RESOLUTION => {
                    if c.kind != ClassKind::Class {
                        // For interfaces and traits HHVM's message is that
                        // for functions: it names the file and the line of
                        // the declaration before this one.
                        let earlier = self.class_members[..i]
                            .iter()
                            .rev()
                            .find(|b| b.kind == member.kind && b.name == member.name)
                            .map_or(0, |b| self.offset_in_source(b.name));
                        format!(
                            "Cannot redeclare `{}()` (previously declared in {}:{})",
                            member.name,
                            self.path,
                            self.line_of(earlier)
                        )
                    } else {
                        format!("Redeclared method `{}::{}`", c.name.name, member.name)
                    }
                }
                MemberName::CONST_ITEM => {
                    format!("Cannot redeclare `{}::{}`", c.name.name, member.name)
                }
                _ => format!("Cannot redeclare `{}::{}`", full_name, member.name),
            };
            self.report(member.span.0, member.span.1, message);
        }
    }
}

use smallvec::SmallVec;

/// FNV-1a; member names are short and this is only used for large classes.
#[derive(Default)]
struct NameHasher(u64);

impl std::hash::Hasher for NameHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 {
            0xcbf29ce484222325
        } else {
            self.0
        };
        for &b in bytes {
            h = (h ^ b as u64).wrapping_mul(0x100000001b3);
        }
        self.0 = h;
    }
}

// ---------------------------------------------------------------------------
// Checks over function bodies and initializers
//
// These need to know where an expression sits (which function it is in,
// whether that function is static, async, a generator, what contexts it
// declares, ...), so they run over each top-level definition once it has been
// parsed.

/// What kind of function the code being checked is directly inside.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Callable {
    /// Not in a function at all (top-level statement, initializer).
    None,
    /// A function or method declaration.
    Decl,
    /// `(...) ==> ...`
    Lambda,
    /// `function(...) {...}` or `async {...}`
    Anon,
}

/// Whether `$$` may be used.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PipeVar {
    Unavailable,
    /// Inside a lambda on the right of a pipe.
    Captured,
    Available,
}

/// The syntactic position `You cannot use this syntax ...` is about.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LvalRoot {
    Unset,
    Assignment,
    Inout,
    IncrementOrDecrement,
    Foreach,
}

impl LvalRoot {
    fn message(self) -> &'static str {
        match self {
            LvalRoot::Unset => "You cannot use this syntax in an `unset()` expression.",
            LvalRoot::Assignment => {
                "You cannot use this syntax on the left-hand side of an assignment."
            }
            LvalRoot::Inout => "You cannot use this syntax with `inout`.",
            LvalRoot::IncrementOrDecrement => {
                "You cannot use this syntax with an increment or decrement statement."
            }
            LvalRoot::Foreach => {
                "You cannot use this syntax as the iterator variable in a `foreach` loop."
            }
        }
    }
}

/// What a nested destructuring target is nested in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nesting {
    None,
    List,
    ShapeTuple,
}

// Capabilities of the enclosing function, as far as its context list says.
const CO_COMPLEX: u8 = 1; // in a function body (or an enum initializer)
const CO_CTOR_OR_CLONE: u8 = 1 << 1;
const CO_IGNORE: u8 = 1 << 2; // `<<__IgnoreCoeffectLocalErrors>>`
const CO_DEFAULTS: u8 = 1 << 3;
const CO_WRITE_PROPS: u8 = 1 << 4;
const CO_WRITE_THIS_PROPS: u8 = 1 << 5;
const CO_READ_GLOBALS: u8 = 1 << 6;
const CO_GLOBALS: u8 = 1 << 7;
/// What a nested declaration inherits from the one around it.
const CO_INHERITED: u8 =
    CO_COMPLEX | CO_CTOR_OR_CLONE | CO_IGNORE | CO_DEFAULTS | CO_WRITE_PROPS | CO_WRITE_THIS_PROPS;
// Flags for `check_expr`.
const STATEMENT: u8 = 1;
const NO_CAPABILITY_CHECK: u8 = 2;

/// Bits of `Parser::marks`.
pub(crate) const MARK_AWAIT: u8 = 1;
pub(crate) const MARK_INOUT: u8 = 2;

const AWAIT_ELSEWHERE: &str = "`await` cannot be used in this location";

/// Something an expression being checked is part of, as far as an `await`
/// in it is concerned.
#[derive(Clone, Copy)]
pub(crate) enum Ancestor<'a> {
    Expr(Expr<'a>),
    /// A statement that runs the `await`s in this expression before
    /// anything else of it, or a lambda that has it for a body.
    Root,
    /// `while` or `do`, with this for a condition.
    Condition,
    /// `match`, with this for a subject.
    Subject,
    /// The condition or the step of a `for`, which run from this first
    /// expression to this last one.
    Clause(Span, Span),
    /// A parameter that has this for a default.
    ParamDefault,
    /// Any other place an expression can be in.
    Elsewhere,
}

/// What its surroundings make of an `await`.
enum AwaitPlace {
    /// Up to what is further out.
    Fine,
    /// Allowed, whatever is further out.
    Root,
    /// Run only some of the time, which takes an unstable feature that is
    /// asked for at this span.
    Conditional(Span),
    /// The same in the head of a loop, where nothing further out matters.
    Loop(Span),
    Invalid(&'static str),
}

const CO_CAPABILITIES: u8 =
    CO_DEFAULTS | CO_WRITE_PROPS | CO_WRITE_THIS_PROPS | CO_READ_GLOBALS | CO_GLOBALS;

#[derive(Clone, Copy)]
struct Scope<'a> {
    /// Inside a class, interface or trait.
    in_class: bool,
    in_enum_class: bool,
    /// Inside a function or method declaration (possibly within lambdas).
    in_function: bool,
    /// That declaration is a static method.
    is_static: bool,
    /// Its name, or `""`.
    fn_name: &'a str,
    callable: Callable,
    is_async: bool,
    is_generator: bool,
    /// The enclosing declaration or lambda has an `inout` parameter.
    has_inout: bool,
    pipe: PipeVar,
    in_concurrent: bool,
    /// Coeffect capabilities (`CO_*`).
    co: u8,
    /// The enclosing function has a `*_local` context.
    local_ctx: bool,
    /// In the code of an expression tree literal (not in a splice).
    in_literal: bool,
}

impl Scope<'_> {
    const TOP: Scope<'static> = Scope {
        in_class: false,
        in_enum_class: false,
        in_function: false,
        is_static: false,
        fn_name: "",
        callable: Callable::None,
        is_async: false,
        is_generator: false,
        has_inout: false,
        pipe: PipeVar::Unavailable,
        in_concurrent: false,
        co: 0,
        local_ctx: false,
        in_literal: false,
    };

    fn has_this(&self) -> bool {
        self.in_class && !(self.in_function && self.is_static)
    }
}

/// The last segment of a possibly qualified name.
fn last_segment(name: &str) -> &str {
    match name.rfind('\\') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

/// Which capabilities a context grants (`CO_*` bits), or `None` for a name
/// that is not a built-in context.
fn context_capabilities(name: &str) -> Option<u8> {
    Some(match name {
        "defaults" => {
            CO_DEFAULTS | CO_WRITE_PROPS | CO_WRITE_THIS_PROPS | CO_GLOBALS | CO_READ_GLOBALS
        }
        "pure" => 0,
        "rx_local" | "rx_shallow" | "rx" => CO_WRITE_PROPS | CO_WRITE_THIS_PROPS,
        "write_this_props" => CO_WRITE_THIS_PROPS,
        "write_props" => CO_WRITE_PROPS | CO_WRITE_THIS_PROPS,
        "zoned_local" | "zoned_shallow" | "zoned" | "leak_safe_local" | "leak_safe_shallow"
        | "leak_safe" => CO_WRITE_PROPS | CO_WRITE_THIS_PROPS | CO_READ_GLOBALS,
        "globals" => CO_GLOBALS | CO_READ_GLOBALS,
        "read_globals" => CO_READ_GLOBALS,
        _ => return None,
    })
}

/// The capabilities a context list provides. Without a list a function has
/// the default capabilities.
fn capabilities_of(ctxs: Option<&Contexts<'_>>) -> u8 {
    let Some(ctxs) = ctxs else {
        return CO_CAPABILITIES;
    };
    let mut caps = 0;
    // Whether the list could include `defaults`: it does when it names it,
    // or names something that is not a known context.
    let mut defaults = false;
    for hint in ctxs.hints {
        match hint.kind {
            HintKind::Name(name) | HintKind::Generic(&GenericHint { name, .. }) => {
                match context_capabilities(last_segment(name)) {
                    Some(c) => {
                        caps |= c & !CO_DEFAULTS;
                        defaults |= c & CO_DEFAULTS != 0;
                    }
                    None => defaults = true,
                }
            }
            HintKind::FunContext(_) => {}
            HintKind::Access(access) => {
                let dependent = match access.root.kind {
                    HintKind::Name(_) | HintKind::Generic(_) => !access.names.is_empty(),
                    HintKind::Var(_) => access.names.len() == 1,
                    _ => false,
                };
                defaults |= !dependent;
            }
            _ => defaults = true,
        }
    }
    if defaults {
        caps |= CO_DEFAULTS;
    }
    caps
}

fn is_local_context(ctxs: Option<&Contexts<'_>>) -> bool {
    ctxs.is_some_and(|c| {
        c.hints.iter().any(|h| {
            matches!(
                h.kind,
                HintKind::Name("zoned_local" | "leak_safe_local" | "rx_local")
            )
        })
    })
}

fn is_superglobal(name: &str) -> bool {
    matches!(name, "$_SERVER" | "$_GET" | "$_POST" | "$_FILES" | "$_ENV")
}

const WRITE_PROPS_ERROR: &str = "Writing to object properties requires the capability WriteProperty, which is not provided by the context. Add the context `[write_props]` after the function parameter list (or amend an existing context with a new comma delineated entry `write_props`).";

impl<'a> Parser<'a> {
    /// Runs the checks that depend on the surrounding function over a
    /// definition that has just been parsed. Only the statements and
    /// initializers the parser flagged while building them are looked at,
    /// so a definition without any costs nothing here.
    pub(crate) fn check_def(&mut self, def: &Def<'a>) {
        if self.flagged.is_empty() {
            return;
        }
        self.flagged.sort_unstable();
        match def {
            Def::Fun(f) => {
                let mut sc = Scope::TOP;
                sc.in_function = true;
                sc.fn_name = f.name.name;
                self.check_declared_fun(&f.fun, f.name.name, sc);
            }
            Def::Class(c) => self.check_class_body(c),
            Def::Stmt(s) => {
                let mut sc = Scope::TOP;
                sc.co = CO_DEFAULTS;
                self.check_stmt(s, &sc, false);
            }
            Def::Const(decl) => {
                for item in decl.items {
                    if let Some(value) = &item.value {
                        self.check_initializer(value, &Scope::TOP);
                    }
                }
            }
            _ => {}
        }
        self.flagged.clear();
    }

    /// Whether the statement or initializer starting at `offset` was flagged.
    #[inline]
    fn is_flagged(&self, offset: u32) -> bool {
        self.flagged.binary_search(&offset).is_ok()
    }

    fn check_initializer(&mut self, value: &Expr<'a>, sc: &Scope<'a>) {
        if self.is_flagged(value.span.start) {
            self.check_under(Ancestor::Elsewhere, value, sc, 0);
        }
    }

    /// Checks an expression that is in `place`.
    fn check_under(&mut self, place: Ancestor<'a>, e: &Expr<'a>, sc: &Scope<'a>, flags: u8) {
        self.ancestors.push(place);
        self.check_expr(e, sc, flags);
        self.ancestors.pop();
    }

    fn check_exprs_under(&mut self, place: Ancestor<'a>, exprs: &[Expr<'a>], sc: &Scope<'a>) {
        self.ancestors.push(place);
        self.check_exprs(exprs, sc);
        self.ancestors.pop();
    }

    fn check_class_body(&mut self, c: &Class<'a>) {
        let mut sc = Scope::TOP;
        match c.kind {
            ClassKind::Class | ClassKind::Interface | ClassKind::Trait => sc.in_class = true,
            ClassKind::EnumClass => {
                sc.in_enum_class = true;
                sc.co = CO_COMPLEX;
            }
            ClassKind::Enum => sc.co = CO_COMPLEX,
        }
        for member in c.members {
            match member {
                Member::Method(m) => {
                    let mut msc = sc;
                    msc.in_function = true;
                    msc.is_static = m.flags & method_flags::STATIC != 0;
                    msc.fn_name = m.name.name;
                    self.check_declared_fun(&m.fun, m.name.name, msc);
                }
                Member::Property(p) => {
                    for item in p.items {
                        if let Some(value) = &item.value {
                            self.check_initializer(value, &sc);
                        }
                    }
                }
                Member::Const(decl) => {
                    for item in decl.items {
                        if let Some(value) = &item.value {
                            self.check_initializer(value, &sc);
                        }
                    }
                }
                Member::Enumerator(e) => self.check_initializer(&e.value, &sc),
                Member::EnumClassMember(m) => {
                    if let Some(value) = &m.value {
                        self.check_initializer(value, &sc);
                    }
                }
                Member::XhpAttributes(decls) => {
                    for decl in *decls {
                        if let XhpAttrDecl::Attr(attr) = decl
                            && let Some(default) = &attr.default
                        {
                            self.check_initializer(default, &sc);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// A function or method declaration: works out what its body may do and
    /// checks parameters and body.
    fn check_declared_fun(&mut self, fun: &Fun<'a>, name: &str, mut sc: Scope<'a>) {
        // Nothing flagged inside this function?
        let first = self.flagged.partition_point(|&o| o < fun.span.start);
        if first == self.flagged.len() || self.flagged[first] >= fun.span.end {
            return;
        }
        sc.callable = Callable::Decl;
        sc.is_async = fun.is_async();
        sc.is_generator = fun.has_yield();
        sc.has_inout = fun.params.iter().any(|p| p.inout.is_some());
        sc.pipe = PipeVar::Unavailable;
        sc.in_concurrent = false;
        sc.local_ctx = is_local_context(fun.ctxs.as_ref());
        sc.co = CO_COMPLEX | capabilities_of(fun.ctxs.as_ref());
        if name == "__construct" || name == "__clone" {
            sc.co |= CO_CTOR_OR_CLONE;
        }
        if !fun.attrs.is_empty() && has_attr(fun.attrs, "__IgnoreCoeffectLocalErrors") {
            sc.co |= CO_IGNORE;
        }
        self.check_fun_parts(fun, &sc);
    }

    fn check_fun_parts(&mut self, fun: &Fun<'a>, sc: &Scope<'a>) {
        for param in fun.params {
            if let Some(default) = &param.default
                && self.is_flagged(default.span.start)
            {
                self.check_under(Ancestor::ParamDefault, default, sc, 0);
            }
        }
        self.check_block(fun.body, sc, true);
    }

    /// A lambda, anonymous function or `async` block.
    fn check_closure(&mut self, fun: &Fun<'a>, kind: Callable, e: &Expr<'a>, outer: &Scope<'a>) {
        if outer.local_ctx && fun.ctxs.is_none() && !matches!(e.kind, ExprKind::AsyncBlock(_)) {
            self.report_tree(
                e.span.start,
                e.span.end,
                "Closures in local contexts require explicit annotation. Consider adding explicit contexts to this closure e.g. [leak_safe_local] or [zoned_local] or [defaults].",
            );
        }
        let mut sc = *outer;
        sc.callable = kind;
        sc.is_async = fun.is_async();
        sc.is_generator = fun.has_yield();
        sc.has_inout = kind == Callable::Lambda && fun.params.iter().any(|p| p.inout.is_some());
        // The functions in an expression tree literal share its `$$`.
        if sc.pipe == PipeVar::Available && !outer.in_literal {
            sc.pipe = PipeVar::Captured;
        }
        sc.in_concurrent = false;
        sc.local_ctx = is_local_context(fun.ctxs.as_ref());
        let mut co = (outer.co & (CO_INHERITED & !CO_IGNORE)) | CO_COMPLEX;
        match fun.ctxs.as_ref() {
            Some(ctxs) => co = (co & !CO_CAPABILITIES) | capabilities_of(Some(ctxs)),
            None => co |= outer.co & CO_CAPABILITIES,
        }
        if !fun.attrs.is_empty() && has_attr(fun.attrs, "__IgnoreCoeffectLocalErrors") {
            co |= CO_IGNORE;
        }
        sc.co = co;
        self.check_fun_parts(fun, &sc);
    }

    /// `fn_body` is set for the statements directly in a function body.
    fn check_block(&mut self, block: Block<'a>, sc: &Scope<'a>, fn_body: bool) {
        for stmt in block {
            self.check_stmt(stmt, sc, fn_body);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt<'a>, sc: &Scope<'a>, fn_body: bool) {
        // The statements HHVM counts an `await` in them (wherever in them
        // it is) as part of. The expression body of a lambda is no statement.
        let roots_awaits = match stmt.kind {
            StmtKind::Expr(_)
            | StmtKind::Throw(_)
            | StmtKind::Echo(_)
            | StmtKind::Unset(_)
            | StmtKind::If(_)
            | StmtKind::For(_)
            | StmtKind::Foreach(_)
            | StmtKind::Switch(_) => true,
            StmtKind::Return(value) => value.is_none_or(|e| e.span.start != stmt.span.start),
            _ => false,
        };
        if roots_awaits {
            self.await_roots.push(*stmt);
        }
        self.check_stmt_parts(stmt, sc, fn_body);
        if roots_awaits {
            self.await_roots.pop();
        }
    }

    fn check_stmt_parts(&mut self, stmt: &Stmt<'a>, sc: &Scope<'a>, fn_body: bool) {
        // The expressions of a statement are only looked at when the parser
        // flagged the statement; nested statements carry their own flags.
        match stmt.kind {
            StmtKind::Noop | StmtKind::Break | StmtKind::Continue | StmtKind::YieldBreak => {}
            StmtKind::Expr(e) => {
                if self.is_flagged(stmt.span.start) {
                    // An assignment is fine as a statement of its own, but
                    // not when wrapped in parentheses.
                    let bare = e.span.start == stmt.span.start;
                    self.check_under(Ancestor::Root, e, sc, if bare { STATEMENT } else { 0 });
                }
            }
            StmtKind::Throw(e) => {
                if self.is_flagged(stmt.span.start) {
                    self.check_under(Ancestor::Root, e, sc, 0);
                }
            }
            StmtKind::Return(value) => {
                let Some(e) = value else { return };
                if stmt.span.start == e.span.start {
                    // The expression body of a lambda, which is part of the
                    // (flagged) statement the lambda is in.
                    self.check_under(Ancestor::Root, e, sc, 0);
                    return;
                }
                if sc.is_generator {
                    self.report(
                        stmt.span.start,
                        stmt.span.end,
                        "You cannot return a value in a generator (a generator is a function that uses `yield`)",
                    );
                }
                if self.is_flagged(stmt.span.start) {
                    self.check_under(Ancestor::Root, e, sc, 0);
                }
            }
            StmtKind::Echo(echo) | StmtKind::Unset(echo) => {
                if self.is_flagged(stmt.span.start) {
                    self.check_exprs_under(Ancestor::Root, echo.args, sc);
                }
            }
            StmtKind::If(node) => {
                if self.is_flagged(stmt.span.start) {
                    self.check_under(Ancestor::Root, &node.cond, sc, 0);
                }
                self.check_block(node.then, sc, false);
                if let Some(otherwise) = node.otherwise {
                    self.check_block(otherwise, sc, false);
                }
            }
            StmtKind::While(node) => {
                if self.is_flagged(stmt.span.start) {
                    self.check_under(Ancestor::Condition, &node.cond, sc, 0);
                }
                self.check_block(node.body, sc, false);
            }
            StmtKind::Do(node) => {
                self.check_block(node.body, sc, false);
                if self.is_flagged(stmt.span.start) {
                    self.check_under(Ancestor::Condition, &node.cond, sc, 0);
                }
            }
            StmtKind::For(node) => {
                if self.is_flagged(stmt.span.start) {
                    for e in node.init {
                        let flags = if self.is_bare(e) { STATEMENT } else { 0 };
                        self.check_under(Ancestor::Root, e, sc, flags);
                    }
                    if let Some(cond) = &node.cond {
                        self.check_under(Ancestor::Clause(cond.span, cond.span), cond, sc, 0);
                    }
                    if let (Some(first), Some(last)) = (node.step.first(), node.step.last()) {
                        let clause = Ancestor::Clause(first.span, last.span);
                        for e in node.step {
                            let flags = if self.is_bare(e) { STATEMENT } else { 0 };
                            self.check_under(clause, e, sc, flags);
                        }
                    }
                }
                self.check_block(node.body, sc, false);
            }
            StmtKind::Foreach(node) => {
                if self.is_flagged(stmt.span.start) {
                    self.check_under(Ancestor::Root, &node.collection, sc, 0);
                    if let Some(key) = &node.key {
                        self.check_under(Ancestor::Elsewhere, key, sc, 0);
                    }
                    self.check_under(Ancestor::Elsewhere, &node.value, sc, 0);
                }
                self.check_block(node.body, sc, false);
            }
            StmtKind::Switch(node) => {
                let flagged = self.is_flagged(stmt.span.start);
                if flagged {
                    self.check_under(Ancestor::Root, &node.subject, sc, 0);
                }
                for case in node.cases {
                    if let (true, Some(label)) = (flagged, &case.label) {
                        self.check_under(Ancestor::Elsewhere, label, sc, 0);
                    }
                    self.check_block(case.body, sc, false);
                }
            }
            StmtKind::Match(node) => {
                if self.is_flagged(stmt.span.start) {
                    self.check_under(Ancestor::Subject, &node.subject, sc, 0);
                }
                for arm in node.arms {
                    self.check_block(arm.body, sc, false);
                }
            }
            StmtKind::Try(node) => {
                self.check_block(node.body, sc, false);
                for catch in node.catches {
                    self.check_block(catch.body, sc, false);
                }
                if let Some(finally) = node.finally {
                    self.check_block(finally, sc, false);
                }
            }
            StmtKind::Using(node) => {
                if node.body.is_none() && !(fn_body && sc.callable != Callable::None) {
                    self.report(
                        stmt.span.start,
                        stmt.span.end,
                        "Using statement in function scoped form may only be used at the top level of a function or a method",
                    );
                }
                if self.is_flagged(stmt.span.start) {
                    for e in node.exprs {
                        let flags = if self.using_expr_is_bare(node, e) {
                            STATEMENT
                        } else {
                            0
                        };
                        self.check_under(Ancestor::Root, e, sc, flags);
                    }
                }
                if let Some(body) = node.body {
                    self.check_block(body, sc, false);
                }
            }
            StmtKind::Concurrent(block) => {
                self.check_concurrent(stmt, block, sc);
                let mut inner = *sc;
                inner.in_concurrent = true;
                self.concurrent_depth += 1;
                self.check_block(block, &inner, false);
                self.concurrent_depth -= 1;
            }
            StmtKind::Block(block) => self.check_block(block, sc, false),
        }
    }

    /// A `concurrent` block holds two or more expression statements, each
    /// with an `await`, and is not nested in another one.
    fn check_concurrent(&mut self, stmt: &Stmt<'a>, block: Block<'a>, sc: &Scope<'a>) {
        let (start, end) = (stmt.span.start, stmt.span.end);
        if sc.in_concurrent {
            self.report(start, end, "`concurrent` blocks cannot be nested.");
        }
        // Without braces `concurrent` is followed by a single statement,
        // which ends where it does.
        if block.last().is_some_and(|last| last.span.end == end) {
            self.report(start, end, INVALID_CONCURRENT);
            return;
        }
        if block.len() < 2 {
            self.report(
                start,
                end,
                "Expected 2 or more statements in concurrent block. `concurrent` wrapping nothing or a single statement is not useful or already implied.",
            );
        }
        for s in block {
            match s.kind {
                StmtKind::Expr(e) => {
                    if !has_await(e) {
                        self.report(
                            s.span.start,
                            s.span.end,
                            "Statement without an `await` in a concurrent block",
                        );
                    }
                }
                _ => self.report(s.span.start, s.span.end, INVALID_CONCURRENT),
            }
        }
        let found = self.errors.len();
        for s in block {
            self.writes_in_stmt(s);
        }
        self.found_at(found, start);
    }

    fn check_exprs(&mut self, exprs: &[Expr<'a>], sc: &Scope<'a>) {
        for e in exprs {
            self.check_expr(e, sc, 0);
        }
    }

    /// Checks an expression and everything in it. `flags` holds `STATEMENT`
    /// when the expression is a statement of its own (or an item of a `for`
    /// header or `using` list), the only place an assignment may appear.
    fn check_expr(&mut self, e: &Expr<'a>, sc: &Scope<'a>, flags: u8) {
        self.ancestors.push(Ancestor::Expr(*e));
        self.check_expr_parts(e, sc, flags);
        self.ancestors.pop();
    }

    fn check_expr_parts(&mut self, e: &Expr<'a>, sc: &Scope<'a>, flags: u8) {
        let statement = flags & STATEMENT != 0;
        // Set when the operands of `e` are exempt from the capability check.
        let mut exempt = 0;
        if sc.co & (CO_COMPLEX | CO_IGNORE | CO_DEFAULTS) == CO_COMPLEX
            && flags & NO_CAPABILITY_CHECK == 0
            && self.check_coeffects(e, sc)
        {
            exempt = NO_CAPABILITY_CHECK;
        }
        match e.kind {
            ExprKind::Null
            | ExprKind::True
            | ExprKind::False
            | ExprKind::Omitted
            | ExprKind::Id(_)
            | ExprKind::Float(_)
            | ExprKind::String(_)
            | ExprKind::Int(_)
            | ExprKind::FunctionPointer(_)
            | ExprKind::EnumClassLabel(_)
            | ExprKind::Package(_) => {}
            ExprKind::Lvar(name) => self.check_variable(e, name, sc, true),
            ExprKind::ExpressionTree(tree) => {
                let mut inner = *sc;
                inner.in_literal = true;
                match tree.body {
                    EtBody::Expr(body) => self.check_expr(&body, &inner, 0),
                    EtBody::Block(block) => self.check_block(block, &inner, false),
                }
            }
            ExprKind::EtSplice(splice) => {
                let mut inner = *sc;
                inner.in_literal = false;
                self.check_expr(&splice.expr, &inner, 0);
            }
            ExprKind::String2(parts) => {
                for part in parts {
                    self.check_string_part(part, sc);
                }
            }
            ExprKind::PrefixedString(p) => self.check_expr(&p.expr, sc, 0),
            ExprKind::Shape(fields, _) => {
                for field in fields {
                    self.check_expr(field.value(), sc, 0);
                }
            }
            ExprKind::ValCollection(c) => self.check_exprs(c.items, sc),
            ExprKind::KeyValCollection(c) => {
                for (k, v) in c.fields {
                    self.check_expr(k, sc, 0);
                    self.check_expr(v, sc, 0);
                }
            }
            ExprKind::Collection(c) => {
                for field in c.fields {
                    match field {
                        Afield::Value(v) => self.check_expr(v, sc, 0),
                        Afield::KeyValue(k, v) => {
                            self.check_expr(k, sc, 0);
                            self.check_expr(v, sc, 0);
                        }
                    }
                }
            }
            ExprKind::Tuple(items, _) | ExprKind::List(items) => self.check_exprs(items, sc),
            ExprKind::Readonly(inner) => self.check_expr(inner, sc, exempt),
            ExprKind::Await(inner) => {
                self.check_await(e);
                self.check_expr(inner, sc, 0);
            }
            ExprKind::Clone(inner)
            | ExprKind::OptionalEntry(inner)
            | ExprKind::Delay(inner)
            | ExprKind::Print(inner)
            | ExprKind::Import(_, inner)
            | ExprKind::Nameof(inner)
            | ExprKind::Dollar(inner)
            | ExprKind::InoutArg(inner) => self.check_expr(inner, sc, 0),
            ExprKind::NamedArg(arg) => self.check_expr(&arg.value, sc, 0),
            ExprKind::ArrayGet(get) => {
                self.check_expr(&get.base, sc, 0);
                if let Some(index) = &get.index {
                    self.check_expr(index, sc, 0);
                }
            }
            ExprKind::ObjGet(get) => {
                self.check_expr(&get.object, sc, 0);
                if !matches!(get.member.kind, ExprKind::Id(_) | ExprKind::Lvar(_)) {
                    self.check_expr(&get.member, sc, 0);
                }
            }
            ExprKind::ClassGet(access) | ExprKind::ClassConst(access) => {
                self.check_scope_resolution(e, access, sc.in_class);
                self.check_expr(&access.class, sc, 0);
            }
            ExprKind::Call(call) => {
                self.check_expr(&call.func, sc, 0);
                self.check_exprs(call.args, sc);
                if let Some(unpacked) = call.unpacked {
                    self.check_expr(unpacked, sc, 0);
                }
            }
            ExprKind::Yield(field) => {
                self.check_yield(e, sc);
                match field {
                    Afield::Value(v) => self.check_expr(v, sc, 0),
                    Afield::KeyValue(k, v) => {
                        self.check_expr(k, sc, 0);
                        self.check_expr(v, sc, 0);
                    }
                }
            }
            ExprKind::Cast(cast) => self.check_expr(&cast.expr, sc, 0),
            ExprKind::Unop(op, operand) => {
                if !statement && matches!(op, Uop::Incr | Uop::Decr | Uop::PostIncr | Uop::PostDecr)
                {
                    self.report_assignment_in_expression(operand);
                }
                self.check_expr(operand, sc, 0);
            }
            ExprKind::Binop(b) => {
                self.check_expr(&b.lhs, sc, 0);
                self.check_expr(&b.rhs, sc, 0);
            }
            ExprKind::Assign(assign) => {
                if !statement {
                    self.report_assignment_in_expression(&assign.lhs);
                }
                self.check_expr(&assign.lhs, sc, exempt);
                self.check_expr(&assign.rhs, sc, exempt);
            }
            ExprKind::Pipe(pipe) => {
                self.check_expr(&pipe.lhs, sc, 0);
                let mut inner = *sc;
                inner.pipe = PipeVar::Available;
                self.check_expr(&pipe.rhs, &inner, 0);
            }
            ExprKind::Eif(c) => {
                self.check_expr(&c.cond, sc, 0);
                if let Some(then) = &c.then {
                    self.check_expr(then, sc, 0);
                }
                self.check_expr(&c.otherwise, sc, 0);
            }
            ExprKind::Is(test)
            | ExprKind::As(test)
            | ExprKind::NullableAs(test)
            | ExprKind::Upcast(test) => self.check_expr(&test.expr, sc, 0),
            ExprKind::New(new) => {
                self.check_expr(&new.class, sc, 0);
                self.check_exprs(new.args, sc);
                if let Some(unpacked) = new.unpacked {
                    self.check_expr(unpacked, sc, 0);
                }
            }
            ExprKind::Efun(efun) => self.check_closure(&efun.fun, Callable::Anon, e, sc),
            ExprKind::Lfun(fun) => self.check_closure(fun, Callable::Lambda, e, sc),
            ExprKind::AsyncBlock(fun) => self.check_closure(fun, Callable::Anon, e, sc),
            ExprKind::Xml(xml) => {
                for attr in xml.attrs {
                    match attr {
                        XhpAttribute::Simple(_, value) => self.check_expr(value, sc, 0),
                        XhpAttribute::Spread(value) => self.check_expr(value, sc, 0),
                    }
                }
                self.check_exprs(xml.children, sc);
            }
        }
    }

    /// An `await` runs when the statement it is in does, before the rest of
    /// it; one whose turn would depend on that rest is not allowed. Follows
    /// HHVM's walk from the `await` outwards.
    fn check_await(&mut self, e: &Expr<'a>) {
        let conditional = self.features & Feature::AllowConditionalAwaitSyntax.bit() != 0;
        let mut child = *e;
        // Coming out of a splice, nothing in the expression tree around it
        // has a say.
        let mut in_tree = false;
        // The innermost entry is `e` itself.
        for i in (0..self.ancestors.len() - 1).rev() {
            let place = match self.ancestors[i] {
                Ancestor::Expr(parent) => {
                    let place = match parent.kind {
                        ExprKind::ExpressionTree(_) => {
                            in_tree = false;
                            AwaitPlace::Fine
                        }
                        _ if in_tree => AwaitPlace::Fine,
                        ExprKind::EtSplice(_) => {
                            in_tree = true;
                            AwaitPlace::Fine
                        }
                        _ => self.await_place(&parent, &child),
                    };
                    child = parent;
                    place
                }
                _ if in_tree => AwaitPlace::Fine,
                Ancestor::Root => AwaitPlace::Root,
                Ancestor::Subject if conditional => AwaitPlace::Invalid(AWAIT_ELSEWHERE),
                Ancestor::Subject | Ancestor::Condition => {
                    AwaitPlace::Loop(self.term_span(child.span, true))
                }
                Ancestor::Clause(first, last) => {
                    AwaitPlace::Loop(self.list_span(first, last, false))
                }
                Ancestor::ParamDefault => {
                    AwaitPlace::Invalid("`await` cannot be used in a parameter default value")
                }
                Ancestor::Elsewhere => AwaitPlace::Invalid(AWAIT_ELSEWHERE),
            };
            match place {
                AwaitPlace::Fine => {}
                AwaitPlace::Conditional(_) if conditional => {}
                AwaitPlace::Root => break,
                AwaitPlace::Conditional(node) | AwaitPlace::Loop(node) => {
                    self.require_feature(
                        node.start,
                        node.end,
                        Feature::AllowConditionalAwaitSyntax,
                    );
                    break;
                }
                AwaitPlace::Invalid(message) => {
                    self.report(e.span.start, e.span.end, message);
                    break;
                }
            }
        }
        // HHVM looks over the statement once for every `await` in it.
        if self.concurrent_depth == 0
            && let Some(root) = self.await_roots.last().copied()
        {
            let found = self.errors.len();
            self.writes_in_stmt(&root);
            self.found_at(found, e.span.start);
        }
    }

    /// The errors from `found` on are ones HHVM finds when it gets to
    /// `offset`, ahead of those about what comes after it. Moves them in
    /// front of those, for errors about the same text to end up in HHVM's
    /// order.
    fn found_at(&mut self, found: usize, offset: u32) {
        let count = self.errors.len() - found;
        if count > 0
            && let Some(at) = self.errors[..found].iter().position(|e| e.start >= offset)
        {
            self.errors[at..].rotate_right(count);
        }
    }

    /// Whether an item of a list is written without parentheses around it.
    fn is_bare(&self, item: &Expr<'a>) -> bool {
        self.term_span(item.span, false) == item.span
    }

    /// The same for an expression of a `using`, which may be on its own
    /// inside the parentheses of the statement.
    fn using_expr_is_bare(&self, node: &Using<'a>, e: &Expr<'a>) -> bool {
        match node.exprs {
            [_] => e.span.start == node.exprs_span.start,
            _ => self.is_bare(e),
        }
    }

    /// What `parent` makes of an `await` in its operand `child`.
    fn await_place(&self, parent: &Expr<'a>, child: &Expr<'a>) -> AwaitPlace {
        let is = |operand: &Expr<'a>| operand.span == child.span;
        let conditional = || AwaitPlace::Conditional(self.term_span(child.span, false));
        match parent.kind {
            ExprKind::Shape(..)
            | ExprKind::ValCollection(_)
            | ExprKind::KeyValCollection(_)
            | ExprKind::Collection(_)
            | ExprKind::Tuple(..)
            | ExprKind::Clone(_)
            | ExprKind::Readonly(_)
            | ExprKind::Print(_)
            | ExprKind::Await(_)
            | ExprKind::ArrayGet(_)
            | ExprKind::ClassGet(_)
            | ExprKind::ClassConst(_)
            | ExprKind::Yield(_)
            | ExprKind::Cast(_)
            | ExprKind::Unop(Uop::Not | Uop::Tilde | Uop::Plus | Uop::Minus, _)
            | ExprKind::Is(_)
            | ExprKind::As(_)
            | ExprKind::NullableAs(_)
            | ExprKind::New(_)
            | ExprKind::Xml(_) => AwaitPlace::Fine,
            // Whether the right-hand side runs depends on the left.
            ExprKind::Binop(b) => match b.op {
                Bop::AmpAmp | Bop::BarBar | Bop::QuestionQuestion if !is(&b.lhs) => conditional(),
                _ => AwaitPlace::Fine,
            },
            ExprKind::Assign(assign) => {
                if is(&assign.rhs) && assign.op != Some(Bop::QuestionQuestion) {
                    AwaitPlace::Fine
                } else {
                    conditional()
                }
            }
            ExprKind::Pipe(pipe) if !pipe.nullsafe || is(&pipe.lhs) => AwaitPlace::Fine,
            ExprKind::Eif(c) if is(&c.cond) => AwaitPlace::Fine,
            ExprKind::ObjGet(get) if !get.nullsafe || is(&get.object) => AwaitPlace::Fine,
            // The arguments of `$a?->f(...)` are not evaluated when `$a` is
            // null. HHVM points at all of them.
            ExprKind::Call(call) => match call.func.kind {
                ExprKind::ObjGet(get) if get.nullsafe && get.is_method && !is(&call.func) => {
                    let last = call.unpacked.or(call.args.last());
                    match (call.args.first().or(call.unpacked), last) {
                        (Some(first), Some(last)) => {
                            AwaitPlace::Conditional(self.list_span(first.span, last.span, true))
                        }
                        _ => conditional(),
                    }
                }
                _ => AwaitPlace::Fine,
            },
            ExprKind::List(items) => {
                let mut written = items
                    .iter()
                    .filter(|item| !matches!(item.kind, ExprKind::Omitted));
                match written.next() {
                    Some(first) => {
                        let last = written.next_back().unwrap_or(first);
                        AwaitPlace::Conditional(self.list_span(first.span, last.span, true))
                    }
                    None => conditional(),
                }
            }
            ExprKind::String2(_) => AwaitPlace::Conditional(parent.span),
            ExprKind::Pipe(_)
            | ExprKind::Eif(_)
            | ExprKind::ObjGet(_)
            | ExprKind::Unop(..)
            | ExprKind::PrefixedString(_)
            | ExprKind::Import(..)
            | ExprKind::InoutArg(_) => conditional(),
            _ => AwaitPlace::Invalid(AWAIT_ELSEWHERE),
        }
    }

    /// The extent of an expression with the parentheses written around it.
    /// `enclosed` is set for an expression that is the only thing between
    /// the parentheses of the construct it belongs to, which do not count.
    fn term_span(&self, inner: Span, enclosed: bool) -> Span {
        let bytes = self.bytes;
        let open_before = |mut at: usize| {
            while at > 0 && bytes[at - 1].is_ascii_whitespace() {
                at -= 1;
            }
            (at > 0 && bytes[at - 1] == b'(').then(|| at - 1)
        };
        let close_after = |mut at: usize| {
            while at < bytes.len() && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            (at < bytes.len() && bytes[at] == b')').then(|| at + 1)
        };
        let (mut opens, mut at) = (0, inner.start as usize);
        while let Some(open) = open_before(at) {
            opens += 1;
            at = open;
        }
        let (mut closes, mut at) = (0, inner.end as usize);
        while let Some(close) = close_after(at) {
            closes += 1;
            at = close;
        }
        let pairs = usize::min(opens, closes).saturating_sub(enclosed as usize);
        let (mut start, mut end) = (inner.start as usize, inner.end as usize);
        for _ in 0..pairs {
            start = open_before(start).unwrap_or(start);
            end = close_after(end).unwrap_or(end);
        }
        Span::new(start as u32, end as u32)
    }

    /// The extent of a comma-separated list from its first and last item.
    fn list_span(&self, first: Span, last: Span, enclosed: bool) -> Span {
        if first == last {
            self.term_span(first, enclosed)
        } else {
            Span::new(
                self.term_span(first, false).start,
                self.term_span(last, false).end,
            )
        }
    }

    /// Reports the writes in a statement that has an `await` (or is in a
    /// `concurrent` block) which are not statements of their own: when they
    /// happen relative to the `await` is not what the code says.
    fn writes_in_stmt(&mut self, stmt: &Stmt<'a>) {
        match stmt.kind {
            StmtKind::Noop | StmtKind::Break | StmtKind::Continue | StmtKind::YieldBreak => {}
            StmtKind::Expr(e) => self.writes_in_expr(e, e.span.start == stmt.span.start),
            StmtKind::Throw(e) => self.writes_in_expr(e, false),
            StmtKind::Return(value) => {
                if let Some(e) = value {
                    self.writes_in_expr(e, false);
                }
            }
            StmtKind::Echo(echo) | StmtKind::Unset(echo) => self.writes_in_exprs(echo.args, false),
            StmtKind::If(node) => {
                self.writes_in_expr(&node.cond, false);
                self.writes_in_block(node.then);
                if let Some(otherwise) = node.otherwise {
                    self.writes_in_block(otherwise);
                }
            }
            StmtKind::While(node) | StmtKind::Do(node) => {
                self.writes_in_expr(&node.cond, false);
                self.writes_in_block(node.body);
            }
            StmtKind::For(node) => {
                for e in node.init {
                    self.writes_in_expr(e, self.is_bare(e));
                }
                if let Some(cond) = &node.cond {
                    self.writes_in_expr(cond, false);
                }
                for e in node.step {
                    self.writes_in_expr(e, self.is_bare(e));
                }
                self.writes_in_block(node.body);
            }
            StmtKind::Foreach(node) => {
                self.writes_in_expr(&node.collection, false);
                if let Some(key) = &node.key {
                    self.writes_in_expr(key, false);
                }
                self.writes_in_expr(&node.value, false);
                self.writes_in_block(node.body);
            }
            StmtKind::Switch(node) => {
                self.writes_in_expr(&node.subject, false);
                for case in node.cases {
                    if let Some(label) = &case.label {
                        self.writes_in_expr(label, false);
                    }
                    self.writes_in_block(case.body);
                }
            }
            StmtKind::Match(node) => {
                self.writes_in_expr(&node.subject, false);
                for arm in node.arms {
                    self.writes_in_block(arm.body);
                }
            }
            StmtKind::Try(node) => {
                self.writes_in_block(node.body);
                for catch in node.catches {
                    self.writes_in_block(catch.body);
                }
                if let Some(finally) = node.finally {
                    self.writes_in_block(finally);
                }
            }
            StmtKind::Using(node) => {
                for e in node.exprs {
                    self.writes_in_expr(e, self.using_expr_is_bare(node, e));
                }
                if let Some(body) = node.body {
                    self.writes_in_block(body);
                }
            }
            StmtKind::Concurrent(block) | StmtKind::Block(block) => self.writes_in_block(block),
        }
    }

    fn writes_in_block(&mut self, block: Block<'a>) {
        for stmt in block {
            self.writes_in_stmt(stmt);
        }
    }

    fn writes_in_exprs(&mut self, exprs: &[Expr<'a>], statement: bool) {
        for e in exprs {
            self.writes_in_expr(e, statement);
        }
    }

    /// `statement` is set for an expression that is a statement of its own,
    /// and for a call that is all of what such a statement assigns.
    fn writes_in_expr(&mut self, e: &Expr<'a>, statement: bool) {
        match e.kind {
            ExprKind::Assign(assign) => {
                if !statement {
                    self.report_assignment_in_expression(&assign.lhs);
                }
                self.writes_in_expr(&assign.lhs, false);
                let call = matches!(assign.rhs.kind, ExprKind::Call(_))
                    && assign.rhs.span.end == e.span.end;
                self.writes_in_expr(&assign.rhs, statement && call);
            }
            ExprKind::Unop(Uop::Incr | Uop::Decr | Uop::PostIncr | Uop::PostDecr, operand) => {
                if !statement {
                    self.report_assignment_in_expression(operand);
                }
                self.writes_in_expr(operand, false);
            }
            ExprKind::Call(call) => {
                self.writes_in_expr(&call.func, false);
                for arg in call.args {
                    match arg.kind {
                        ExprKind::InoutArg(target) => {
                            if !statement {
                                self.report(
                                    target.span.start,
                                    target.span.end,
                                    "An `inout` argument cannot be combined with `await` in the same statement. Pull it out into a separate statement.",
                                );
                            }
                            self.writes_in_expr(target, false);
                        }
                        _ => self.writes_in_expr(arg, false),
                    }
                }
                if let Some(unpacked) = call.unpacked {
                    self.writes_in_expr(unpacked, false);
                }
            }
            ExprKind::ExpressionTree(tree) => match tree.body {
                EtBody::Expr(body) => self.writes_in_expr(&body, false),
                EtBody::Block(block) => self.writes_in_block(block),
            },
            _ => each_operand(e, &mut |operand| self.writes_in_expr(operand, false)),
        }
    }

    /// An assignment or `++` / `--` used for its value rather than as a
    /// statement; reported at the thing being assigned to.
    #[cold]
    fn report_assignment_in_expression(&mut self, target: &Expr<'a>) {
        self.report(
            target.span.start,
            target.span.end,
            "Assignments can no longer be used as expressions. Pull the assignment out into a separate statement.",
        );
    }

    fn check_closure_attrs(&mut self, attrs: &[Attribute<'a>]) {
        for attr in attrs {
            if matches!(attr.name.name, "__Memoize" | "__MemoizeLSB") {
                // The attribute extends over its arguments, whose end is
                // the closing parenthesis.
                let end = self.attribute_end(attr);
                self.report(
                    attr.name.span.start,
                    end,
                    "`<<__Memoize>>` attribute is not allowed on lambdas or anonymous functions.",
                );
            }
        }
    }

    /// Where an attribute ends: after its name, or after the `)` of its
    /// argument list.
    fn attribute_end(&self, attr: &Attribute<'a>) -> u32 {
        let b = self.bytes;
        let mut i = match attr.args.last() {
            Some(arg) => arg.span.end as usize,
            None => attr.name.span.end as usize,
        };
        let name_end = i;
        // Find the `(`...`)` after the name, if any.
        let mut j = name_end;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if attr.args.is_empty() && (j >= b.len() || b[j] != b'(') {
            return name_end as u32;
        }
        let mut depth = 0i32;
        while i < b.len() {
            match b[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth <= 0 {
                        return (i + 1) as u32;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        name_end as u32
    }

    /// What is embedded in a string. `$x`, `$x->y` and `$x[..]` written
    /// directly in the text are no member selection or subscript to HHVM's
    /// syntax checks, which only look at the variables in them; and the
    /// variable of `{$x}` has the extent of the braces in the tree.
    fn check_string_part(&mut self, part: &Expr<'a>, sc: &Scope<'a>) {
        let start = part.span.start as usize;
        // The last piece of a string the file ends in may be empty.
        let first = self.bytes.get(start).copied().unwrap_or(0);
        // `${$x->y}` is an expression like any other.
        let direct = first == b'$' && !(start >= 2 && &self.bytes[start - 2..start] == b"${");
        match part.kind {
            ExprKind::Lvar(name) if first == b'{' => {
                let var = Expr {
                    span: Span::new(part.span.start + 1, part.span.start + 1 + name.len() as u32),
                    kind: part.kind,
                };
                self.check_variable(&var, name, sc, true);
            }
            ExprKind::Lvar(name) => self.check_variable(part, name, sc, true),
            ExprKind::ObjGet(get) if direct => {
                if let ExprKind::Lvar(name) = get.object.kind {
                    self.check_variable(&get.object, name, sc, true);
                }
            }
            ExprKind::ArrayGet(get) if direct => {
                if let ExprKind::Lvar(name) = get.base.kind {
                    self.check_variable(&get.base, name, sc, true);
                }
                if let Some(index) = &get.index
                    && let ExprKind::Lvar(name) = index.kind
                {
                    self.check_variable(index, name, sc, true);
                }
            }
            _ => self.check_expr(part, sc, 0),
        }
    }

    /// `$this`, superglobals and `$$` are only meaningful in some places.
    /// `expression` is false for a variable that is no expression.
    fn check_variable(&mut self, e: &Expr<'a>, name: &str, sc: &Scope<'a>, expression: bool) {
        let b = name.as_bytes();
        if b.len() < 2 {
            return;
        }
        let (start, end) = (e.span.start, e.span.end);
        match b[1] {
            b't' if name == "$this" => {
                if expression && !sc.has_this() {
                    if sc.in_enum_class {
                        self.report(start, end, "`$this` cannot be used inside an enum class");
                    }
                    if sc.in_function && sc.fn_name != "include" {
                        self.report(
                            start,
                            end,
                            "`$this` cannot be used in functions and static methods",
                        );
                    }
                }
                if sc.in_class && sc.is_static {
                    self.report_tree(
                        start,
                        end,
                        "Don't use `$this` in a static method, use `static::` instead",
                    );
                }
            }
            b'$' if expression => match sc.pipe {
                PipeVar::Available => {}
                PipeVar::Unavailable => self.report(
                    start,
                    end,
                    "Pipe variables must occur only in the RHS of pipe expressions",
                ),
                PipeVar::Captured => {
                    self.require_feature(start, end, Feature::CapturePipeVariables)
                }
            },
            b'G' if expression && name == "$GLOBALS" => self.report(
                start,
                end,
                "`$GLOBALS` variable is removed from the language. Use HH\\global functions",
            ),
            b'_' if expression && is_superglobal(name) => self.report(
                start,
                end,
                format!(
                    "`{}` is no longer accessible as a magic global variable. Use the HH\\global_* functions instead.",
                    name
                ),
            ),
            _ => {}
        }
    }

    fn check_yield(&mut self, e: &Expr<'a>, sc: &Scope<'a>) {
        let (start, end) = (e.span.start, e.span.end);
        if sc.in_function && sc.fn_name != "__invoke" && is_magic_method(sc.fn_name) {
            self.report(
                start,
                end,
                "`yield` is not allowed in constructors or magic methods",
            );
        }
        if sc.callable == Callable::None {
            self.report(start, end, "`yield` can only be used inside a function");
        }
        if sc.has_inout {
            self.report(
                start,
                end,
                if sc.is_async {
                    "Parameters may not be marked `inout` on `async` generators"
                } else {
                    "Parameters may not be marked `inout` on generators"
                },
            );
        }
    }

    /// Checks a call that has just been parsed; `start` is where the call
    /// expression begins.
    pub(crate) fn check_call(&mut self, start: u32, call: &Call<'a>) {
        if let ExprKind::Id(name) = call.func.kind
            && name.len() >= 11
            && matches!(
                name,
                "\\HH\\meth_caller" | "HH\\meth_caller" | "meth_caller"
            )
            && call.args.len() == 2
            && call.unpacked.is_none()
        {
            for arg in call.args {
                if let ExprKind::String(s) = arg.kind
                    && s.contains(&b'$')
                {
                    self.report_tree(
                        arg.span.start,
                        arg.span.end,
                        "`meth_caller` cannot be called on strings that contain `$`",
                    );
                }
            }
        }
        if !call.targs.is_empty() {
            let dynamic = match call.func.kind {
                ExprKind::ClassGet(_) => true,
                ExprKind::ObjGet(get) => matches!(get.member.kind, ExprKind::Lvar(_)),
                _ => false,
            };
            if dynamic {
                self.report_from(
                    start,
                    "Generics type parameters are disallowed on dynamic method calls",
                );
            }
        }
    }

    /// `A::b`, `A::$b`: what may stand before the `::`. `in_class` says
    /// whether the expression is inside a class, interface or trait.
    pub(crate) fn check_scope_resolution(
        &mut self,
        e: &Expr<'a>,
        access: &ClassAccess<'a>,
        in_class: bool,
    ) {
        let (dynamic, self_or_parent, valid, text) = match access.class.kind {
            // In parentheses nothing is a class name.
            _ if access.class.span.start != e.span.start => (true, false, false, ""),
            ExprKind::Id(name) if name.eq_ignore_ascii_case("namespace") => {
                (true, false, false, name)
            }
            ExprKind::Id(name) => (false, matches!(name, "self" | "parent"), true, name),
            ExprKind::Lvar(_) => (true, false, true, ""),
            ExprKind::Null
            | ExprKind::True
            | ExprKind::False
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_)
            | ExprKind::String2(_) => (false, false, false, ""),
            _ => (true, false, false, ""),
        };
        if valid && !dynamic && !self_or_parent && !text.eq_ignore_ascii_case("namespace") {
            return;
        }
        let (start, end) = (e.span.start, e.span.end);
        if !valid {
            self.report(
                start,
                end,
                "Only classnames and variables are allowed before `::`.",
            );
        }
        let is_class = matches!(e.kind, ExprKind::ClassConst(_))
            && access.name.name.eq_ignore_ascii_case("class");
        if (dynamic || !valid) && is_class {
            self.report(
                start,
                end,
                "Dynamic class names are not allowed in compile-time `::class` fetch",
            );
        }
        if text.eq_ignore_ascii_case("namespace") {
            self.report(start, end, "Namespace cannot be used as a classname");
        }
        if self_or_parent && is_class && !in_class {
            self.report(
                start,
                end,
                format!(
                    "Cannot access `{}::class` when no class scope is active",
                    text
                ),
            );
        }
    }

    /// `Vector {...}` and friends: only the built-in collection classes can
    /// be initialized this way, each with its own kind of element.
    pub(crate) fn check_collection_literal(&mut self, start: u32, c: &Collection<'a>) {
        let end = self.prev_end;
        let name = c.name.name;
        let is_standard = |n: &str| {
            [
                "pair",
                "vector",
                "map",
                "set",
                "immvector",
                "immmap",
                "immset",
            ]
            .iter()
            .any(|s| n.eq_ignore_ascii_case(s))
        };
        // The class, when it is one of the standard collections.
        let class = if !name.contains('\\') {
            if ["dict", "vec", "keyset"]
                .iter()
                .any(|s| name.eq_ignore_ascii_case(s))
            {
                self.report(
                    start,
                    end,
                    "Initializers of `vec`, `dict` and `keyset` should use `[...]` instead of `{...}`.",
                );
                return;
            }
            if self.et_active {
                // What the class may be initialised with is up to the DSL.
                return;
            }
            Some(name).filter(|n| is_standard(n))
        } else {
            // `HH\Vector` (at the top level) or `\HH\Vector`.
            let rest = match name.strip_prefix('\\') {
                Some(rest) => Some(rest),
                None if self.ns_name == "\\" => Some(name),
                None => None,
            };
            rest.and_then(|rest| {
                let (ns, short) = rest.split_once('\\')?;
                (ns.eq_ignore_ascii_case("hh") && is_standard(short)).then_some(short)
            })
        };
        let Some(class) = class else {
            self.report(
                start,
                end,
                "Cannot use collection initialization for non-collection class.",
            );
            return;
        };
        let keyed = class.eq_ignore_ascii_case("map") || class.eq_ignore_ascii_case("immmap");
        let has_key_value = c.fields.iter().any(|f| matches!(f, Afield::KeyValue(..)));
        let has_value = c.fields.iter().any(|f| matches!(f, Afield::Value(_)));
        if !keyed && has_key_value {
            let written = self.collection_name_text(c);
            self.report(
                start,
                end,
                format!(
                    "Cannot use key value initializer for `{}`. It does not allow keys.",
                    written
                ),
            );
        } else if (keyed || self.et_active) && has_value {
            let written = self.collection_name_text(c);
            self.report(
                start,
                end,
                format!(
                    "Cannot use value initializer for `{}`. It requires `key => value`.",
                    written
                ),
            );
        } else if class.eq_ignore_ascii_case("pair") && c.fields.len() != 2 {
            self.report(
                start,
                end,
                if c.fields.is_empty() {
                    "Initializer needed for Pair object"
                } else {
                    "Pair objects must have exactly 2 elements"
                },
            );
        }
    }

    /// The class of a collection literal as written, type arguments
    /// included.
    fn collection_name_text(&self, c: &Collection<'a>) -> &'a str {
        let start = c.name.span.start as usize;
        let mut end = c.name.span.end as usize;
        if let Some(last) = c.targs.last() {
            // Up to the `>` that closes the type arguments.
            end = last.span.end as usize;
            while end < self.bytes.len() && self.bytes[end] != b'>' {
                end += 1;
            }
            end = (end + 1).min(self.bytes.len());
        }
        &self.src[start..end]
    }

    /// `unset` only removes an element from a container: unsetting a local,
    /// a property or a static property is not part of the language.
    pub(crate) fn check_unset_target(&mut self, e: &Expr<'a>) {
        match e.kind {
            // `?->` is turned down for a reason of its own.
            ExprKind::ArrayGet(_) => self.check_lvalue(e, LvalRoot::Unset, Nesting::None),
            ExprKind::ObjGet(get) if get.nullsafe => {
                self.check_lvalue(e, LvalRoot::Unset, Nesting::None)
            }
            _ => self.report(
                e.span.start,
                e.span.end,
                "`unset` is only supported on container elements, e.g. `unset($x[$k]);`",
            ),
        }
    }

    /// Checks something that is written to: the left side of an assignment,
    /// the operand of `++`, an `inout` argument, a `foreach` variable or an
    /// `unset` argument.
    pub(crate) fn check_lvalue(&mut self, e: &Expr<'a>, root: LvalRoot, nesting: Nesting) {
        let (start, end) = (e.span.start, e.span.end);
        match e.kind {
            ExprKind::ObjGet(get) if root == LvalRoot::Inout && !get.nullsafe => {
                self.report(start, end, "You cannot decorate this argument as `inout`")
            }
            ExprKind::List(_)
            | ExprKind::ClassGet(_)
            | ExprKind::ClassConst(_)
            | ExprKind::Shape(..)
            | ExprKind::Tuple(..)
                if root == LvalRoot::Inout =>
            {
                self.report(start, end, "You cannot decorate this argument as `inout`")
            }
            ExprKind::List(_) if nesting == Nesting::ShapeTuple => self.report(
                start,
                end,
                "You cannot nest a `list` inside a `tuple` or `shape`",
            ),
            ExprKind::Shape(..) | ExprKind::Tuple(..) if nesting == Nesting::List => self.report(
                start,
                end,
                "You cannot nest a `shape` or `tuple` inside a `list`",
            ),
            ExprKind::List(items) => {
                for item in items {
                    self.check_lvalue(item, root, Nesting::List);
                }
            }
            ExprKind::Tuple(items, _) => {
                self.require_feature(start, end, Feature::ShapeAndTupleDestructuring);
                for item in items {
                    let target = match item.kind {
                        ExprKind::OptionalEntry(inner) => inner,
                        _ => item,
                    };
                    self.check_lvalue(target, root, Nesting::ShapeTuple);
                }
            }
            ExprKind::Shape(fields, _) => {
                self.require_feature(start, end, Feature::ShapeAndTupleDestructuring);
                for field in fields {
                    match field {
                        // A variable standing for a field is fine as it is.
                        ShapeField::Field { .. } | ShapeField::Optional(_) if field.is_punned() => {
                        }
                        ShapeField::Field { .. } | ShapeField::Optional(_) => {
                            self.check_lvalue(field.value(), root, Nesting::ShapeTuple)
                        }
                        ShapeField::Splat { .. } => {
                            // HHVM gets to the assignment before it gets to
                            // what the splat itself has wrong.
                            let span = field.span();
                            let at = self
                                .errors
                                .iter()
                                .position(|e| e.start == span.start && e.end == span.end)
                                .unwrap_or(self.errors.len());
                            self.errors.insert(
                                at,
                                crate::SyntaxError {
                                    start: span.start,
                                    end: span.end,
                                    message: root.message().into(),
                                },
                            );
                        }
                    }
                }
            }
            ExprKind::ObjGet(get) => {
                if get.nullsafe {
                    self.report(start, end, "`?->` operator is not allowed in write context");
                } else if matches!(get.member.kind, ExprKind::Id(name) if name.starts_with(':')) {
                    self.report(start, end, "`->:` operator is not allowed in write context");
                }
            }
            ExprKind::Lvar(name) => {
                if name == "$this" || name == "$$" {
                    self.report(start, end, root.message());
                }
                if root == LvalRoot::Inout && (name == "$GLOBALS" || is_superglobal(name)) {
                    self.report(start, end, "You cannot decorate this argument as `inout`");
                }
            }
            ExprKind::ArrayGet(get) => {
                self.check_lvalue(&get.base, root, nesting);
                if root == LvalRoot::Inout && get.index.is_none() {
                    self.report(start, end, "You cannot decorate this argument as `inout`");
                }
            }
            ExprKind::Unop(..)
            | ExprKind::Clone(_)
            | ExprKind::Await(_)
            | ExprKind::Delay(_)
            | ExprKind::Readonly(_)
            | ExprKind::Print(_) => self.report(
                start,
                end,
                "Unary expression is not allowed in write context",
            ),
            ExprKind::Omitted | ExprKind::Dollar(_) | ExprKind::ClassGet(_) => {}
            ExprKind::Id("_") if nesting == Nesting::ShapeTuple => {}
            _ => self.report(start, end, root.message()),
        }
    }

    /// Reports writes the enclosing function's contexts do not allow.
    /// Returns whether the operands of `e` are exempt from this check.
    fn check_coeffects(&mut self, e: &Expr<'a>, sc: &Scope<'a>) -> bool {
        let written = match e.kind {
            ExprKind::Assign(assign) => Some(&assign.lhs),
            ExprKind::Unop(Uop::Incr | Uop::Decr | Uop::PostIncr | Uop::PostDecr, operand) => {
                Some(operand)
            }
            _ => None,
        };
        if sc.co & CO_WRITE_PROPS == 0
            && let Some(target) = written
        {
            // Writes through `$this` need less: `write_this_props`, or
            // being in a constructor. HHVM looks at the write twice,
            // once for each capability, and says the same thing each
            // time it objects: once about a write that is not through
            // `$this`, and then, where `$this` may not be written
            // through either, once more about that write and twice
            // about one through `$this`.
            let this_ok = sc.co & CO_WRITE_THIS_PROPS != 0 || sc.co & CO_CTOR_OR_CLONE != 0;
            let mut target = target;
            loop {
                match target.kind {
                    ExprKind::ArrayGet(get) => target = &get.base,
                    ExprKind::ObjGet(get) => {
                        let on_this = matches!(get.object.kind, ExprKind::Lvar("$this"));
                        let times = match (on_this, this_ok) {
                            (true, true) => 0,
                            (false, true) => 1,
                            (_, false) => 2,
                        };
                        for _ in 0..times {
                            self.report_tree_pass(3, e.span.start, e.span.end, WRITE_PROPS_ERROR);
                        }
                        break;
                    }
                    _ => break,
                }
            }
        }
        if sc.co & CO_GLOBALS != 0 {
            return false;
        }
        let read_globals = sc.co & CO_READ_GLOBALS != 0;
        match e.kind {
            ExprKind::Assign(assign) => {
                if let ExprKind::ClassGet(_) = assign.lhs.kind {
                    self.report_tree_pass(
                        3,
                        assign.lhs.span.start,
                        assign.lhs.span.end,
                        "Writing to static properties requires the capability Globals, which is not provided by the context. Add the context `[globals]` after the function parameter list (or amend an existing context with a new comma delineated entry `globals`).",
                    );
                    if read_globals {
                        // Neither side is then checked itself, only what
                        // is inside them.
                        return true;
                    }
                }
                false
            }
            // `readonly Foo::$x` is how a static is read without `globals`.
            ExprKind::Readonly(_) => read_globals,
            ExprKind::ClassGet(_) => {
                self.report_tree_pass(
                    3,
                    e.span.start,
                    e.span.end,
                    if read_globals {
                        "Creating mutable references to static properties requires the capability Globals, which is not provided by the context. Enclose the static in a readonly expression."
                    } else {
                        "Reading static properties requires the capability ReadGlobals, which is not provided by the context. Add the context `[read_globals]` after the function parameter list (or amend an existing context with a new comma delineated entry `read_globals`)."
                    },
                );
                false
            }
            _ => false,
        }
    }
}

/// Whether an integer literal is written in base ten.
fn is_decimal(digits: &str) -> bool {
    digits == "0" || !digits.starts_with('0')
}

/// Whether an expression contains an `await` (outside nested functions).
fn has_await(e: &Expr<'_>) -> bool {
    if let ExprKind::Await(_) = e.kind {
        return true;
    }
    let mut found = false;
    each_operand(e, &mut |operand| found = found || has_await(operand));
    found
}

/// Calls `f` with each expression directly inside `e`, leaving out what is
/// in the body of a function and the statements of an expression tree.
fn each_operand<'a>(e: &Expr<'a>, f: &mut impl FnMut(&Expr<'a>)) {
    match e.kind {
        ExprKind::Null
        | ExprKind::True
        | ExprKind::False
        | ExprKind::Omitted
        | ExprKind::Lvar(_)
        | ExprKind::Id(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::String(_)
        | ExprKind::FunctionPointer(_)
        | ExprKind::EnumClassLabel(_)
        | ExprKind::Package(_)
        | ExprKind::Efun(_)
        | ExprKind::Lfun(_)
        | ExprKind::AsyncBlock(_) => {}
        ExprKind::ExpressionTree(tree) => {
            if let EtBody::Expr(body) = &tree.body {
                f(body);
            }
        }
        ExprKind::EtSplice(splice) => f(&splice.expr),
        ExprKind::PrefixedString(p) => f(&p.expr),
        ExprKind::Shape(fields, _) => fields.iter().for_each(|field| f(field.value())),
        ExprKind::String2(items) | ExprKind::Tuple(items, _) | ExprKind::List(items) => {
            items.iter().for_each(f)
        }
        ExprKind::ValCollection(c) => c.items.iter().for_each(f),
        ExprKind::KeyValCollection(c) => {
            for (k, v) in c.fields {
                f(k);
                f(v);
            }
        }
        ExprKind::Collection(c) => {
            for field in c.fields {
                each_field_operand(field, f);
            }
        }
        ExprKind::Yield(field) => each_field_operand(field, f),
        ExprKind::Clone(inner)
        | ExprKind::OptionalEntry(inner)
        | ExprKind::Await(inner)
        | ExprKind::Delay(inner)
        | ExprKind::Readonly(inner)
        | ExprKind::Print(inner)
        | ExprKind::Import(_, inner)
        | ExprKind::Nameof(inner)
        | ExprKind::Dollar(inner)
        | ExprKind::InoutArg(inner)
        | ExprKind::Unop(_, inner) => f(inner),
        ExprKind::NamedArg(arg) => f(&arg.value),
        ExprKind::ArrayGet(get) => {
            f(&get.base);
            if let Some(index) = &get.index {
                f(index);
            }
        }
        ExprKind::ObjGet(get) => {
            f(&get.object);
            f(&get.member);
        }
        ExprKind::ClassGet(access) | ExprKind::ClassConst(access) => f(&access.class),
        ExprKind::Call(call) => {
            f(&call.func);
            call.args.iter().for_each(&mut *f);
            if let Some(unpacked) = call.unpacked {
                f(unpacked);
            }
        }
        ExprKind::Cast(cast) => f(&cast.expr),
        ExprKind::Binop(b) => {
            f(&b.lhs);
            f(&b.rhs);
        }
        ExprKind::Assign(assign) => {
            f(&assign.lhs);
            f(&assign.rhs);
        }
        ExprKind::Pipe(pipe) => {
            f(&pipe.lhs);
            f(&pipe.rhs);
        }
        ExprKind::Eif(c) => {
            f(&c.cond);
            if let Some(then) = &c.then {
                f(then);
            }
            f(&c.otherwise);
        }
        ExprKind::Is(test)
        | ExprKind::As(test)
        | ExprKind::NullableAs(test)
        | ExprKind::Upcast(test) => f(&test.expr),
        ExprKind::New(new) => {
            f(&new.class);
            new.args.iter().for_each(&mut *f);
            if let Some(unpacked) = new.unpacked {
                f(unpacked);
            }
        }
        ExprKind::Xml(xml) => {
            for attr in xml.attrs {
                match attr {
                    XhpAttribute::Simple(_, value) | XhpAttribute::Spread(value) => f(value),
                }
            }
            xml.children.iter().for_each(f);
        }
    }
}

fn each_field_operand<'a>(field: &Afield<'a>, f: &mut impl FnMut(&Expr<'a>)) {
    match field {
        Afield::Value(v) => f(v),
        Afield::KeyValue(k, v) => {
            f(k);
            f(v);
        }
    }
}

/// An assignment or `++` / `--`.
fn is_write(e: &Expr<'_>) -> bool {
    matches!(
        e.kind,
        ExprKind::Assign(_)
            | ExprKind::Unop(Uop::Incr | Uop::Decr | Uop::PostIncr | Uop::PostDecr, _)
    )
}

/// A write target that is certainly valid: a variable (other than `$this`),
/// a property or static property, or an element of one of those.
fn is_plain_lvalue(mut e: &Expr<'_>) -> bool {
    loop {
        match e.kind {
            ExprKind::Lvar(name) => return name != "$this" && name != "$$",
            ExprKind::ArrayGet(get) => e = &get.base,
            ExprKind::ObjGet(get) => {
                return !get.nullsafe
                    && !matches!(get.member.kind, ExprKind::Id(name) if name.starts_with(':'));
            }
            ExprKind::ClassGet(_) => return true,
            _ => return false,
        }
    }
}

// Hooks the parser calls while building the tree. They report what can be
// decided on the spot and flag the enclosing statement when something has to
// be judged later, with the whole function in view.
impl<'a> Parser<'a> {
    /// A variable has been read as an expression.
    #[inline]
    pub(crate) fn note_variable(&mut self, tok: crate::lexer::Token) {
        if matches!(self.bytes[tok.start as usize + 1], b't' | b'G' | b'_') {
            self.note_special_variable(tok.start as usize, tok.end as usize);
        }
    }

    /// [`note_variable`](Self::note_variable) for the variable written
    /// from `start` to `end` in the text of a string.
    #[inline]
    pub(crate) fn note_variable_at(&mut self, start: usize, end: usize) {
        if matches!(self.bytes[start + 1], b't' | b'G' | b'_') {
            self.note_special_variable(start, end);
        }
    }

    fn note_special_variable(&mut self, start: usize, end: usize) {
        match &self.src[start..end] {
            // Fine in an instance method, a matter for the later checks
            // elsewhere.
            "$this" => {
                if self.class.is_none() || self.fn_static {
                    self.interest = true;
                }
            }
            name => {
                if name == "$GLOBALS" || is_superglobal(name) {
                    self.interest = true;
                }
            }
        }
    }

    /// `target` is assigned to, or incremented / decremented.
    pub(crate) fn note_write(&mut self, target: &Expr<'a>, root: LvalRoot) {
        self.assigns += 1;
        if self.strict_co {
            self.interest = true;
        }
        if !is_plain_lvalue(target) {
            if let ExprKind::Await(_) = target.kind {
                self.interest = true;
            }
            self.check_lvalue(target, root, Nesting::None);
        }
    }

    /// An `await` starts at `start`. Only one that is the first thing of an
    /// expression that roots `await`s is sure to be where it may be.
    #[inline]
    pub(crate) fn note_await(&mut self, start: u32) {
        self.marks |= MARK_AWAIT;
        if start != self.await_ok_at {
            self.interest = true;
        }
    }

    /// Parses an expression that an `await` may be the start of.
    #[inline]
    pub(crate) fn parse_root_expr(&mut self) -> crate::parser::R<Expr<'a>> {
        self.await_ok_at = self.tok.start;
        self.parse_expr()
    }

    /// `class::name` has been parsed; `kind` is the resulting expression.
    #[inline]
    pub(crate) fn note_scope_resolution(&mut self, class: &Expr<'a>, kind: &ExprKind<'a>) {
        let plain = match class.kind {
            ExprKind::Id(name) => match name.len() {
                4 | 6 => self.class.is_some() || !matches!(name, "self" | "parent"),
                9 => !name.eq_ignore_ascii_case("namespace"),
                _ => true,
            },
            _ => false,
        };
        if !plain || (self.strict_co && matches!(kind, ExprKind::ClassGet(_))) {
            self.interest = true;
        }
    }

    /// `is` and `as` cannot test against a function type or a `<<__Soft>>`
    /// type.
    #[cold]
    pub(crate) fn check_type_test_hint(&mut self, is: bool, hint: &Hint<'a>) {
        let word = if is { "is" } else { "as" };
        let what = if matches!(hint.kind, HintKind::Fun(_)) {
            "__Callable"
        } else {
            "__Soft"
        };
        self.report(
            hint.span.start,
            hint.span.end,
            format!(
                "`{}` typehints cannot be used with `{}` expressions",
                what, word
            ),
        );
    }

    /// Called before the body of a lambda, anonymous function or `async`
    /// block is parsed. Returns the value to put back into `strict_co`
    /// afterwards.
    pub(crate) fn enter_closure(&mut self, has_contexts: bool) -> bool {
        let outer = self.strict_co;
        if has_contexts {
            self.strict_co = true;
        }
        if self.strict_co {
            self.interest = true;
        }
        outer
    }

    /// Checks the attributes and return type of a lambda, anonymous
    /// function or `async` block.
    pub(crate) fn check_closure_signature(
        &mut self,
        attrs: &[Attribute<'a>],
        flags: u8,
        ret: Option<&Hint<'a>>,
    ) {
        if !attrs.is_empty() {
            self.check_closure_attrs(attrs);
        }
        if let Some(ret) = ret {
            self.check_async_return(flags, ret);
        }
    }

    /// `async` functions that are not generators must declare an
    /// `Awaitable` return type (if they declare one at all).
    pub(crate) fn check_async_return(&mut self, flags: u8, ret: &Hint<'a>) {
        if flags & (fun_flags::ASYNC | fun_flags::HAS_YIELD) != fun_flags::ASYNC {
            return;
        }
        let mut hint = ret;
        while let HintKind::Soft(inner) = hint.kind {
            hint = inner;
        }
        let awaitable =
            |name: &str| matches!(name, "\\HH\\Awaitable" | "HH\\Awaitable" | "Awaitable");
        match hint.kind {
            HintKind::Name(name) if awaitable(name) => {}
            HintKind::Generic(g) if awaitable(g.name) => {
                if g.args.len() > 1 {
                    self.report_tree(
                        ret.span.start,
                        ret.span.end,
                        "`Awaitable<_>` takes exactly one type argument.",
                    );
                }
            }
            _ => self.report_tree(
                ret.span.start,
                ret.span.end,
                "`async` functions must have an `Awaitable` return type.",
            ),
        }
    }

    /// A statement has been parsed: decides whether the later checks need
    /// to look at its expressions.
    #[inline]
    pub(crate) fn note_statement(&mut self, stmt: &Stmt<'a>) {
        // Assignments are expected where they are statements themselves.
        let allowed = match stmt.kind {
            StmtKind::Expr(e) => (is_write(e) && e.span.start == stmt.span.start) as u32,
            StmtKind::For(node) => node
                .init
                .iter()
                .chain(node.step)
                .filter(|e| is_write(e) && self.is_bare(e))
                .count() as u32,
            StmtKind::Using(node) => {
                if node.body.is_none() {
                    self.interest = true;
                }
                node.exprs
                    .iter()
                    .filter(|e| is_write(e) && self.using_expr_is_bare(node, e))
                    .count() as u32
            }
            StmtKind::Concurrent(_) | StmtKind::YieldBreak => {
                self.interest = true;
                0
            }
            _ => 0,
        };
        // A write that is fine by itself may not be next to an `await`.
        if self.assigns > allowed || self.marks == MARK_AWAIT | MARK_INOUT {
            self.interest = true;
        }
        if self.interest {
            self.flagged.push(stmt.span.start);
        }
    }

    /// Parses the value of a constant, property, parameter default or
    /// enumerator, flagging it like a statement when it needs checking.
    pub(crate) fn parse_initializer(&mut self) -> crate::parser::R<Expr<'a>> {
        let (outer_interest, outer_assigns) = (self.interest, self.assigns);
        self.interest = false;
        self.assigns = 0;
        self.await_ok_at = u32::MAX;
        let r = self.parse_expr();
        if let Ok(e) = &r
            && (self.interest || self.assigns > 0)
        {
            self.interest = true;
            self.flagged.push(e.span.start);
        }
        self.interest |= outer_interest;
        self.assigns = outer_assigns;
        r
    }
}

impl<'a> Parser<'a> {
    /// Checks a member of an `enum class` whose `;` has just been consumed.
    pub(crate) fn check_enum_class_member(
        &mut self,
        start: u32,
        m: &Modifiers,
        name: Id<'a>,
        has_value: bool,
    ) {
        self.check_modifiers(m, "Enum class constants", false, |k| k == T::Abstract);
        if m.is_abstract && has_value {
            self.report_from(
                start,
                "Abstract enum class constants must not provide any initial value",
            );
        }
        if !m.is_abstract && !has_value {
            self.report_from(
                start,
                "Concrete enum class constants must have an initial value",
            );
        }
        if name.name.eq_ignore_ascii_case("class") {
            self.report_from(start, "Enum class members cannot be named `class`");
        }
    }
}

/// How the file's namespaces are declared.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum NamespaceStyle {
    #[default]
    Unspecified,
    /// `namespace Foo { ... }`
    Bracketed,
    /// `namespace Foo;`
    Unbracketed,
}

/// What has been seen at the top level of the file, for the rules about how
/// namespace and module declarations may be arranged.
#[derive(Default)]
pub(crate) struct FileState {
    /// Nesting depth of `namespace ... { }` blocks.
    namespace_depth: u32,
    style: NamespaceStyle,
    /// The namespace declarations at the top level: extent and whether they
    /// have a `{ }` body.
    top_namespaces: SmallVec<[(u32, u32, bool); 2]>,
    /// Whether the first real declaration of the file is a namespace
    /// declaration; `None` until one is seen.
    first_is_namespace: Option<bool>,
    /// There is code at the top level other than namespace declarations,
    /// `use` clauses, file attributes and `module` membership.
    top_level_code: bool,
    /// Definitions are being collected into a `namespace Foo;`.
    pub(crate) in_open_namespace: bool,
    /// A definition other than file attributes has been seen.
    past_first_def: bool,
    /// A `new module` declaration has been seen.
    past_module_def: bool,
    /// The file declared `module name;` for a module other than `default`.
    in_named_module: bool,
    /// HHVM gave up on a declaration inside the `namespace { }` block
    /// being parsed, and with it on the rest of the block.
    pub(crate) block_aborted: bool,
}

/// What [`Parser::enter_namespace_block`] hands to
/// [`Parser::leave_namespace_block`].
pub(crate) struct NamespaceBlock<'a> {
    outer_name: String,
    outer_names: Names<'a>,
    style_before: NamespaceStyle,
}

const MIXED_NAMESPACES: &str =
    "Cannot mix bracketed namespace declarations with unbracketed namespace declarations";

impl<'a> Parser<'a> {
    fn note_top_level_namespace(&mut self, start: u32, end: u32, bracketed: bool) {
        if self.file.namespace_depth != 0 {
            return;
        }
        self.file.top_namespaces.push((start, end, bracketed));
        self.file.first_is_namespace.get_or_insert(true);
        if !self.file.in_open_namespace {
            self.file.past_first_def = true;
        }
    }

    /// `namespace name;` has been parsed (`start` is the keyword, the last
    /// consumed token the `;`).
    pub(crate) fn note_namespace_statement(&mut self, start: u32, name: &str) {
        let end = self.prev_end;
        match self.file.style {
            NamespaceStyle::Bracketed => self.report(end - 1, end, MIXED_NAMESPACES),
            NamespaceStyle::Unspecified => self.file.style = NamespaceStyle::Unbracketed,
            NamespaceStyle::Unbracketed => {}
        }
        // HHVM's checker never leaves a namespace declared this way, so a
        // second one is recorded as nested in the first.
        self.ns_name = combine_names(&self.ns_name, name);
        self.reset_names();
        self.note_top_level_namespace(start, end, false);
    }

    /// Called before the body of `namespace name { ... }`.
    pub(crate) fn enter_namespace_block(&mut self, name: &str) -> NamespaceBlock<'a> {
        let outer_name = if name.is_empty() {
            self.ns_name.clone()
        } else {
            let inner = combine_names(&self.ns_name, name);
            std::mem::replace(&mut self.ns_name, inner)
        };
        let style_before = self.file.style;
        if style_before == NamespaceStyle::Unspecified {
            self.file.style = NamespaceStyle::Bracketed;
        }
        self.file.namespace_depth += 1;
        let outer_names = self.enter_names_block();
        NamespaceBlock {
            outer_name,
            outer_names,
            style_before,
        }
    }

    /// Called after the `}` of a namespace block; `start` is the
    /// `namespace` keyword and `lbrace` the opening brace.
    pub(crate) fn leave_namespace_block(
        &mut self,
        block: NamespaceBlock<'a>,
        start: u32,
        lbrace: u32,
    ) {
        let end = self.prev_end;
        self.file.namespace_depth -= 1;
        if self.file.namespace_depth == 0 {
            self.file.block_aborted = false;
        }
        self.ns_name = block.outer_name;
        self.names = block.outer_names;
        if block.style_before == NamespaceStyle::Unbracketed {
            self.report(lbrace, end, MIXED_NAMESPACES);
        }
        self.note_top_level_namespace(start, end, true);
    }

    /// A definition (other than a namespace) has been parsed.
    pub(crate) fn note_def(&mut self, def: &Def<'a>) {
        if self.unit_aborted && self.file.namespace_depth != 0 {
            // The rest of the enclosing `namespace { }` block goes too.
            self.file.block_aborted = true;
        }
        self.end_unit();
        if let Def::SetModule(_) = def {
            self.in_module = true;
        }
        if self.file.namespace_depth != 0 {
            return;
        }
        if !matches!(
            def,
            Def::Use(_) | Def::FileAttributes(_) | Def::SetModule(_)
        ) {
            self.file.top_level_code = true;
            self.file.first_is_namespace.get_or_insert(false);
        }
        if self.file.in_open_namespace {
            return;
        }
        // `module name;` comes before every other definition, and only code
        // in the default module may declare modules, one per file.
        match def {
            Def::Stmt(_) | Def::FileAttributes(_) => {}
            Def::SetModule(name) => {
                if self.file.past_first_def {
                    // A name that is not there has a place all the same,
                    // one character wide.
                    self.report_tree_pass(
                        1,
                        name.span.start,
                        name.span.end.max(name.span.start + 1),
                        format!(
                            "`module {};` must be unique, and precede any other declarations in a file.",
                            name.name
                        ),
                    );
                }
                self.file.past_first_def = true;
                self.file.in_named_module = name.name != "default";
            }
            Def::Module(decl) => {
                let (start, end) = (decl.name.span.start, decl.name.span.end);
                if self.file.in_named_module {
                    self.report_tree_pass(
                        1,
                        start,
                        end,
                        "You cannot declare new modules within an existing module",
                    );
                }
                if self.file.past_module_def {
                    self.report_tree_pass(
                        1,
                        start,
                        end,
                        "You cannot declare more than one module within a file",
                    );
                }
                self.file.past_first_def = true;
                self.file.past_module_def = true;
            }
            _ => self.file.past_first_def = true,
        }
    }

    /// The whole file has been parsed.
    pub(crate) fn check_file_end(&mut self) {
        let namespaces = std::mem::take(&mut self.file.top_namespaces);
        for (start, end, bracketed) in namespaces {
            if self.file.first_is_namespace == Some(false) {
                self.report(
                    start,
                    end,
                    "Namespace declaration statement has to be the very first statement in the script",
                );
            }
            if bracketed && self.file.top_level_code {
                self.report(start, end, "No code may exist outside of namespace {}");
            }
        }
        if !self.lexer.retired_variables.is_empty() {
            let mut uses = std::mem::take(&mut self.lexer.retired_variables);
            uses.sort_unstable();
            uses.dedup();
            for start in uses {
                let end = start + crate::lexer::RETIRED_VARIABLE.len() as u32;
                self.report(start, end, "prosecco_tap is no longer available");
            }
        }
        if self.errors.is_empty() && self.tree_errors.is_empty() && self.lowering_list.is_empty() {
            return;
        }
        self.order_errors();
    }
}

impl<'a> Parser<'a> {
    /// Checks the `as` / `super` bounds of a type alias: `type` aliases
    /// cannot have any, and `super` bounds on a `newtype` are a gated
    /// feature.
    pub(crate) fn check_alias_bounds(
        &mut self,
        keyword: crate::lexer::Token,
        bounds: &[(ConstraintKind, Hint<'a>)],
    ) {
        if keyword.kind == T::Type {
            self.report(
                keyword.start,
                keyword.end,
                "A type alias declaration cannot both use `type` and have a constraint. Did you mean `newtype`?",
            );
            return;
        }
        if let Some((_, hint)) = bounds.iter().find(|(k, _)| *k == ConstraintKind::Super) {
            // The bound starts at its `super` keyword.
            let mut i = hint.span.start as usize;
            while i > 0 && self.bytes[i - 1].is_ascii_whitespace() {
                i -= 1;
            }
            if self.bytes[..i].ends_with(b"super") {
                self.require_feature(i as u32 - 5, hint.span.end, Feature::NewtypeSuperBounds);
            }
        }
    }
}

const ATTRIBUTE_ARG_NOT_LITERAL: &str = "Attribute arguments must be literals";

impl<'a> Parser<'a> {
    /// An integer literal long enough that it might not fit, or one with a
    /// leading zero: HHVM rejects anything beyond a signed 64-bit integer
    /// and an octal literal with a digit that is not octal.
    #[cold]
    pub(crate) fn check_int_literal(&mut self, tok: crate::lexer::Token) {
        let text = self.tok_text(tok);
        let b = text.as_bytes();
        let (radix, digits) = match b {
            [b'0', b'x' | b'X', rest @ ..] => (16, rest),
            [b'0', b'b' | b'B', rest @ ..] => (2, rest),
            [b'0', rest @ ..] => (8, rest),
            _ => (10, b),
        };
        let mut value: i64 = 0;
        let mut overflow = false;
        for &c in digits {
            let Some(d) = (c as char).to_digit(radix) else {
                // `089`: HHVM gives up on the declaration.
                if radix == 8 {
                    self.report_lowering(tok.start, tok.end, "Invalid digit for octal integers");
                    self.abort_lowering(
                        tok.start,
                        tok.end,
                        format!("Encountered unexpected text `{text}`, was expecting a octal."),
                    );
                }
                return;
            };
            match value
                .checked_mul(radix as i64)
                .and_then(|v| v.checked_add(d as i64))
            {
                Some(v) => value = v,
                None => {
                    overflow = true;
                    break;
                }
            }
        }
        if !overflow {
            return;
        }
        if radix == 10 {
            self.report(
                tok.start,
                tok.end,
                format!("Decimal number is too big: `{}`", text),
            );
        }
        self.report_lowering(
            tok.start,
            tok.end,
            format!("{} is out of the range of 64-bit float values", text),
        );
    }

    /// The namespaces HHVM keeps contexts and capabilities in cannot be
    /// named directly.
    #[cold]
    pub(crate) fn check_coeffect_namespace(&mut self, hint: &Hint<'a>, name: &str) {
        let name = name.strip_prefix('\\').unwrap_or(name);
        if (name.starts_with("HH\\Contexts") || name.starts_with("HH\\Capabilities"))
            && !self.decl_mode
        {
            self.report_lowering(
                hint.span.start,
                hint.span.end,
                "Direct references to coeffects namespaces are not allowed",
            );
        }
    }

    /// The values of `enum {...}` as the type of an XHP attribute, of which
    /// there has to be one, are plain strings and integers.
    pub(crate) fn check_xhp_enum_values(&mut self, values: &[Expr<'a>]) {
        if values.is_empty() {
            let at = self.missing_pos();
            self.report(at, at, "At least one enumerated item is expected.");
        }
        for value in values {
            let written = self.term_span(value.span, false);
            let literal = match value.kind {
                ExprKind::Int(digits) => is_decimal(digits),
                ExprKind::String(_) => {
                    matches!(self.bytes[value.span.start as usize], b'\'' | b'"')
                }
                _ => false,
            };
            if !literal || written != value.span {
                self.report(
                    written.start,
                    written.end,
                    "Expected integer or string literal.",
                );
            }
        }
    }

    /// What HHVM checks about the attributes of a function, method, class,
    /// enum, type alias or type parameter, in particular about the ones
    /// parsed last: to be called once they turn out to belong to one.
    pub(crate) fn check_declaration_attrs(&mut self, attrs: &[Attribute<'a>]) {
        self.check_simplihack_attrs();
        for attr in attrs {
            match attr.name.name {
                "__RequirePackage" => self.check_require_package(attr),
                "__SoftRequirePackage" => self.check_soft_require_package(attr),
                _ => {}
            }
        }
    }

    /// `<<__RequirePackage("name")>>`
    #[cold]
    fn check_require_package(&mut self, attr: &Attribute<'a>) {
        match attr.args {
            [name] => self.check_package_name(name, true),
            args => {
                let end = self.attribute_end(attr);
                self.report(
                    attr.name.span.start,
                    end,
                    format!(
                        "The `__RequirePackage` attribute expects exactly 1 argument, {} given",
                        args.len()
                    ),
                );
            }
        }
    }

    /// `<<__SoftRequirePackage("name")>>`, with a sampling rate for an
    /// optional second argument.
    #[cold]
    fn check_soft_require_package(&mut self, attr: &Attribute<'a>) {
        let wrong_arity = |p: &mut Self| {
            let end = p.attribute_end(attr);
            p.report(
                attr.name.span.start,
                end,
                format!(
                    "The `__SoftRequirePackage` attribute expects exactly 1 or 2 arguments, {} given",
                    attr.args.len()
                ),
            );
        };
        let Some(name) = attr.args.first() else {
            return wrong_arity(self);
        };
        self.check_package_name(name, attr.args.len() == 1);
        let Some(rate) = attr.args.get(1) else { return };
        let written = self.term_span(rate.span, false);
        let decimal = matches!(rate.kind, ExprKind::Int(digits) if is_decimal(digits));
        if !decimal || written != rate.span {
            self.report(
                written.start,
                written.end,
                "The second argument to `__SoftRequirePackage`, if it exists, must be an integer literal",
            );
        } else if attr.args.len() > 2 {
            wrong_arity(self);
        }
    }

    /// The package an attribute names has to be a plain string literal.
    /// `sole` is set for the only argument.
    fn check_package_name(&mut self, name: &Expr<'a>, sole: bool) {
        let written = self.term_span(name.span, sole);
        let quoted = matches!(name.kind, ExprKind::String(_))
            && matches!(self.bytes[name.span.start as usize], b'\'' | b'"');
        if !quoted || written != name.span {
            self.report(
                written.start,
                written.end,
                "The first argument to `__(Soft)RequirePackage` must be a string literal",
            );
        }
    }

    /// Arguments of an attribute have to be literals, `Foo::class`,
    /// collections of those, or string concatenations.
    pub(crate) fn check_attribute_args(&mut self, attr: &Attribute<'a>) {
        let memoized = matches!(attr.name.name, "__Memoize" | "__MemoizeLSB");
        for arg in attr.args {
            self.check_attribute_arg(arg, attr.name.name, memoized);
        }
    }

    fn check_attribute_arg(&mut self, e: &Expr<'a>, attr: &str, memoized: bool) {
        match e.kind {
            ExprKind::Null
            | ExprKind::True
            | ExprKind::False
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_)
            | ExprKind::String2(_)
            | ExprKind::Nameof(_) => {}
            ExprKind::EnumClassLabel(label) if memoized => {
                if !matches!(
                    label.label,
                    "KeyedByIC" | "MakeICInaccessible" | "NotKeyedByICAndLeakIC__DO_NOT_USE"
                ) {
                    self.report_lowering(
                        e.span.start,
                        e.span.end,
                        format!("Invalid label for `{}`.", attr),
                    );
                }
            }
            ExprKind::ClassConst(access) if access.name.name == "class" => {}
            ExprKind::Unop(
                op @ (Uop::Not | Uop::Tilde | Uop::Plus | Uop::Minus | Uop::Incr | Uop::Decr),
                operand,
            ) => {
                self.check_attribute_arg(operand, attr, memoized);
                if !matches!(op, Uop::Plus | Uop::Minus) {
                    self.report_lowering(e.span.start, e.span.end, ATTRIBUTE_ARG_NOT_LITERAL);
                }
            }
            ExprKind::Await(operand)
            | ExprKind::Delay(operand)
            | ExprKind::Clone(operand)
            | ExprKind::Readonly(operand)
            | ExprKind::Print(operand) => {
                self.check_attribute_arg(operand, attr, memoized);
                self.report_lowering(e.span.start, e.span.end, ATTRIBUTE_ARG_NOT_LITERAL);
            }
            ExprKind::Binop(b) if b.op == Bop::Dot => {
                self.check_attribute_arg(&b.lhs, attr, memoized);
                self.check_attribute_arg(&b.rhs, attr, memoized);
            }
            ExprKind::ValCollection(c) => {
                for item in c.items {
                    self.check_attribute_arg(item, attr, memoized);
                }
            }
            ExprKind::KeyValCollection(c) => {
                for (k, v) in c.fields {
                    self.check_attribute_arg(k, attr, memoized);
                    self.check_attribute_arg(v, attr, memoized);
                }
            }
            ExprKind::Tuple(items, _) => {
                for item in items {
                    self.check_attribute_arg(item, attr, memoized);
                }
            }
            ExprKind::Shape(fields, _) => {
                for field in fields {
                    if let Some(ShapeKey::ClassConst(key)) = field.key()
                        && key.1.name != "class"
                    {
                        self.report_lowering(
                            key.0.span.start,
                            key.1.span.end,
                            ATTRIBUTE_ARG_NOT_LITERAL,
                        );
                    }
                    self.check_attribute_arg(field.value(), attr, memoized);
                }
            }
            _ => self.report_lowering(e.span.start, e.span.end, ATTRIBUTE_ARG_NOT_LITERAL),
        }
    }

    /// Checks the attributes HHVM gives a meaning of their own once the
    /// closing `>>` of `<<...>>` (which starts at `spec_start`) is consumed.
    pub(crate) fn check_special_attributes(&mut self, attrs: &[Attribute<'a>], spec_start: u32) {
        let spec_end = self.prev_end;
        for attr in attrs {
            let name = attr.name.name;
            if name.eq_ignore_ascii_case("__reified")
                || name.eq_ignore_ascii_case("__hasreifiedparent")
            {
                self.report_lowering(
                    spec_start,
                    spec_end,
                    "`__Reified` and `__HasReifiedParent` attributes may not be provided by the user",
                );
            } else if name.eq_ignore_ascii_case("__Soft") && !attr.args.is_empty() {
                self.report_lowering(
                    spec_start,
                    spec_end,
                    "The `__Soft` attribute does not take arguments.",
                );
            } else if matches!(name, "__Memoize" | "__MemoizeLSB")
                && let Some(first) = attr.args.first()
            {
                if !matches!(first.kind, ExprKind::EnumClassLabel(_)) {
                    self.report_lowering(
                        first.span.start,
                        first.span.end,
                        format!(
                            "The first argument to `{}` must be a label, e.g. `#KeyedByIC`.",
                            name
                        ),
                    );
                }
                if let Some(second) = attr.args.get(1) {
                    self.report_lowering(
                        second.span.start,
                        second.span.end,
                        format!(
                            "`{}` takes 1 argument when using `{}`.",
                            name,
                            first.span.text(self.src)
                        ),
                    );
                }
            }
        }
    }
}

impl<'a> Parser<'a> {
    /// A `switch` may have one `default`, and it comes last.
    pub(crate) fn check_switch_defaults(&mut self, cases: &[Case<'a>]) {
        let mut defaults = cases.iter().filter(|c| c.label.is_none());
        let first = defaults.next();
        let second = defaults.next();
        if let Some(second) = second {
            self.report_lowering(
                second.label_span.start,
                second.label_span.end,
                "There can be only one `default` case in `switch`",
            );
        }
        let last_default = cases.iter().rev().find(|c| c.label.is_none()).or(first);
        if let (Some(default), Some(last)) = (last_default, cases.last())
            && last.label.is_some()
        {
            self.report_lowering(
                default.label_span.start,
                default.label_span.end,
                "Default case in `switch` must be terminal",
            );
        }
    }

    /// A `require` clause in a class body, starting at `start` and ending
    /// with the `;` just consumed.
    pub(crate) fn check_require(&mut self, start: u32, kind: RequireKind, hint: &Hint<'a>) {
        let name = hint.span.text(self.src);
        match self
            .class_requires
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            None => self.class_requires.push((name, kind)),
            Some((_, earlier)) if *earlier == kind => {}
            Some(_) => self.report_from(start, format!("Conflicting requirements for `{}`", name)),
        }
        if kind == RequireKind::Implements
            && matches!(self.class, Some(c) if c.kind != ClassKind::Trait)
        {
            self.report_from(start, "Only traits may use `require implements`.");
        }
    }

    /// What follows `new` must be able to denote a class: a variable, a
    /// property or element of one, or a static property.
    pub(crate) fn check_new_designator(&mut self, e: &Expr<'a>, in_scope_resolution: bool) {
        let (start, end) = (e.span.start, e.span.end);
        let kind = match e.kind {
            ExprKind::Id(_) | ExprKind::Lvar(_) => return,
            ExprKind::ArrayGet(get) => {
                if get.index.is_none() {
                    self.report(
                        start,
                        end,
                        "A subscript expression `[]` on the right side of an instanceof operator must have an index",
                    );
                } else {
                    self.check_new_designator(&get.base, in_scope_resolution);
                }
                return;
            }
            ExprKind::ObjGet(get) if !get.nullsafe => {
                if in_scope_resolution {
                    self.report(
                        start,
                        end,
                        "A scope resolution `::` on the right side of an instanceof operator cannot contain a member selection `->`",
                    );
                } else {
                    self.check_new_designator(&get.object, false);
                }
                return;
            }
            ExprKind::ClassGet(access) => {
                if !matches!(access.class.kind, ExprKind::Id(_)) {
                    self.check_new_designator(&access.class, true);
                }
                return;
            }
            ExprKind::ClassConst(_) => {
                self.report(
                    start,
                    end,
                    "A scope resolution `::` on the right side of an `instanceof` operator must start with a class name, `self`, `parent`, or `static`, and end with a variable",
                );
                return;
            }
            ExprKind::Null
            | ExprKind::True
            | ExprKind::False
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_)
            | ExprKind::String2(_) => "literal",
            ExprKind::Call(call) => match call.func.kind {
                // The keywords: no function can be called that.
                ExprKind::Id("eval") => "eval_expression",
                ExprKind::Id("isset") => "isset_expression",
                _ => "function_call_expression",
            },
            ExprKind::New(_) => "object_creation_expression",
            ExprKind::Import(..) => "inclusion_expression",
            ExprKind::Yield(_) => "yield_expression",
            ExprKind::Shape(..) => "shape_expression",
            ExprKind::List(_) => "list_expression",
            ExprKind::Tuple(..) => "tuple_expression",
            ExprKind::Efun(_) => "anonymous_function",
            ExprKind::Lfun(_) => "lambda_expression",
            ExprKind::Unop(Uop::PostIncr | Uop::PostDecr, _) => "postfix_unary_expression",
            ExprKind::Unop(..)
            | ExprKind::Await(_)
            | ExprKind::Delay(_)
            | ExprKind::Clone(_)
            | ExprKind::Print(_)
            | ExprKind::Readonly(_) => "prefix_unary_expression",
            // Other kinds are too rare to be worth naming exactly.
            _ => return,
        };
        self.report(
            start,
            end,
            format!(
                "`new` requires a class name or local variable, but got: `{}`",
                kind
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Names declared and imported at the top level
//
// A `use` clause must not reuse a name that is already taken in the current
// namespace, and a declaration must not take a name that a `use` clause (or
// a mention of a built-in type in a parameter) has already claimed.

#[derive(Clone, Copy, PartialEq, Eq)]
enum NameKind {
    /// Imported by a `use` clause.
    Use,
    /// Declared in this file.
    Def,
    /// A built-in type from the `HH` namespace mentioned in a parameter.
    ImplicitUse,
}

struct NameEntry<'a> {
    /// The name as it is referred to (alias, or last segment).
    key: &'a str,
    kind: NameKind,
    /// Where it was first used or declared.
    start: u32,
    /// The fully qualified name it stands for.
    full: String,
    /// Declared in (or imported into) the global namespace.
    global: bool,
}

/// The names taken so far in the current namespace.
#[derive(Default)]
pub(crate) struct Names<'a> {
    /// Classes and other types; compared without regard to case.
    classes: Vec<NameEntry<'a>>,
    /// Namespaces; compared without regard to case.
    namespaces: Vec<NameEntry<'a>>,
    functions: Vec<NameEntry<'a>>,
    constants: Vec<NameEntry<'a>>,
    /// Where the attributes of the function marked `__EntryPoint` start.
    entry_point: Option<u32>,
    /// The built-in types (by index in `AUTOIMPORT_TYPES`) already looked
    /// at in this namespace.
    implicit_seen: u64,
    /// Too many names to keep checking cheaply.
    disabled: bool,
}

/// How many names per kind are tracked before giving up on a file.
const MAX_TRACKED_NAMES: usize = 512;

/// The types HHVM imports from the `HH` namespace into every file, sorted.
const AUTOIMPORT_TYPES: &[&str] = &[
    "AnyArray",
    "AsyncFunctionWaitHandle",
    "AsyncGenerator",
    "AsyncGeneratorWaitHandle",
    "AsyncIterator",
    "AsyncKeyedIterator",
    "AwaitAllWaitHandle",
    "Awaitable",
    "Collection",
    "ConcurrentWaitHandle",
    "ConditionWaitHandle",
    "Container",
    "ExternalThreadEventWaitHandle",
    "IMemoizeParam",
    "ImmMap",
    "ImmSet",
    "ImmVector",
    "InvariantException",
    "Iterable",
    "Iterator",
    "KeyedContainer",
    "KeyedIterable",
    "KeyedIterator",
    "KeyedTraversable",
    "Map",
    "ObjprofObjectStats",
    "ObjprofPathsStats",
    "ObjprofStringStats",
    "Pair",
    "PriorityBridgeWaitHandle",
    "RescheduleWaitHandle",
    "ResumableWaitHandle",
    "Set",
    "Shapes",
    "SleepWaitHandle",
    "StaticWaitHandle",
    "Traversable",
    "TypeStructure",
    "TypeStructureKind",
    "UNSAFESingletonMemoizeParam",
    "Vector",
    "WaitableWaitHandle",
    "XenonSample",
    "class_or_classname",
    "classname",
    "concrete",
    "darray",
    "dict",
    "keyset",
    "string",
    "supportdyn",
    "typename",
    "varray",
    "varray_or_darray",
    "vec",
    "vec_or_dict",
];

/// The index of `name` in `AUTOIMPORT_TYPES`.
fn autoimport_index(name: &str) -> Option<usize> {
    // Only a few first letters occur at all.
    if !matches!(
        *name.as_bytes().first()?,
        b'A' | b'C'
            | b'E'
            | b'I'
            | b'K'
            | b'M'
            | b'O'
            | b'P'
            | b'R'
            | b'S'
            | b'T'
            | b'U'
            | b'V'
            | b'W'
            | b'X'
            | b'c'
            | b'd'
            | b'k'
            | b's'
            | b't'
            | b'v'
    ) {
        return None;
    }
    AUTOIMPORT_TYPES.binary_search(&name).ok()
}

fn find_name<'e, 'a>(
    entries: &'e [NameEntry<'a>],
    key: &str,
    ignore_case: bool,
) -> Option<&'e NameEntry<'a>> {
    entries.iter().find(|e| {
        if ignore_case {
            e.key.eq_ignore_ascii_case(key)
        } else {
            e.key == key
        }
    })
}

fn add_name<'a>(entries: &mut Vec<NameEntry<'a>>, entry: NameEntry<'a>, ignore_case: bool) {
    let existing = entries.iter_mut().find(|e| {
        if ignore_case {
            e.key.eq_ignore_ascii_case(entry.key)
        } else {
            e.key == entry.key
        }
    });
    match existing {
        Some(slot) => *slot = entry,
        None => entries.push(entry),
    }
}

impl<'a> Parser<'a> {
    /// The 1-based line of an offset.
    /// Where in the source `text`, a piece of it, starts.
    fn offset_in_source(&self, text: &str) -> u32 {
        let start = (text.as_ptr() as usize).wrapping_sub(self.src.as_ptr() as usize);
        if start <= self.src.len() {
            start as u32
        } else {
            0
        }
    }

    fn line_of(&self, offset: u32) -> usize {
        self.bytes[..offset as usize]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1
    }

    fn names_full(&mut self) -> bool {
        let n = &self.names;
        if n.classes.len() > MAX_TRACKED_NAMES
            || n.functions.len() > MAX_TRACKED_NAMES
            || n.constants.len() > MAX_TRACKED_NAMES
            || n.namespaces.len() > MAX_TRACKED_NAMES
        {
            self.names.disabled = true;
        }
        self.names.disabled
    }

    /// Forgets the names of the namespace being left (`namespace Foo;`).
    pub(crate) fn reset_names(&mut self) {
        let disabled = self.names.disabled;
        self.names = Names::default();
        self.names.disabled = disabled;
    }

    /// Starts the name table of a `namespace { }` block, which only keeps
    /// the functions of the global namespace. Returns the table to put back
    /// afterwards.
    pub(crate) fn enter_names_block(&mut self) -> Names<'a> {
        let mut outer = std::mem::take(&mut self.names);
        self.names.disabled = outer.disabled;
        for e in outer.functions.iter().filter(|e| e.global) {
            self.names.functions.push(NameEntry {
                key: e.key,
                kind: e.kind,
                start: e.start,
                full: e.full.clone(),
                global: true,
            });
        }
        outer.disabled |= self.names.disabled;
        outer
    }

    /// Records the built-in types a parameter's type mentions: they count
    /// as uses of those names.
    pub(crate) fn note_param_hint(&mut self, hint: &Hint<'a>) {
        match hint.kind {
            HintKind::Name(name) => self.note_type_name(name, hint.span.start),
            HintKind::Generic(g) => {
                for arg in g.args {
                    self.note_param_hint(arg);
                }
                self.note_type_name(g.name, hint.span.start);
            }
            HintKind::Option(inner) | HintKind::Like(inner) | HintKind::Soft(inner) => {
                self.note_param_hint(inner)
            }
            HintKind::ClassPtr(_, inner) => self.note_param_hint(inner),
            HintKind::Fun(f) => {
                for p in f.params {
                    self.note_param_hint(&p.hint);
                }
                self.note_param_hint(&f.ret);
            }
            HintKind::Tuple(t) => {
                for h in t.required.iter().chain(t.optional) {
                    self.note_param_hint(h);
                }
                if let Some(h) = &t.variadic {
                    self.note_param_hint(h);
                }
                if let Some(h) = &t.splat {
                    self.note_param_hint(h);
                }
            }
            HintKind::Shape(s) => {
                for field in s.fields {
                    match field {
                        ShapeHintElement::Field(field) => self.note_param_hint(&field.hint),
                        ShapeHintElement::Splat(hint) => self.note_param_hint(hint),
                        _ => {}
                    }
                }
                if let Some(hint) = &s.unknown_fields {
                    self.note_param_hint(hint);
                }
            }
            HintKind::Union(hints) | HintKind::Intersection(hints) => {
                for h in hints {
                    self.note_param_hint(h);
                }
            }
            HintKind::Access(access) => self.note_param_hint(&access.root),
            _ => {}
        }
    }

    #[inline]
    fn note_type_name(&mut self, name: &'a str, start: u32) {
        let Some(index) = autoimport_index(name) else {
            return;
        };
        if self.names.implicit_seen & (1 << index) != 0 || self.names.disabled {
            return;
        }
        self.names.implicit_seen |= 1 << index;
        if find_name(&self.names.classes, name, true).is_none() {
            self.names.classes.push(NameEntry {
                key: name,
                kind: NameKind::ImplicitUse,
                start,
                full: format!("HH\\{}", name),
                global: false,
            });
        }
    }

    /// A class, trait, enum or type alias is declared: the name must not
    /// have been claimed by a `use` clause or a built-in type.
    pub(crate) fn check_declared_type_name(&mut self, name: Id<'a>) {
        if self.names_full() {
            return;
        }
        let full = combine_names(&self.ns_name, name.name);
        if let Some(e) = find_name(&self.names.classes, name.name, true)
            && e.full != full
            && e.kind != NameKind::Def
        {
            let line = self.line_of(e.start);
            let shown = full.strip_prefix('\\').unwrap_or(&full);
            let message = if e.kind == NameKind::ImplicitUse {
                format!(
                    "Cannot declare `{}` because the name was implicitly used on line {}; implicit use of names from the HH namespace can be suppressed by adding an explicit `use` statement earlier in the current namespace block",
                    shown, line
                )
            } else {
                format!(
                    "Cannot declare `{}` because the name was explicitly used earlier via a `use` statement on line {}",
                    shown, line
                )
            };
            self.report(name.span.start, name.span.end, message);
            return;
        }
        let global = self.ns_name == "\\";
        add_name(
            &mut self.names.classes,
            NameEntry {
                key: name.name,
                kind: NameKind::Def,
                start: name.span.start,
                full,
                global,
            },
            true,
        );
    }

    /// A top-level function is declared: its name has to be new to the
    /// namespace, and a file has one `__EntryPoint` at most. Both messages
    /// say where the earlier declaration is.
    fn check_redeclared_function(&mut self, h: &FunHeader<'_, 'a>) {
        if self.names_full() {
            return;
        }
        let global = self.ns_name == "\\";
        let earlier = find_name(&self.names.functions, h.name.name, false)
            .filter(|e| e.kind == NameKind::Def && e.global == global)
            .map(|e| e.start);
        if let Some(earlier) = earlier {
            let message = format!(
                "Cannot redeclare `{}()` (previously declared in {}:{})",
                h.name.name,
                self.path,
                self.line_of(earlier)
            );
            self.report(h.header.0, h.header.1, message);
        }
        if has_attr(h.attrs, "__EntryPoint") {
            if let Some(earlier) = self.names.entry_point {
                let message = format!(
                    "Only one `__EntryPoint` annotation is permitted per file (previous `__EntryPoint` annotation in {}:{})",
                    self.path,
                    self.line_of(earlier)
                );
                self.report(h.span.0, h.span.1, message);
            }
            self.names.entry_point = Some(h.attrs[0].name.span.start);
        }
    }

    /// A top-level function (`function` set) or constant is declared.
    pub(crate) fn check_declared_value_name(&mut self, name: Id<'a>, function: bool) {
        if self.names_full() {
            return;
        }
        let full = combine_names(&self.ns_name, name.name);
        let entries = if function {
            &self.names.functions
        } else {
            &self.names.constants
        };
        if let Some(e) = find_name(entries, name.name, false)
            && e.kind != NameKind::Def
        {
            let line = self.line_of(e.start);
            let shown = full.strip_prefix('\\').unwrap_or(&full);
            let message = format!(
                "Cannot declare `{}` because the name was explicitly used earlier via a `use` statement on line {}",
                shown, line
            );
            self.report(name.span.start, name.span.end, message);
        }
        let global = self.ns_name == "\\";
        let entry = NameEntry {
            key: name.name,
            kind: NameKind::Def,
            start: name.span.start,
            full,
            global,
        };
        if function {
            add_name(&mut self.names.functions, entry, false);
        } else {
            add_name(&mut self.names.constants, entry, false);
        }
    }

    /// One clause of a `use` declaration. `prefix` is the `Foo\` of a group
    /// use and `name` the clause's own name as written.
    pub(crate) fn check_use_clause(
        &mut self,
        kind: UseKind,
        prefix: Option<&str>,
        name: Id<'a>,
        alias: Option<Id<'a>>,
    ) {
        if self.names_full() {
            return;
        }
        let text = name.name;
        let qualified = combine_names(prefix.unwrap_or("\\"), text);
        let short = match alias {
            Some(alias) => alias.name,
            None => last_segment(text),
        };
        let shown = text.strip_prefix('\\').unwrap_or(text);
        let (start, end) = (name.span.start, name.span.end);
        let new_use = |full: &str| NameEntry {
            key: short,
            kind: NameKind::Use,
            start,
            full: combine_names("\\", full),
            global: true,
        };
        let in_global_namespace = self.ns_name == "\\";
        match kind {
            UseKind::Default => {
                if text == "strict" {
                    self.report(
                        start,
                        end,
                        "To use strict Hack, place `// strict` after the open tag. If it's already there, remove this line. Hack is strict already.",
                    );
                }
                if let Some(e) = find_name(&self.names.classes, short, true) {
                    if e.full != qualified || e.kind != NameKind::Def {
                        let line = self.line_of(e.start);
                        let message = match e.kind {
                            NameKind::ImplicitUse => format!(
                                "Cannot use `{}` as `{}` because the name was implicitly used on line {}; implicit use of names from the HH namespace can be suppressed by adding an explicit `use` statement earlier in the current namespace block",
                                shown, short, line
                            ),
                            NameKind::Use => format!(
                                "Cannot use `{}` as `{}` because the name was explicitly used earlier via a `use` statement on line {}",
                                shown, short, line
                            ),
                            NameKind::Def => format!(
                                "Cannot use `{}` as `{}` because the name is already in use",
                                shown, short
                            ),
                        };
                        self.report(start, end, message);
                    }
                } else {
                    if find_name(&self.names.namespaces, short, true).is_none() {
                        self.names.namespaces.push(new_use(&qualified));
                    }
                    self.names.classes.push(new_use(&qualified));
                }
            }
            UseKind::Namespace | UseKind::Type | UseKind::Function | UseKind::Const => {
                let (entries, ignore_case, redefinition_matters, what) = match kind {
                    UseKind::Namespace => (&mut self.names.namespaces, true, false, "namespace"),
                    UseKind::Type => (&mut self.names.classes, true, false, "type"),
                    UseKind::Function => (&mut self.names.functions, false, true, "function"),
                    _ => (&mut self.names.constants, false, true, "const"),
                };
                match find_name(entries, short, ignore_case) {
                    Some(e) => {
                        if e.kind != NameKind::Def
                            || (redefinition_matters && (in_global_namespace || e.global))
                        {
                            let message = format!(
                                "Cannot use {} `{}` as `{}` because the name is already in use",
                                what, shown, short
                            );
                            self.report(start, end, message);
                        }
                    }
                    None => entries.push(new_use(&qualified)),
                }
            }
        }
    }
}
