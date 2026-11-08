//! XHP expressions (`<div class="x">text {$expr}</div>`).
//!
//! XHP has its own lexical rules, so this module scans the source directly
//! and only hands control back to the token-based parser for embedded
//! `{...}` expressions.

use super::cst::Trailing;
use super::expr::{Shape, Term};
use super::{Parser, R};
use crate::ast::*;
use crate::cst::{SyntaxKind as K, TokenKind};
use crate::lexer::is_name_start;
use crate::span::Span;
use crate::token::T;

#[inline]
fn is_xhp_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

/// What HHVM's lexer reads inside a tag.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TagKind {
    EndOfFile,
    Name,
    LeftBrace,
    Equal,
    String,
    SlashGreaterThan,
    GreaterThan,
    LessThanSlash,
    /// `<` or `}`.
    Other,
    /// A character that has no business in a tag.
    Invalid,
}

#[derive(Clone, Copy)]
struct TagToken {
    kind: TagKind,
    start: usize,
    end: usize,
}

/// A run of adjacent text tokens in an element body.
struct TextRun {
    /// Start of the whitespace in front of the first token.
    lead: usize,
    first: usize,
    last_end: usize,
    /// End of the whitespace HHVM attaches after the last token.
    trailing_end: usize,
}

impl<'a> Parser<'a> {
    /// Parses an XHP expression; the current token is its `<`.
    pub(crate) fn parse_xhp_term(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        // A `<` no element name follows is not XHP: HHVM takes the token
        // for the expression that should have been there.
        let resume = self.lexer.pos;
        let name = self.tag_token(start as usize + 1);
        if name.kind != TagKind::Name {
            self.lexer.pos = resume;
            let term = self.token_term();
            self.report_missing("An expression is expected here.");
            return Ok(term);
        }
        let (e, end) = self.parse_xhp_element(start as usize, false)?;
        // What is missing after the element is missing after the token
        // read last, which is not always one of the element.
        let last = (self.prev_start, self.prev_end);
        self.resync(end);
        (self.prev_start, self.prev_end) = last;
        Ok(Term {
            e,
            start,
            end: e.span.end,
            shape: Shape::Other,
        })
    }

    /// Skips PHP trivia (recording comments) starting at `pos`.
    fn skip_trivia_at(&mut self, pos: usize) -> usize {
        self.lexer.pos = pos;
        self.lexer.skip_trivia();
        self.lexer.pos
    }

    /// Reads the token of a tag that follows `pos` and the trivia there.
    fn tag_token(&mut self, pos: usize) -> TagToken {
        let start = self.skip_trivia_at(pos);
        let b = self.bytes;
        let at = |i: usize| if i < b.len() { b[i] } else { 0 };
        let (kind, width) = match at(start) {
            _ if start >= b.len() => (TagKind::EndOfFile, 0),
            c if is_name_start(c) || (c == b':' && is_name_start(at(start + 1))) => {
                let end = self.lexer.xhp_element_name_end(start, false);
                (TagKind::Name, end - start)
            }
            b'{' => (TagKind::LeftBrace, 1),
            b'=' => (TagKind::Equal, 1),
            b'>' => (TagKind::GreaterThan, 1),
            b'/' if at(start + 1) == b'>' => (TagKind::SlashGreaterThan, 2),
            b'<' if at(start + 1) == b'/' => (TagKind::LessThanSlash, 2),
            b'<' | b'}' => (TagKind::Other, 1),
            b'"' => {
                let close = b[start + 1..].iter().position(|&c| c == b'"');
                (TagKind::String, close.map_or(b.len() - start, |i| i + 2))
            }
            _ => (TagKind::Invalid, 1),
        };
        TagToken {
            kind,
            start,
            end: start + width,
        }
    }

    /// Consumes a token of a tag that HHVM reads and then leaves out of
    /// its tree, reporting what its lexer has to say about the token.
    /// Returns where the tag goes on.
    fn drop_tag_token(&mut self, token: TagToken) -> usize {
        match token.kind {
            TagKind::Invalid => {
                let at = token.start as u32;
                self.report_syntax(at, at, "This character is invalid.");
            }
            TagKind::String if !self.is_closed_tag_string(token) => {
                self.report_syntax(
                    token.start as u32,
                    token.end as u32,
                    "This string literal is not terminated.",
                );
            }
            _ => {}
        }
        (self.prev_start, self.prev_end) = (token.start as u32, token.end as u32);
        let after = self.missing_span().1;
        if let Some(cst) = &mut self.cst {
            cst.full_end = after;
        }
        after as usize
    }

    fn is_closed_tag_string(&self, token: TagToken) -> bool {
        token.end - token.start >= 2 && self.bytes[token.end - 1] == b'"'
    }

    /// Parses `{ expr }` whose `{` is at `pos`, for the value of an
    /// attribute (or all the attributes of a `spread`) or for a child.
    /// Returns the expression and where its `}` ends, if it has one, in
    /// which case nothing past the `}` is tokenized. Without one the tag
    /// or the body goes on after the last token of the expression, which
    /// `prev_end` then is the end of.
    fn parse_xhp_braced(
        &mut self,
        pos: usize,
        spread: bool,
        in_body: bool,
    ) -> R<(Expr<'a>, Option<usize>)> {
        self.resync(pos);
        self.require(T::LBrace);
        if spread {
            self.require(T::DotDotDot);
        }
        let e = self.parse_expr()?;
        if !self.at(T::RBrace) && !in_body && self.peek_kind() == T::RBrace {
            // HHVM drops one token that stands in front of the `}`.
            self.skip_unexpected_token();
        }
        if !self.at(T::RBrace) {
            self.report_missing("A right brace `}` is expected here.");
            self.c_missing();
            return Ok((e, None));
        }
        Ok((e, Some(self.tok.end as usize)))
    }

    /// Parses the element whose `<` is at `lt`, which a name follows.
    /// Returns the expression, which ends where the last token of the
    /// element does, and the position at which what follows the element
    /// starts; `prev_start` and `prev_end` are left at the token read last.
    /// `in_body` tells whether it is a child of another element.
    ///
    /// For the lossless tree the tokens are recorded as they are scanned.
    /// What follows the element's first and last token is text of the
    /// enclosing element when there is one, and so is no trivia of theirs.
    ///
    /// An element that is not well formed is put up with the way HHVM
    /// does: see the places that report an error.
    fn parse_xhp_element(&mut self, lt: usize, in_body: bool) -> R<(Expr<'a>, usize)> {
        self.enter()?;
        let b = self.bytes;
        let at = |i: usize| if i < b.len() { b[i] } else { 0 };
        let outer = if in_body {
            Trailing::None
        } else {
            Trailing::Code
        };
        self.c_token_with(TokenKind::LessThan, lt as u32, lt as u32 + 1, outer);

        let name_token = self.tag_token(lt + 1);
        let name = Id {
            span: Span::new(name_token.start as u32, name_token.end as u32),
            name: &self.src[name_token.start..name_token.end],
        };
        self.c_token(TokenKind::XHPElementName, name.span.start, name.span.end);
        let mut p = name_token.end;
        // Where the last token of the element ends.
        let mut last_end = p;

        let mut attrs = self.vec();
        let attributes = self.c_mark();
        loop {
            let token = self.tag_token(p);
            if token.kind == TagKind::LeftBrace {
                let (e, close) = self.parse_xhp_braced(token.start, true, false)?;
                p = self.end_xhp_braced(close, Trailing::Code);
                last_end = self.prev_end as usize;
                self.c_node(K::XHPSpreadAttribute);
                attrs.push(XhpAttribute::Spread(e));
                continue;
            }
            if token.kind != TagKind::Name {
                break;
            }
            let attr_name = Id {
                span: Span::new(token.start as u32, token.end as u32),
                name: &self.src[token.start..token.end],
            };
            self.c_token(
                TokenKind::XHPElementName,
                attr_name.span.start,
                attr_name.span.end,
            );
            last_end = token.end;
            let equal = self.tag_token(token.end);
            if equal.kind != TagKind::Equal {
                // HHVM takes the attribute for one without a value and
                // reads past whatever stands there instead of the `=`.
                p = self.drop_tag_token(equal);
                self.c_missing_n(2);
                self.c_node(K::XHPSimpleAttribute);
                attrs.push(XhpAttribute::Simple(attr_name, self.missing_xhp_value(p)));
                continue;
            }
            self.c_token(TokenKind::Equal, equal.start as u32, equal.end as u32);
            last_end = equal.end;
            let value = self.tag_token(equal.end);
            match value.kind {
                TagKind::String if self.is_closed_tag_string(value) => {
                    let (start, close) = (value.start, value.end - 1);
                    // HHVM derives the value from the token *and* its
                    // trivia; refuse the odd cases where that matters.
                    if b[equal.end..start].contains(&b'/') {
                        return self.fail_at(start as u32, "comment before XHP attribute value");
                    }
                    let mut t = close + 1;
                    while matches!(at(t), b' ' | b'\t') {
                        t += 1;
                    }
                    if at(t) == b'/' && at(t + 1) == b'/' {
                        while t < b.len() && b[t] != b'\n' && b[t] != b'\r' {
                            if b[t] == b'"' {
                                return self
                                    .fail_at(start as u32, "quote in comment after XHP attribute");
                            }
                            t += 1;
                        }
                    }
                    let raw: &'a [u8] = &b[start + 1..close];
                    // HHVM's parser cannot cope with a malformed escape here.
                    let Ok(mut text) = self.unescape_double(raw, super::strings::Quote::Double)
                    else {
                        return self.fail_at(start as u32, "malformed string literal");
                    };
                    if text == b"''" || text == b"\"\"" {
                        text = &[];
                    }
                    self.c_token(TokenKind::XHPStringLiteral, start as u32, value.end as u32);
                    self.c_node(K::XHPSimpleAttribute);
                    attrs.push(XhpAttribute::Simple(
                        attr_name,
                        Expr {
                            span: Span::new(start as u32, value.end as u32),
                            kind: ExprKind::String(text),
                        },
                    ));
                    p = value.end;
                    last_end = p;
                }
                TagKind::LeftBrace => {
                    let (e, close) = self.parse_xhp_braced(value.start, false, false)?;
                    p = self.end_xhp_braced(close, Trailing::Code);
                    last_end = self.prev_end as usize;
                    self.c_node(K::BracedExpression);
                    self.c_node(K::XHPSimpleAttribute);
                    attrs.push(XhpAttribute::Simple(attr_name, e));
                }
                _ => {
                    // Whatever stands there instead of a value is read
                    // past as well.
                    p = self.drop_tag_token(value);
                    self.c_missing();
                    self.c_node(K::XHPSimpleAttribute);
                    attrs.push(XhpAttribute::Simple(attr_name, self.missing_xhp_value(p)));
                }
            }
        }

        self.c_list(attributes);
        let mut children = self.vec();
        let after;
        let token = self.tag_token(p);
        p = token.start;
        if token.kind == TagKind::SlashGreaterThan {
            after = token.end;
            last_end = after;
            (self.prev_start, self.prev_end) = (p as u32, after as u32);
            if self.c_on() {
                self.c_token_with(TokenKind::SlashGreaterThan, p as u32, after as u32, outer);
                self.c_node(K::XHPOpen);
                // Neither body nor closing tag.
                self.c_missing_n(2);
            }
        } else if token.kind == TagKind::GreaterThan {
            if self.c_on() {
                self.c_token_with(
                    TokenKind::GreaterThan,
                    p as u32,
                    p as u32 + 1,
                    Trailing::None,
                );
                self.c_node(K::XHPOpen);
            }
            p += 1;
            last_end = p;
            let mut run: Option<TextRun> = None;
            let body = self.c_mark();
            loop {
                let lead = p;
                while is_xhp_space(at(p)) {
                    p += 1;
                }
                if p >= b.len() {
                    break;
                }
                match b[p] {
                    b'{' => {
                        self.flush_xhp_text(&mut run, &mut children);
                        let (e, close) = self.parse_xhp_braced(p, false, true)?;
                        p = self.end_xhp_braced(close, Trailing::None);
                        last_end = self.prev_end as usize;
                        self.c_node(K::BracedExpression);
                        children.push(e);
                    }
                    b'<' if at(p + 1) == b'!' && at(p + 2) == b'-' && at(p + 3) == b'-' => {
                        self.flush_xhp_text(&mut run, &mut children);
                        let end = match b[p + 4..].windows(3).position(|w| w == b"-->") {
                            Some(i) => p + 4 + i + 3,
                            None => {
                                self.report_syntax(
                                    p as u32,
                                    b.len() as u32,
                                    "This XHP comment is not terminated.",
                                );
                                b.len()
                            }
                        };
                        self.c_token_with(
                            TokenKind::XHPComment,
                            p as u32,
                            end as u32,
                            Trailing::None,
                        );
                        p = end;
                        last_end = p;
                    }
                    b'<' if at(p + 1) == b'/' => break,
                    b'<' => {
                        self.flush_xhp_text(&mut run, &mut children);
                        if self.tag_token(p + 1).kind == TagKind::Name {
                            let (e, after) = self.parse_xhp_element(p, true)?;
                            last_end = e.span.end as usize;
                            children.push(e);
                            p = after;
                        } else {
                            // HHVM keeps the `<` as it is.
                            self.report_syntax(p as u32, p as u32 + 1, "A name is expected here.");
                            self.c_token_with(
                                TokenKind::LessThan,
                                p as u32,
                                p as u32 + 1,
                                Trailing::None,
                            );
                            p += 1;
                            last_end = p;
                        }
                    }
                    c => {
                        // A text token: a stray `}` or a run of non-space
                        // characters.
                        let tok_start = p;
                        let trailing_end;
                        if c == b'}' {
                            p += 1;
                            trailing_end = p;
                            self.c_token_with(
                                TokenKind::RightBrace,
                                tok_start as u32,
                                p as u32,
                                Trailing::None,
                            );
                        } else {
                            while p < b.len()
                                && !is_xhp_space(b[p])
                                && !matches!(b[p], b'{' | b'}' | b'<')
                            {
                                p += 1;
                            }
                            if p == b.len() {
                                self.report_syntax(
                                    tok_start as u32,
                                    p as u32,
                                    "This XHP body is not terminated.",
                                );
                            }
                            // Trailing trivia: blanks, then at most one
                            // line break.
                            let mut t = p;
                            while matches!(at(t), b' ' | b'\t') {
                                t += 1;
                            }
                            if at(t) == b'\r' {
                                t += 1;
                                if at(t) == b'\n' {
                                    t += 1;
                                }
                            } else if at(t) == b'\n' {
                                t += 1;
                            }
                            trailing_end = t;
                            self.c_token_with(
                                TokenKind::XHPBody,
                                tok_start as u32,
                                p as u32,
                                Trailing::Xhp,
                            );
                        }
                        last_end = p;
                        match &mut run {
                            Some(r) => {
                                r.last_end = p;
                                r.trailing_end = trailing_end;
                            }
                            None => {
                                run = Some(TextRun {
                                    lead,
                                    first: tok_start,
                                    last_end: p,
                                    trailing_end,
                                });
                            }
                        }
                        p = trailing_end;
                    }
                }
            }
            self.flush_xhp_text(&mut run, &mut children);
            self.c_list(body);
            // `</name>`, of which HHVM takes as much as there is.
            let mut close_name = "";
            let open = self.tag_token(p);
            (self.prev_start, self.prev_end) = (open.start as u32, open.end as u32);
            p = open.end;
            if open.kind == TagKind::LessThanSlash {
                self.c_token(TokenKind::LessThanSlash, open.start as u32, open.end as u32);
                last_end = p;
                let token = self.tag_token(p);
                if token.kind == TagKind::Name {
                    close_name = &self.src[token.start..token.end];
                    self.c_token(
                        TokenKind::XHPElementName,
                        token.start as u32,
                        token.end as u32,
                    );
                    (self.prev_start, self.prev_end) = (token.start as u32, token.end as u32);
                    p = token.end;
                    last_end = p;
                    let token = self.tag_token(p);
                    if token.kind == TagKind::GreaterThan {
                        (self.prev_start, self.prev_end) = (token.start as u32, token.end as u32);
                        p = token.end;
                        last_end = p;
                        self.c_token_with(
                            TokenKind::GreaterThan,
                            token.start as u32,
                            p as u32,
                            outer,
                        );
                    } else {
                        self.report_missing("A closing XHP tag is expected here.");
                        self.c_missing();
                    }
                } else {
                    self.report_missing("A closing XHP tag is expected here.");
                    self.c_missing_n(2);
                }
            } else {
                // The end of the file, which stands in for the `</`.
                self.c_token(TokenKind::EndOfFile, open.start as u32, open.end as u32);
                self.report_missing("A closing XHP tag is expected here.");
                self.c_missing_n(2);
            }
            self.c_node(K::XHPClose);
            if close_name != name.name {
                self.report(
                    lt as u32,
                    last_end as u32,
                    format!(
                        "XHP: mismatched tag: `{close_name}` not the same as `{}`",
                        name.name
                    ),
                );
            }
            after = p;
        } else {
            // HHVM reads past what stands there instead of the `>`, and
            // the element ends without one.
            after = self.drop_tag_token(token);
            self.report_missing("A closing angle bracket `>` is expected here.");
            self.c_missing();
            self.c_node(K::XHPOpen);
            self.c_missing_n(2);
        }

        self.c_node(K::XHPExpression);
        let xml = Xml {
            name,
            attrs: attrs.into_bump_slice(),
            children: children.into_bump_slice(),
        };
        self.leave();
        Ok((
            Expr {
                span: Span::new(lt as u32, last_end as u32),
                kind: ExprKind::Xml(self.alloc(xml)),
            },
            after,
        ))
    }

    /// Finishes `{ expr }` after [`parse_xhp_braced`](Self::parse_xhp_braced):
    /// records the `}`, if there is one, and returns where what follows the
    /// braces starts. `prev_end` is left at the end of the last token.
    fn end_xhp_braced(&mut self, close: Option<usize>, trailing: Trailing) -> usize {
        match close {
            Some(end) => {
                let end = end as u32;
                self.c_token_with(TokenKind::RightBrace, end - 1, end, trailing);
                (self.prev_start, self.prev_end) = (end - 1, end);
                end as usize
            }
            None => self.missing_span().1 as usize,
        }
    }

    /// The value of an attribute that has none, which HHVM's translation
    /// to the AST misses at `pos`.
    fn missing_xhp_value(&mut self, pos: usize) -> Expr<'a> {
        let pos = pos as u32;
        (self.prev_start, self.prev_end) = (pos, pos);
        self.missing_expression();
        Expr {
            span: Span::new(pos, pos),
            kind: ExprKind::Omitted,
        }
    }

    fn flush_xhp_text(
        &mut self,
        run: &mut Option<TextRun>,
        children: &mut super::List<'a, Expr<'a>>,
    ) {
        let Some(r) = run.take() else {
            return;
        };
        let raw: &'a [u8] = &self.bytes[r.lead..r.trailing_end];
        let is_ws = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c);
        // Whitespace runs collapse to one space; borrow when nothing changes.
        let mut needs_copy = false;
        let mut i = 0;
        while i < raw.len() {
            if is_ws(raw[i]) && (raw[i] != b' ' || (i + 1 < raw.len() && is_ws(raw[i + 1]))) {
                needs_copy = true;
                break;
            }
            i += 1;
        }
        let value: &'a [u8] = if needs_copy {
            let mut out = bumpalo::collections::Vec::with_capacity_in(raw.len(), self.arena);
            let mut in_ws = false;
            for &c in raw {
                if is_ws(c) {
                    if !in_ws {
                        out.push(b' ');
                        in_ws = true;
                    }
                } else {
                    out.push(c);
                    in_ws = false;
                }
            }
            out.into_bump_slice()
        } else {
            raw
        };
        children.push(Expr {
            span: Span::new(r.first as u32, r.last_end as u32),
            kind: ExprKind::String(value),
        });
    }
}
