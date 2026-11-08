//! Whitespace and comments.
//!
//! Tokens only record how much trivia they carry; these functions look
//! inside. The rules are HHVM's, which the formatter's handling of comments
//! depends on.

use super::TriviaKind;

/// A piece of trivia: `src[offset..offset + width]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trivium {
    pub kind: TriviaKind,
    pub offset: usize,
    pub width: usize,
}

/// Scans one piece of trivia at `pos`, if there is one.
pub fn scan(src: &[u8], pos: usize) -> Option<Trivium> {
    let rest = &src[pos.min(src.len())..];
    let (kind, width) = match *rest.first()? {
        b' ' | b'\t' => (
            TriviaKind::WhiteSpace,
            rest.iter()
                .take_while(|&&b| b == b' ' || b == b'\t')
                .count(),
        ),
        b'\n' => (TriviaKind::EndOfLine, 1),
        b'\r' => (
            TriviaKind::EndOfLine,
            if rest.get(1) == Some(&b'\n') { 2 } else { 1 },
        ),
        b'/' if rest.get(1) == Some(&b'/') => {
            let width = line_width(rest);
            (single_line_kind(&rest[..width]), width)
        }
        b'/' if rest.get(1) == Some(&b'*') => {
            let width = match find(&rest[2..], b"*/") {
                Some(i) => i + 4,
                None => rest.len(),
            };
            (delimited_kind(&rest[..width]), width)
        }
        _ => return None,
    };
    Some(Trivium {
        kind,
        offset: pos,
        width,
    })
}

/// How much trivia after a token ending at `pos` trails that token:
/// whitespace and `//` comments, up to and including the line break.
/// A `/* */` comment never trails; it leads the next token.
pub fn trailing_width(src: &[u8], pos: usize) -> usize {
    let mut at = pos;
    while let Some(t) = scan(src, at) {
        match t.kind {
            TriviaKind::EndOfLine => return at + t.width - pos,
            TriviaKind::DelimitedComment
            | TriviaKind::FixMe
            | TriviaKind::Ignore
            | TriviaKind::IgnoreError => break,
            _ => at += t.width,
        }
    }
    at - pos
}

/// Where HHVM's own lexer ends the trailing trivia of a token ending at
/// `pos`. It differs from [`trailing_width`] in one way: a `/* */` comment
/// on the token's line trails it too. Hakana's trees do not follow HHVM
/// there, but the places HHVM derives from that extent do: the position of
/// an error about something missing, and which doc comment a declaration
/// has.
pub fn hhvm_trailing_width(src: &[u8], pos: usize) -> usize {
    let mut at = pos;
    while let Some(t) = scan(src, at) {
        match t.kind {
            TriviaKind::EndOfLine => return at + t.width - pos,
            TriviaKind::FixMe | TriviaKind::Ignore | TriviaKind::IgnoreError => break,
            _ => at += t.width,
        }
    }
    at - pos
}

/// Scans one piece of trivia inside XHP text, where only whitespace and
/// line breaks are trivia.
pub fn scan_xhp(src: &[u8], pos: usize) -> Option<Trivium> {
    match *src.get(pos)? {
        b' ' | b'\t' | b'\n' | b'\r' => scan(src, pos),
        _ => None,
    }
}

/// Like [`trailing_width`], for a token inside XHP text.
pub fn trailing_width_xhp(src: &[u8], pos: usize) -> usize {
    let mut at = pos;
    while let Some(t) = scan_xhp(src, at) {
        at += t.width;
        if t.kind == TriviaKind::EndOfLine {
            break;
        }
    }
    at - pos
}

/// The pieces of trivia in `src[offset..offset + width]`. `xhp` says whether
/// the token they belong to is XHP text.
pub fn pieces(src: &[u8], offset: usize, width: usize, xhp: bool) -> Vec<Trivium> {
    let end = (offset + width).min(src.len());
    let mut out = vec![];
    let mut at = offset;
    while at < end {
        let scanned = if xhp {
            scan_xhp(&src[..end], at)
        } else {
            scan(&src[..end], at)
        };
        match scanned {
            Some(t) => {
                at += t.width;
                out.push(t);
            }
            None => break,
        }
    }
    out
}

fn line_width(rest: &[u8]) -> usize {
    rest.iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .unwrap_or(rest.len())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// `// FALLTHROUGH` is its own kind.
fn single_line_kind(comment: &[u8]) -> TriviaKind {
    let body = &comment[2..];
    let body = &body[body
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count()..];
    if body.starts_with(b"FALLTHROUGH") {
        TriviaKind::FallThrough
    } else {
        TriviaKind::SingleLineComment
    }
}

/// `/* HH_FIXME[...] */`, `/* HH_IGNORE_ERROR[...] */` and `/* HH_IGNORE[...] */`
/// are their own kinds.
fn delimited_kind(comment: &[u8]) -> TriviaKind {
    let body = &comment[2..];
    let body = &body[body
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count()..];
    if body.starts_with(b"HH_FIXME") {
        TriviaKind::FixMe
    } else if body.starts_with(b"HH_IGNORE_ERROR") {
        TriviaKind::IgnoreError
    } else if body.starts_with(b"HH_IGNORE") {
        TriviaKind::Ignore
    } else {
        TriviaKind::DelimitedComment
    }
}
