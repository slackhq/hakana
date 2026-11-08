//! Recording the lossless syntax tree (see [`crate::cst`]).
//!
//! All of this is inert unless the parser was started in recording mode;
//! analysis never pays for it.
//!
//! The productions of the grammar say what they have recognised as they go:
//! [`Parser::bump`] records each token, and the `c_*` calls below wrap what
//! is on the builder's stack into nodes. Every production leaves exactly the
//! children HHVM's node of the same kind has, in the same order, with
//! `Missing` standing in for an optional child the source does not have.

use super::Parser;
use crate::cst::{SyntaxKind, TokenKind, trivia};
use crate::lexer::{Token, is_name_start};
use crate::token::T;

/// What follows a token that still belongs to it.
#[derive(Clone, Copy)]
pub(crate) enum Trailing {
    /// Whitespace and `//` comments up to the end of the line.
    Code,
    /// Whitespace up to the end of the line (text inside XHP).
    Xhp,
    /// Nothing: whatever follows leads the next token.
    None,
}

impl<'a> Parser<'a> {
    /// Whether the lossless tree is being recorded.
    #[inline(always)]
    pub(crate) fn c_on(&self) -> bool {
        self.cst.is_some()
    }

    /// The token kind the lossless tree uses for `tok` where nothing about
    /// its context says otherwise.
    fn c_kind(&self, tok: Token) -> TokenKind {
        let text = &self.bytes[tok.start as usize..tok.end as usize];
        match tok.kind {
            T::Eof => TokenKind::EndOfFile,
            T::Name => TokenKind::Name,
            T::Variable => TokenKind::Variable,
            T::Float => TokenKind::FloatingLiteral,
            T::Int => match text {
                [b'0', b'x' | b'X', ..] => TokenKind::HexadecimalLiteral,
                [b'0', b'b' | b'B', ..] => TokenKind::BinaryLiteral,
                [b'0', _, ..] => TokenKind::OctalLiteral,
                _ => TokenKind::DecimalLiteral,
            },
            T::SingleQuoted => TokenKind::SingleQuotedStringLiteral,
            // Taken as it stands (see `whole_string_token`): a whole
            // string, or the start of one that has more to it.
            T::DoubleQuoted => match text {
                [b'"', .., b'"'] => TokenKind::DoubleQuotedStringLiteral,
                _ if tok.end as usize == self.bytes.len() => TokenKind::DoubleQuotedStringLiteral,
                _ => TokenKind::DoubleQuotedStringLiteralHead,
            },
            T::Heredoc => TokenKind::HeredocStringLiteral,
            T::Nowdoc => TokenKind::NowdocStringLiteral,
            T::XhpClassName => TokenKind::XHPClassName,
            T::Error => TokenKind::ErrorToken,
            // These are keywords in any case.
            T::True | T::False => TokenKind::BooleanLiteral,
            T::Null => TokenKind::NullLiteral,
            _ => TokenKind::from_string(text, false).unwrap_or_else(|| {
                // A keyword in another case (an error, but still that
                // keyword).
                let lower = text.to_ascii_lowercase();
                TokenKind::from_string(&lower, false).unwrap_or(TokenKind::Name)
            }),
        }
    }

    /// Records the current token; called as it is consumed.
    #[inline(always)]
    pub(crate) fn c_push_current(&mut self) {
        if self.cst.is_some() {
            self.c_push_token(self.tok);
        }
    }

    #[cold]
    fn c_push_token(&mut self, tok: Token) {
        let kind = self.c_kind(tok);
        if kind == TokenKind::DoubleQuotedStringLiteralHead {
            // HHVM's lexer reads nothing after it.
            self.c_token_with(kind, tok.start, tok.end, Trailing::None);
        } else {
            self.c_token(kind, tok.start, tok.end);
        }
    }

    /// Records a token with the given kind and extent.
    #[inline(always)]
    pub(crate) fn c_token(&mut self, kind: TokenKind, start: u32, end: u32) {
        self.c_token_with(kind, start, end, Trailing::Code);
    }

    /// Records a token that is not followed by the usual trailing trivia.
    #[inline(always)]
    pub(crate) fn c_token_with(&mut self, kind: TokenKind, start: u32, end: u32, rule: Trailing) {
        if self.cst.is_some() {
            self.c_record_token(kind, start, end, rule);
        }
    }

    #[cold]
    fn c_record_token(&mut self, kind: TokenKind, start: u32, end: u32, rule: Trailing) {
        let bytes = self.bytes;
        if let Some(cst) = &mut self.cst {
            match rule {
                Trailing::Code => cst.token(bytes, kind, start, end),
                Trailing::Xhp => {
                    let trailing = trivia::trailing_width_xhp(bytes, end as usize);
                    cst.token_with_trailing(kind, start, end, trailing as u32);
                }
                Trailing::None => cst.token_with_trailing(kind, start, end, 0),
            }
        }
    }

    /// Records that an optional child is absent.
    #[inline(always)]
    pub(crate) fn c_missing(&mut self) {
        if let Some(cst) = &mut self.cst {
            cst.missing();
        }
    }

    /// Records `count` absent children in a row.
    #[inline(always)]
    pub(crate) fn c_missing_n(&mut self, count: usize) {
        if let Some(cst) = &mut self.cst {
            cst.missing_n(count);
        }
    }

    /// Wraps the children recorded last (as many as `kind` has) in a node.
    #[inline(always)]
    pub(crate) fn c_node(&mut self, kind: SyntaxKind) {
        if let Some(cst) = &mut self.cst {
            cst.node(kind);
        }
    }

    /// A position to hand to [`Parser::c_list`] once the items are recorded.
    #[inline(always)]
    pub(crate) fn c_mark(&self) -> usize {
        match &self.cst {
            Some(cst) => cst.mark(),
            None => 0,
        }
    }

    /// Wraps everything recorded since `mark` in a list.
    #[inline(always)]
    pub(crate) fn c_list(&mut self, mark: usize) {
        if let Some(cst) = &mut self.cst {
            cst.list(mark);
        }
    }

    /// Ends a list item: the item recorded last and its separator, which is
    /// the token just consumed if `separated` and absent otherwise.
    #[inline(always)]
    pub(crate) fn c_list_item(&mut self, separated: bool) {
        if let Some(cst) = &mut self.cst {
            cst.list_item(separated);
        }
    }

    /// Changes the kind of the token recorded last.
    #[inline(always)]
    pub(crate) fn c_retag(&mut self, kind: TokenKind) {
        if let Some(cst) = &mut self.cst {
            cst.retag(kind);
        }
    }

    /// The token recorded last is a name here, even if it is spelled like a
    /// keyword.
    #[inline(always)]
    pub(crate) fn c_name(&mut self) {
        self.c_retag(TokenKind::Name);
    }

    /// The first token recorded since `mark` is a name here, even if it is
    /// spelled like a keyword.
    #[inline(always)]
    pub(crate) fn c_name_at(&mut self, mark: usize) {
        if let Some(t) = self.cst.as_mut().and_then(|cst| cst.first_token_mut(mark)) {
            t.kind = TokenKind::Name;
        }
    }

    /// Re-reads the first token recorded since `mark` the way HHVM does
    /// where a name is expected: only the keywords that can never be names
    /// stay keywords.
    #[inline(always)]
    pub(crate) fn c_name_unless_reserved(&mut self, mark: usize) {
        if self.cst.is_some() {
            self.c_relex_name(mark);
        }
    }

    #[cold]
    fn c_relex_name(&mut self, mark: usize) {
        let bytes = self.bytes;
        if let Some(t) = self.cst.as_mut().and_then(|cst| cst.first_token_mut(mark)) {
            let text = &bytes[t.start()..t.end()];
            // Only something spelled like a name can be one.
            if text.first().is_some_and(|&c| is_name_start(c)) && t.kind != TokenKind::XHPClassName
            {
                t.kind = TokenKind::from_string(text, true).unwrap_or(TokenKind::Name);
            }
        }
    }

    /// Takes the trailing trivia away from the token recorded last.
    #[inline(always)]
    pub(crate) fn c_drop_trailing(&mut self) {
        if let Some(cst) = &mut self.cst {
            cst.drop_trailing();
        }
    }

    /// Records `count` absent children in front of what has been recorded
    /// since `mark`.
    #[inline(always)]
    pub(crate) fn c_insert_missing_n(&mut self, mark: usize, count: usize) {
        if let Some(cst) = &mut self.cst {
            cst.insert_missing(mark, count);
        }
    }

    /// Forgets the entry recorded at `mark`.
    #[inline(always)]
    pub(crate) fn c_remove(&mut self, mark: usize) {
        if let Some(cst) = &mut self.cst {
            cst.remove(mark);
        }
    }

    /// Makes the entry recorded at `mark` follow everything recorded after
    /// it.
    #[inline(always)]
    pub(crate) fn c_move_last(&mut self, mark: usize) {
        if let Some(cst) = &mut self.cst {
            cst.move_to_top(mark);
        }
    }

    /// HHVM leaves a name bare in some places where it could be a type: if
    /// what has been recorded since `mark` is a name wrapped as a simple
    /// type, records just the name, read the way a name is read.
    #[inline(always)]
    pub(crate) fn c_bare_name(&mut self, mark: usize) {
        if let Some(cst) = &mut self.cst {
            cst.unwrap(SyntaxKind::SimpleTypeSpecifier);
            self.c_relex_name(mark);
        }
    }

    /// If the node recorded last is a simple type, records the token in it
    /// instead.
    #[inline(always)]
    pub(crate) fn c_bare_type(&mut self) {
        if let Some(cst) = &mut self.cst {
            cst.unwrap(SyntaxKind::SimpleTypeSpecifier);
        }
    }

    /// Gives the parameter modifiers recorded since `mark` their places
    /// among the children of the parameter.
    #[inline(always)]
    pub(crate) fn c_parameter_modifiers(&mut self, mark: usize) {
        if let Some(cst) = &mut self.cst {
            cst.parameter_modifiers(mark);
        }
    }

    /// HHVM's nodes for `vec<T>`, `darray<K, V>`, `classname<T>` and the
    /// like have the type arguments themselves as children: replaces the
    /// `TypeArguments` recorded last by its `<`, each argument followed by
    /// its separator, and its `>`. The arguments of a `dict` stay one list,
    /// and HHVM reads each the way it reads a return type, where `noreturn`
    /// is the bare keyword.
    #[inline(always)]
    pub(crate) fn c_inline_type_arguments(&mut self, dict: bool) {
        if let Some(cst) = &mut self.cst {
            let at = cst.mark();
            cst.splice_node();
            if dict {
                cst.bare_items(at, TokenKind::Noreturn);
            } else {
                cst.splice_list(at);
            }
        }
    }
}
