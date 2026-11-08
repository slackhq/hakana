//! Hand-written lexer for Hack.
//!
//! Unlike HHVM's full-fidelity lexer this one does not build trivia: whitespace
//! is skipped, and comments are appended to a side table as they are passed.
//! Tokens are plain `(kind, start, end)` triples over the source bytes.
//!
//! Double-quoted strings, heredocs and XHP are context-sensitive, so the lexer
//! only reports where they *start*; the parser drives the dedicated scanning
//! routines for their contents and then calls [`Lexer::seek`] to resume.

use crate::token::{MixedCase, T, keyword, mixed_case_keyword};

#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub kind: T,
    pub start: u32,
    pub end: u32,
    /// Where the trivia (whitespace and comments) in front of this token
    /// starts, i.e. the end of the previous token.
    pub trivia_start: u32,
    /// The token is a `Name` spelled like a *reserved* keyword in upper or
    /// mixed case. HHVM treats those as the keyword plus an error, so the
    /// parser bails out when it meets one outside a "keywords are names"
    /// position.
    pub mixed_case_kw: bool,
    /// Likewise for keywords that are not reserved (`String`, `Int`, ...).
    /// These are fine wherever HHVM reads non-reserved keywords as names,
    /// and an error wherever it expects a keyword (type positions).
    pub mixed_case_soft: bool,
    /// A `// FALLTHROUGH` comment sits in the leading trivia of this token.
    pub leading_fallthrough: bool,
}

impl Token {
    pub const DUMMY: Token = Token {
        kind: T::Eof,
        start: 0,
        end: 0,
        trivia_start: 0,
        mixed_case_kw: false,
        mixed_case_soft: false,
        leading_fallthrough: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    /// `// ...`
    Line,
    /// `/* ... */`
    Block,
    /// `/* HH_FIXME[1234] ... */`
    FixMe,
    /// `/* HH_IGNORE[1234] ... */`
    Ignore,
    /// `/* HH_IGNORE_ERROR[1234] ... */`
    IgnoreError,
}

#[derive(Clone, Copy, Debug)]
pub struct RawComment {
    pub kind: CommentKind,
    /// Offset of the opening `/`.
    pub start: u32,
    /// Offset just past the comment (past `*/`, or at the newline).
    pub end: u32,
    /// Start of the first token after the comment.
    pub next_token: u32,
    /// A `//` comment on the same line as the code before it.
    pub trailing: bool,
    /// A `//` comment that is the first thing (bar blanks) on the line after
    /// the preceding token, or at the very start of the file.
    pub first_leading: bool,
}

pub struct Lexer<'a> {
    pub src: &'a [u8],
    pub pos: usize,
    pub comments: Vec<RawComment>,
    /// Comments before this offset have already been recorded; re-lexing the
    /// same region (lookahead, speculation) must not record them twice.
    comment_hwm: usize,
    /// Set when the lexer meets something it cannot tokenize.
    pub error: Option<(u32, &'static str)>,
    /// Whether the trivia skipped last contained a leading `// FALLTHROUGH`.
    saw_fallthrough: bool,
    /// A position that starts a line even though no token precedes it on
    /// that line (just after a `#!` line).
    pub line_start: usize,
    /// Where a `/*` comment that is never closed starts. It runs to the
    /// end of the file.
    pub unterminated_comment: Option<u32>,
    /// Where the variable HHVM has retired, `$prosecco_tap`, is used; a
    /// place is listed once for every time it was read.
    pub retired_variables: Vec<u32>,
}

pub const RETIRED_VARIABLE: &[u8] = b"$prosecco_tap";

#[inline]
pub fn is_name_start(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphabetic() || c >= 0x7f
}

#[inline]
pub fn is_name_char(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Lexer {
            src,
            pos: 0,
            comments: Vec::new(),
            comment_hwm: 0,
            error: None,
            saw_fallthrough: false,
            line_start: 0,
            unterminated_comment: None,
            retired_variables: Vec::new(),
        }
    }

    #[inline]
    pub fn at(&self, i: usize) -> u8 {
        // Returning 0 past the end keeps scanning loops branch-light; a real
        // NUL byte is rejected as an invalid character like HHVM does.
        if i < self.src.len() { self.src[i] } else { 0 }
    }

    #[inline]
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn fail(&mut self, at: usize, msg: &'static str) {
        if self.error.is_none() {
            self.error = Some((at as u32, msg));
        }
    }

    /// Skips whitespace and comments, recording the comments.
    pub fn skip_trivia(&mut self) {
        let src = self.src;
        let len = src.len();
        let mut i = self.pos;
        let first_new = self.comments.len();
        // HHVM gives everything up to the end of the line (bar block
        // comments) to the previous token as trailing trivia; only what
        // follows is "leading" for the next token.
        let mut leading = i == self.line_start;
        // Only blanks seen so far in the leading part.
        let mut first = leading;
        self.saw_fallthrough = false;
        loop {
            if i >= len {
                break;
            }
            match src[i] {
                b' ' | b'\t' => i += 1,
                b'\n' | b'\r' => {
                    if src[i] == b'\r' && i + 1 < len && src[i + 1] == b'\n' {
                        i += 1;
                    }
                    i += 1;
                    if leading {
                        first = false;
                    } else {
                        leading = true;
                        first = true;
                    }
                }
                b'/' => {
                    let next = if i + 1 < len { src[i + 1] } else { 0 };
                    if next == b'/' {
                        let start = i;
                        i += 2;
                        while i < len && src[i] != b'\n' && src[i] != b'\r' {
                            i += 1;
                        }
                        // `// FALLTHROUGH` is its own trivia kind in HHVM
                        // and is never reported as a comment.
                        let mut ws = start + 2;
                        while ws < i && (src[ws] == b' ' || src[ws] == b'\t') {
                            ws += 1;
                        }
                        let is_fallthrough = src[ws..i].starts_with(b"FALLTHROUGH");
                        if is_fallthrough && leading {
                            self.saw_fallthrough = true;
                        }
                        if start >= self.comment_hwm {
                            if !is_fallthrough {
                                self.comments.push(RawComment {
                                    kind: CommentKind::Line,
                                    start: start as u32,
                                    end: i as u32,
                                    next_token: 0,
                                    trailing: !leading,
                                    first_leading: leading && first,
                                });
                            }
                            self.comment_hwm = i;
                        }
                        if leading {
                            first = false;
                        }
                    } else if next == b'*' {
                        let start = i;
                        leading = true;
                        first = false;
                        i += 2;
                        loop {
                            if i + 1 >= len {
                                self.unterminated_comment = Some(start as u32);
                                self.pos = len;
                                return;
                            }
                            if src[i] == b'*' && src[i + 1] == b'/' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                        if start >= self.comment_hwm {
                            let mut ws = start + 2;
                            while ws < i && (src[ws] == b' ' || src[ws] == b'\t') {
                                ws += 1;
                            }
                            let body = &src[ws..i];
                            let kind = if body.starts_with(b"HH_FIXME") {
                                CommentKind::FixMe
                            } else if body.starts_with(b"HH_IGNORE_ERROR") {
                                CommentKind::IgnoreError
                            } else if body.starts_with(b"HH_IGNORE") {
                                CommentKind::Ignore
                            } else {
                                CommentKind::Block
                            };
                            self.comments.push(RawComment {
                                kind,
                                start: start as u32,
                                end: i as u32,
                                next_token: 0,
                                trailing: false,
                                first_leading: false,
                            });
                            self.comment_hwm = i;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        for c in &mut self.comments[first_new..] {
            c.next_token = i as u32;
        }
        self.pos = i;
    }

    /// Lexes the next token in the default (PHP) mode.
    pub fn next_token(&mut self) -> Token {
        let trivia_start = self.pos as u32;
        self.skip_trivia();
        let leading_fallthrough = self.saw_fallthrough;
        let start = self.pos;
        let (kind, mixed) = self.scan();
        Token {
            kind,
            start: start as u32,
            end: self.pos as u32,
            trivia_start,
            mixed_case_kw: mixed == 1,
            mixed_case_soft: mixed == 2,
            leading_fallthrough,
        }
    }

    #[inline]
    fn adv(&mut self, n: usize, kind: T) -> (T, u8) {
        self.pos += n;
        (kind, 0)
    }

    fn scan(&mut self) -> (T, u8) {
        let i = self.pos;
        if i >= self.src.len() {
            return (T::Eof, 0);
        }
        let c0 = self.src[i];
        let c1 = self.at(i + 1);
        let c2 = self.at(i + 2);
        match c0 {
            b'[' => self.adv(1, T::LBracket),
            b']' => self.adv(1, T::RBracket),
            b'(' => self.adv(1, T::LParen),
            b')' => self.adv(1, T::RParen),
            b'{' => self.adv(1, T::LBrace),
            b'}' => self.adv(1, T::RBrace),
            b'.' => match c1 {
                b'=' => self.adv(2, T::DotEq),
                b'0'..=b'9' => {
                    self.scan_after_decimal_point();
                    (T::Float, 0)
                }
                b'.' if c2 == b'.' => self.adv(3, T::DotDotDot),
                _ => self.adv(1, T::Dot),
            },
            b'-' => match c1 {
                b'=' => self.adv(2, T::MinusEq),
                b'-' => self.adv(2, T::MinusMinus),
                b'>' => self.adv(2, T::Arrow),
                _ => self.adv(1, T::Minus),
            },
            b'+' => match c1 {
                b'=' => self.adv(2, T::PlusEq),
                b'+' => self.adv(2, T::PlusPlus),
                _ => self.adv(1, T::Plus),
            },
            b'*' => match (c1, c2) {
                (b'=', _) => self.adv(2, T::StarEq),
                (b'*', b'=') => self.adv(3, T::StarStarEq),
                (b'*', _) => self.adv(2, T::StarStar),
                _ => self.adv(1, T::Star),
            },
            b'~' => self.adv(1, T::Tilde),
            b'!' => match (c1, c2) {
                (b'=', b'=') => self.adv(3, T::BangEqEq),
                (b'=', _) => self.adv(2, T::BangEq),
                _ => self.adv(1, T::Bang),
            },
            b'$' => {
                if c1 == b'$' {
                    if c2 == b'$' || c2 == b'{' || is_name_start(c2) {
                        self.adv(1, T::Dollar)
                    } else {
                        self.adv(2, T::DollarDollar)
                    }
                } else if is_name_start(c1) {
                    let start = self.pos;
                    self.pos += 2;
                    self.skip_name_end();
                    if self.pos - start == RETIRED_VARIABLE.len()
                        && &self.src[start..self.pos] == RETIRED_VARIABLE
                    {
                        self.retired_variables.push(start as u32);
                    }
                    (T::Variable, 0)
                } else {
                    self.adv(1, T::Dollar)
                }
            }
            b'/' => {
                if c1 == b'=' {
                    self.adv(2, T::SlashEq)
                } else {
                    self.adv(1, T::Slash)
                }
            }
            b'%' => {
                if c1 == b'=' {
                    self.adv(2, T::PercentEq)
                } else {
                    self.adv(1, T::Percent)
                }
            }
            b'<' => match (c1, c2) {
                (b'<', b'<') => {
                    // Heredoc or nowdoc; the parser scans the header and body.
                    let mut j = i + 3;
                    while self.at(j) == b' ' || self.at(j) == b'\t' {
                        j += 1;
                    }
                    let kind = if self.at(j) == b'\'' {
                        T::Nowdoc
                    } else {
                        T::Heredoc
                    };
                    self.adv(3, kind)
                }
                (b'<', b'=') => self.adv(3, T::LtLtEq),
                (b'=', b'>') => self.adv(3, T::Spaceship),
                (b'=', _) => self.adv(2, T::LtEq),
                (b'<', _) => self.adv(2, T::LtLt),
                _ => self.adv(1, T::Lt),
            },
            b'>' => match (c1, c2) {
                (b'>', b'=') => self.adv(3, T::GtGtEq),
                (b'>', _) => self.adv(2, T::GtGt),
                (b'=', _) => self.adv(2, T::GtEq),
                _ => self.adv(1, T::Gt),
            },
            b'=' => match (c1, c2) {
                (b'=', b'=') => self.adv(3, T::EqEqEq),
                (b'=', b'>') => self.adv(3, T::LambdaArrow),
                (b'=', _) => self.adv(2, T::EqEq),
                (b'>', _) => self.adv(2, T::FatArrow),
                _ => self.adv(1, T::Eq),
            },
            b'^' => {
                if c1 == b'=' {
                    self.adv(2, T::CaretEq)
                } else {
                    self.adv(1, T::Caret)
                }
            }
            b'|' => match (c1, c2) {
                (b'=', _) => self.adv(2, T::BarEq),
                (b'>', _) => self.adv(2, T::BarGt),
                (b'?', b'>') => self.adv(3, T::BarQGt),
                (b'|', _) => self.adv(2, T::BarBar),
                _ => self.adv(1, T::Bar),
            },
            b'&' => match c1 {
                b'=' => self.adv(2, T::AmpEq),
                b'&' => self.adv(2, T::AmpAmp),
                _ => self.adv(1, T::Amp),
            },
            b'?' => match (c1, c2) {
                (b':', _) => self.adv(2, T::QuestionColon),
                (b'-', b'>') => self.adv(3, T::QArrow),
                (b'?', b'=') => self.adv(3, T::QQEq),
                (b'?', _) => self.adv(2, T::QQ),
                (b'a', b's') if !is_name_start(self.at(i + 3)) => self.adv(3, T::QuestionAs),
                _ => self.adv(1, T::Question),
            },
            b':' => {
                if c1 == b':' {
                    self.adv(2, T::ColonColon)
                } else {
                    self.adv(1, T::Colon)
                }
            }
            b';' => self.adv(1, T::Semicolon),
            b',' => self.adv(1, T::Comma),
            b'@' => self.adv(1, T::At),
            b'0' => match c1 {
                b'x' | b'X' => {
                    self.pos += 2;
                    if !self.at(self.pos).is_ascii_hexdigit() {
                        self.fail(i, "malformed hexadecimal literal");
                        return (T::Error, 0);
                    }
                    while self.at(self.pos).is_ascii_hexdigit() {
                        self.pos += 1;
                    }
                    (T::Int, 0)
                }
                b'b' | b'B' => {
                    self.pos += 2;
                    if !matches!(self.at(self.pos), b'0' | b'1') {
                        self.fail(i, "malformed binary literal");
                        return (T::Error, 0);
                    }
                    while matches!(self.at(self.pos), b'0' | b'1') {
                        self.pos += 1;
                    }
                    (T::Int, 0)
                }
                _ => (self.scan_octal_or_float(), 0),
            },
            b'1'..=b'9' => {
                self.skip_digits();
                match self.at(self.pos) {
                    b'.' => {
                        self.scan_after_decimal_point();
                        (T::Float, 0)
                    }
                    b'e' | b'E' => {
                        self.scan_exponent();
                        (T::Float, 0)
                    }
                    _ => (T::Int, 0),
                }
            }
            b'\'' => {
                let len = self.src.len();
                let mut j = i + 1;
                loop {
                    if j >= len {
                        // Not closed: all that is left of the file, which
                        // the parser has something to say about.
                        self.pos = len;
                        return (T::SingleQuoted, 0);
                    }
                    match self.src[j] {
                        b'\\' => j += 2,
                        b'\'' => break,
                        _ => j += 1,
                    }
                }
                self.pos = j + 1;
                (T::SingleQuoted, 0)
            }
            b'"' => self.adv(1, T::DoubleQuoted),
            b'`' => self.adv(1, T::Backtick),
            b'\\' => self.adv(1, T::Backslash),
            b'#' => self.adv(1, T::Hash),
            _ => {
                if is_name_start(c0) {
                    self.pos += 1;
                    self.skip_name_end();
                    let text = &self.src[i..self.pos];
                    // Every keyword starts with a lowercase letter or `_`.
                    if (c0 >= b'a' || c0 == b'_')
                        && let Some(kw) = keyword(text)
                    {
                        return (kw, 0);
                    }
                    if text.len() <= 12 && text.iter().any(u8::is_ascii_uppercase) {
                        match mixed_case_keyword(text) {
                            MixedCase::No => {}
                            MixedCase::Literal(kw) => return (kw, 0),
                            MixedCase::Keyword(kw) => {
                                return (T::Name, if kw.is_reserved() { 1 } else { 2 });
                            }
                        }
                    }
                    (T::Name, 0)
                } else {
                    // No token starts like this: the parser says so when
                    // it takes the token (see `Parser::whole_token`).
                    self.pos += 1;
                    (T::Error, 0)
                }
            }
        }
    }

    #[inline]
    pub fn skip_name_end(&mut self) {
        let src = self.src;
        let mut j = self.pos;
        while j < src.len() && is_name_char(src[j]) {
            j += 1;
        }
        self.pos = j;
    }

    #[inline]
    fn skip_digits(&mut self) {
        while self.at(self.pos).is_ascii_digit() {
            self.pos += 1;
        }
    }

    fn scan_exponent(&mut self) {
        let start = self.pos;
        let c = self.at(self.pos + 1);
        self.pos += if c == b'+' || c == b'-' { 2 } else { 1 };
        if !self.at(self.pos).is_ascii_digit() {
            self.fail(start, "malformed floating point literal");
            return;
        }
        self.skip_digits();
    }

    fn scan_after_decimal_point(&mut self) {
        self.pos += 1;
        self.skip_digits();
        if matches!(self.at(self.pos), b'e' | b'E') {
            self.scan_exponent();
        }
    }

    /// Mirrors HHVM's `scan_octal_or_float`, including its treatment of
    /// literals such as `089` (reported as octal, rejected later).
    fn scan_octal_or_float(&mut self) -> T {
        self.pos += 1;
        match self.at(self.pos) {
            b'.' => {
                self.scan_after_decimal_point();
                T::Float
            }
            b'e' | b'E' => {
                self.scan_exponent();
                T::Float
            }
            b'0'..=b'9' => {
                // Digits that are not octal are part of the literal all the
                // same: the parser is the one to object to them.
                self.skip_digits();
                match self.at(self.pos) {
                    b'e' | b'E' => {
                        self.scan_exponent();
                        T::Float
                    }
                    b'.' => {
                        self.scan_after_decimal_point();
                        T::Float
                    }
                    _ => T::Int,
                }
            }
            _ => T::Int,
        }
    }

    // ---------------------------------------------------------------------
    // XHP

    /// Scans an XHP element name (`foo:bar-baz`) starting at `pos`, returning
    /// its end. With `attribute` set, `:` does not continue the first label
    /// (attribute names may contain `-` but a `:` ends them).
    pub fn xhp_element_name_end(&self, pos: usize, attribute: bool) -> usize {
        let src = self.src;
        let mut j = pos + 1;
        let mut attribute = attribute;
        loop {
            while j < src.len() && is_name_char(src[j]) {
                j += 1;
            }
            let c0 = self.at(j);
            let c1 = self.at(j + 1);
            if ((!attribute && c0 == b':') || c0 == b'-') && is_name_start(c1) {
                j += 2;
                attribute = false;
            } else {
                return j;
            }
        }
    }

    /// Whether an XHP class name (`:foo`) starts at `pos`.
    #[inline]
    pub fn is_xhp_class_name_at(&self, pos: usize) -> bool {
        self.at(pos) == b':' && is_name_start(self.at(pos + 1))
    }

    /// Scans `:foo:bar-baz` at `pos`, returning its end.
    pub fn xhp_class_name_end(&self, pos: usize) -> usize {
        self.xhp_element_name_end(pos + 1, false)
    }

    /// Scans an XHP category name (`%foo:bar-baz`) at `pos`.
    pub fn xhp_category_name_end(&self, pos: usize) -> usize {
        self.xhp_element_name_end(pos + 1, false)
    }
}
