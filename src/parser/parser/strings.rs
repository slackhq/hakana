//! String literals: escape processing and interpolation.
//!
//! Chunk boundaries and escape handling follow HHVM's lexer
//! (`scan_string_literal_in_progress`), parser (`parse_string_literal`) and
//! lowerer (`p_string2`, `mk_str`) so literal values and the positions of
//! interpolated pieces come out identical.

use super::cst::Trailing;
use super::{Parser, R};
use crate::ast::*;
use crate::cst::{SyntaxKind as K, TokenKind};
use crate::lexer::{is_name_char, is_name_start};
use crate::span::Span;
use crate::token::T;

/// What HHVM's translation to the AST says of `${expr}` in a string.
const DOLLAR_BRACES: &str = "The `${x}` syntax is disallowed in Hack. Use `{$x}` instead.";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quote {
    Double,
    Heredoc,
}

impl<'a> Parser<'a> {
    /// Where the text of a single-quoted string ends: in front of the
    /// closing quote, or with the file when there is none, which HHVM's
    /// lexer says where the string starts.
    #[inline]
    pub(crate) fn single_quoted_end(&mut self, tok: crate::lexer::Token) -> u32 {
        if tok.end as usize == self.bytes.len() {
            self.check_single_quoted_end(tok)
        } else {
            tok.end - 1
        }
    }

    #[cold]
    fn check_single_quoted_end(&mut self, tok: crate::lexer::Token) -> u32 {
        let text = &self.bytes[tok.start as usize + 1..tok.end as usize];
        let mut i = 0;
        while i < text.len() {
            match text[i] {
                b'\\' => i += 2,
                b'\'' => return tok.end - 1,
                _ => i += 1,
            }
        }
        self.report_syntax(
            tok.start,
            tok.start,
            "This string literal is not terminated.",
        );
        tok.end
    }

    /// Unescapes the body of a single-quoted string (`start..end` excludes
    /// the quotes).
    pub(crate) fn unescape_single(&mut self, start: u32, end: u32) -> R<&'a [u8]> {
        let raw = &self.bytes[start as usize..end as usize];
        if !raw.contains(&b'\\') {
            return Ok(raw);
        }
        let mut out = bumpalo::collections::Vec::with_capacity_in(raw.len(), self.arena);
        let mut i = 0;
        while i < raw.len() {
            let c = raw[i];
            if c != b'\\' {
                out.push(c);
            } else {
                i += 1;
                if i >= raw.len() {
                    return self.fail_at(start, "malformed string literal");
                }
                let c = raw[i];
                if c != b'\'' && c != b'\\' {
                    out.push(b'\\');
                }
                out.push(c);
            }
            i += 1;
        }
        Ok(out.into_bump_slice())
    }

    /// Unescapes a chunk of a double-quoted string or heredoc. A `\u{..}`
    /// escape can be malformed: the error says how, in HHVM's words.
    pub(crate) fn unescape_double(
        &self,
        raw: &'a [u8],
        quote: Quote,
    ) -> Result<&'a [u8], &'static str> {
        if !raw.contains(&b'\\') {
            return Ok(raw);
        }
        let mut out = bumpalo::collections::Vec::with_capacity_in(raw.len(), self.arena);
        let mut i = 0;
        let len = raw.len();
        while i < len {
            let c = raw[i];
            i += 1;
            if c != b'\\' || i >= len {
                out.push(c);
                continue;
            }
            let c = raw[i];
            i += 1;
            match c {
                b'\'' => out.extend_from_slice(b"\\'"),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'v' => out.push(0x0b),
                b'e' => out.push(0x1b),
                b'f' => out.push(0x0c),
                b'\\' => out.push(b'\\'),
                b'$' => out.push(b'$'),
                b'"' => {
                    if quote == Quote::Double {
                        out.push(b'"');
                    } else {
                        out.extend_from_slice(b"\\\"");
                    }
                }
                b'u' if i < len && raw[i] == b'{' => {
                    i += 1;
                    let digits_start = i;
                    let limit = (i + 6).min(len);
                    while i < limit && raw[i] != b'}' {
                        i += 1;
                    }
                    let digits = &raw[digits_start..i];
                    let n = std::str::from_utf8(digits)
                        .ok()
                        .and_then(|d| u32::from_str_radix(d, 16).ok());
                    let Some(n) = n else {
                        return Err("invalid numeric escape");
                    };
                    if n <= 0x7f {
                        out.push(n as u8);
                    } else if n <= 0x7ff {
                        out.push(0xc0 | (n >> 6) as u8);
                        out.push(0x80 | (n & 0x3f) as u8);
                    } else if n <= 0xffff {
                        out.push(0xe0 | (n >> 12) as u8);
                        out.push(0x80 | ((n >> 6) & 0x3f) as u8);
                        out.push(0x80 | (n & 0x3f) as u8);
                    } else if n <= 0x10ffff {
                        out.push(0xf0 | (n >> 18) as u8);
                        out.push(0x80 | ((n >> 12) & 0x3f) as u8);
                        out.push(0x80 | ((n >> 6) & 0x3f) as u8);
                        out.push(0x80 | (n & 0x3f) as u8);
                    } else {
                        return Err("UTF-8 codepoint too large");
                    }
                    if i >= len {
                        return Err("string ended early");
                    }
                    if raw[i] != b'}' {
                        return Err("Invalid UTF-8 escape sequence");
                    }
                    i += 1;
                }
                b'x' | b'X' => {
                    let digits_start = i;
                    let limit = (i + 2).min(len);
                    while i < limit && raw[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    if i == digits_start {
                        out.push(b'\\');
                        out.push(c);
                    } else {
                        let digits = std::str::from_utf8(&raw[digits_start..i]).unwrap();
                        out.push(u8::from_str_radix(digits, 16).unwrap());
                    }
                }
                b'0'..=b'7' => {
                    i -= 1;
                    let digits_start = i;
                    let limit = (i + 3).min(len);
                    while i < limit && matches!(raw[i], b'0'..=b'7') {
                        i += 1;
                    }
                    let digits = std::str::from_utf8(&raw[digits_start..i]).unwrap();
                    out.push(u32::from_str_radix(digits, 8).unwrap() as u8);
                }
                c => {
                    out.push(b'\\');
                    out.push(c);
                }
            }
        }
        Ok(out.into_bump_slice())
    }

    /// Advances past the escape sequence at `pos` (which holds a backslash)
    /// the way HHVM's in-string lexer does. This only decides which bytes are
    /// shielded from being read as `$`, `{` or `"`.
    fn skip_escape(&self, pos: usize) -> usize {
        let at = |i: usize| self.lexer.at(i);
        match at(pos + 1) {
            b'\\'
            | b'"'
            | b'$'
            | b'e'
            | b'f'
            | b'n'
            | b'r'
            | b't'
            | b'v'
            | b'`'
            | b'0'..=b'7'
            | b'{' => pos + 2,
            b'x' => {
                if !at(pos + 2).is_ascii_hexdigit() {
                    pos + 2
                } else if !at(pos + 3).is_ascii_hexdigit() {
                    pos + 3
                } else {
                    pos + 4
                }
            }
            b'u' => {
                if at(pos + 2) == b'{' {
                    if at(pos + 3) == b'$' {
                        pos + 2
                    } else {
                        let mut i = pos + 3;
                        while at(i).is_ascii_hexdigit() {
                            i += 1;
                        }
                        if at(i) == b'}' { i + 1 } else { i }
                    }
                } else {
                    pos + 2
                }
            }
            _ => pos + 1,
        }
    }

    /// Parses a double-quoted string; the current token is its opening quote.
    pub(crate) fn parse_double_quoted(&mut self) -> R<Expr<'a>> {
        debug_assert!(self.at(T::DoubleQuoted));
        let start = self.tok.start as usize;
        self.parse_interpolated(start, start + 1, Quote::Double, b"")
    }

    /// Parses a heredoc or nowdoc; the current token is the `<<<`.
    pub(crate) fn parse_heredoc(&mut self) -> R<Expr<'a>> {
        let start = self.tok.start as usize;
        let (name, quote, body) = self.scan_docstring_header(start, true);
        if quote != b'\'' {
            return self.parse_interpolated(start, body, Quote::Heredoc, name);
        }
        let end = self.scan_nowdoc_remainder(start, body, name, true);
        // The value is what lies between the first and the last newline.
        let token = &self.bytes[start..end];
        let first_nl = token.iter().position(|&c| c == b'\n');
        let last_nl = token.iter().rposition(|&c| c == b'\n');
        let value: &'a [u8] = match (first_nl, last_nl) {
            (Some(f), Some(l)) if f < l => &token[f + 1..l],
            _ => &[],
        };
        self.c_token(TokenKind::NowdocStringLiteral, start as u32, end as u32);
        self.c_node(K::LiteralExpression);
        self.resync(end);
        Ok(Expr {
            span: Span::new(start as u32, end as u32),
            kind: ExprKind::String(value),
        })
    }

    /// Reads the first line of the heredoc or nowdoc whose `<<<` is at
    /// `start`, the way HHVM's lexer does (`scan_docstring_header`): what
    /// is wrong with it is said about the text from the `<<<` up to where
    /// the lexer has got, which then carries on. Returns the name (empty
    /// if there is none), the quote in front of it (or whatever stands
    /// there) and where the text of the string starts. Without `report`
    /// nothing is said: HHVM's parser is only looking ahead.
    fn scan_docstring_header(&mut self, start: usize, report: bool) -> (&'a [u8], u8, usize) {
        let b = self.bytes;
        let at = |i: usize| if i < b.len() { b[i] } else { 0 };
        let newline = |c: u8| c == b'\n' || c == b'\r';
        let mut i = start + 3;
        while at(i) == b' ' || at(i) == b'\t' {
            i += 1;
        }
        let quote = at(i);
        let quoted = quote == b'\'' || quote == b'"';
        if quoted {
            i += 1;
        }
        let name_start = i;
        if is_name_start(at(i)) {
            i += 1;
            while is_name_char(at(i)) {
                i += 1;
            }
        } else if report {
            self.report_lexical(start as u32, i as u32, "A name is expected here.");
        }
        let name = &b[name_start..i];
        if quoted {
            if at(i) == quote {
                i += 1;
            } else if report {
                let message = if quote == b'"' {
                    "A double quote is expected here."
                } else {
                    "A single quote is expected here."
                };
                self.report_lexical(start as u32, i as u32, message);
            }
        }
        // The header must be followed directly by a newline; whatever
        // stands there instead is part of it.
        if report && !newline(at(i)) {
            self.report_lexical(start as u32, i as u32, "A newline is expected here.");
        }
        while i < b.len() && !newline(b[i]) {
            i += 1;
        }
        if at(i) == b'\r' && at(i + 1) == b'\n' {
            i += 2;
        } else if i < b.len() {
            i += 1;
        }
        (name, quote, i)
    }

    /// Finds the end of the nowdoc whose `<<<` is at `start` and whose text
    /// starts at `i`, the way HHVM's lexer does
    /// (`scan_docstring_remainder`): line by line until one starts with the
    /// terminator.
    fn scan_nowdoc_remainder(
        &mut self,
        start: usize,
        mut i: usize,
        name: &[u8],
        report: bool,
    ) -> usize {
        let b = self.bytes;
        let at = |i: usize| if i < b.len() { b[i] } else { 0 };
        let newline = |c: u8| c == b'\n' || c == b'\r';
        loop {
            let c0 = at(i + name.len());
            let c1 = at(i + name.len() + 1);
            if (newline(c0) || (c0 == b';' && newline(c1))) && b[i..].starts_with(name) {
                return i + name.len();
            }
            while i < b.len() && !newline(b[i]) {
                i += 1;
            }
            if i >= b.len() {
                // The file ends first.
                if report {
                    self.report_lexical(start as u32, i as u32, "A newline is expected here.");
                }
                return i;
            }
            if b[i] == b'\r' && at(i + 1) == b'\n' {
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    /// Makes the current token, the start of a string, the token HHVM's
    /// lexer reads there when its parser is not after a string: the whole
    /// of a heredoc or nowdoc (found the way a nowdoc's end is), and of a
    /// double-quoted string all there is up to the first thing that could
    /// be part of an interpolation. With `consumed` the token is about to
    /// be taken, which is when HHVM reports what its lexer finds wrong
    /// with it.
    #[cold]
    pub(crate) fn whole_string_token(&mut self, consumed: bool) {
        let start = self.tok.start as usize;
        let b = self.bytes;
        let end = if self.at(T::DoubleQuoted) {
            let mut i = start + 1;
            loop {
                match b.get(i) {
                    None => {
                        if consumed {
                            self.report_lexical(
                                start as u32,
                                i as u32,
                                "This string literal is not terminated.",
                            );
                        }
                        break i;
                    }
                    Some(b'"') => break i + 1,
                    Some(0) => {
                        if consumed {
                            self.report_lexical(
                                start as u32,
                                i as u32,
                                "This character is invalid.",
                            );
                        }
                        i += 1;
                    }
                    Some(&c)
                        if matches!(c, b'\\' | b'$' | b'{' | b'[' | b']' | b'-')
                            || c.is_ascii_digit()
                            || is_name_start(c) =>
                    {
                        break i;
                    }
                    Some(_) => i += 1,
                }
            }
        } else {
            let (name, _, body) = self.scan_docstring_header(start, consumed);
            self.scan_nowdoc_remainder(start, body, name, consumed)
        };
        // Reporting an error may have ended the parse (see
        // `report_syntax`).
        if self.tok.start as usize == start {
            let kind = self.tok.kind;
            self.retokenize(kind, end);
        }
    }

    /// Whether the heredoc terminator `name` starts at `pos`: it must follow
    /// a newline and be followed by a newline or `;` + newline.
    fn is_heredoc_end(&self, pos: usize, name: &[u8]) -> bool {
        let b = self.bytes;
        if pos == 0 || !matches!(b[pos - 1], b'\n' | b'\r') {
            return false;
        }
        if !b[pos..].starts_with(name) {
            return false;
        }
        let c0 = self.lexer.at(pos + name.len());
        let c1 = self.lexer.at(pos + name.len() + 1);
        let nl = |c: u8| c == b'\n' || c == b'\r';
        nl(c0) || (c0 == b';' && nl(c1))
    }

    /// Scans the body of an interpolated string. `token_start` is where the
    /// literal starts (opening quote or `<<<`), `pos` the first body byte.
    ///
    /// HHVM's lexer reads the text of such a string as tokens of its own
    /// (names, numbers, brackets, ...) and its parser merges those that
    /// turn out to be just text. The scan here goes byte by byte instead,
    /// and stops at the same places.
    fn parse_interpolated(
        &mut self,
        token_start: usize,
        mut pos: usize,
        quote: Quote,
        heredoc_name: &[u8],
    ) -> R<Expr<'a>> {
        let b = self.bytes;
        let len = b.len();
        let mut parts = self.vec::<Expr<'a>>();
        // Start of the literal chunk being accumulated.
        let mut chunk_start = Some(token_start);
        let mut is_first_chunk = true;
        let end;
        // The file ends before the string does.
        let mut unterminated = false;
        // Where the first chunk ends, and what HHVM's translation to the
        // AST had reported when it got to the string (see
        // `malformed_string`).
        let mut head_end = token_start;
        let mut before = (0, false, false);
        // The lossless tree has a token for each stretch of literal text;
        // this is where the one being scanned starts, and whether it is the
        // first.
        let c_parts = self.c_mark();
        let mut c_text = token_start;
        let mut c_first = true;

        // Emits the literal chunk accumulated so far (if any), ending at
        // `$chunk_end`.
        macro_rules! flush_chunk {
            ($chunk_end:expr, $is_last:expr) => {{
                if let Some(cs) = chunk_start.take() {
                    self.push_chunk(
                        &mut parts,
                        cs,
                        $chunk_end,
                        quote,
                        is_first_chunk,
                        $is_last,
                        (token_start, head_end),
                    );
                }
            }};
        }

        // The text so far ends at `pos`, where something embedded starts.
        macro_rules! end_text {
            () => {{
                if is_first_chunk {
                    head_end = pos;
                    before = (
                        self.lowering_list.len(),
                        self.lowering_errors,
                        self.unit_aborted,
                    );
                }
                flush_chunk!(pos, false);
                is_first_chunk = false;
                self.c_string_text(quote, c_text, pos, c_first, false);
            }};
        }

        loop {
            if pos >= len {
                // The string is all that is left of the file. HHVM's lexer
                // says so where it gives up: at the end, having read the
                // string piece by piece, or about the whole of a string
                // without anything in it that makes a piece.
                let start = if quote == Quote::Heredoc || !self.in_string_head(token_start, len) {
                    len
                } else {
                    token_start
                };
                self.report_lexical(
                    start as u32,
                    len as u32,
                    "This string literal is not terminated.",
                );
                if chunk_start.is_none() {
                    chunk_start = Some(pos);
                }
                end = len;
                unterminated = true;
                break;
            }
            let c = b[pos];
            if is_name_start(c) {
                if quote == Quote::Heredoc && self.is_heredoc_end(pos, heredoc_name) {
                    if chunk_start.is_none() {
                        chunk_start = Some(pos);
                    }
                    end = pos + heredoc_name.len();
                    break;
                }
                if chunk_start.is_none() {
                    chunk_start = Some(pos);
                }
                pos += 1;
                while pos < len && is_name_char(b[pos]) {
                    pos += 1;
                }
                continue;
            }
            match c {
                b'"' if quote == Quote::Double => {
                    if chunk_start.is_none() {
                        chunk_start = Some(pos);
                    }
                    end = pos + 1;
                    break;
                }
                b'$' => {
                    let next = self.lexer.at(pos + 1);
                    if is_name_start(next) {
                        end_text!();
                        pos = self.parse_embedded_variable(&mut parts, pos)?;
                        (c_text, c_first) = (pos, false);
                    } else if next == b'{' {
                        end_text!();
                        self.c_string_token(TokenKind::Dollar, pos, pos + 1);
                        pos = self.parse_braced_in_string(&mut parts, pos + 1, false)?;
                        (c_text, c_first) = (pos, false);
                    } else {
                        if chunk_start.is_none() {
                            chunk_start = Some(pos);
                        }
                        pos += 1;
                    }
                }
                b'{' => {
                    if self.lexer.at(pos + 1) == b'$' {
                        end_text!();
                        pos = self.parse_braced_in_string(&mut parts, pos, true)?;
                        (c_text, c_first) = (pos, false);
                    } else {
                        if chunk_start.is_none() {
                            chunk_start = Some(pos);
                        }
                        pos += 1;
                    }
                }
                b'\\' => {
                    if chunk_start.is_none() {
                        chunk_start = Some(pos);
                    }
                    pos = self.skip_escape(pos);
                }
                0 => {
                    // HHVM's lexer says so and reads on: about the string
                    // so far while it has met nothing but plain text, and
                    // at the character after that.
                    let start = if quote == Quote::Double
                        && is_first_chunk
                        && self.in_string_head(token_start, pos)
                    {
                        token_start
                    } else {
                        pos
                    };
                    self.report_lexical(start as u32, pos as u32, "This character is invalid.");
                    if chunk_start.is_none() {
                        chunk_start = Some(pos);
                    }
                    pos += 1;
                }
                _ => {
                    if chunk_start.is_none() {
                        chunk_start = Some(pos);
                    }
                    pos += 1;
                }
            }
        }

        if self.c_on() {
            self.c_string_text(quote, c_text, end, c_first, true);
            if !c_first {
                self.c_list(c_parts);
            }
            self.c_node(K::LiteralExpression);
        }
        let whole = Span::new(token_start as u32, end as u32);
        if is_first_chunk {
            // No interpolation at all: a plain literal.
            let raw: &'a [u8] = match quote {
                // A string that is not closed may be no more than its
                // opening quote.
                Quote::Double if unterminated => match &b[token_start + 1..end] {
                    [text @ .., b'"'] => text,
                    text => text,
                },
                Quote::Double => &b[token_start + 1..end - 1],
                Quote::Heredoc => {
                    let token = &b[token_start..end];
                    let first_nl = token.iter().position(|&c| c == b'\n');
                    let last_nl = token.iter().rposition(|&c| c == b'\n');
                    match (first_nl, last_nl) {
                        (Some(f), Some(l)) if f < l => &token[f + 1..l],
                        _ => &[],
                    }
                }
            };
            let value = match self.unescape_double(raw, quote) {
                Ok(value) => value,
                Err(_) => {
                    // HHVM says so and takes the string to be empty.
                    self.report_lowering(
                        whole.start,
                        whole.end,
                        format!(
                            "Malformed string literal <<{}>>",
                            String::from_utf8_lossy(raw)
                        ),
                    );
                    &[]
                }
            };
            self.resync(end);
            return Ok(Expr {
                span: whole,
                kind: ExprKind::String(value),
            });
        }
        // The last piece of a string the file ends in may not be what the
        // end of a string looks like: HHVM's translation to the AST says
        // so first thing, and takes the piece as it is.
        let malformed = unterminated && {
            let last = &b[chunk_start.unwrap_or(end)..end];
            match quote {
                Quote::Double => !matches!(last, [.., b'"' | b'`']),
                Quote::Heredoc => last.is_empty(),
            }
        };
        if malformed {
            self.malformed_string(token_start as u32, head_end as u32, before);
        }
        flush_chunk!(end, !malformed);
        self.resync(end);
        Ok(Expr {
            span: whole,
            kind: ExprKind::String2(parts.into_bump_slice()),
        })
    }

    /// Whether HHVM's lexer, reading the double-quoted string that starts
    /// at `token_start`, is still on its first token at `pos`: it is as
    /// long as it has met nothing that could be part of an interpolation.
    #[cold]
    fn in_string_head(&self, token_start: usize, pos: usize) -> bool {
        !self.bytes[token_start + 1..pos].iter().any(|&c| {
            matches!(c, b'\\' | b'$' | b'{' | b'[' | b']' | b'-')
                || c.is_ascii_digit()
                || is_name_start(c)
        })
    }

    /// What HHVM's translation to the AST says of an interpolated string
    /// whose last piece is not the end of one (the file ends first), about
    /// its first piece (`start..end`). It says so before it looks at any of
    /// the pieces: `before` is what it had reported until then (how many
    /// errors, whether any, and whether it had given up on the
    /// declaration).
    #[cold]
    fn malformed_string(&mut self, start: u32, end: u32, before: (usize, bool, bool)) {
        let (at, lowering_errors, unit_aborted) = before;
        if unit_aborted {
            return;
        }
        // Having said this, it would not have said what it only says when
        // it has reported nothing yet.
        if !lowering_errors
            && self.first_only_at != usize::MAX
            && self.first_only_at >= at
            && self.first_only_at < self.lowering_list.len()
        {
            self.lowering_list.remove(self.first_only_at);
        }
        self.first_only_at = usize::MAX;
        self.lowering_list.insert(
            at.min(self.lowering_list.len()),
            crate::SyntaxError {
                start,
                end,
                message: "Malformed String2 SyntaxList".into(),
            },
        );
        self.lowering_errors = true;
    }

    /// Emits one literal chunk of an interpolated string. `head` is the
    /// extent of the string's first chunk, which is where HHVM puts what it
    /// has to say about the shape of the last.
    #[allow(clippy::too_many_arguments)]
    fn push_chunk(
        &mut self,
        parts: &mut super::List<'a, Expr<'a>>,
        mut start: usize,
        mut end: usize,
        quote: Quote,
        is_first: bool,
        is_last: bool,
        head: (usize, usize),
    ) {
        let b = self.bytes;
        // HHVM unescapes the head and tail tokens of a heredoc with the
        // heredoc rules and anything in between with double-quote rules.
        let mut rules = Quote::Double;
        match quote {
            Quote::Double => {
                if is_first {
                    // Drop the opening quote.
                    if end - start <= 1 {
                        return;
                    }
                    start += 1;
                }
                if is_last {
                    // Drop the closing quote.
                    if end - start <= 1 {
                        return;
                    }
                    end -= 1;
                }
            }
            Quote::Heredoc => {
                // A heredoc without the line breaks that set its text off
                // makes HHVM give up on the declaration.
                let no_newline = |p: &mut Self| {
                    p.abort_lowering(head.0 as u32, head.1 as u32, "newline not found");
                };
                if is_first {
                    rules = Quote::Heredoc;
                    let Some(nl) = b[start..end].iter().position(|&c| c == b'\n') else {
                        return no_newline(self);
                    };
                    if nl + 1 == end - start {
                        return;
                    }
                    start += nl + 1;
                }
                if is_last {
                    rules = Quote::Heredoc;
                    let chunk = &b[start..end];
                    if chunk.len() < 2 {
                        return no_newline(self);
                    }
                    let Some(nl) = chunk[..chunk.len() - 1].iter().rposition(|&c| c == b'\n')
                    else {
                        return no_newline(self);
                    };
                    if nl == 0 {
                        return;
                    }
                    end = start + nl;
                }
            }
        }
        let raw: &'a [u8] = &b[start..end];
        let mut value = match self.unescape_double(raw, rules) {
            Ok(value) => value,
            Err(message) => {
                // HHVM gives up on the declaration.
                self.abort_lowering(start as u32, end as u32, message);
                &[]
            }
        };
        if rules == Quote::Double && (value == b"''" || value == b"\"\"") {
            value = &[];
        }
        parts.push(Expr {
            span: Span::new(start as u32, end as u32),
            kind: ExprKind::String(value),
        });
    }

    /// `$x`, `$x[key]` or `$x->prop` inside a string. Returns the position
    /// after the embedded expression.
    fn parse_embedded_variable(
        &mut self,
        parts: &mut super::List<'a, Expr<'a>>,
        pos: usize,
    ) -> R<usize> {
        let b = self.bytes;
        let len = b.len();
        let at = |i: usize| if i < len { b[i] } else { 0 };
        let mut end = pos + 2;
        while is_name_char(at(end)) {
            end += 1;
        }
        self.note_variable_at(pos, end);
        if &b[pos..end] == crate::lexer::RETIRED_VARIABLE {
            self.lexer.retired_variables.push(pos as u32);
        }
        self.c_string_token(TokenKind::Variable, pos, end);
        self.c_node(K::VariableExpression);
        let var = Expr {
            span: Span::new(pos as u32, end as u32),
            kind: ExprKind::Lvar(&self.src[pos..end]),
        };
        if at(end) == b'-' && at(end + 1) == b'>' && is_name_start(at(end + 2)) {
            let name_start = end + 2;
            let mut name_end = name_start + 1;
            while is_name_char(at(name_end)) {
                name_end += 1;
            }
            self.c_string_token(TokenKind::MinusGreaterThan, end, name_start);
            self.c_string_token(TokenKind::Name, name_start, name_end);
            self.c_node(K::EmbeddedMemberSelectionExpression);
            let member = Expr {
                span: Span::new(name_start as u32, name_end as u32),
                kind: ExprKind::Id(&self.src[name_start..name_end]),
            };
            let node = ObjGet {
                object: var,
                member,
                nullsafe: false,
                is_method: false,
            };
            parts.push(Expr {
                span: Span::new(pos as u32, name_end as u32),
                kind: ExprKind::ObjGet(self.alloc(node)),
            });
            return Ok(name_end);
        }
        if at(end) == b'[' {
            let idx_start = end + 1;
            let c = at(idx_start);
            // What HHVM's lexer makes of what follows the `[`: a name, a
            // variable or a number are an offset if a `]` follows.
            let offset = if is_name_start(c) {
                let mut e = idx_start + 1;
                while is_name_char(at(e)) {
                    e += 1;
                }
                let raw: &'a [u8] = &b[idx_start..e];
                Some((ExprKind::String(raw), e))
            } else if c == b'$' && is_name_start(at(idx_start + 1)) {
                let mut e = idx_start + 2;
                while is_name_char(at(e)) {
                    e += 1;
                }
                self.note_variable_at(idx_start, e);
                Some((ExprKind::Lvar(&self.src[idx_start..e]), e))
            } else if c.is_ascii_digit() {
                let mut e = idx_start;
                let mut digits = true;
                if c == b'0' && matches!(at(e + 1), b'x' | b'X') {
                    e += 2;
                    digits = at(e).is_ascii_hexdigit();
                    while at(e).is_ascii_hexdigit() {
                        e += 1;
                    }
                } else if c == b'0' && matches!(at(e + 1), b'b' | b'B') {
                    e += 2;
                    digits = matches!(at(e), b'0' | b'1');
                    while matches!(at(e), b'0' | b'1') {
                        e += 1;
                    }
                } else {
                    while at(e).is_ascii_digit() {
                        e += 1;
                    }
                }
                // A prefix without digits is no number.
                digits.then(|| (ExprKind::Int(&self.src[idx_start..e]), e))
            } else {
                None
            };
            let Some((index, idx_end)) = offset.filter(|&(_, e)| at(e) == b']') else {
                // HHVM takes the `[` and reports the offset and the `]` as
                // missing; what is there is text.
                self.c_string_token(TokenKind::LeftBracket, end, idx_start);
                self.c_missing_n(2);
                self.c_node(K::EmbeddedSubscriptExpression);
                self.report_syntax(
                    end as u32,
                    idx_start as u32,
                    "A simple offset expression is expected here",
                );
                let node = ArrayGet {
                    base: var,
                    index: None,
                };
                parts.push(Expr {
                    span: Span::new(pos as u32, idx_start as u32),
                    kind: ExprKind::ArrayGet(self.alloc(node)),
                });
                return Ok(idx_start);
            };
            if let ExprKind::Int(digits) = index {
                let digits = digits.as_bytes();
                if digits[0] == b'0'
                    && !matches!(digits.get(1), Some(b'x' | b'X' | b'b' | b'B'))
                    && digits.iter().any(|d| matches!(d, b'8' | b'9'))
                {
                    // Read as octal, which it is not.
                    let (start, end) = (idx_start as u32, idx_end as u32);
                    self.report_lowering(start, end, "Invalid digit for octal integers");
                    let text = &self.src[idx_start..idx_end];
                    self.abort_lowering(
                        start,
                        end,
                        format!("Encountered unexpected text `{text}`, was expecting a octal."),
                    );
                }
            }
            if self.c_on() {
                self.c_string_token(TokenKind::LeftBracket, end, idx_start);
                match index {
                    ExprKind::Lvar(_) => {
                        self.c_string_token(TokenKind::Variable, idx_start, idx_end);
                        self.c_node(K::VariableExpression);
                    }
                    ExprKind::Int(digits) => {
                        let kind = match digits.as_bytes() {
                            [b'0', b'x' | b'X', ..] => TokenKind::HexadecimalLiteral,
                            [b'0', b'b' | b'B', ..] => TokenKind::BinaryLiteral,
                            _ => TokenKind::DecimalLiteral,
                        };
                        self.c_string_token(kind, idx_start, idx_end);
                        self.c_node(K::LiteralExpression);
                    }
                    _ => self.c_string_token(TokenKind::Name, idx_start, idx_end),
                }
                self.c_string_token(TokenKind::RightBracket, idx_end, idx_end + 1);
                self.c_node(K::EmbeddedSubscriptExpression);
            }
            let node = ArrayGet {
                base: var,
                index: Some(Expr {
                    span: Span::new(idx_start as u32, idx_end as u32),
                    kind: index,
                }),
            };
            parts.push(Expr {
                span: Span::new(pos as u32, (idx_end + 1) as u32),
                kind: ExprKind::ArrayGet(self.alloc(node)),
            });
            return Ok(idx_end + 1);
        }
        parts.push(var);
        Ok(end)
    }

    /// An expression in braces inside a string: `{$expr}` (`dollar_inside`)
    /// or what follows the `$` of `${expr}`. `brace` is where the `{` is.
    /// This is HHVM's `parse_braced_expression_in_string`, special cases
    /// and recovery included: a `}` that is not there is reported, and what
    /// stands in its place is text of the string again. Returns where that
    /// text starts.
    fn parse_braced_in_string(
        &mut self,
        parts: &mut super::List<'a, Expr<'a>>,
        brace: usize,
        dollar_inside: bool,
    ) -> R<usize> {
        let after = brace + 1;
        self.c_string_token(TokenKind::LeftBrace, brace, after);
        // HHVM's translation to the AST says that `${expr}` is not Hack
        // before it gets to the expression.
        let not_hack = if dollar_inside {
            None
        } else {
            self.begin_lowering_error(brace as u32, DOLLAR_BRACES)
        };
        self.resync(after);
        // The lexer has read the `{`, and nothing after it.
        (self.prev_start, self.prev_end) = (brace as u32, after as u32);
        self.no_trailing = after as u32;
        let first = self.tok;
        let next = if first.kind.is_name_like() {
            self.peek()
        } else {
            first
        };
        // Where the last token that is part of the tree ends, when that is
        // not the last one read.
        let mut tree_end = None;
        // The extent of the expression itself, where the checks that run
        // later are to see it as written rather than as HHVM's AST has it
        // (with the extent of the braces).
        let mut own_span = None;
        let kind = if first.kind.is_name_like() && next.kind == T::RBrace {
            // `${name}`: the name stands for the variable.
            self.bump();
            self.c_name();
            ExprKind::String(self.tok_text(first).as_bytes())
        } else if first.kind.is_name_like()
            && next.kind == T::LBracket
            && !dollar_inside
            && first.start as usize == after
            && next.start == first.end
        {
            // `${name[expr]}`: an element of the variable.
            self.bump();
            self.c_name();
            self.bump();
            self.c_drop_trailing();
            self.no_trailing = self.prev_end;
            let index = self.parse_expr_term()?.e;
            self.require(T::RBracket);
            self.c_node(K::SubscriptExpression);
            let index = (!self.absent(&index)).then_some(index);
            ExprKind::ArrayGet(self.alloc(ArrayGet {
                base: Expr {
                    span: Self::tok_span(first),
                    kind: ExprKind::Id(self.tok_text(first)),
                },
                index,
            }))
        } else if first.kind.is_name_like() && super::expr::is_assignment_op(next.kind) {
            // `${name = ...}`: HHVM takes the name, reads the operator
            // without doing anything with it, and wants the `}` there.
            self.bump();
            self.c_name();
            tree_end = Some(self.prev_end);
            let operator = self.tok.end;
            self.advance();
            if let Some(cst) = &mut self.cst {
                cst.full_end = operator;
            }
            self.no_trailing = operator;
            ExprKind::String(self.tok_text(first).as_bytes())
        } else {
            let term = self.parse_expr_term()?;
            // Only a handful of expressions may follow `{$`.
            let allowed = match term.e.kind {
                ExprKind::Call(_) | ExprKind::ArrayGet(_) | ExprKind::ObjGet(_) => true,
                ExprKind::Lvar(name) => name != "$$",
                ExprKind::Dollar(_) => true,
                _ => false,
            };
            if !dollar_inside {
                own_span = Some(term.e.span);
            } else if !(allowed && term.start == term.e.span.start) {
                own_span = Some(term.e.span);
                let end = self.missing_span().0;
                self.report_syntax(
                    brace as u32,
                    end,
                    "The only legal expressions inside a `{$...}`-expression embedded in a string are variables, function calls, subscript expressions, and member access expressions",
                );
            }
            term.e.kind
        };
        let end = if self.at(T::RBrace) {
            let end = self.tok.end as usize;
            self.c_string_token(TokenKind::RightBrace, self.tok.start as usize, end);
            self.end_lowering_error(not_hack, end as u32);
            end
        } else {
            self.report_missing("A right brace `}` is expected here.");
            self.c_missing();
            self.end_lowering_error(not_hack, tree_end.unwrap_or(self.prev_end));
            // The lexer is past what trails the last token it read: the
            // text of the string resumes there.
            self.hhvm_lexer_offset()
        };
        self.c_node(K::EmbeddedBracedExpression);
        parts.push(Expr {
            span: own_span.unwrap_or(Span::new(brace as u32, end as u32)),
            kind,
        });
        Ok(end)
    }

    // ------------------------------------------------------------------
    // The lossless tree

    /// Records a token inside a string literal, where nothing is trivia.
    #[inline(always)]
    fn c_string_token(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.c_token_with(kind, start as u32, end as u32, Trailing::None);
    }

    /// Records the literal text `start..end` of a string as one token.
    /// `first` and `last` tell whether it opens and closes the literal:
    /// those tokens are there even when they are only the quote. Trivia can
    /// only follow the closing one.
    #[inline(always)]
    fn c_string_text(&mut self, quote: Quote, start: usize, end: usize, first: bool, last: bool) {
        if self.c_on() {
            self.c_record_string_text(quote, start, end, first, last);
        }
    }

    #[cold]
    fn c_record_string_text(
        &mut self,
        quote: Quote,
        start: usize,
        end: usize,
        first: bool,
        last: bool,
    ) {
        let heredoc = quote == Quote::Heredoc;
        let kind = match (first, last) {
            (true, true) if heredoc => TokenKind::HeredocStringLiteral,
            (true, true) => TokenKind::DoubleQuotedStringLiteral,
            (true, false) if heredoc => TokenKind::HeredocStringLiteralHead,
            (true, false) => TokenKind::DoubleQuotedStringLiteralHead,
            (false, true) if heredoc => TokenKind::HeredocStringLiteralTail,
            (false, true) => TokenKind::DoubleQuotedStringLiteralTail,
            (false, false) if start == end => return,
            (false, false) => TokenKind::StringLiteralBody,
        };
        let rule = if last { Trailing::Code } else { Trailing::None };
        self.c_token_with(kind, start as u32, end as u32, rule);
    }

    /// Whether HHVM's lexer reads the double-quoted string `start..end` in
    /// one go: it does when the string has nothing in it that could be part
    /// of an interpolation, which rules out names and digits too.
    pub(crate) fn c_lexed_in_one_go(&self, start: u32, end: u32) -> bool {
        // A string the file ends in has no closing quote.
        let body = match &self.bytes[start as usize + 1..end as usize] {
            [body @ .., b'"'] => body,
            body => body,
        };
        !body.iter().any(|&c| {
            matches!(c, b'\\' | b'$' | b'{' | b'[' | b']' | b'-' | 0)
                || c.is_ascii_digit()
                || is_name_start(c)
        })
    }
}
