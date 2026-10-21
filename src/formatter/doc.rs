// Ported from HHVM hackfmt. Copyright (c) Meta Platforms, Inc. and affiliates.
// Licensed under the MIT license; see NOTICE and LICENSE in this directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Simple(i32),
    Always,
    Parental,
}
impl Rule {
    pub fn cost(self) -> i32 {
        match self {
            Self::Simple(c) => c,
            Self::Always => 0,
            Self::Parental => 1,
        }
    }
}
#[derive(Clone, Debug)]
pub enum Doc {
    Nil,
    Concat(Vec<Doc>),
    Text(String, usize),
    Comment(String, usize),
    LineComment(String, usize),
    Ignore(usize),
    Split,
    SplitWith(i32),
    Newline,
    BlankLine,
    Space,
    Span(Vec<Doc>),
    Nest(Vec<Doc>),
    Block(Vec<Doc>),
    Conditional(Vec<Doc>),
    With(Rule, Box<Doc>),
    Lazy(Rule, Box<Doc>, Box<Doc>),
    Multiline(Vec<String>, usize),
    Literal(Box<Doc>),
    Number(Box<Doc>),
    ConcatOp(Box<Doc>),
    Override(Box<Doc>),
    Comma(bool),
}
pub fn cat(xs: Vec<Doc>) -> Doc {
    Doc::Concat(xs)
}
pub fn rule(r: Rule, d: Doc) -> Doc {
    Doc::With(r, Box::new(d))
}
pub fn lazy(r: Rule, a: Doc, b: Doc) -> Doc {
    Doc::Lazy(r, Box::new(a), Box::new(b))
}
pub fn space_split() -> Doc {
    cat(vec![Doc::Space, Doc::Split])
}
impl Doc {
    pub fn printable(&self) -> bool {
        match self {
            Self::Text(..)
            | Self::Comment(..)
            | Self::LineComment(..)
            | Self::Multiline(..)
            | Self::Literal(..)
            | Self::Number(..)
            | Self::ConcatOp(..) => true,
            Self::Concat(xs)
            | Self::Span(xs)
            | Self::Nest(xs)
            | Self::Block(xs)
            | Self::Conditional(xs) => xs.iter().any(Self::printable),
            Self::With(_, x) | Self::Override(x) => x.printable(),
            Self::Lazy(_, a, b) => a.printable() || b.printable(),
            _ => false,
        }
    }
    pub fn has_split(&self) -> bool {
        match self {
            Self::Split
            | Self::SplitWith(..)
            | Self::Newline
            | Self::BlankLine
            | Self::Multiline(..)
            | Self::Literal(..) => true,
            Self::Concat(xs)
            | Self::Span(xs)
            | Self::Nest(xs)
            | Self::Block(xs)
            | Self::Conditional(xs) => xs.iter().any(Self::has_split),
            Self::With(_, x) | Self::Override(x) => x.has_split(),
            Self::Lazy(_, a, b) => a.has_split() || b.has_split(),
            _ => false,
        }
    }
}
