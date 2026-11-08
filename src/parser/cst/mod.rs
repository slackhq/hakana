//! The lossless syntax tree: every token of the file, with the whitespace
//! and comments around it, arranged in nodes.
//!
//! This is what the linters and the formatter work on. Its node kinds, the
//! order of their children and its token kinds are those of HHVM's
//! full-fidelity tree (see `kinds.rs`, generated from HHVM's definitions),
//! because that is the shape hackfmt's rules and the HHAST linters are
//! written against. The analysis tree in [`crate::aast`] is something else
//! entirely: no tokens, no trivia, built for speed.
//!
//! The tree is produced by the same parser, in a recording mode: each token
//! it consumes is pushed on a stack, and each construct it recognises pops
//! its children off and pushes a node ([`Builder`]).

mod kinds;
pub mod trivia;

pub use kinds::{SyntaxKind, TokenKind, TriviaKind};

/// A token and the extent of the trivia attached to it.
///
/// Trivia before a token belongs to it, except for what follows the previous
/// token on its line (whitespace and `//` comments up to the line break),
/// which trails that one. See [`trivia`] for looking inside those ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Where the leading trivia starts.
    pub offset: u32,
    pub leading_width: u32,
    pub width: u32,
    pub trailing_width: u32,
}

impl Token {
    /// Where the token's own text starts.
    #[inline]
    pub fn start(&self) -> usize {
        (self.offset + self.leading_width) as usize
    }

    /// Where the token's own text ends.
    #[inline]
    pub fn end(&self) -> usize {
        self.start() + self.width as usize
    }

    /// Where the trailing trivia ends.
    #[inline]
    pub fn full_end(&self) -> usize {
        self.end() + self.trailing_width as usize
    }

    pub fn text<'s>(&self, src: &'s str) -> &'s str {
        &src[self.start()..self.end()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// An optional child that is not there.
    Missing,
    Token(Token),
    /// One or more nodes; an empty list is `Missing`.
    List(Vec<Node>),
    /// A node of the given kind, with exactly `kind.children().len()`
    /// children.
    Syntax(SyntaxKind, Vec<Node>),
}

impl Node {
    pub fn kind(&self) -> SyntaxKind {
        match self {
            Node::Missing => SyntaxKind::Missing,
            Node::Token(t) => SyntaxKind::Token(t.kind),
            Node::List(_) => SyntaxKind::SyntaxList,
            Node::Syntax(kind, _) => *kind,
        }
    }

    pub fn children(&self) -> &[Node] {
        match self {
            Node::Missing | Node::Token(_) => &[],
            Node::List(children) | Node::Syntax(_, children) => children,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, Node::Missing)
    }

    pub fn first_token(&self) -> Option<&Token> {
        match self {
            Node::Token(t) => Some(t),
            _ => self.children().iter().find_map(Node::first_token),
        }
    }

    pub fn last_token(&self) -> Option<&Token> {
        match self {
            Node::Token(t) => Some(t),
            _ => self.children().iter().rev().find_map(Node::last_token),
        }
    }

    /// An indented rendering of the tree, for tests and debugging.
    pub fn dump(&self, src: &str) -> String {
        let mut out = String::new();
        self.dump_into(src, 0, None, &mut out);
        out
    }

    fn dump_into(&self, src: &str, depth: usize, name: Option<&str>, out: &mut String) {
        use std::fmt::Write;
        for _ in 0..depth {
            out.push_str("  ");
        }
        if let Some(name) = name {
            let _ = write!(out, "{name}: ");
        }
        match self {
            Node::Missing => out.push_str("Missing\n"),
            Node::Token(t) => {
                let _ = writeln!(
                    out,
                    "{:?} {:?} @{} lead={} trail={}",
                    t.kind,
                    src.get(t.start()..t.end()).unwrap_or("<out of range>"),
                    t.start(),
                    t.leading_width,
                    t.trailing_width
                );
            }
            Node::List(children) => {
                out.push_str("List\n");
                for child in children {
                    child.dump_into(src, depth + 1, None, out);
                }
            }
            Node::Syntax(kind, children) => {
                let _ = writeln!(out, "{}", kind.name());
                let names = kind.children();
                for (i, child) in children.iter().enumerate() {
                    child.dump_into(src, depth + 1, names.get(i).copied(), out);
                }
            }
        }
    }
}

/// Where two trees first differ, as a path from the root and a description;
/// `None` if they are the same.
pub fn first_difference(a: &Node, b: &Node) -> Option<String> {
    fn go(a: &Node, b: &Node, path: &mut Vec<String>) -> Option<String> {
        let here = |path: &Vec<String>, what: String| Some(format!("{}: {what}", path.join("/")));
        match (a, b) {
            (Node::Missing, Node::Missing) => None,
            (Node::Token(x), Node::Token(y)) => {
                if x == y {
                    None
                } else {
                    here(path, format!("token {x:?} vs {y:?}"))
                }
            }
            (Node::List(xs), Node::List(ys)) | (Node::Syntax(_, xs), Node::Syntax(_, ys))
                if a.kind() == b.kind() =>
            {
                if xs.len() != ys.len() {
                    return here(
                        path,
                        format!(
                            "{} has {} children vs {}",
                            a.kind().name(),
                            xs.len(),
                            ys.len()
                        ),
                    );
                }
                let names = a.kind().children();
                for (i, (x, y)) in xs.iter().zip(ys).enumerate() {
                    path.push(match names.get(i) {
                        Some(name) => format!("{}.{name}", a.kind().name()),
                        None => format!("{}[{i}]", a.kind().name()),
                    });
                    let found = go(x, y, path);
                    path.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            _ => here(path, format!("{} vs {}", describe(a), describe(b))),
        }
    }
    fn describe(n: &Node) -> String {
        match n {
            Node::Token(t) => format!("token {:?} @{}", t.kind, t.start()),
            _ => match n.first_token() {
                Some(t) => format!("{} @{}", n.kind().name(), t.start()),
                None => n.kind().name().to_string(),
            },
        }
    }
    go(a, b, &mut vec![])
}

/// Builds the tree bottom-up while the parser runs.
///
/// Tokens are pushed as they are consumed. When the parser has recognised a
/// construct it calls [`Builder::node`], which replaces the construct's
/// children — the topmost entries of the stack — with one node. Children the
/// source does not have are pushed explicitly with [`Builder::missing`].
///
/// Where the parser only learns late what it is looking at, the entries
/// already on the stack are adjusted in place ([`Builder::insert_missing`],
/// [`Builder::remove`] and the more specific helpers below).
///
/// The parser calls these methods from its hottest paths, behind a check
/// that fails whenever it is not recording, so they are marked cold: that
/// keeps them, and the branches leading to them, out of the way.
#[derive(Debug, Default)]
pub struct Builder {
    pub(crate) stack: Vec<Node>,
    /// Where the trailing trivia of the last token pushed ends: the next
    /// token's leading trivia starts here.
    pub(crate) full_end: u32,
    /// A request did not fit what is on the stack, so the tree cannot be
    /// trusted. The parser reports that instead of a tree.
    pub(crate) inconsistent: bool,
}

impl Builder {
    /// A position in the stack, to hand to [`Builder::list`] later.
    #[inline]
    pub fn mark(&self) -> usize {
        self.stack.len()
    }

    /// Pushes a token whose text is `src[start..end]`. Everything between
    /// the previous token's trailing trivia and `start` becomes its leading
    /// trivia; its trailing trivia is found by scanning past `end`.
    #[cold]
    pub fn token(&mut self, src: &[u8], kind: TokenKind, start: u32, end: u32) {
        let trailing_width = trivia::trailing_width(src, end as usize) as u32;
        self.token_with_trailing(kind, start, end, trailing_width);
    }

    /// Like [`Builder::token`], for tokens whose trailing trivia follows
    /// other rules (inside strings and XHP).
    #[cold]
    pub fn token_with_trailing(&mut self, kind: TokenKind, start: u32, end: u32, trailing: u32) {
        let offset = self.full_end.min(start);
        self.stack.push(Node::Token(Token {
            kind,
            offset,
            leading_width: start - offset,
            width: end - start,
            trailing_width: trailing,
        }));
        self.full_end = end + trailing;
    }

    /// Pushes an absent optional child.
    #[cold]
    pub fn missing(&mut self) {
        self.stack.push(Node::Missing);
    }

    /// Pushes `count` absent children.
    #[cold]
    pub fn missing_n(&mut self, count: usize) {
        self.stack.extend(std::iter::repeat_n(Node::Missing, count));
    }

    /// Replaces the topmost `kind.children().len()` entries with a node.
    #[cold]
    pub fn node(&mut self, kind: SyntaxKind) {
        let arity = kind.children().len();
        if self.stack.len() < arity {
            self.inconsistent = true;
            return;
        }
        let children = self.stack.split_off(self.stack.len() - arity);
        self.stack.push(Node::Syntax(kind, children));
    }

    /// Replaces everything pushed since `mark` with a list (`Missing` if
    /// that is nothing).
    #[cold]
    pub fn list(&mut self, mark: usize) {
        if mark > self.stack.len() {
            self.inconsistent = true;
            return;
        }
        let items = self.stack.split_off(mark);
        self.stack.push(if items.is_empty() {
            Node::Missing
        } else {
            Node::List(items)
        });
    }

    /// Ends a list item: replaces the item on top of the stack and its
    /// separator by one node. With `separated` the separator is the token
    /// above the item; otherwise there is none.
    #[cold]
    pub fn list_item(&mut self, separated: bool) {
        if !separated {
            self.stack.push(Node::Missing);
        }
        self.node(SyntaxKind::ListItem);
    }

    /// Changes the kind of the token on top of the stack: the same text can
    /// be a keyword or a plain name depending on where it stands.
    #[cold]
    pub fn retag(&mut self, kind: TokenKind) {
        if let Some(Node::Token(t)) = self.stack.last_mut() {
            t.kind = kind;
        }
    }

    /// The first token of the entry at `at`.
    #[cold]
    pub fn first_token_mut(&mut self, at: usize) -> Option<&mut Token> {
        fn first(node: &mut Node) -> Option<&mut Token> {
            match node {
                Node::Missing => None,
                Node::Token(t) => Some(t),
                Node::List(children) | Node::Syntax(_, children) => {
                    children.iter_mut().find_map(first)
                }
            }
        }
        first(self.stack.get_mut(at)?)
    }

    /// Takes the trailing trivia away from the token pushed last; it will
    /// lead the next token instead.
    #[cold]
    pub fn drop_trailing(&mut self) {
        fn last(node: &mut Node) -> Option<&mut Token> {
            match node {
                Node::Missing => None,
                Node::Token(t) => Some(t),
                Node::List(children) | Node::Syntax(_, children) => {
                    children.iter_mut().rev().find_map(last)
                }
            }
        }
        if let Some(t) = self.stack.last_mut().and_then(last) {
            t.trailing_width = 0;
            self.full_end = t.end() as u32;
        }
    }

    /// Makes the entry at `at` (and those after it) follow `count` absent
    /// optional children.
    #[cold]
    pub fn insert_missing(&mut self, at: usize, count: usize) {
        if at > self.stack.len() {
            self.inconsistent = true;
            return;
        }
        self.stack
            .splice(at..at, std::iter::repeat_n(Node::Missing, count));
    }

    /// Drops the entry at `at`: something the parser consumed that has no
    /// place in the tree.
    #[cold]
    pub fn remove(&mut self, at: usize) {
        if at >= self.stack.len() {
            self.inconsistent = true;
            return;
        }
        self.stack.remove(at);
    }

    /// Moves the entry at `at` above everything pushed after it: a child
    /// the source has in front of the ones that precede it in the node.
    #[cold]
    pub fn move_to_top(&mut self, at: usize) {
        if at >= self.stack.len() {
            self.inconsistent = true;
            return;
        }
        let node = self.stack.remove(at);
        self.stack.push(node);
    }

    /// If the node on top of the stack is of the given kind and has a
    /// single child, replaces it by that child.
    #[cold]
    pub fn unwrap(&mut self, kind: SyntaxKind) {
        let Some(Node::Syntax(top, children)) = self.stack.last_mut() else {
            return;
        };
        if *top == kind && children.len() == 1 {
            let child = children.remove(0);
            self.stack.pop();
            self.stack.push(child);
        }
    }

    /// Replaces the node on top of the stack by its children.
    #[cold]
    pub fn splice_node(&mut self) {
        match self.stack.pop() {
            Some(Node::Syntax(_, children)) => self.stack.extend(children),
            _ => self.inconsistent = true,
        }
    }

    /// Replaces the list at `at` by the items in it and their separators,
    /// side by side.
    #[cold]
    pub fn splice_list(&mut self, at: usize) {
        if at >= self.stack.len() {
            self.inconsistent = true;
            return;
        }
        let rest = self.stack.split_off(at + 1);
        match self.stack.pop() {
            Some(Node::List(items)) => {
                for item in items {
                    match item {
                        Node::Syntax(SyntaxKind::ListItem, parts) => self.stack.extend(parts),
                        other => self.stack.push(other),
                    }
                }
            }
            other => self.stack.extend(other),
        }
        self.stack.extend(rest);
    }

    /// In the list at `at`, replaces each item that is a simple type made
    /// of a `kind` token by the token itself.
    #[cold]
    pub fn bare_items(&mut self, at: usize, kind: TokenKind) {
        let Some(Node::List(items)) = self.stack.get_mut(at) else {
            return;
        };
        for item in items {
            let Node::Syntax(SyntaxKind::ListItem, parts) = item else {
                continue;
            };
            let Some(Node::Syntax(SyntaxKind::SimpleTypeSpecifier, name)) = parts.first() else {
                continue;
            };
            if matches!(name.as_slice(), [Node::Token(t)] if t.kind == kind) {
                parts[0] = name[0].clone();
            }
        }
    }

    /// Arranges the parameter modifier tokens pushed since `mark` into the
    /// four children a parameter has for them: `optional`, `inout`, `named`
    /// and `readonly`, each in its own place when they are written in that
    /// order, and otherwise simply in the order written.
    #[cold]
    pub fn parameter_modifiers(&mut self, mark: usize) {
        if mark > self.stack.len() {
            self.inconsistent = true;
            return;
        }
        let written = self.stack.split_off(mark);
        let slot = |node: &Node| match node {
            Node::Token(t) => match t.kind {
                TokenKind::Optional => 0,
                TokenKind::Inout => 1,
                TokenKind::Named => 2,
                _ => 3,
            },
            _ => 3,
        };
        let in_order = written
            .windows(2)
            .all(|pair| slot(&pair[0]) < slot(&pair[1]));
        let mut slots = [Node::Missing, Node::Missing, Node::Missing, Node::Missing];
        for (i, node) in written.into_iter().take(4).enumerate() {
            let at = if in_order { slot(&node) } else { i };
            slots[at] = node;
        }
        self.stack.extend(slots);
    }
}
