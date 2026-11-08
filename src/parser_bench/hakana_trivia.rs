//! The one place where Hakana's parser deliberately differs from HHVM's: a
//! `/* */` comment never trails the token before it. In HHVM a block comment
//! on the same line as the preceding token belongs to that token's trailing
//! trivia, and HHVM's comment scourer, which only ever looks at leading
//! trivia, then never reports it. Hakana attaches the comment (and whatever
//! follows it) to the next token instead, so that every block comment is
//! seen — `/* HAKANA_FIXME[...] */` and friends depend on that.
//!
//! This module restates HHVM's results under that rule, so that the oracle
//! can be an unmodified HHVM:
//!
//! * [`repartition`] moves the trivia in HHVM's lossless tree;
//! * [`scour`] is HHVM's comment scourer (`scour_comment.rs`), run over the
//!   moved trivia.

use std::sync::Arc;

use hakana_parser::cst;
use oxidized::pos::Pos;
use oxidized::prim_defs::Comment;
use oxidized::scoured_comments::ScouredComments;
use parser_core_types::indexed_source_text::IndexedSourceText;
use parser_core_types::lexable_token::LexableToken;
use parser_core_types::source_text::SourceText;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::syntax_variant_generated::SyntaxVariant;
use parser_core_types::syntax_trait::SyntaxTrait;
use parser_core_types::token_kind::TokenKind;
use parser_core_types::trivia_kind::TriviaKind;
use relative_path::RelativePath;

/// A token of HHVM's tree with the extents of its trivia.
struct TokenTrivia {
    is_xhp_body: bool,
    /// Where the leading trivia starts.
    offset: usize,
    leading_width: usize,
    /// Where the token's text ends.
    end: usize,
    trailing_width: usize,
    /// The position HHVM gives the token (for the line of a fixme).
    pos: Pos,
    /// Inside a compound statement, where `HH_FIXME`s are honoured.
    in_block: bool,
}

#[derive(Clone, Copy)]
struct Piece {
    kind: TriviaKind,
    offset: usize,
    width: usize,
}

struct Scanner<'a> {
    arena: &'a bumpalo::Bump,
    source: &'a SourceText<'a>,
}

impl Scanner<'_> {
    /// The pieces of trivia in a range, as HHVM's lexer splits them.
    fn pieces(&self, offset: usize, width: usize, xhp: bool) -> Vec<Piece> {
        // HHVM's scanner trusts that the range ends where a piece of trivia
        // does, and never returns if it does not (which is the case for the
        // tokens its parser skipped in a broken file and filed as trivia).
        // It cannot read past the end of a text that stops there.
        let end = (offset + width).min(self.source.length());
        let clipped = SourceText::make(self.source.file_path_rc(), &self.source.text()[..end]);
        let trivia = if xhp {
            positioned_by_ref_parser::scan_leading_xhp_trivia(self.arena, &clipped, offset, width)
        } else {
            positioned_by_ref_parser::scan_leading_php_trivia(self.arena, &clipped, offset, width)
        };
        trivia
            .into_iter()
            .map(|t| Piece {
                kind: t.kind,
                offset: t.offset,
                width: t.width,
            })
            .collect()
    }
}

fn tokens<'a>(
    node: &'a PositionedSyntax<'a>,
    in_block: bool,
    source: &IndexedSourceText<'_>,
    out: &mut Vec<TokenTrivia>,
) {
    match &node.children {
        SyntaxVariant::Token(t) => {
            let start = t.start_offset();
            out.push(TokenTrivia {
                is_xhp_body: t.kind() == TokenKind::XHPBody,
                offset: start - t.leading_width(),
                leading_width: t.leading_width(),
                end: start + t.width(),
                trailing_width: t.trailing_width(),
                pos: node
                    .position_exclusive(source)
                    .map_or(Pos::NONE, Into::into),
                in_block,
            });
        }
        other => {
            let in_block = in_block || matches!(other, SyntaxVariant::CompoundStatement(_));
            for child in node.iter_children() {
                tokens(child, in_block, source, out);
            }
        }
    }
}

/// Applies Hakana's rule to the trivia extents: what follows the first block
/// comment in a token's trailing trivia, that comment included, leads the
/// next token instead.
fn move_block_comments(tokens: &mut [TokenTrivia], scanner: &Scanner<'_>) {
    for i in 0..tokens.len().saturating_sub(1) {
        let (end, width, xhp) = (
            tokens[i].end,
            tokens[i].trailing_width,
            tokens[i].is_xhp_body,
        );
        if width == 0 {
            continue;
        }
        let Some(comment) = scanner
            .pieces(end, width, xhp)
            .into_iter()
            .find(|p| p.kind == TriviaKind::DelimitedComment)
        else {
            continue;
        };
        let moved = end + width - comment.offset;
        tokens[i].trailing_width -= moved;
        tokens[i + 1].offset -= moved;
        tokens[i + 1].leading_width += moved;
    }
}

/// HHVM's lossless tree with Hakana's attachment of block comments, in the
/// form Hakana's own parser produces its tree.
pub fn repartition(node: &mut cst::Node, src: &[u8]) {
    fn collect<'n>(node: &'n mut cst::Node, out: &mut Vec<&'n mut cst::Token>) {
        match node {
            cst::Node::Token(t) => out.push(t),
            cst::Node::Missing => {}
            cst::Node::List(children) | cst::Node::Syntax(_, children) => {
                children.iter_mut().for_each(|c| collect(c, out))
            }
        }
    }
    let mut tokens = vec![];
    collect(node, &mut tokens);
    for i in 0..tokens.len().saturating_sub(1) {
        let token = &*tokens[i];
        if token.trailing_width == 0 {
            continue;
        }
        let xhp = token.kind == cst::TokenKind::XHPBody;
        let Some(comment) =
            cst::trivia::pieces(src, token.end(), token.trailing_width as usize, xhp)
                .into_iter()
                .find(|p| p.kind == cst::TriviaKind::DelimitedComment)
        else {
            continue;
        };
        let moved = (token.full_end() - comment.offset) as u32;
        tokens[i].trailing_width -= moved;
        tokens[i + 1].offset -= moved;
        tokens[i + 1].leading_width += moved;
    }
}

/// What HHVM's comment scourer reports for `root` once block comments are
/// attached the way Hakana attaches them. The scourer's own behaviour is
/// kept as it is, oddities included: after a token's leading trivia it
/// means to read the trailing trivia but reads from the start of the leading
/// trivia again, by the rule for trailing trivia.
pub fn scour<'a>(
    root: &'a PositionedSyntax<'a>,
    indexed: &'a IndexedSourceText<'a>,
    arena: &'a bumpalo::Bump,
    path: &Arc<RelativePath>,
) -> ScouredComments {
    let source = indexed.source_text();
    let scanner = Scanner { arena, source };
    let mut all = vec![];
    tokens(root, false, indexed, &mut all);
    move_block_comments(&mut all, &scanner);

    let pos = |start: usize, end: usize| -> Pos { indexed.relative_pos(start, end).into() };
    let mut acc = ScouredComments::new();
    for token in &all {
        let leading = scanner.pieces(token.offset, token.leading_width, token.is_xhp_body);
        // The "trailing" pass: from the start of the leading trivia up to
        // the first line break, stopping short of a fixme or block comment.
        let mut reread = vec![];
        for piece in &leading {
            match piece.kind {
                TriviaKind::EndOfLine => {
                    reread.push(*piece);
                    break;
                }
                TriviaKind::FixMe
                | TriviaKind::Ignore
                | TriviaKind::IgnoreError
                | TriviaKind::DelimitedComment => break,
                _ => reread.push(*piece),
            }
        }
        for piece in leading.iter().chain(reread.iter()) {
            let (start, end) = (piece.offset, piece.offset + piece.width - 1);
            match piece.kind {
                TriviaKind::WhiteSpace
                | TriviaKind::EndOfLine
                | TriviaKind::FallThrough
                | TriviaKind::ExtraTokenError => {}
                TriviaKind::DelimitedComment => {
                    let text_start = start + 2;
                    let len = end - text_start - 1;
                    acc.comments.push((
                        pos(end - 1, end),
                        Comment::CmtBlock(source.sub_as_str(text_start, len).to_string()),
                    ));
                }
                TriviaKind::SingleLineComment => {
                    let text_start = start + if source.text()[start] == b'#' { 1 } else { 2 };
                    let len = end + 1 - text_start;
                    let mut text = source.sub_as_str(text_start, len).to_string();
                    text.push('\n');
                    acc.comments
                        .push((pos(text_start, end), Comment::CmtLine(text)));
                }
                TriviaKind::FixMe | TriviaKind::Ignore | TriviaKind::IgnoreError => {
                    let text = source.sub(start, piece.width);
                    let line = token.pos.line() as isize;
                    let comment_pos = pos(start, end + 1);
                    match fixme_code(text) {
                        Some(code) => match piece.kind {
                            TriviaKind::Ignore => acc.add_to_ignores(line, code, comment_pos),
                            TriviaKind::FixMe => {
                                let in_hhi = path.prefix() == relative_path::Prefix::Hhi;
                                if token.in_block || in_hhi {
                                    acc.add_to_fixmes(line, code, comment_pos);
                                } else {
                                    acc.add_to_misuses(line, code, comment_pos);
                                }
                            }
                            _ => {}
                        },
                        None => acc.add_format_error(token.pos.clone()),
                    }
                }
            }
        }
    }
    acc.comments.reverse();
    acc
}

/// The error code of `HH_FIXME[1234]` and its relatives.
fn fixme_code(text: &[u8]) -> Option<isize> {
    for marker in [&b"HH_FIXME"[..], b"HH_IGNORE_ERROR", b"HH_IGNORE"] {
        let Some(at) = text.windows(marker.len()).position(|w| w == marker) else {
            continue;
        };
        let rest = &text[at + marker.len()..];
        let rest = &rest[rest
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\t' | b'\n'))
            .count()..];
        let Some(rest) = rest.strip_prefix(b"[") else {
            continue;
        };
        let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0 && rest.get(digits) == Some(&b']') {
            return std::str::from_utf8(&rest[..digits]).ok()?.parse().ok();
        }
    }
    None
}
