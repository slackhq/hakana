//! Token kinds and keyword tables.
//!
//! The keyword set mirrors `TokenKind::from_string` in HHVM's
//! `parser/token_kind.rs`. The `keywords` test in `hakana-parser-bench`
//! compares the two, so an HHVM upgrade that adds or re-classifies a keyword
//! is noticed.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum T {
    Eof,
    Error,

    // Names and literals
    Name,
    Variable,
    Int,
    Float,
    /// `'...'`
    SingleQuoted,
    /// `"..."`; the parser re-scans the contents for interpolation.
    DoubleQuoted,
    /// `<<<ID` ... `ID`; the parser re-scans the contents for interpolation.
    Heredoc,
    /// `<<<'ID'` ... `ID`
    Nowdoc,
    /// `:foo:bar` (only produced on request)
    XhpClassName,

    // Punctuation
    LBracket,
    RBracket,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Dot,
    DotEq,
    DotDotDot,
    Arrow,
    QArrow,
    PlusPlus,
    MinusMinus,
    StarStar,
    StarStarEq,
    Star,
    StarEq,
    Plus,
    PlusEq,
    Minus,
    MinusEq,
    Tilde,
    Bang,
    Dollar,
    DollarDollar,
    Slash,
    SlashEq,
    Percent,
    PercentEq,
    Spaceship,
    LtLt,
    LtLtEq,
    GtGt,
    GtGtEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    EqEq,
    EqEqEq,
    BangEq,
    BangEqEq,
    Caret,
    CaretEq,
    Bar,
    BarEq,
    BarBar,
    BarGt,
    BarQGt,
    Amp,
    AmpEq,
    AmpAmp,
    Question,
    QuestionAs,
    QuestionColon,
    QQ,
    QQEq,
    Colon,
    ColonColon,
    Semicolon,
    Eq,
    Comma,
    At,
    FatArrow,
    LambdaArrow,
    Backtick,
    Backslash,
    Hash,

    // Keywords. `KW_FIRST..=KW_LAST` is contiguous.
    Abstract,
    Arraykey,
    As,
    Async,
    Attribute,
    Await,
    Binary,
    Bool,
    Boolean,
    Break,
    Case,
    Catch,
    Category,
    Children,
    Class,
    Classname,
    Clone,
    Concurrent,
    Const,
    Construct,
    Continue,
    Ctx,
    Darray,
    Default,
    Delay,
    Dict,
    Do,
    Double,
    Echo,
    Else,
    Empty,
    Endif,
    Enum,
    Eval,
    Extends,
    Fallthrough,
    False,
    File,
    Final,
    Finally,
    Float_,
    For,
    Foreach,
    Function,
    Global,
    If,
    Implements,
    Include,
    IncludeOnce,
    Inout,
    Instanceof,
    Insteadof,
    Int_,
    Integer,
    Interface,
    Internal,
    Is,
    Isset,
    Keyset,
    Lateinit,
    Let,
    List,
    Match,
    Mixed,
    Module,
    Named,
    Nameof,
    Namespace,
    New,
    Newctx,
    Newtype,
    Noreturn,
    Null,
    Num,
    Optional,
    Absent,
    Package,
    Parent,
    Print,
    Private,
    Protected,
    Public,
    Readonly,
    Real,
    Reify,
    Require,
    RequireOnce,
    Required,
    Resource,
    Return,
    SelfKw,
    Shape,
    Static,
    String_,
    Super,
    Switch,
    This,
    Throw,
    Trait,
    True,
    Try,
    Tuple,
    Type,
    Unset,
    Upcast,
    Use,
    Using,
    Var,
    Varray,
    Vec,
    Void,
    Where,
    While,
    With,
    Xhp,
    Yield,
}

impl T {
    #[inline]
    pub fn is_keyword(self) -> bool {
        (self as u8) >= (T::Abstract as u8)
    }

    /// Whether this token can stand in for an identifier in positions that
    /// accept any keyword as a name (member names, constant names, ...).
    #[inline]
    pub fn is_name_like(self) -> bool {
        self == T::Name || self.is_keyword()
    }

    /// Keywords that HHVM only treats as keywords in specific positions; in
    /// "non-reserved as name" positions they lex as plain names.
    pub fn is_reserved(self) -> bool {
        !matches!(
            self,
            T::True
                | T::False
                | T::Null
                | T::Arraykey
                | T::Attribute
                | T::Binary
                | T::Bool
                | T::Boolean
                | T::Category
                | T::Children
                | T::Classname
                | T::Darray
                | T::Dict
                | T::Double
                | T::Enum
                | T::Fallthrough
                | T::Float_
                | T::File
                | T::Int_
                | T::Integer
                | T::Is
                | T::Keyset
                | T::Match
                | T::Mixed
                | T::Newctx
                | T::Newtype
                | T::Noreturn
                | T::Num
                | T::Parent
                | T::Real
                | T::Reify
                | T::Resource
                | T::SelfKw
                | T::String_
                | T::Super
                | T::This
                | T::Type
                | T::Upcast
                | T::Varray
                | T::Vec
                | T::Void
                | T::With
                | T::Where
                | T::Xhp
                | T::Internal
                | T::Let
                | T::Optional
                | T::Absent
        ) && self.is_keyword()
    }
}

/// Exact (case-sensitive) keyword lookup.
pub fn keyword(text: &[u8]) -> Option<T> {
    // Keywords are 2..=12 bytes of lowercase ASCII or '_'.
    if text.len() < 2 || text.len() > 12 {
        return None;
    }
    Some(match text {
        b"abstract" => T::Abstract,
        b"arraykey" => T::Arraykey,
        b"as" => T::As,
        b"async" => T::Async,
        b"attribute" => T::Attribute,
        b"await" => T::Await,
        b"binary" => T::Binary,
        b"bool" => T::Bool,
        b"boolean" => T::Boolean,
        b"break" => T::Break,
        b"case" => T::Case,
        b"catch" => T::Catch,
        b"category" => T::Category,
        b"children" => T::Children,
        b"class" => T::Class,
        b"classname" => T::Classname,
        b"clone" => T::Clone,
        b"concurrent" => T::Concurrent,
        b"const" => T::Const,
        b"__construct" => T::Construct,
        b"continue" => T::Continue,
        b"ctx" => T::Ctx,
        b"darray" => T::Darray,
        b"default" => T::Default,
        b"delay" => T::Delay,
        b"dict" => T::Dict,
        b"do" => T::Do,
        b"double" => T::Double,
        b"echo" => T::Echo,
        b"else" => T::Else,
        b"empty" => T::Empty,
        b"endif" => T::Endif,
        b"enum" => T::Enum,
        b"eval" => T::Eval,
        b"extends" => T::Extends,
        b"fallthrough" => T::Fallthrough,
        b"false" => T::False,
        b"file" => T::File,
        b"final" => T::Final,
        b"finally" => T::Finally,
        b"float" => T::Float_,
        b"for" => T::For,
        b"foreach" => T::Foreach,
        b"function" => T::Function,
        b"global" => T::Global,
        b"if" => T::If,
        b"implements" => T::Implements,
        b"include" => T::Include,
        b"include_once" => T::IncludeOnce,
        b"inout" => T::Inout,
        b"instanceof" => T::Instanceof,
        b"insteadof" => T::Insteadof,
        b"int" => T::Int_,
        b"integer" => T::Integer,
        b"interface" => T::Interface,
        b"internal" => T::Internal,
        b"is" => T::Is,
        b"isset" => T::Isset,
        b"keyset" => T::Keyset,
        b"lateinit" => T::Lateinit,
        b"let" => T::Let,
        b"list" => T::List,
        b"match" => T::Match,
        b"mixed" => T::Mixed,
        b"module" => T::Module,
        b"named" => T::Named,
        b"nameof" => T::Nameof,
        b"namespace" => T::Namespace,
        b"new" => T::New,
        b"newctx" => T::Newctx,
        b"newtype" => T::Newtype,
        b"noreturn" => T::Noreturn,
        b"null" => T::Null,
        b"num" => T::Num,
        b"optional" => T::Optional,
        b"absent" => T::Absent,
        b"package" => T::Package,
        b"parent" => T::Parent,
        b"print" => T::Print,
        b"private" => T::Private,
        b"protected" => T::Protected,
        b"public" => T::Public,
        b"readonly" => T::Readonly,
        b"real" => T::Real,
        b"reify" => T::Reify,
        b"require" => T::Require,
        b"require_once" => T::RequireOnce,
        b"required" => T::Required,
        b"resource" => T::Resource,
        b"return" => T::Return,
        b"self" => T::SelfKw,
        b"shape" => T::Shape,
        b"static" => T::Static,
        b"string" => T::String_,
        b"super" => T::Super,
        b"switch" => T::Switch,
        b"this" => T::This,
        b"throw" => T::Throw,
        b"trait" => T::Trait,
        b"true" => T::True,
        b"try" => T::Try,
        b"tuple" => T::Tuple,
        b"type" => T::Type,
        b"unset" => T::Unset,
        b"upcast" => T::Upcast,
        b"use" => T::Use,
        b"using" => T::Using,
        b"var" => T::Var,
        b"varray" => T::Varray,
        b"vec" => T::Vec,
        b"void" => T::Void,
        b"where" => T::Where,
        b"while" => T::While,
        b"with" => T::With,
        b"xhp" => T::Xhp,
        b"yield" => T::Yield,
        _ => return None,
    })
}

/// Keywords HHVM matches case-insensitively (reporting an error for anything
/// that isn't lowercase, except `true`/`false`/`null`).
const CASE_INSENSITIVE: &[&[u8]] = &[
    b"abstract",
    b"as",
    b"bool",
    b"boolean",
    b"break",
    b"case",
    b"catch",
    b"class",
    b"clone",
    b"const",
    b"continue",
    b"default",
    b"do",
    b"echo",
    b"else",
    b"empty",
    b"endif",
    b"eval",
    b"extends",
    b"false",
    b"final",
    b"finally",
    b"for",
    b"foreach",
    b"function",
    b"global",
    b"if",
    b"implements",
    b"include",
    b"include_once",
    b"inout",
    b"instanceof",
    b"insteadof",
    b"int",
    b"integer",
    b"interface",
    b"isset",
    b"list",
    b"namespace",
    b"new",
    b"null",
    b"parent",
    b"print",
    b"private",
    b"protected",
    b"public",
    b"require",
    b"require_once",
    b"return",
    b"self",
    b"static",
    b"string",
    b"switch",
    b"throw",
    b"trait",
    b"try",
    b"true",
    b"unset",
    b"use",
    b"using",
    b"var",
    b"void",
    b"while",
    b"yield",
];

pub enum MixedCase {
    /// Not a keyword in any casing.
    No,
    /// `TRUE`, `False`, `NULL`, ...: accepted silently.
    Literal(T),
    /// An upper/mixed-case spelling of this keyword. Where HHVM's lexer is
    /// looking for keywords it treats the name as the keyword and reports
    /// an error.
    Keyword(T),
}

/// Classifies a name that is not an exact keyword but may be one modulo case.
/// Only called for names containing an uppercase ASCII letter.
pub fn mixed_case_keyword(text: &[u8]) -> MixedCase {
    if text.len() < 2 || text.len() > 12 {
        return MixedCase::No;
    }
    let mut buf = [0u8; 12];
    let lower = &mut buf[..text.len()];
    for (dst, src) in lower.iter_mut().zip(text) {
        *dst = src.to_ascii_lowercase();
    }
    match &*lower {
        b"true" => MixedCase::Literal(T::True),
        b"false" => MixedCase::Literal(T::False),
        b"null" => MixedCase::Literal(T::Null),
        lower => {
            if CASE_INSENSITIVE.contains(&lower) {
                match keyword(lower) {
                    Some(kw) => MixedCase::Keyword(kw),
                    None => MixedCase::No,
                }
            } else {
                MixedCase::No
            }
        }
    }
}
