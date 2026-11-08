//! The analysis tree: what Hakana's scanner and analyzer consume.
//!
//! [`crate::ast`] is the syntax tree, close to the source. This tree is what
//! [`crate::lower`] makes of it: names and shapes follow the abstract syntax
//! the analyzer was written against (constructor-promoted properties
//! expanded, `echo` as a call, explicit `Noop`s, class members sorted into
//! kinds, ...), and every node carries a full [`Pos`] with line information
//! because issues are reported from here.
//!
//! Like the syntax tree it lives in a caller-owned arena: nodes are `Copy`,
//! children are `&'a T` / `&'a [T]`, text is `&'a str` borrowed from the
//! source wherever possible.

use std::ops::Deref;

pub use bstr::BStr;

pub use crate::span::LineIndex;

// ---------------------------------------------------------------------------
// Positions

/// Where a node is in the source: a byte range.
///
/// Lines and columns are not stored; they are looked up in the file's
/// [`LineIndex`] when a position has to be shown (`line`, `to_raw_span`, ...).
/// Keeping positions to eight bytes matters because nearly every node has one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pos {
    start: u32,
    end: u32,
}

impl Pos {
    /// "No position", used for nodes the lowering synthesises.
    pub const NONE: Pos = Pos {
        start: u32::MAX,
        end: u32::MAX,
    };

    #[inline]
    pub fn new(start: u32, end: u32) -> Pos {
        Pos { start, end }
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        self.start == u32::MAX
    }

    /// Offset of the first byte; 0 for [`Pos::NONE`].
    #[inline]
    pub fn start_offset(&self) -> usize {
        if self.is_none() {
            0
        } else {
            self.start as usize
        }
    }

    /// Offset just past the last byte; 0 for [`Pos::NONE`].
    #[inline]
    pub fn end_offset(&self) -> usize {
        if self.is_none() { 0 } else { self.end as usize }
    }

    /// The line the position starts on (1-based); 0 for [`Pos::NONE`].
    pub fn line(&self, lines: &LineIndex) -> usize {
        self.to_raw_span(lines).start.line()
    }

    /// `(line, offset of the start of that line, offset)` for both ends.
    /// All zeros for [`Pos::NONE`].
    pub fn to_start_and_end_lnum_bol_offset(
        &self,
        lines: &LineIndex,
    ) -> ((usize, usize, usize), (usize, usize, usize)) {
        let span = self.to_raw_span(lines);
        (
            (
                span.start.line(),
                span.start.beg_of_line(),
                span.start.offset(),
            ),
            (span.end.line(), span.end.beg_of_line(), span.end.offset()),
        )
    }

    /// Both ends with their lines worked out.
    pub fn to_raw_span(&self, lines: &LineIndex) -> PosSpanRaw {
        if self.is_none() {
            let nowhere = FilePos {
                line: 0,
                bol: 0,
                offset: 0,
            };
            return PosSpanRaw {
                start: nowhere,
                end: nowhere,
            };
        }
        let at = |offset: u32| {
            let (line, bol) = lines.line_of(offset);
            FilePos { line, bol, offset }
        };
        PosSpanRaw {
            start: at(self.start),
            end: at(self.end),
        }
    }

    /// The range from the start of `a` to the end of `b`.
    #[inline]
    pub fn btw(a: &Pos, b: &Pos) -> Pos {
        Pos {
            start: a.start,
            end: b.end,
        }
    }

    /// The smallest range covering both `a` and `b`: it starts where the
    /// earlier of the two starts and ends where the later of the two ends.
    ///
    /// `None` when exactly one of them is [`Pos::NONE`]: "no position" is in
    /// no file, so it cannot be combined with a position that is in one.
    pub fn merge(a: &Pos, b: &Pos) -> Option<Pos> {
        if a.is_none() != b.is_none() {
            return None;
        }
        if a.is_none() {
            return Some(Pos::NONE);
        }
        Some(Pos {
            start: a.start.min(b.start),
            end: a.end.max(b.end),
        })
    }

    /// `false` for a range at the very start of the file that is at most one
    /// character long, which is what a position that was never filled in
    /// looks like; `true` for everything else.
    pub fn is_valid(&self) -> bool {
        self.is_none() || self.start != 0 || self.end > 1
    }
}

impl std::fmt::Debug for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_none() {
            return write!(f, "Pos::NONE");
        }
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// The position of a comment, with its lines and columns spelled out.
///
/// Comment positions are not plain byte ranges: for compatibility with what
/// comment-driven features were built on, their lines and columns are worked
/// out in ways the line table alone cannot reproduce (see the lowering).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommentPos {
    pub start: FilePos,
    pub end: FilePos,
}

impl CommentPos {
    #[inline]
    pub fn start_offset(&self) -> usize {
        self.start.offset()
    }

    #[inline]
    pub fn end_offset(&self) -> usize {
        self.end.offset()
    }

    /// The line the comment starts on (1-based).
    #[inline]
    pub fn line(&self) -> usize {
        self.start.line()
    }

    pub fn to_raw_span(&self) -> PosSpanRaw {
        PosSpanRaw {
            start: self.start,
            end: self.end,
        }
    }

    pub fn to_start_and_end_lnum_bol_offset(
        &self,
    ) -> ((usize, usize, usize), (usize, usize, usize)) {
        (
            (
                self.start.line(),
                self.start.beg_of_line(),
                self.start.offset(),
            ),
            (self.end.line(), self.end.beg_of_line(), self.end.offset()),
        )
    }
}

// ---------------------------------------------------------------------------
// Names

/// An identifier: position and text.
#[derive(Clone, Copy, Debug)]
pub struct Id<'a>(pub Pos, pub &'a str);

impl<'a> Id<'a> {
    #[inline]
    pub fn pos(&self) -> &Pos {
        &self.0
    }
    #[inline]
    pub fn name(&self) -> &'a str {
        self.1
    }
}

pub type Sid<'a> = Id<'a>;
pub type ClassName<'a> = Id<'a>;
pub type Pstring<'a> = (Pos, &'a str);
pub type PositionedByteString<'a> = (Pos, &'a BStr);
pub type DocComment<'a> = Pstring<'a>;

/// A local variable: the counter distinguishes the `$$` of nested pipes.
pub type LocalId<'a> = (isize, &'a str);

#[derive(Clone, Copy, Debug)]
pub struct Lid<'a>(pub Pos, pub LocalId<'a>);

impl<'a> Lid<'a> {
    #[inline]
    pub fn name(&self) -> &'a str {
        self.1.1
    }
    #[inline]
    pub fn pos(&self) -> &Pos {
        &self.0
    }
}

pub mod local_id {
    use super::LocalId;

    #[inline]
    pub fn get_name<'a>(id: &LocalId<'a>) -> &'a str {
        id.1
    }
}

// ---------------------------------------------------------------------------
// Small enums

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Variance {
    Covariant,
    Contravariant,
    Invariant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConstraintKind {
    ConstraintAs,
    ConstraintEq,
    ConstraintSuper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Abstraction {
    Concrete,
    Abstract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClassishKind {
    Cclass(Abstraction),
    Cinterface,
    Ctrait,
    Cenum,
    CenumClass(Abstraction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Pinout(Pos),
    Pnormal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadonlyKind {
    Readonly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamNamed {
    ParamNamed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OptionalKind {
    Optional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SplatKind {
    Splat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperatorNullFlavor {
    Regular,
    Nullsafe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropOrMethod {
    IsProp,
    IsMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FunKind {
    FSync,
    FAsync,
    FGenerator,
    FAsyncGenerator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bop {
    Plus,
    Minus,
    Star,
    Slash,
    Eqeq,
    Eqeqeq,
    Starstar,
    Diff,
    Diff2,
    Ampamp,
    Barbar,
    Lt,
    Lte,
    Gt,
    Gte,
    Dot,
    Amp,
    Bar,
    Ltlt,
    Gtgt,
    Percent,
    Xor,
    Cmp,
    QuestionQuestion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Uop {
    Utild,
    Unot,
    Uplus,
    Uminus,
    Uincr,
    Udecr,
    Upincr,
    Updecr,
    Usilence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Visibility {
    Private,
    Public,
    Protected,
    Internal,
    ProtectedInternal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhpEnumValue<'a> {
    XEVInt(isize),
    XEVString(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tprim {
    Tnull,
    Tvoid,
    Tint,
    Tbool,
    Tfloat,
    Tresource,
    Tnum,
    Tarraykey,
    Tnoreturn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypedefVisibility {
    Transparent,
    Opaque,
    OpaqueModule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReifyKind {
    Erased,
    SoftReified,
    Reified,
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeFieldName<'a> {
    SFlitStr(PositionedByteString<'a>),
    SFclassname(Id<'a>),
    SFclassConst(Id<'a>, Pstring<'a>),
}

/// Whether a file is a declaration file (`.hhi`) or ordinary code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Mhhi,
    Mstrict,
}

// ---------------------------------------------------------------------------
// Program and statements

#[derive(Clone, Copy, Debug)]
pub struct Program<'a>(pub &'a [Def<'a>]);

impl<'a> Deref for Program<'a> {
    type Target = [Def<'a>];
    #[inline]
    fn deref(&self) -> &[Def<'a>] {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Block<'a>(pub &'a [Stmt<'a>]);

impl<'a> Deref for Block<'a> {
    type Target = [Stmt<'a>];
    #[inline]
    fn deref(&self) -> &[Stmt<'a>] {
        self.0
    }
}

impl<'a> IntoIterator for &Block<'a> {
    type Item = &'a Stmt<'a>;
    type IntoIter = std::slice::Iter<'a, Stmt<'a>>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FinallyBlock<'a>(pub &'a [Stmt<'a>]);

impl<'a> Deref for FinallyBlock<'a> {
    type Target = [Stmt<'a>];
    #[inline]
    fn deref(&self) -> &[Stmt<'a>] {
        self.0
    }
}

impl<'a> IntoIterator for &FinallyBlock<'a> {
    type Item = &'a Stmt<'a>;
    type IntoIter = std::slice::Iter<'a, Stmt<'a>>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LoopCond<'a>(
    pub Option<&'a [Lid<'a>]>,
    pub Option<Block<'a>>,
    pub Expr<'a>,
);

#[derive(Clone, Copy, Debug)]
pub struct LoopIter<'a>(
    pub Option<&'a [Lid<'a>]>,
    pub Option<Block<'a>>,
    pub &'a [Expr<'a>],
);

#[derive(Clone, Copy, Debug)]
pub struct Stmt<'a>(pub Pos, pub Stmt_<'a>);

impl<'a> Stmt<'a> {
    #[inline]
    pub fn new(pos: Pos, stmt: Stmt_<'a>) -> Self {
        Stmt(pos, stmt)
    }
    #[inline]
    pub fn noop(pos: Pos) -> Self {
        Stmt(pos, Stmt_::Noop)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Stmt_<'a> {
    Noop,
    Fallthrough,
    Expr(&'a Expr<'a>),
    Break,
    Continue,
    Throw(&'a Expr<'a>),
    Return(&'a Option<Expr<'a>>),
    YieldBreak,
    Awaitall(&'a (&'a [(Lid<'a>, Expr<'a>)], Block<'a>)),
    Concurrent(Block<'a>),
    If(&'a (Expr<'a>, Block<'a>, Block<'a>)),
    Do(&'a (Block<'a>, LoopCond<'a>)),
    While(&'a (LoopCond<'a>, Block<'a>)),
    Using(&'a UsingStmt<'a>),
    For(
        &'a (
            &'a [Expr<'a>],
            Option<LoopCond<'a>>,
            LoopIter<'a>,
            Block<'a>,
        ),
    ),
    Switch(&'a (Expr<'a>, &'a [Case<'a>], Option<DefaultCase<'a>>)),
    Match(&'a StmtMatch<'a>),
    Foreach(&'a (Expr<'a>, AsExpr<'a>, Block<'a>)),
    Try(&'a (Block<'a>, &'a [Catch<'a>], FinallyBlock<'a>)),
    Block(&'a (Option<&'a [Lid<'a>]>, Block<'a>)),
    Markup(&'a Pstring<'a>),
}

impl Stmt_<'_> {
    #[inline]
    pub fn is_noop(&self) -> bool {
        matches!(self, Stmt_::Noop)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UsingStmt<'a> {
    pub is_block_scoped: bool,
    pub has_await: bool,
    pub exprs: (Pos, &'a [Expr<'a>]),
    pub block: Block<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum AsExpr<'a> {
    AsV(Expr<'a>),
    AsKv(Expr<'a>, Expr<'a>),
    AwaitAsV(Pos, Expr<'a>),
    AwaitAsKv(Pos, Expr<'a>, Expr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct StmtMatch<'a> {
    pub expr: Expr<'a>,
    pub arms: &'a [StmtMatchArm<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct StmtMatchArm<'a> {
    pub pat: Pattern<'a>,
    pub body: Block<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum Pattern<'a> {
    PVar(&'a PatVar<'a>),
    PRefinement(&'a PatRefinement<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct PatVar<'a> {
    pub pos: Pos,
    pub id: Option<Lid<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct PatRefinement<'a> {
    pub pos: Pos,
    pub id: Option<Lid<'a>>,
    pub hint: Hint<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Case<'a>(pub Expr<'a>, pub Block<'a>);

#[derive(Clone, Copy, Debug)]
pub struct DefaultCase<'a>(pub Pos, pub Block<'a>);

#[derive(Clone, Copy, Debug)]
pub enum GenCase<'a> {
    Case(Case<'a>),
    Default(DefaultCase<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct Catch<'a>(pub ClassName<'a>, pub Lid<'a>, pub Block<'a>);

// ---------------------------------------------------------------------------
// Expressions

#[derive(Clone, Copy, Debug)]
pub struct ClassId<'a>(pub (), pub Pos, pub ClassId_<'a>);

#[derive(Clone, Copy, Debug)]
pub enum ClassId_<'a> {
    CIparent,
    CIself,
    CIstatic,
    CIexpr(Expr<'a>),
    CIreified(ClassName<'a>),
    CI(ClassName<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct Expr<'a>(pub (), pub Pos, pub Expr_<'a>);

impl<'a> Expr<'a> {
    #[inline]
    pub fn new(_: (), pos: Pos, expr: Expr_<'a>) -> Self {
        Expr((), pos, expr)
    }
    #[inline]
    pub fn pos(&self) -> &Pos {
        &self.1
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CollectionTarg<'a> {
    CollectionTV(Targ<'a>),
    CollectionTKV(Targ<'a>, Targ<'a>),
}

#[derive(Clone, Copy, Debug)]
pub enum FunctionPtrId<'a> {
    FPId(Sid<'a>),
    FPClassConst(ClassId<'a>, Pstring<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct ExpressionTree<'a> {
    pub class: ClassName<'a>,
    pub runtime_expr: Expr<'a>,
    pub free_vars: Option<&'a [Lid<'a>]>,
}

#[derive(Clone, Copy, Debug)]
pub struct As_<'a> {
    pub expr: Expr<'a>,
    pub hint: Hint<'a>,
    pub is_nullable: bool,
    pub enforce_deep: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct EtSplice<'a> {
    pub extract_client_type: bool,
    pub contains_await: bool,
    pub macro_variables: Option<&'a [Lid<'a>]>,
    pub temp_lid: Option<LocalId<'a>>,
    pub spliced_expr: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeExprField<'a> {
    SFField(ShapeFieldName<'a>, Expr<'a>),
    SFSplat(Expr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct DestructureShape<'a> {
    pub pos: Pos,
    pub fields: &'a [DestructureShapeField<'a>],
    pub ellipsis: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DestructureShapeField<'a> {
    pub optional: bool,
    pub name: ShapeFieldName<'a>,
    pub target: DestructureTarget<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct DestructureTuple<'a> {
    pub pos: Pos,
    pub entries: &'a [DestructureTupleEntry<'a>],
    pub ellipsis: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DestructureTupleEntry<'a> {
    pub optional: bool,
    pub target: DestructureTarget<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct DestructureTarget<'a>(pub (), pub (), pub DestructureTarget_<'a>);

#[derive(Clone, Copy, Debug)]
pub enum DestructureTarget_<'a> {
    DtLvar(Lid<'a>),
    DtWildcard(Pos),
    DtShape(DestructureShape<'a>),
    DtTuple(DestructureTuple<'a>),
}

// The shapes of the variants are HHVM's.
#[allow(clippy::type_complexity)]
#[derive(Clone, Copy, Debug)]
pub enum Expr_<'a> {
    Null,
    True,
    False,
    Shape(&'a [ShapeExprField<'a>]),
    ValCollection(&'a ((Pos, VcKind), Option<Targ<'a>>, &'a [Expr<'a>])),
    KeyValCollection(
        &'a (
            (Pos, KvcKind),
            Option<(Targ<'a>, Targ<'a>)>,
            &'a [Field<'a>],
        ),
    ),
    This,
    Omitted,
    Invalid(&'a Option<Expr<'a>>),
    Id(&'a Sid<'a>),
    Lvar(&'a Lid<'a>),
    Dollardollar(&'a Lid<'a>),
    Clone(&'a Expr<'a>),
    ArrayGet(&'a (Expr<'a>, Option<Expr<'a>>)),
    ObjGet(&'a (Expr<'a>, Expr<'a>, OperatorNullFlavor, PropOrMethod)),
    ClassGet(&'a (ClassId<'a>, Pstring<'a>, PropOrMethod)),
    ClassConst(&'a (ClassId<'a>, Pstring<'a>)),
    Call(&'a CallExpr<'a>),
    FunctionPointer(&'a (FunctionPtrId<'a>, &'a [Targ<'a>], FunctionPointerSource)),
    Int(&'a str),
    Float(&'a str),
    String(&'a BStr),
    String2(&'a [Expr<'a>]),
    PrefixedString(&'a (&'a str, Expr<'a>)),
    Yield(&'a Afield<'a>),
    Await(&'a Expr<'a>),
    Delay(&'a Expr<'a>),
    ReadonlyExpr(&'a Expr<'a>),
    Tuple(&'a [Expr<'a>]),
    List(&'a [Expr<'a>]),
    DestructureShape(&'a DestructureShape<'a>),
    DestructureTuple(&'a DestructureTuple<'a>),
    Cast(&'a (Hint<'a>, Expr<'a>)),
    Unop(&'a (Uop, Expr<'a>)),
    Binop(&'a Binop<'a>),
    Assign(&'a (Expr<'a>, Option<Bop>, Expr<'a>)),
    Pipe(&'a (Lid<'a>, Expr<'a>, Expr<'a>, OperatorNullFlavor)),
    Eif(&'a (Expr<'a>, Option<Expr<'a>>, Expr<'a>)),
    Is(&'a (Expr<'a>, Hint<'a>)),
    As(&'a As_<'a>),
    Upcast(&'a (Expr<'a>, Hint<'a>)),
    New(
        &'a (
            ClassId<'a>,
            &'a [Targ<'a>],
            &'a [Argument<'a>],
            Option<Expr<'a>>,
            (),
        ),
    ),
    Efun(&'a Efun<'a>),
    Lfun(&'a (Fun_<'a>, &'a [CaptureLid<'a>])),
    Xml(&'a (ClassName<'a>, &'a [XhpAttribute<'a>], &'a [Expr<'a>])),
    Import(&'a (ImportFlavor, Expr<'a>)),
    Collection(&'a (ClassName<'a>, Option<CollectionTarg<'a>>, &'a [Afield<'a>])),
    ExpressionTree(&'a ExpressionTree<'a>),
    Lplaceholder(&'a Pos),
    MethodCaller(&'a (ClassName<'a>, Pstring<'a>)),
    Pair(&'a (Option<(Targ<'a>, Targ<'a>)>, Expr<'a>, Expr<'a>)),
    ETSplice(&'a EtSplice<'a>),
    EnumClassLabel(&'a (Option<ClassName<'a>>, &'a str)),
    Hole(&'a (Expr<'a>, (), (), HoleSource<'a>)),
    Package(&'a Sid<'a>),
    Nameof(&'a ClassId<'a>),
}

impl<'a> Expr_<'a> {
    #[inline]
    pub fn as_id(&self) -> Option<&'a Sid<'a>> {
        match self {
            Expr_::Id(id) => Some(id),
            _ => None,
        }
    }
    #[inline]
    pub fn as_lvar(&self) -> Option<&'a Lid<'a>> {
        match self {
            Expr_::Lvar(lid) => Some(lid),
            _ => None,
        }
    }
    #[inline]
    pub fn as_call(&self) -> Option<&'a CallExpr<'a>> {
        match self {
            Expr_::Call(c) => Some(c),
            _ => None,
        }
    }
    #[inline]
    pub fn as_string(&self) -> Option<&'a BStr> {
        match self {
            Expr_::String(s) => Some(s),
            _ => None,
        }
    }
    #[inline]
    pub fn is_lvar(&self) -> bool {
        matches!(self, Expr_::Lvar(_))
    }
    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, Expr_::Null)
    }
    #[inline]
    pub fn is_id(&self) -> bool {
        matches!(self, Expr_::Id(_))
    }
    #[inline]
    pub fn is_call(&self) -> bool {
        matches!(self, Expr_::Call(_))
    }
    #[inline]
    pub fn is_await(&self) -> bool {
        matches!(self, Expr_::Await(_))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HoleSource<'a> {
    Typing,
    UnsafeCast(&'a [Hint<'a>]),
    UnsafeNonnullCast,
    EnforcedCast(&'a [Hint<'a>]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionPointerSource {
    Code,
    Lowered,
}

#[derive(Clone, Copy, Debug)]
pub struct Binop<'a> {
    pub bop: Bop,
    pub lhs: Expr<'a>,
    pub rhs: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Field<'a>(pub Expr<'a>, pub Expr<'a>);

#[derive(Clone, Copy, Debug)]
pub enum Afield<'a> {
    AFvalue(Expr<'a>),
    AFkvalue(Expr<'a>, Expr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct XhpSimple<'a> {
    pub name: Pstring<'a>,
    pub type_: (),
    pub expr: Expr<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum XhpAttribute<'a> {
    XhpSimple(XhpSimple<'a>),
    XhpSpread(Expr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub enum FunParamInfo<'a> {
    ParamOptional(Option<Expr<'a>>),
    ParamRequired,
    ParamVariadic,
}

#[derive(Clone, Copy, Debug)]
pub struct FunParam<'a> {
    pub annotation: (),
    pub type_hint: TypeHint<'a>,
    pub pos: Pos,
    pub name: &'a str,
    pub info: FunParamInfo<'a>,
    pub readonly: Option<ReadonlyKind>,
    pub splat: Option<SplatKind>,
    pub callconv: ParamKind,
    pub named: Option<ParamNamed>,
    pub user_attributes: UserAttributes<'a>,
    pub visibility: Option<Visibility>,
}

#[derive(Clone, Copy, Debug)]
pub struct Fun_<'a> {
    pub span: Pos,
    pub readonly_this: Option<ReadonlyKind>,
    pub annotation: (),
    pub readonly_ret: Option<ReadonlyKind>,
    pub tparams: &'a [HintTparam<'a>],
    pub ret: TypeHint<'a>,
    pub params: &'a [FunParam<'a>],
    pub ctxs: Option<Contexts<'a>>,
    pub unsafe_ctxs: Option<Contexts<'a>>,
    pub body: FuncBody<'a>,
    pub fun_kind: FunKind,
    pub user_attributes: UserAttributes<'a>,
    pub external: bool,
    pub hidden: bool,
    pub doc_comment: Option<DocComment<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct CaptureLid<'a>(pub (), pub Lid<'a>);

#[derive(Clone, Copy, Debug)]
pub struct Efun<'a> {
    pub fun: Fun_<'a>,
    pub use_: &'a [CaptureLid<'a>],
    pub closure_class_name: Option<&'a str>,
    pub is_expr_tree_virtual_expr: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct FuncBody<'a> {
    pub fb_ast: Block<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct TypeHint<'a>(pub (), pub TypeHint_<'a>);

pub type TypeHint_<'a> = Option<Hint<'a>>;

#[derive(Clone, Copy, Debug)]
pub struct Targ<'a>(pub (), pub Hint<'a>);

#[derive(Clone, Copy, Debug)]
pub struct CallExpr<'a> {
    pub func: Expr<'a>,
    pub targs: &'a [Targ<'a>],
    pub args: &'a [Argument<'a>],
    pub unpacked_arg: Option<Expr<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub enum Argument<'a> {
    Ainout(Pos, Expr<'a>),
    Anormal(Expr<'a>),
    Anamed(Sid<'a>, Expr<'a>),
}

impl<'a> Argument<'a> {
    /// The argument's expression, whatever its passing mode.
    #[inline]
    pub fn to_expr_ref(&self) -> &Expr<'a> {
        match self {
            Argument::Ainout(_, e) | Argument::Anormal(e) | Argument::Anamed(_, e) => e,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UserAttribute<'a> {
    pub name: Sid<'a>,
    pub params: &'a [Expr<'a>],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UserAttributes<'a>(pub &'a [UserAttribute<'a>]);

impl<'a> Deref for UserAttributes<'a> {
    type Target = [UserAttribute<'a>];
    #[inline]
    fn deref(&self) -> &[UserAttribute<'a>] {
        self.0
    }
}

impl<'a> IntoIterator for &UserAttributes<'a> {
    type Item = &'a UserAttribute<'a>;
    type IntoIter = std::slice::Iter<'a, UserAttribute<'a>>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FileAttribute<'a> {
    pub user_attributes: UserAttributes<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct Tparam<'a> {
    pub variance: Variance,
    pub name: Sid<'a>,
    pub constraints: &'a [(ConstraintKind, Hint<'a>)],
    pub reified: ReifyKind,
    pub user_attributes: UserAttributes<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequireKind {
    RequireExtends,
    RequireImplements,
    RequireClass,
    RequireThisAs,
}

// ---------------------------------------------------------------------------
// Classes

#[derive(Clone, Copy, Debug)]
pub struct Class_<'a> {
    pub span: Pos,
    pub annotation: (),
    pub mode: Mode,
    pub final_: bool,
    pub is_xhp: bool,
    pub has_xhp_keyword: bool,
    pub kind: ClassishKind,
    pub name: ClassName<'a>,
    pub tparams: &'a [Tparam<'a>],
    pub extends: &'a [ClassHint<'a>],
    pub uses: &'a [TraitHint<'a>],
    pub xhp_attr_uses: &'a [XhpAttrHint<'a>],
    pub xhp_category: Option<(Pos, &'a [Pstring<'a>])>,
    pub reqs: &'a [ClassReq<'a>],
    pub implements: &'a [ClassHint<'a>],
    pub consts: &'a [ClassConst<'a>],
    pub typeconsts: &'a [ClassTypeconstDef<'a>],
    pub vars: &'a [ClassVar<'a>],
    pub methods: &'a [Method_<'a>],
    pub xhp_children: &'a [(Pos, XhpChild<'a>)],
    pub xhp_attrs: &'a [XhpAttr<'a>],
    pub user_attributes: UserAttributes<'a>,
    pub docs_url: Option<&'a str>,
    pub enum_: Option<Enum_<'a>>,
    pub doc_comment: Option<DocComment<'a>>,
    pub internal: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ClassReq<'a>(pub ClassHint<'a>, pub RequireKind);

pub type ClassHint<'a> = Hint<'a>;
pub type TraitHint<'a> = Hint<'a>;
pub type XhpAttrHint<'a> = Hint<'a>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhpAttrTag {
    Required,
    LateInit,
}

#[derive(Clone, Copy, Debug)]
pub struct XhpAttr<'a>(
    pub TypeHint<'a>,
    pub ClassVar<'a>,
    pub Option<XhpAttrTag>,
    pub Option<(Pos, &'a [Expr<'a>])>,
);

#[derive(Clone, Copy, Debug)]
pub enum ClassConstKind<'a> {
    CCAbstract(Option<Expr<'a>>),
    CCConcrete(Expr<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct ClassConst<'a> {
    pub user_attributes: UserAttributes<'a>,
    pub type_: Option<Hint<'a>>,
    pub id: Sid<'a>,
    pub kind: ClassConstKind<'a>,
    pub span: Pos,
    pub doc_comment: Option<DocComment<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClassAbstractTypeconst<'a> {
    pub as_constraint: Option<Hint<'a>>,
    pub super_constraint: Option<Hint<'a>>,
    pub default: Option<Hint<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClassConcreteTypeconst<'a> {
    pub c_tc_type: Hint<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum ClassTypeconst<'a> {
    TCAbstract(ClassAbstractTypeconst<'a>),
    TCConcrete(ClassConcreteTypeconst<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct ClassTypeconstDef<'a> {
    pub user_attributes: UserAttributes<'a>,
    pub name: Sid<'a>,
    pub kind: ClassTypeconst<'a>,
    pub span: Pos,
    pub doc_comment: Option<DocComment<'a>>,
    pub is_ctx: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct XhpAttrInfo<'a> {
    pub like: Option<Pos>,
    pub tag: Option<XhpAttrTag>,
    pub enum_values: &'a [XhpEnumValue<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct ClassVar<'a> {
    pub final_: bool,
    pub xhp_attr: Option<XhpAttrInfo<'a>>,
    pub abstract_: bool,
    pub readonly: bool,
    pub visibility: Visibility,
    pub type_: TypeHint<'a>,
    pub id: Sid<'a>,
    pub expr: Option<Expr<'a>>,
    pub user_attributes: UserAttributes<'a>,
    pub doc_comment: Option<DocComment<'a>>,
    pub is_static: bool,
    pub span: Pos,
}

#[derive(Clone, Copy, Debug)]
pub struct Method_<'a> {
    pub span: Pos,
    pub annotation: (),
    pub final_: bool,
    pub abstract_: bool,
    pub static_: bool,
    pub readonly_this: bool,
    pub visibility: Visibility,
    pub name: Sid<'a>,
    pub tparams: &'a [Tparam<'a>],
    pub where_constraints: &'a [WhereConstraintHint<'a>],
    pub params: &'a [FunParam<'a>],
    pub ctxs: Option<Contexts<'a>>,
    pub unsafe_ctxs: Option<Contexts<'a>>,
    pub body: FuncBody<'a>,
    pub fun_kind: FunKind,
    pub user_attributes: UserAttributes<'a>,
    pub readonly_ret: Option<ReadonlyKind>,
    pub ret: TypeHint<'a>,
    pub external: bool,
    pub hidden: bool,
    pub doc_comment: Option<DocComment<'a>>,
}

// ---------------------------------------------------------------------------
// Other definitions

#[derive(Clone, Copy, Debug)]
pub struct TypedefVisibilityAndHint<'a> {
    pub vis: TypedefVisibility,
    pub hint: Hint<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct TypedefCaseTypeVariant<'a> {
    pub hint: Hint<'a>,
    pub where_constraints: &'a [WhereConstraintHint<'a>],
}

#[derive(Clone, Copy, Debug)]
pub enum TypedefAssignment<'a> {
    SimpleTypeDef(TypedefVisibilityAndHint<'a>),
    CaseType(TypedefCaseTypeVariant<'a>, &'a [TypedefCaseTypeVariant<'a>]),
}

#[derive(Clone, Copy, Debug)]
pub struct Typedef<'a> {
    pub annotation: (),
    pub name: Sid<'a>,
    pub tparams: &'a [Tparam<'a>],
    pub as_constraint: Option<Hint<'a>>,
    pub super_constraint: Option<Hint<'a>>,
    pub assignment: TypedefAssignment<'a>,
    pub runtime_type: Hint<'a>,
    pub user_attributes: UserAttributes<'a>,
    pub mode: Mode,
    pub span: Pos,
    pub is_ctx: bool,
    pub internal: bool,
    pub docs_url: Option<&'a str>,
    pub doc_comment: Option<DocComment<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Gconst<'a> {
    pub annotation: (),
    pub mode: Mode,
    pub name: Sid<'a>,
    pub type_: Option<Hint<'a>>,
    pub value: Expr<'a>,
    pub span: Pos,
}

#[derive(Clone, Copy, Debug)]
pub struct FunDef<'a> {
    pub mode: Mode,
    pub name: Sid<'a>,
    pub fun: Fun_<'a>,
    pub internal: bool,
    pub tparams: &'a [Tparam<'a>],
    pub where_constraints: &'a [WhereConstraintHint<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct ModuleDef<'a> {
    pub annotation: (),
    pub name: Id<'a>,
    pub user_attributes: UserAttributes<'a>,
    pub span: Pos,
    pub mode: Mode,
    pub doc_comment: Option<DocComment<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClassAlias<'a> {
    pub name: ClassName<'a>,
    pub tparams: &'a [Tparam<'a>],
    pub original: ClassName<'a>,
    pub original_tparams: &'a [Tparam<'a>],
}

#[derive(Clone, Copy, Debug)]
pub enum Def<'a> {
    Fun(&'a FunDef<'a>),
    Class(&'a Class_<'a>),
    ClassAlias(&'a ClassAlias<'a>),
    Stmt(&'a Stmt<'a>),
    Typedef(&'a Typedef<'a>),
    Constant(&'a Gconst<'a>),
    Namespace(&'a (Sid<'a>, &'a [Def<'a>])),
    NamespaceUse(&'a [(NsKind, Sid<'a>, Sid<'a>)]),
    FileAttributes(&'a FileAttribute<'a>),
    Module(&'a ModuleDef<'a>),
    SetModule(&'a Sid<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NsKind {
    NSNamespace,
    NSClass,
    NSClassAndNamespace,
    NSFun,
    NSConst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFlavor {
    Include,
    Require,
    IncludeOnce,
    RequireOnce,
}

#[derive(Clone, Copy, Debug)]
pub enum XhpChild<'a> {
    ChildName(Sid<'a>),
    ChildList(&'a [XhpChild<'a>]),
    ChildUnary(&'a XhpChild<'a>, XhpChildOp),
    ChildBinary(&'a XhpChild<'a>, &'a XhpChild<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhpChildOp {
    ChildStar,
    ChildPlus,
    ChildQuestion,
}

// ---------------------------------------------------------------------------
// Hints

#[derive(Clone, Copy, Debug)]
pub struct Hint<'a>(pub Pos, pub &'a Hint_<'a>);

pub type VariadicHint<'a> = Option<Hint<'a>>;

#[derive(Clone, Copy, Debug)]
pub struct Contexts<'a>(pub Pos, pub &'a [Context<'a>]);

pub type Context<'a> = Hint<'a>;

#[derive(Clone, Copy, Debug)]
pub struct HfParamInfo<'a> {
    pub kind: ParamKind,
    pub readonlyness: Option<ReadonlyKind>,
    pub optional: Option<OptionalKind>,
    pub splat: Option<SplatKind>,
    pub named: Option<&'a str>,
}

#[derive(Clone, Copy, Debug)]
pub struct HintFun<'a> {
    pub is_readonly: Option<ReadonlyKind>,
    pub tparams: &'a [HintTparam<'a>],
    pub param_tys: &'a [Hint<'a>],
    pub param_info: &'a [Option<HfParamInfo<'a>>],
    pub variadic_ty: VariadicHint<'a>,
    pub named_variadic_ty: VariadicHint<'a>,
    pub ctxs: Option<Contexts<'a>>,
    pub return_ty: Hint<'a>,
    pub is_readonly_return: Option<ReadonlyKind>,
}

#[derive(Clone, Copy, Debug)]
pub struct HintTparam<'a> {
    pub name: Sid<'a>,
    pub user_attributes: &'a [Sid<'a>],
    pub constraints: &'a [(ConstraintKind, Hint<'a>)],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassPtrKind {
    CKclass,
    CKenum,
}

#[derive(Clone, Copy, Debug)]
pub enum Hint_<'a> {
    Hprim(Tprim),
    Happly(ClassName<'a>, &'a [Hint<'a>]),
    Hoption(Hint<'a>),
    Hlike(Hint<'a>),
    Hfun(HintFun<'a>),
    Htuple(TupleInfo<'a>),
    HclassPtr(ClassPtrKind, Hint<'a>),
    Hshape(NastShapeInfo<'a>),
    Haccess(Hint<'a>, &'a [Sid<'a>]),
    Hsoft(Hint<'a>),
    Hrefinement(Hint<'a>, &'a [Refinement<'a>]),
    Hmixed,
    Hwildcard,
    Hnonnull,
    Habstr(&'a str),
    HvecOrDict(Option<Hint<'a>>, Hint<'a>),
    Hthis,
    Hdynamic,
    Hnothing,
    Hunion(&'a [Hint<'a>]),
    Hintersection(&'a [Hint<'a>]),
    HfunContext(&'a str),
    Hvar(&'a str),
}

#[derive(Clone, Copy, Debug)]
pub enum Refinement<'a> {
    Rctx(Sid<'a>, CtxRefinement<'a>),
    Rtype(Sid<'a>, TypeRefinement<'a>),
}

#[derive(Clone, Copy, Debug)]
pub enum TypeRefinement<'a> {
    TRexact(Hint<'a>),
    TRloose(TypeRefinementBounds<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct TypeRefinementBounds<'a> {
    pub lower: &'a [Hint<'a>],
    pub upper: &'a [Hint<'a>],
}

#[derive(Clone, Copy, Debug)]
pub enum CtxRefinement<'a> {
    CRexact(Hint<'a>),
    CRloose(CtxRefinementBounds<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct CtxRefinementBounds<'a> {
    pub lower: Option<Hint<'a>>,
    pub upper: Option<Hint<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeFieldInfo<'a> {
    pub optional: bool,
    pub hint: Hint<'a>,
    pub name: ShapeFieldName<'a>,
}

#[derive(Clone, Copy, Debug)]
pub enum ShapeElement<'a> {
    SEField(ShapeFieldInfo<'a>),
    SESplat(Hint<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct NastShapeInfo<'a> {
    pub allows_unknown_fields: bool,
    pub field_map: &'a [ShapeElement<'a>],
    pub unknown_fields_type: Option<Hint<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct TupleInfo<'a> {
    pub required: &'a [Hint<'a>],
    pub extra: TupleExtra<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct TupleExtraInfo<'a> {
    pub optional: &'a [Hint<'a>],
    pub variadic: Option<Hint<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub enum TupleExtra<'a> {
    Hextra(TupleExtraInfo<'a>),
    Hsplat(Hint<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KvcKind {
    Map,
    ImmMap,
    Dict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VcKind {
    Vector,
    ImmVector,
    Vec,
    Set,
    ImmSet,
    Keyset,
}

#[derive(Clone, Copy, Debug)]
pub struct Enum_<'a> {
    pub base: Hint<'a>,
    pub constraint: Option<Hint<'a>>,
    pub includes: &'a [Hint<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct WhereConstraintHint<'a>(pub Hint<'a>, pub ConstraintKind, pub Hint<'a>);

// ---------------------------------------------------------------------------
// Comments

/// A comment, as reported alongside the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comment {
    /// `// text` — the text keeps its trailing newline.
    CmtLine(String),
    /// `/* text */`
    CmtBlock(String),
}

/// `HH_FIXME` codes by line: line -> error code -> position of the comment.
pub type Fixmes = std::collections::BTreeMap<isize, std::collections::BTreeMap<isize, Pos>>;

/// The comments and suppression markers of a file.
#[derive(Clone, Debug, Default)]
pub struct ScouredComments {
    pub comments: Vec<(CommentPos, Comment)>,
    /// `HH_FIXME`s inside statement blocks, which are the ones that count.
    pub fixmes: Fixmes,
    pub ignores: Fixmes,
    /// `HH_FIXME`s outside statement blocks.
    pub misuses: Fixmes,
}

// ---------------------------------------------------------------------------
// Small conveniences

impl FunKind {
    pub fn is_async(self) -> bool {
        matches!(self, FunKind::FAsync | FunKind::FAsyncGenerator)
    }
}

impl XhpAttrTag {
    pub fn is_required(&self) -> bool {
        matches!(self, XhpAttrTag::Required)
    }

    pub fn is_late_init(&self) -> bool {
        matches!(self, XhpAttrTag::LateInit)
    }
}

impl<'a> TypeHint<'a> {
    pub fn get_hint(&self) -> &TypeHint_<'a> {
        &self.1
    }
}

impl TypedefVisibility {
    pub fn is_opaque(&self) -> bool {
        matches!(self, TypedefVisibility::Opaque)
    }
}

/// One end of a position, with its line worked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FilePos {
    line: u32,
    bol: u32,
    offset: u32,
}

impl FilePos {
    /// From a 1-based line, the offset at which that line starts, and the
    /// offset itself.
    pub fn new(line: usize, beg_of_line: usize, offset: usize) -> FilePos {
        FilePos {
            line: line as u32,
            bol: beg_of_line as u32,
            offset: offset as u32,
        }
    }

    /// 1-based line number.
    pub fn line(&self) -> usize {
        self.line as usize
    }

    /// 0-based column.
    pub fn column(&self) -> usize {
        (self.offset - self.bol) as usize
    }

    /// Offset of the beginning of the line.
    pub fn beg_of_line(&self) -> usize {
        self.bol as usize
    }

    pub fn offset(&self) -> usize {
        self.offset as usize
    }
}

/// The two ends of a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PosSpanRaw {
    pub start: FilePos,
    pub end: FilePos,
}

/// The text of an [`Id`].
pub type Id_ = str;

impl ClassishKind {
    pub fn is_cclass(&self) -> bool {
        matches!(self, ClassishKind::Cclass(_))
    }

    pub fn is_cinterface(&self) -> bool {
        matches!(self, ClassishKind::Cinterface)
    }

    pub fn is_ctrait(&self) -> bool {
        matches!(self, ClassishKind::Ctrait)
    }

    pub fn is_cenum(&self) -> bool {
        matches!(self, ClassishKind::Cenum)
    }

    pub fn is_cenum_class(&self) -> bool {
        matches!(self, ClassishKind::CenumClass(_))
    }
}

impl Uop {
    pub fn is_uincr(&self) -> bool {
        matches!(self, Uop::Uincr)
    }

    pub fn is_udecr(&self) -> bool {
        matches!(self, Uop::Udecr)
    }

    pub fn is_upincr(&self) -> bool {
        matches!(self, Uop::Upincr)
    }

    pub fn is_updecr(&self) -> bool {
        matches!(self, Uop::Updecr)
    }
}

impl<'a> Def<'a> {
    pub fn as_fun(&self) -> Option<&'a FunDef<'a>> {
        match self {
            Def::Fun(f) => Some(f),
            _ => None,
        }
    }

    pub fn as_class(&self) -> Option<&'a Class_<'a>> {
        match self {
            Def::Class(c) => Some(c),
            _ => None,
        }
    }
}

impl<'a> Argument<'a> {
    pub fn as_ainout(&self) -> Option<(&Pos, &Expr<'a>)> {
        match self {
            Argument::Ainout(pos, e) => Some((pos, e)),
            _ => None,
        }
    }

    pub fn as_anormal(&self) -> Option<&Expr<'a>> {
        match self {
            Argument::Anormal(e) => Some(e),
            _ => None,
        }
    }

    pub fn is_ainout(&self) -> bool {
        matches!(self, Argument::Ainout(..))
    }
}

impl<'a> Expr_<'a> {
    pub fn as_binop(&self) -> Option<&'a Binop<'a>> {
        match self {
            Expr_::Binop(b) => Some(b),
            _ => None,
        }
    }
}

impl<'a> Stmt_<'a> {
    pub fn is_return(&self) -> bool {
        matches!(self, Stmt_::Return(_))
    }

    pub fn is_do(&self) -> bool {
        matches!(self, Stmt_::Do(_))
    }

    pub fn is_while(&self) -> bool {
        matches!(self, Stmt_::While(_))
    }

    pub fn is_for(&self) -> bool {
        matches!(self, Stmt_::For(_))
    }

    pub fn is_foreach(&self) -> bool {
        matches!(self, Stmt_::Foreach(_))
    }

    pub fn as_expr(&self) -> Option<&'a Expr<'a>> {
        match self {
            Stmt_::Expr(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_return(&self) -> Option<&'a Option<Expr<'a>>> {
        match self {
            Stmt_::Return(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_if(&self) -> Option<(&'a Expr<'a>, &'a Block<'a>, &'a Block<'a>)> {
        match self {
            Stmt_::If(p) => Some((&p.0, &p.1, &p.2)),
            _ => None,
        }
    }

    pub fn as_do(&self) -> Option<(&'a Block<'a>, &'a LoopCond<'a>)> {
        match self {
            Stmt_::Do(p) => Some((&p.0, &p.1)),
            _ => None,
        }
    }

    pub fn as_while(&self) -> Option<(&'a LoopCond<'a>, &'a Block<'a>)> {
        match self {
            Stmt_::While(p) => Some((&p.0, &p.1)),
            _ => None,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn as_for(
        &self,
    ) -> Option<(
        &'a [Expr<'a>],
        &'a Option<LoopCond<'a>>,
        &'a LoopIter<'a>,
        &'a Block<'a>,
    )> {
        match self {
            Stmt_::For(p) => Some((p.0, &p.1, &p.2, &p.3)),
            _ => None,
        }
    }

    pub fn as_foreach(&self) -> Option<(&'a Expr<'a>, &'a AsExpr<'a>, &'a Block<'a>)> {
        match self {
            Stmt_::Foreach(p) => Some((&p.0, &p.1, &p.2)),
            _ => None,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn as_switch(&self) -> Option<(&'a Expr<'a>, &'a [Case<'a>], &'a Option<DefaultCase<'a>>)> {
        match self {
            Stmt_::Switch(p) => Some((&p.0, p.1, &p.2)),
            _ => None,
        }
    }

    pub fn as_try(&self) -> Option<(&'a Block<'a>, &'a [Catch<'a>], &'a FinallyBlock<'a>)> {
        match self {
            Stmt_::Try(p) => Some((&p.0, p.1, &p.2)),
            _ => None,
        }
    }
}

impl<'a> IntoIterator for &Program<'a> {
    type Item = &'a Def<'a>;
    type IntoIter = std::slice::Iter<'a, Def<'a>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> Def<'a> {
    pub fn as_namespace(&self) -> Option<(&'a Sid<'a>, &'a [Def<'a>])> {
        match self {
            Def::Namespace(ns) => Some((&ns.0, ns.1)),
            _ => None,
        }
    }
}

impl ParamKind {
    pub fn is_pnormal(&self) -> bool {
        matches!(self, ParamKind::Pnormal)
    }

    pub fn is_pinout(&self) -> bool {
        matches!(self, ParamKind::Pinout(_))
    }
}

impl<'a> Lid<'a> {
    pub fn as_local_id(&self) -> &LocalId<'a> {
        &self.1
    }
}

impl<'a> Expr<'a> {
    /// The name of the variable, if this is a local variable.
    pub fn lvar_name(&self) -> Option<&'a str> {
        match &self.2 {
            Expr_::Lvar(lid) => Some(lid.1.1),
            _ => None,
        }
    }
}
