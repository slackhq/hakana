// Copyright (c) Meta Platforms, Inc. and affiliates. MIT; see LICENSE.
use parser_core_types::{
    lexable_token::LexableToken, source_text::SourceText,
    syntax_by_ref::positioned_syntax::PositionedSyntax, syntax_error::SyntaxError,
    syntax_kind::SyntaxKind, token_kind::TokenKind, trivia_kind::TriviaKind,
};
use positioned_by_ref_parser::{parse_script, scan_leading_php_trivia, scan_leading_xhp_trivia};
use std::{borrow::Cow, sync::Arc};
#[derive(Clone, Debug)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub text: String,
    pub start: usize,
}
impl Trivia {
    pub fn invisible(&self) -> bool {
        matches!(self.kind, TriviaKind::WhiteSpace | TriviaKind::EndOfLine)
    }
    pub fn end(&self) -> usize {
        self.start + self.text.len()
    }
    /// HH_FIXME/HH_IGNORE_ERROR comments, `/* HHAST_FIXME[...] */`, Hakana's
    /// own `/* HAKANA_FIXME[...] */` and `/* HAKANA_IGNORE[...] */`, and
    /// comments containing `@lint-ignore`.
    pub fn is_fixme_comment(&self) -> bool {
        match self.kind {
            TriviaKind::FixMe | TriviaKind::Ignore | TriviaKind::IgnoreError => true,
            TriviaKind::DelimitedComment => {
                let body = self.text.trim_start_matches("/*");
                let body = body.strip_prefix('*').unwrap_or(body).trim_start();
                ["HAKANA_FIXME[", "HAKANA_IGNORE[", "HHAST_FIXME["]
                    .iter()
                    .any(|prefix| body.starts_with(prefix))
                    || self.text.contains("@lint-ignore")
            }
            TriviaKind::SingleLineComment => self.text.contains("@lint-ignore"),
            _ => false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub start: usize,
    pub leading: Vec<Trivia>,
    pub trailing: Vec<Trivia>,
}
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: SyntaxKind,
    pub children: Vec<Node>,
    pub token: Option<Token>,
}
impl Node {
    // Children retain HHVM's generated syntax field order. The formatter and
    // parser are pinned to the same submodule revision.
    pub fn c(&self, i: usize) -> &Node {
        &self.children[i]
    }
    pub fn missing(&self) -> bool {
        self.kind == SyntaxKind::Missing
    }
    pub fn list(&self) -> &[Node] {
        if self.missing() {
            &[]
        } else if self.kind == SyntaxKind::SyntaxList {
            &self.children
        } else {
            std::slice::from_ref(self)
        }
    }
    pub fn first(&self) -> Option<&Token> {
        self.token
            .as_ref()
            .or_else(|| self.children.iter().find_map(Node::first))
    }
    pub fn last(&self) -> Option<&Token> {
        self.token
            .as_ref()
            .or_else(|| self.children.iter().rev().find_map(Node::last))
    }
    fn first_mut(&mut self) -> Option<&mut Token> {
        if self.token.is_some() {
            self.token.as_mut()
        } else {
            self.children.iter_mut().find_map(Node::first_mut)
        }
    }
    fn last_mut(&mut self) -> Option<&mut Token> {
        if self.token.is_some() {
            self.token.as_mut()
        } else {
            self.children.iter_mut().rev().find_map(Node::last_mut)
        }
    }
    pub fn leading(&self) -> &[Trivia] {
        self.first().map_or(&[], |t| &t.leading)
    }
    pub fn trailing(&self) -> &[Trivia] {
        self.last().map_or(&[], |t| &t.trailing)
    }
    pub fn without_leading(&self) -> (Vec<Trivia>, Cow<'_, Self>) {
        if self.leading().is_empty() {
            return (vec![], Cow::Borrowed(self));
        }
        let mut n = self.clone();
        let tr = n
            .first_mut()
            .map(|t| std::mem::take(&mut t.leading))
            .unwrap_or_default();
        (tr, Cow::Owned(n))
    }
    pub fn without_trailing(&self) -> (Cow<'_, Self>, Vec<Trivia>) {
        // In particular, the last argument of a call may be an entire closure.
        // Avoid cloning that subtree when there is no trivia to detach.
        if self.trailing().is_empty() {
            return (Cow::Borrowed(self), vec![]);
        }
        let mut n = self.clone();
        let tr = n
            .last_mut()
            .map(|t| std::mem::take(&mut t.trailing))
            .unwrap_or_default();
        (Cow::Owned(n), tr)
    }
    pub fn with_leading(&self, tr: Vec<Trivia>) -> Self {
        let mut n = self.clone();
        if let Some(t) = n.first_mut() {
            t.leading = tr;
        }
        n
    }
    pub fn trailing_newline(&self) -> bool {
        self.trailing()
            .iter()
            .any(|t| t.kind == TriviaKind::EndOfLine)
    }
    pub fn text(&self) -> String {
        if let Some(t) = &self.token {
            t.leading
                .iter()
                .map(|t| t.text.as_str())
                .chain(std::iter::once(t.text.as_str()))
                .chain(t.trailing.iter().map(|t| t.text.as_str()))
                .collect()
        } else {
            self.children.iter().map(Node::text).collect()
        }
    }
    pub fn token_text(&self) -> &str {
        self.token.as_ref().map_or("", |t| &t.text)
    }
    pub fn is_token(&self, s: &str) -> bool {
        self.token_text() == s && !self.missing()
    }
    pub fn start(&self) -> usize {
        self.first().map_or(0, |t| t.start)
    }
    pub fn end(&self) -> usize {
        self.last().map_or(0, |t| t.start + t.text.len())
    }
    pub fn all_tokens<'a>(&'a self, out: &mut Vec<&'a Token>) {
        if let Some(t) = &self.token {
            out.push(t);
        } else {
            for c in &self.children {
                c.all_tokens(out);
            }
        }
    }
    fn all_tokens_mut<'a>(&'a mut self, out: &mut Vec<&'a mut Token>) {
        if let Some(t) = &mut self.token {
            out.push(t);
        } else {
            for c in &mut self.children {
                c.all_tokens_mut(out);
            }
        }
    }
}
fn convert<'a>(
    n: &'a PositionedSyntax<'a>,
    arena: &'a bumpalo::Bump,
    src: &'a SourceText<'a>,
) -> Node {
    if let Some(t) = n.get_token() {
        let start = t.start_offset();
        let end = start + t.width();
        let xhp = t.kind() == TokenKind::XHPBody;
        let scan = |offset, width| {
            let tr = if xhp {
                scan_leading_xhp_trivia(arena, src, offset, width)
            } else {
                scan_leading_php_trivia(arena, src, offset, width)
            };
            tr.into_iter()
                .map(|t| Trivia {
                    kind: t.kind,
                    text: String::from_utf8_lossy(&src.text()[t.offset..t.offset + t.width])
                        .into_owned(),
                    start: t.offset,
                })
                .collect()
        };
        Node {
            kind: SyntaxKind::Token(t.kind()),
            children: vec![],
            token: Some(Token {
                kind: t.kind(),
                text: String::from_utf8_lossy(&src.text()[start..end]).into_owned(),
                start,
                leading: scan(start - t.leading_width(), t.leading_width()),
                trailing: scan(end, t.trailing_width()),
            }),
        }
    } else {
        Node {
            kind: n.kind(),
            children: n.iter_children().map(|c| convert(c, arena, src)).collect(),
            token: None,
        }
    }
}
pub fn parse(text: &str, xhp_modifier: bool) -> (Node, Vec<SyntaxError>) {
    let arena = bumpalo::Bump::new();
    let src = SourceText::make(
        Arc::new(relative_path::RelativePath::make(
            relative_path::Prefix::Dummy,
            "input.hack".into(),
        )),
        text.as_bytes(),
    );
    let (root, errors, _) = parse_script(
        &arena,
        &src,
        parser_core_types::parser_env::ParserEnv {
            enable_xhp_class_modifier: xhp_modifier,
            ..Default::default()
        },
    );
    let mut node = convert(&root, &arena, &src);
    // Hakana's lexer intentionally leaves block comments in leading trivia for
    // analysis. Restore upstream attachment locally, without changing that lexer.
    let mut tokens = vec![];
    node.all_tokens_mut(&mut tokens);
    for i in 1..tokens.len() {
        let (before, after) = tokens.split_at_mut(i);
        let previous = &mut before[i - 1];
        let current = &mut after[0];
        if current
            .leading
            .first()
            .is_none_or(|t| t.kind != TriviaKind::DelimitedComment)
        {
            continue;
        }
        if previous
            .trailing
            .iter()
            .any(|t| t.kind == TriviaKind::EndOfLine)
        {
            continue;
        }
        let mut count = 0;
        for tr in &current.leading {
            if tr.is_fixme_comment() {
                break;
            }
            count += 1;
            if tr.kind == TriviaKind::EndOfLine {
                break;
            }
        }
        previous.trailing.extend(current.leading.drain(..count));
    }
    (node, errors)
}
