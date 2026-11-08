//! The syntax tree produced by [`crate::parse`].
//!
//! Design notes:
//!
//! * Every node lives in a single [`bumpalo::Bump`] arena owned by the caller.
//!   Nodes are plain `Copy` data: children are `&'a T` / `&'a [T]`, never
//!   `Box`/`Vec`, so building a tree performs no per-node heap allocation and
//!   dropping it is a single arena reset.
//! * Identifiers and most literals borrow straight from the source text.
//!   Only strings containing escape sequences are copied (into the arena).
//! * Positions are byte-offset [`Span`]s (8 bytes). Lines and columns come
//!   from a per-file [`crate::LineIndex`] when somebody asks for them.
//! * The tree stays close to the source: things HHVM's lowerer synthesises
//!   (implicit `Noop`s, constructor-promotion assignments, `MemberOf<..>`
//!   wrappers, context desugaring, ...) are left to consumers.

use std::num::NonZeroU32;

use crate::span::Span;

pub type Block<'a> = &'a [Stmt<'a>];

/// Offset of the `...` that ends a destructuring pattern
/// (`tuple($a, ...) = $t`), if it has one.
pub type Rest = Option<NonZeroU32>;

/// An identifier with its position. `name` is a slice of the source file.
#[derive(Clone, Copy, Debug)]
pub struct Id<'a> {
    pub span: Span,
    pub name: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct Program<'a> {
    pub defs: &'a [Def<'a>],
    /// The `#!` line and/or `<?hh` header, when the file has one.
    pub header: Option<Header<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Header<'a> {
    pub span: Span,
    /// The hashbang line without its newline, or `""`.
    pub hashbang: &'a str,
}

// ---------------------------------------------------------------------------
// Definitions

#[derive(Clone, Copy, Debug)]
pub enum Def<'a> {
    Fun(&'a FunDef<'a>),
    Class(&'a Class<'a>),
    ClassAlias(&'a ClassAlias<'a>),
    Typedef(&'a Typedef<'a>),
    Const(&'a ConstDecl<'a>),
    Namespace(&'a Namespace<'a>),
    Use(&'a [UseClause<'a>]),
    FileAttributes(&'a [Attribute<'a>]),
    /// `module a.b;`
    SetModule(Id<'a>),
    /// `new module a.b {}`
    Module(&'a ModuleDecl<'a>),
    Stmt(&'a Stmt<'a>),
}

/// `class Name<T> = Original<T>;`
#[derive(Clone, Copy, Debug)]
pub struct ClassAlias<'a> {
    pub name: Id<'a>,
    pub tparams: &'a [Tparam<'a>],
    pub original: Id<'a>,
    pub original_tparams: &'a [Tparam<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct ModuleDecl<'a> {
    pub span: Span,
    pub attrs: &'a [Attribute<'a>],
    /// The dotted name, as written.
    pub name: Id<'a>,
    pub doc_comment: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub struct Namespace<'a> {
    pub name: Id<'a>,
    /// For `namespace Foo;` this holds every definition up to the next
    /// namespace declaration.
    pub defs: &'a [Def<'a>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseKind {
    /// `use Foo\Bar;`
    Default,
    Namespace,
    Type,
    Function,
    Const,
}

#[derive(Clone, Copy, Debug)]
pub struct UseClause<'a> {
    pub kind: UseKind,
    /// The imported name as written (group prefixes already applied).
    pub name: Id<'a>,
    pub alias: Option<Id<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Attribute<'a> {
    pub name: Id<'a>,
    pub args: &'a [Expr<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct FunDef<'a> {
    pub name: Id<'a>,
    pub fun: Fun<'a>,
    pub internal: bool,
}

/// Flags for [`Fun::flags`].
pub mod fun_flags {
    pub const ASYNC: u8 = 1;
    /// The body contains a `yield` (not counting nested functions).
    pub const HAS_YIELD: u8 = 1 << 1;
    pub const READONLY_RETURN: u8 = 1 << 2;
    /// `readonly function` / `readonly ($x) ==> ...`
    pub const READONLY_THIS: u8 = 1 << 3;
    /// Declared with `;` instead of a body.
    pub const NO_BODY: u8 = 1 << 4;
}

/// Everything shared by functions, methods, closures and lambdas.
#[derive(Clone, Copy, Debug)]
pub struct Fun<'a> {
    pub span: Span,
    pub attrs: &'a [Attribute<'a>],
    pub tparams: &'a [Tparam<'a>],
    pub params: &'a [Param<'a>],
    pub ret: Option<Hint<'a>>,
    pub ctxs: Option<Contexts<'a>>,
    pub where_constraints: &'a [WhereConstraint<'a>],
    /// A lambda with an expression body holds a single `Return`.
    pub body: Block<'a>,
    pub flags: u8,
    pub doc_comment: Option<Span>,
}

impl Fun<'_> {
    #[inline]
    pub fn is_async(&self) -> bool {
        self.flags & fun_flags::ASYNC != 0
    }
    #[inline]
    pub fn has_yield(&self) -> bool {
        self.flags & fun_flags::HAS_YIELD != 0
    }
    #[inline]
    pub fn has_body(&self) -> bool {
        self.flags & fun_flags::NO_BODY == 0
    }
}

pub mod param_flags {
    pub const VARIADIC: u8 = 1;
    pub const READONLY: u8 = 1 << 1;
    /// `optional` keyword.
    pub const OPTIONAL: u8 = 1 << 2;
    pub const NAMED: u8 = 1 << 3;
    /// `...` written before the type (splat).
    pub const SPLAT: u8 = 1 << 4;
}

#[derive(Clone, Copy, Debug)]
pub struct Param<'a> {
    /// `$name`, or `...` for an anonymous variadic.
    pub name: Id<'a>,
    pub hint: Option<Hint<'a>>,
    pub default: Option<Expr<'a>>,
    pub attrs: &'a [Attribute<'a>],
    /// Set for constructor-promoted properties.
    pub visibility: Option<Visibility>,
    /// Position of the `inout` keyword.
    pub inout: Option<Span>,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
    Protected,
    Internal,
    ProtectedInternal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variance {
    Covariant,
    Contravariant,
    Invariant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintKind {
    As,
    Eq,
    Super,
}

#[derive(Clone, Copy, Debug)]
pub struct Tparam<'a> {
    pub name: Id<'a>,
    pub variance: Variance,
    pub reified: bool,
    pub attrs: &'a [Attribute<'a>],
    pub constraints: &'a [(ConstraintKind, Hint<'a>)],
}

#[derive(Clone, Copy, Debug)]
pub struct WhereConstraint<'a> {
    pub left: Hint<'a>,
    pub kind: ConstraintKind,
    pub right: Hint<'a>,
}

/// A context list such as `[io, rx]`.
#[derive(Clone, Copy, Debug)]
pub struct Contexts<'a> {
    pub span: Span,
    pub hints: &'a [Hint<'a>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Trait,
    Enum,
    EnumClass,
}

pub mod class_flags {
    pub const ABSTRACT: u8 = 1;
    pub const FINAL: u8 = 1 << 1;
    /// Declared as `xhp class foo:bar`.
    pub const XHP_KEYWORD: u8 = 1 << 2;
    /// The name is an XHP name (`:foo:bar` or `xhp class foo:bar`).
    pub const XHP: u8 = 1 << 3;
    pub const INTERNAL: u8 = 1 << 4;
}

#[derive(Clone, Copy, Debug)]
pub struct Class<'a> {
    pub span: Span,
    pub name: Id<'a>,
    pub kind: ClassKind,
    pub flags: u8,
    pub attrs: &'a [Attribute<'a>],
    pub tparams: &'a [Tparam<'a>],
    pub extends: &'a [Hint<'a>],
    pub implements: &'a [Hint<'a>],
    /// `enum Foo: int` / `enum class Foo: Bar`
    pub enum_base: Option<Hint<'a>>,
    /// `enum Foo: int as arraykey`
    pub enum_constraint: Option<Hint<'a>>,
    /// Members in source order.
    pub members: &'a [Member<'a>],
    pub doc_comment: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub enum Member<'a> {
    Const(&'a ClassConstDecl<'a>),
    TypeConst(&'a TypeConst<'a>),
    Property(&'a PropertyDecl<'a>),
    Method(&'a Method<'a>),
    /// `use A, B;` (traits, or included enums inside an `enum`)
    Use(&'a [Hint<'a>]),
    Require(&'a Require<'a>),
    /// `A = 1;` inside an `enum`
    Enumerator(&'a Enumerator<'a>),
    /// `Type NAME = value;` inside an `enum class`
    EnumClassMember(&'a EnumClassMember<'a>),
    XhpAttributes(&'a [XhpAttrDecl<'a>]),
    XhpChildren(&'a XhpChildrenDecl<'a>),
    XhpCategory(&'a XhpCategoryDecl<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct ClassConstDecl<'a> {
    pub span: Span,
    pub attrs: &'a [Attribute<'a>],
    pub hint: Option<Hint<'a>>,
    pub is_abstract: bool,
    pub items: &'a [ConstItem<'a>],
    pub doc_comment: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub struct ConstItem<'a> {
    pub name: Id<'a>,
    pub value: Option<Expr<'a>>,
}

/// A top-level `const` declaration.
#[derive(Clone, Copy, Debug)]
pub struct ConstDecl<'a> {
    pub span: Span,
    pub hint: Option<Hint<'a>>,
    pub items: &'a [ConstItem<'a>],
}

#[derive(Clone, Copy, Debug)]
pub enum TypeConstValue<'a> {
    /// `const type T = int;`
    Type(Option<Hint<'a>>),
    /// `const ctx C = [io];`
    Ctx(Option<Contexts<'a>>),
}

#[derive(Clone, Copy, Debug)]
pub struct TypeConst<'a> {
    pub span: Span,
    pub attrs: &'a [Attribute<'a>],
    pub name: Id<'a>,
    pub is_abstract: bool,
    pub value: TypeConstValue<'a>,
    /// `as` / `super` bounds, in source order. For `ctx` constants each bound
    /// is the intersection of a context list.
    pub constraints: &'a [(ConstraintKind, Hint<'a>)],
    pub doc_comment: Option<Span>,
}

pub mod prop_flags {
    pub const STATIC: u8 = 1;
    pub const ABSTRACT: u8 = 1 << 1;
    pub const FINAL: u8 = 1 << 2;
    pub const READONLY: u8 = 1 << 3;
}

#[derive(Clone, Copy, Debug)]
pub struct PropertyDecl<'a> {
    pub attrs: &'a [Attribute<'a>],
    pub flags: u8,
    pub visibility: Visibility,
    pub hint: Option<Hint<'a>>,
    pub items: &'a [PropItem<'a>],
    pub doc_comment: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub struct PropItem<'a> {
    /// The declarator: name plus initializer.
    pub span: Span,
    /// Includes the leading `$`.
    pub name: Id<'a>,
    pub value: Option<Expr<'a>>,
}

pub mod method_flags {
    pub const STATIC: u8 = 1;
    pub const ABSTRACT: u8 = 1 << 1;
    pub const FINAL: u8 = 1 << 2;
}

#[derive(Clone, Copy, Debug)]
pub struct Method<'a> {
    pub name: Id<'a>,
    pub fun: Fun<'a>,
    /// `None` when no visibility modifier was written.
    pub visibility: Option<Visibility>,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequireKind {
    Extends,
    Implements,
    Class,
    /// `require this as Foo;`
    ThisAs,
}

#[derive(Clone, Copy, Debug)]
pub struct Require<'a> {
    pub kind: RequireKind,
    pub hint: Hint<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Enumerator<'a> {
    pub name: Id<'a>,
    pub value: Expr<'a>,
    pub doc_comment: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub struct EnumClassMember<'a> {
    pub name: Id<'a>,
    pub hint: Hint<'a>,
    pub value: Option<Expr<'a>>,
    pub is_abstract: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhpAttrTag {
    Required,
    LateInit,
}

#[derive(Clone, Copy, Debug)]
pub enum XhpAttrDecl<'a> {
    /// `attribute :other:element;` — inherit another element's attributes.
    Use(Hint<'a>),
    Attr(&'a XhpAttr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub enum XhpAttrType<'a> {
    Hint(Hint<'a>),
    /// `enum {'a', 'b'}`; the span covers `enum {...}`.
    Enum {
        span: Span,
        /// Position of a leading `~`.
        like: Option<Span>,
        values: &'a [Expr<'a>],
    },
}

#[derive(Clone, Copy, Debug)]
pub struct XhpAttr<'a> {
    pub ty: XhpAttrType<'a>,
    /// Without the leading `:` HHVM adds.
    pub name: Id<'a>,
    pub default: Option<Expr<'a>>,
    /// Span of the `= value` initializer including the `=`.
    pub default_span: Span,
    pub tag: Option<XhpAttrTag>,
}

#[derive(Clone, Copy, Debug)]
pub struct XhpChildrenDecl<'a> {
    pub span: Span,
    pub child: XhpChild<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhpChildOp {
    Star,
    Plus,
    Question,
}

#[derive(Clone, Copy, Debug)]
pub enum XhpChild<'a> {
    /// A name, `empty`, `any`, `pcdata`, a category `%foo`, or an element.
    Name(Id<'a>),
    List(&'a [XhpChild<'a>]),
    Unary(&'a XhpChild<'a>, XhpChildOp),
    /// `a | b`
    Binary(&'a XhpChild<'a>, &'a XhpChild<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct XhpCategoryDecl<'a> {
    pub span: Span,
    /// Each name includes its leading `%`.
    pub categories: &'a [Id<'a>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedefKind {
    /// `type`
    Transparent,
    /// `newtype`
    Opaque,
    /// `module newtype`
    OpaqueModule,
    /// `case type`
    CaseType,
    /// `newctx`: the alias is the intersection of a context list, as are its
    /// bounds.
    CtxAlias,
}

#[derive(Clone, Copy, Debug)]
pub struct Typedef<'a> {
    pub span: Span,
    pub name: Id<'a>,
    pub kind: TypedefKind,
    pub attrs: &'a [Attribute<'a>],
    pub tparams: &'a [Tparam<'a>],
    /// `as` / `super` bounds in source order.
    pub constraints: &'a [(ConstraintKind, Hint<'a>)],
    /// The aliased type. For a `case type` with several variants this is
    /// their union, spanning the whole variant list.
    pub hint: Hint<'a>,
    /// The variants of a `case type`; empty for other aliases.
    pub variants: &'a [CaseTypeVariant<'a>],
    pub internal: bool,
    pub doc_comment: Option<Span>,
}

/// `int where T super int` in a `case type`.
#[derive(Clone, Copy, Debug)]
pub struct CaseTypeVariant<'a> {
    pub hint: Hint<'a>,
    pub where_constraints: &'a [WhereConstraint<'a>],
}

// ---------------------------------------------------------------------------
// Type hints

#[derive(Clone, Copy, Debug)]
pub struct Hint<'a> {
    pub span: Span,
    pub kind: HintKind<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum HintKind<'a> {
    /// A named type without type arguments: `Foo`, `int`, `this`, `mixed`.
    /// The name always starts where the hint starts.
    Name(&'a str),
    /// A named type with type arguments: `Foo<T>`, `vec<int>`.
    Generic(&'a GenericHint<'a>),
    /// `?T`
    Option(&'a Hint<'a>),
    /// `~T`
    Like(&'a Hint<'a>),
    /// `<<__Soft>> T`
    Soft(&'a Hint<'a>),
    Fun(&'a FunHint<'a>),
    Tuple(&'a TupleHint<'a>),
    Shape(&'a ShapeHint<'a>),
    /// `T::TFoo::TBar`
    Access(&'a AccessHint<'a>),
    /// `T with { type A = int; ctx C as [io] }`
    Refinement(&'a RefinementHint<'a>),
    Union(&'a [Hint<'a>]),
    Intersection(&'a [Hint<'a>]),
    /// `class<T>` / `enum<T>`
    ClassPtr(ClassPtrKind, &'a Hint<'a>),
    /// `_`
    Wildcard,
    /// `$x` as the root of a dependent context (`$x::C`).
    Var(&'a str),
    /// `ctx $f`
    FunContext(&'a str),
}

/// A type with constraints on its type and context constants.
#[derive(Clone, Copy, Debug)]
pub struct RefinementHint<'a> {
    pub base: Hint<'a>,
    pub members: &'a [RefinementMember<'a>],
}

/// `type A = int`, `type A as B super C`, `ctx C = [io]`, `ctx C as [io]`
#[derive(Clone, Copy, Debug)]
pub struct RefinementMember<'a> {
    pub is_ctx: bool,
    pub name: Id<'a>,
    /// The right-hand side of `=`. For a context member this is the
    /// intersection of the context list.
    pub exact: Option<Hint<'a>>,
    /// `as` / `super` bounds in source order (context lists as intersections).
    pub bounds: &'a [(ConstraintKind, Hint<'a>)],
}

/// A type constant access such as `this::TFoo::TBar`.
#[derive(Clone, Copy, Debug)]
pub struct AccessHint<'a> {
    /// A named type, or a variable in a dependent context (`$x::C`).
    pub root: Hint<'a>,
    pub names: &'a [Id<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct GenericHint<'a> {
    pub name: &'a str,
    pub args: &'a [Hint<'a>],
}

impl<'a> Hint<'a> {
    /// The span of the name of a [`HintKind::Name`] or [`HintKind::Generic`].
    #[inline]
    pub fn name_span(&self, name: &str) -> Span {
        Span::new(self.span.start, self.span.start + name.len() as u32)
    }

    /// The name and type arguments of a named type.
    #[inline]
    pub fn as_named(&self) -> Option<(&'a str, &'a [Hint<'a>])> {
        match self.kind {
            HintKind::Name(name) => Some((name, &[])),
            HintKind::Generic(g) => Some((g.name, g.args)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassPtrKind {
    Class,
    Enum,
}

pub mod fun_hint_flags {
    pub const READONLY: u8 = 1;
    pub const OPTIONAL: u8 = 1 << 1;
    pub const SPLAT: u8 = 1 << 2;
    pub const VARIADIC: u8 = 1 << 3;
}

#[derive(Clone, Copy, Debug)]
pub struct FunHintParam<'a> {
    pub hint: Hint<'a>,
    /// Position of `inout`.
    pub inout: Option<Span>,
    /// `named` parameter name.
    pub named: Option<&'a str>,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct FunHint<'a> {
    /// `(function<T>(T): T)`
    pub tparams: &'a [Tparam<'a>],
    pub params: &'a [FunHintParam<'a>],
    pub ctxs: Option<Contexts<'a>>,
    pub ret: Hint<'a>,
    /// `(readonly function(...): T)`
    pub is_readonly: bool,
    pub readonly_ret: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TupleHint<'a> {
    pub required: &'a [Hint<'a>],
    pub optional: &'a [Hint<'a>],
    pub variadic: Option<Hint<'a>>,
    pub splat: Option<Hint<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeKey<'a> {
    /// A string literal; the span includes the quotes.
    Str { span: Span, value: &'a [u8] },
    /// `Foo::BAR`: the class and the constant.
    ClassConst(&'a (Id<'a>, Id<'a>)),
    /// `nameof Foo`: where the key starts, and the class.
    Nameof { start: u32, class: &'a Id<'a> },
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeHintField<'a> {
    pub key: ShapeKey<'a>,
    pub optional: bool,
    pub hint: Hint<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeHintElement<'a> {
    /// `'x' => int`
    Field(ShapeHintField<'a>),
    /// `...T`: the fields of another shape type.
    Splat(Hint<'a>),
    /// `absent 'x'`: the field must not be present.
    Absent { span: Span, key: ShapeKey<'a> },
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeHint<'a> {
    pub fields: &'a [ShapeHintElement<'a>],
    /// Ends with `...` or `T...`.
    pub open: bool,
    /// The `T` of a trailing `T...`: the type of the fields not listed.
    pub unknown_fields: Option<Hint<'a>>,
}

// ---------------------------------------------------------------------------
// Statements

#[derive(Clone, Copy, Debug)]
pub struct Stmt<'a> {
    pub span: Span,
    pub kind: StmtKind<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum StmtKind<'a> {
    /// An empty statement (`;`) used as an unbraced body.
    Noop,
    Expr(&'a Expr<'a>),
    Break,
    Continue,
    Throw(&'a Expr<'a>),
    Return(Option<&'a Expr<'a>>),
    YieldBreak,
    Echo(&'a Echo<'a>),
    Unset(&'a Echo<'a>),
    If(&'a If<'a>),
    While(&'a While<'a>),
    Do(&'a While<'a>),
    For(&'a For<'a>),
    Foreach(&'a Foreach<'a>),
    Switch(&'a Switch<'a>),
    Match(&'a Match<'a>),
    Try(&'a Try<'a>),
    Using(&'a Using<'a>),
    Concurrent(Block<'a>),
    Block(Block<'a>),
}

/// `echo a, b;` or `unset($a, $b);`
#[derive(Clone, Copy, Debug)]
pub struct Echo<'a> {
    /// Position of the keyword.
    pub keyword: Span,
    pub args: &'a [Expr<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct If<'a> {
    pub cond: Expr<'a>,
    pub then: Block<'a>,
    /// `None` when there is no `else`. An `else if` is an `else` holding a
    /// single `If`.
    pub otherwise: Option<Block<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct While<'a> {
    pub cond: Expr<'a>,
    pub body: Block<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct For<'a> {
    pub init: &'a [Expr<'a>],
    pub cond: Option<Expr<'a>>,
    pub step: &'a [Expr<'a>],
    pub body: Block<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Foreach<'a> {
    pub collection: Expr<'a>,
    /// Position of `await` in `foreach ($x await as ...)`.
    pub await_span: Option<Span>,
    pub key: Option<Expr<'a>>,
    pub value: Expr<'a>,
    pub body: Block<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Switch<'a> {
    pub subject: Expr<'a>,
    pub cases: &'a [Case<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct Case<'a> {
    /// `None` for `default:`.
    pub label: Option<Expr<'a>>,
    /// Span of `case X:` / `default:`.
    pub label_span: Span,
    /// Statements of the section this label ends; empty for a label that is
    /// directly followed by another label.
    pub body: Block<'a>,
    /// Control continues into the next case: either the label shares the
    /// next label's statements, or the section is marked `// FALLTHROUGH`.
    pub fallthrough: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Match<'a> {
    pub subject: Expr<'a>,
    pub arms: &'a [MatchArm<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct MatchArm<'a> {
    pub pattern: Pattern<'a>,
    pub body: Block<'a>,
}

/// What a pattern calls the value it matches.
#[derive(Clone, Copy, Debug)]
pub enum PatternBinding<'a> {
    /// `$x`
    Variable(&'a str),
    /// `_`, or any other name, which HHVM reads as a misspelt `_`.
    Wildcard(&'a str),
}

/// The pattern of a `match` arm: `_`, `$x`, or either refined by a type
/// (`_: int`).
#[derive(Clone, Copy, Debug)]
pub struct Pattern<'a> {
    pub span: Span,
    pub binding: PatternBinding<'a>,
    pub hint: Option<Hint<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Try<'a> {
    pub body: Block<'a>,
    pub catches: &'a [Catch<'a>],
    pub finally: Option<Block<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Catch<'a> {
    pub class: Id<'a>,
    pub var: Id<'a>,
    pub body: Block<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Using<'a> {
    pub has_await: bool,
    /// Span of the expression list.
    pub exprs_span: Span,
    pub exprs: &'a [Expr<'a>],
    /// `None` for the function-scoped form (`using $x = f();`), which covers
    /// the rest of the enclosing block.
    pub body: Option<Block<'a>>,
    /// For the function-scoped form written directly inside a `{ ... }`
    /// block: the span of that block. `Span::NONE` otherwise.
    pub enclosing_block: Span,
}

// ---------------------------------------------------------------------------
// Expressions

#[derive(Clone, Copy, Debug)]
pub struct Expr<'a> {
    pub span: Span,
    pub kind: ExprKind<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uop {
    /// `~`
    Tilde,
    /// `!`
    Not,
    Plus,
    Minus,
    /// `++$x`
    Incr,
    /// `--$x`
    Decr,
    /// `$x++`
    PostIncr,
    /// `$x--`
    PostDecr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bop {
    Plus,
    Minus,
    Star,
    Slash,
    EqEq,
    EqEqEq,
    StarStar,
    Diff,
    Diff2,
    AmpAmp,
    BarBar,
    Lt,
    Lte,
    Gt,
    Gte,
    Dot,
    Amp,
    Bar,
    LtLt,
    GtGt,
    Percent,
    Xor,
    Cmp,
    QuestionQuestion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFlavor {
    Include,
    Require,
    IncludeOnce,
    RequireOnce,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcKind {
    Vec,
    Keyset,
}

#[derive(Clone, Copy, Debug)]
pub enum ExprKind<'a> {
    Null,
    True,
    False,
    /// A missing element, as in `list(, $b)`.
    Omitted,
    /// `$expr`, the `$` operator of PHP applied to something (`$$x`,
    /// `${'name'}`): never valid, but parsed, and its operand is checked
    /// like any other expression. The span starts at the `$`.
    Dollar(&'a Expr<'a>),
    /// `$x`, `$this`, `$$`. The name includes the `$`.
    Lvar(&'a str),
    /// A bare name: constant, function, class, `self`, `parent`, `static`.
    Id(&'a str),
    Int(&'a str),
    Float(&'a str),
    /// A string literal with escapes resolved.
    String(&'a [u8]),
    /// An interpolated string: literal pieces and embedded expressions.
    String2(&'a [Expr<'a>]),
    /// `re"..."`
    PrefixedString(&'a PrefixedString<'a>),
    Shape(&'a [ShapeField<'a>], Rest),
    /// `vec[..]`, `keyset[..]`
    ValCollection(&'a ValCollection<'a>),
    /// `dict[..]`
    KeyValCollection(&'a KeyValCollection<'a>),
    /// `Vector {..}`, `Map {..}`, ...
    Collection(&'a Collection<'a>),
    Tuple(&'a [Expr<'a>], Rest),
    /// `optional $x` as an entry of a tuple pattern. The span starts at the
    /// keyword.
    OptionalEntry(&'a Expr<'a>),
    List(&'a [Expr<'a>]),
    Clone(&'a Expr<'a>),
    Await(&'a Expr<'a>),
    Delay(&'a Expr<'a>),
    Readonly(&'a Expr<'a>),
    Print(&'a Expr<'a>),
    ArrayGet(&'a ArrayGet<'a>),
    ObjGet(&'a ObjGet<'a>),
    /// `Foo::$bar`
    ClassGet(&'a ClassAccess<'a>),
    /// `Foo::BAR`, `Foo::bar` (as a call target), `Foo::class`
    ClassConst(&'a ClassAccess<'a>),
    Call(&'a Call<'a>),
    /// `foo<>`, `Foo::bar<>`
    FunctionPointer(&'a FunctionPointer<'a>),
    Yield(&'a Afield<'a>),
    Cast(&'a Cast<'a>),
    Unop(Uop, &'a Expr<'a>),
    Binop(&'a Binop<'a>),
    Assign(&'a Assign<'a>),
    /// `lhs |> rhs`
    Pipe(&'a Pipe<'a>),
    /// `c ? a : b` and `a ?: b`
    Eif(&'a Eif<'a>),
    Is(&'a TypeTest<'a>),
    As(&'a TypeTest<'a>),
    /// `?as`
    NullableAs(&'a TypeTest<'a>),
    Upcast(&'a TypeTest<'a>),
    New(&'a New<'a>),
    /// `function(..) use (..) {..}`
    Efun(&'a Efun<'a>),
    /// `(..) ==> ..`
    Lfun(&'a Fun<'a>),
    /// `async { .. }`
    AsyncBlock(&'a Fun<'a>),
    Xml(&'a Xml<'a>),
    Import(ImportFlavor, &'a Expr<'a>),
    /// `#Label`, `Foo#Label`
    EnumClassLabel(&'a EnumClassLabel<'a>),
    /// `` Dsl`...` ``
    ExpressionTree(&'a ExpressionTree<'a>),
    /// `${...}` inside an expression tree. The span starts at the `$`.
    EtSplice(&'a EtSplice<'a>),
    /// `nameof Foo`
    Nameof(&'a Expr<'a>),
    /// `package foo`
    Package(&'a Id<'a>),
    /// `inout $x` as a call argument. The span starts at the keyword.
    InoutArg(&'a Expr<'a>),
    /// `name = value` as a call argument.
    NamedArg(&'a NamedArg<'a>),
}

/// An expression tree literal: code that is handed to the class named in
/// front of it rather than run.
#[derive(Clone, Copy, Debug)]
pub struct ExpressionTree<'a> {
    /// The class in front of the opening backtick.
    pub prefix: Option<Id<'a>>,
    pub body: EtBody<'a>,
    /// The extent of the body, parentheses around it included.
    pub body_span: Span,
}

#[derive(Clone, Copy, Debug)]
pub enum EtBody<'a> {
    Expr(Expr<'a>),
    /// `` Dsl`{ ... }` ``
    Block(Block<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct EtSplice<'a> {
    pub expr: Expr<'a>,
    /// The extent of the expression, parentheses around it included.
    pub span: Span,
}

#[derive(Clone, Copy, Debug)]
pub struct PrefixedString<'a> {
    pub prefix: &'a str,
    pub expr: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeField<'a> {
    /// `'key' => value`, or `$x` standing for `'x' => $x` (the key then has
    /// the span of the variable).
    Field { key: ShapeKey<'a>, value: Expr<'a> },
    /// `?'key' => $x`: a field of a destructuring pattern that may be
    /// absent.
    Optional(&'a OptionalShapeField<'a>),
    /// `...$e`: the fields of another shape. `start` is where the `...` is.
    Splat { start: u32, value: Expr<'a> },
}

#[derive(Clone, Copy, Debug)]
pub struct OptionalShapeField<'a> {
    /// Where the `?` is.
    pub question: u32,
    pub key: ShapeKey<'a>,
    pub value: Expr<'a>,
}

impl<'a> ShapeField<'a> {
    /// The expression of the field: its value, or the shape being splatted.
    #[inline]
    pub fn value(&self) -> &Expr<'a> {
        match self {
            ShapeField::Field { value, .. } | ShapeField::Splat { value, .. } => value,
            ShapeField::Optional(field) => &field.value,
        }
    }

    /// The key of the field; a splat has none.
    #[inline]
    pub fn key(&self) -> Option<&ShapeKey<'a>> {
        match self {
            ShapeField::Field { key, .. } => Some(key),
            ShapeField::Optional(field) => Some(&field.key),
            ShapeField::Splat { .. } => None,
        }
    }

    #[inline]
    pub fn is_optional(&self) -> bool {
        matches!(self, ShapeField::Optional(_))
    }

    /// Whether this is a variable standing for a field of its name.
    #[inline]
    pub fn is_punned(&self) -> bool {
        matches!(self.key(), Some(ShapeKey::Str { span, .. }) if *span == self.value().span)
    }

    /// The extent of the field as written.
    pub fn span(&self) -> Span {
        let start = match self {
            ShapeField::Field { key, .. } => match key {
                ShapeKey::Str { span, .. } => span.start,
                ShapeKey::ClassConst((class, _)) => class.span.start,
                ShapeKey::Nameof { start, .. } => *start,
            },
            ShapeField::Optional(field) => field.question,
            ShapeField::Splat { start, .. } => *start,
        };
        Span::new(start, self.value().span.end)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ValCollection<'a> {
    pub kind: VcKind,
    /// Position of the `vec` / `keyset` keyword.
    pub keyword: Span,
    pub targ: Option<Hint<'a>>,
    pub items: &'a [Expr<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct KeyValCollection<'a> {
    pub keyword: Span,
    pub targs: Option<(Hint<'a>, Hint<'a>)>,
    pub fields: &'a [(Expr<'a>, Expr<'a>)],
}

#[derive(Clone, Copy, Debug)]
pub struct Collection<'a> {
    pub name: Id<'a>,
    pub targs: &'a [Hint<'a>],
    pub fields: &'a [Afield<'a>],
}

#[derive(Clone, Copy, Debug)]
pub enum Afield<'a> {
    Value(Expr<'a>),
    KeyValue(Expr<'a>, Expr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct ArrayGet<'a> {
    pub base: Expr<'a>,
    /// `None` for `$x[]`.
    pub index: Option<Expr<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ObjGet<'a> {
    pub object: Expr<'a>,
    /// An `Id` for `->name`, otherwise the dynamic member expression.
    pub member: Expr<'a>,
    pub nullsafe: bool,
    /// `true` when this is the receiver of a method call (`$a->b(..)`), as
    /// opposed to a property read — including one that is then invoked,
    /// such as `($a->b)(..)`.
    pub is_method: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ClassAccess<'a> {
    /// Usually an `Id` (including `self` / `parent` / `static`).
    pub class: Expr<'a>,
    /// For `ClassGet` the name includes the `$`.
    pub name: Id<'a>,
}

/// `name = value` in an argument list.
#[derive(Clone, Copy, Debug)]
pub struct NamedArg<'a> {
    pub name: Id<'a>,
    pub value: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Call<'a> {
    pub func: Expr<'a>,
    pub targs: &'a [Hint<'a>],
    /// Ordinary arguments are plain expressions; `inout $x` and
    /// `name = value` appear as [`ExprKind::InoutArg`] and
    /// [`ExprKind::NamedArg`].
    pub args: &'a [Expr<'a>],
    /// `...$rest`
    pub unpacked: Option<&'a Expr<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct FunctionPointer<'a> {
    /// An `Id` or a `ClassConst`.
    pub target: Expr<'a>,
    pub targs: &'a [Hint<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct Cast<'a> {
    pub hint: Hint<'a>,
    pub expr: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Binop<'a> {
    pub op: Bop,
    pub lhs: Expr<'a>,
    pub rhs: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Assign<'a> {
    pub lhs: Expr<'a>,
    /// `Some` for compound assignment (`+=`, `??=`, ...).
    pub op: Option<Bop>,
    pub rhs: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Pipe<'a> {
    pub lhs: Expr<'a>,
    pub rhs: Expr<'a>,
    /// `|?>`
    pub nullsafe: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Eif<'a> {
    pub cond: Expr<'a>,
    pub then: Option<Expr<'a>>,
    pub otherwise: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct TypeTest<'a> {
    pub expr: Expr<'a>,
    pub hint: Hint<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct New<'a> {
    pub class: Expr<'a>,
    pub targs: &'a [Hint<'a>],
    pub args: &'a [Expr<'a>],
    pub unpacked: Option<&'a Expr<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Efun<'a> {
    pub fun: Fun<'a>,
    pub uses: &'a [Id<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct Xml<'a> {
    /// The element name as written, without HHVM's leading `:`.
    pub name: Id<'a>,
    pub attrs: &'a [XhpAttribute<'a>],
    /// Text runs (as `String`, whitespace-normalised) and embedded
    /// expressions / elements.
    pub children: &'a [Expr<'a>],
}

#[derive(Clone, Copy, Debug)]
pub enum XhpAttribute<'a> {
    Simple(Id<'a>, Expr<'a>),
    /// `{...$attrs}`
    Spread(Expr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct EnumClassLabel<'a> {
    pub class: Option<Id<'a>>,
    pub label: &'a str,
}
