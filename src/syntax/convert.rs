//! From the tree the parser records to the typed one.

use bumpalo::Bump;
use hakana_parser::cst::{self, Node, trivia};

use crate::parser_env::ParserEnv;
use crate::source_text::SourceText;
use crate::syntax_by_ref::positioned_syntax::PositionedSyntax;
use crate::syntax_by_ref::positioned_token::{PositionedToken, PositionedTokenImpl};
use crate::syntax_by_ref::positioned_value::PositionedValue;
use crate::syntax_by_ref::syntax::Syntax;
use crate::syntax_by_ref::syntax_variant_generated::SyntaxVariant;
use crate::syntax_error::SyntaxError;
use crate::token_kind::TokenKind;

/// Parses a file into its lossless syntax tree, allocated in `arena`.
///
/// Syntax errors come back alongside the tree, which then has the shape
/// recovery gave it. If the parser could not recover at all, the tree is an
/// empty script.
pub fn parse_script<'a>(
    arena: &'a Bump,
    source: &SourceText<'_>,
    env: ParserEnv,
) -> (PositionedSyntax<'a>, Vec<SyntaxError>) {
    let options = hakana_parser::ParserOptions {
        enable_xhp_class_modifier: env.enable_xhp_class_modifier,
        disable_xhp_children_declarations: env.disable_xhp_children_declarations,
    };
    let is_hhi = source.file_path().ends_with(".hhi");
    match std::str::from_utf8(source.text())
        .map_err(|_| hakana_parser::ParseError {
            offset: 0,
            message: "the file is not valid UTF-8",
        })
        .and_then(|text| hakana_parser::parse_cst(text, is_hhi, options))
    {
        Ok(tree) => {
            let errors = tree
                .errors
                .iter()
                .map(|e| SyntaxError {
                    start_offset: e.start as usize,
                    end_offset: e.end as usize,
                    message: e.message.clone(),
                })
                .collect();
            (from_cst(arena, source.text(), &tree.root), errors)
        }
        Err(e) => {
            let empty = Node::Syntax(
                cst::SyntaxKind::Script,
                vec![Node::List(vec![Node::Syntax(
                    cst::SyntaxKind::EndOfFile,
                    vec![Node::Token(cst::Token {
                        kind: TokenKind::EndOfFile,
                        offset: 0,
                        leading_width: 0,
                        width: 0,
                        trailing_width: 0,
                    })],
                )])],
            );
            let error = SyntaxError {
                start_offset: e.offset as usize,
                end_offset: e.offset as usize,
                message: e.message.into(),
            };
            (from_cst(arena, source.text(), &empty), vec![error])
        }
    }
}

/// Converts a recorded tree. `src` is the text it was parsed from.
pub fn from_cst<'a>(arena: &'a Bump, src: &[u8], node: &Node) -> PositionedSyntax<'a> {
    // Where a missing node is: just past the token before it and that
    // token's trailing trivia.
    let mut position = 0;
    convert(arena, src, node, &mut position)
}

fn convert<'a>(
    arena: &'a Bump,
    src: &[u8],
    node: &Node,
    position: &mut usize,
) -> PositionedSyntax<'a> {
    match node {
        Node::Missing => Syntax::make(
            SyntaxVariant::Missing,
            PositionedValue::Missing { offset: *position },
        ),
        Node::Token(t) => {
            let xhp = t.kind == TokenKind::XHPBody;
            let kinds = |offset: usize, width: usize| {
                trivia::pieces(src, offset, width, xhp)
                    .iter()
                    .fold(0u16, |bits, piece| bits | 1 << piece.kind as u8)
            };
            let token = PositionedToken(arena.alloc(PositionedTokenImpl {
                kind: t.kind,
                offset: t.offset as usize,
                leading_width: t.leading_width as usize,
                width: t.width as usize,
                trailing_width: t.trailing_width as usize,
                leading_kinds: kinds(t.offset as usize, t.leading_width as usize),
                trailing_kinds: kinds(t.end(), t.trailing_width as usize),
            }));
            *position = t.full_end();
            Syntax::make(
                SyntaxVariant::Token(token),
                PositionedValue::from_token(token),
            )
        }
        Node::List(items) => {
            let start = *position;
            let children = convert_all(arena, src, items, position);
            // A list is where its children with text are.
            let value = PositionedValue::from_values(
                children.iter().map(|c| &c.value).filter(|v| v.width() > 0),
                start,
            );
            Syntax::make(SyntaxVariant::SyntaxList(children), value)
        }
        Node::Syntax(kind, items) => {
            let start = *position;
            let children = convert_all(arena, src, items, position);
            let value = PositionedValue::from_values(children.iter().map(|c| &c.value), start);
            Syntax::make(SyntaxVariant::from_children(*kind, children), value)
        }
    }
}

fn convert_all<'a>(
    arena: &'a Bump,
    src: &[u8],
    items: &[Node],
    position: &mut usize,
) -> &'a [PositionedSyntax<'a>] {
    let mut children = bumpalo::collections::Vec::with_capacity_in(items.len(), arena);
    for item in items {
        children.push(convert(arena, src, item, position));
    }
    children.into_bump_slice()
}
