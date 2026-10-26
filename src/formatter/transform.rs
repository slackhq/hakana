// Ported from HHVM hackfmt/hack_format.ml. Copyright (c) Meta Platforms, Inc.
// and affiliates. Licensed under MIT; see LICENSE.
use crate::{
    Config,
    doc::*,
    syntax::{Node, Trivia},
};
use Doc::*;
use Rule::*;
use parser_core_types::{syntax_kind::SyntaxKind, token_kind::TokenKind, trivia_kind::TriviaKind};
pub struct Formatter<'a> {
    pub env: &'a Config,
}
fn present(n: &Node, d: Doc) -> Doc {
    if n.missing() { Nil } else { d }
}
fn string(s: String, w: usize) -> Doc {
    if s.contains('\n') {
        Multiline(s.split('\n').map(str::to_owned).collect(), w)
    } else {
        Text(s, w)
    }
}
fn has_nl(tr: &[Trivia]) -> bool {
    tr.iter().any(|t| t.kind == TriviaKind::EndOfLine)
}
fn has_ws(tr: &[Trivia]) -> bool {
    tr.iter().any(|t| t.kind == TriviaKind::WhiteSpace)
}
fn ignores(tr: &[Trivia]) -> Doc {
    cat(tr.iter().map(|t| Ignore(t.text.len())).collect())
}
fn invisibles(tr: &[Trivia]) -> Doc {
    let mut n = 0;
    cat(tr
        .iter()
        .map(|t| {
            let d = Ignore(t.text.len());
            if t.kind == TriviaKind::EndOfLine {
                n += 1;
                cat(vec![d, if n <= 1 { BlankLine } else { Nil }])
            } else {
                d
            }
        })
        .collect())
}
fn trivia(tr: &[Trivia], is_leading: bool) -> Doc {
    struct Acc {
        leading: Vec<Trivia>,
        trailing: Vec<Trivia>,
        comment: Option<Doc>,
        delimited: bool,
        nl: bool,
        ws: bool,
        docs: Vec<Doc>,
    }
    impl Acc {
        fn flush(&mut self) {
            if self.comment.is_some() {
                self.nl = has_nl(&self.trailing);
                self.ws = has_ws(&self.trailing);
            }
            self.docs.push(cat(vec![
                invisibles(&self.leading),
                self.comment.take().unwrap_or(Nil),
                ignores(&self.trailing),
                if self.delimited && self.ws {
                    Space
                } else if self.nl {
                    Newline
                } else {
                    Nil
                },
            ]));
            self.leading.clear();
            self.trailing.clear();
        }
    }
    let mut a = Acc {
        leading: vec![],
        trailing: vec![],
        comment: None,
        delimited: false,
        nl: false,
        ws: false,
        docs: vec![],
    };
    let mut leading = is_leading;
    let mut indent = 0;
    for t in tr {
        match t.kind {
            TriviaKind::ExtraTokenError
            | TriviaKind::FixMe
            | TriviaKind::Ignore
            | TriviaKind::IgnoreError
            | TriviaKind::DelimitedComment => {
                let ws = has_ws(if leading { &a.leading } else { &a.trailing });
                a.flush();
                let mut lines = t.text.split('\n');
                let first = lines.next().unwrap_or("");
                let mut ds = vec![Comment(first.into(), first.len())];
                for line in lines {
                    let prefix = line
                        .bytes()
                        .take_while(|b| *b == b' ' || *b == b'\t')
                        .count();
                    let cut = if is_leading {
                        indent.min(prefix)
                    } else {
                        prefix
                    };
                    ds.push(cat(vec![
                        Ignore(1),
                        Newline,
                        Ignore(cut),
                        Comment(line[cut..].into(), line.len() - cut),
                    ]));
                }
                let should_break = leading
                    && !matches!(
                        t.kind,
                        TriviaKind::FixMe | TriviaKind::Ignore | TriviaKind::IgnoreError
                    );
                a.comment = Some(cat(vec![
                    if should_break {
                        Newline
                    } else if ws {
                        Space
                    } else {
                        Nil
                    },
                    cat(ds),
                ]));
                a.delimited = true;
                leading = false;
            }
            TriviaKind::FallThrough | TriviaKind::SingleLineComment => {
                a.flush();
                a.comment = Some(cat(vec![
                    if leading { Newline } else { Space },
                    LineComment(t.text.clone(), t.text.len()),
                ]));
                a.delimited = false;
                leading = false;
            }
            TriviaKind::EndOfLine => {
                indent = 0;
                if leading {
                    a.leading.push(t.clone());
                } else {
                    a.trailing.push(t.clone());
                    a.flush();
                }
                leading = true;
            }
            TriviaKind::WhiteSpace => {
                if leading {
                    indent = t.text.len();
                    a.leading.push(t.clone());
                } else {
                    a.trailing.push(t.clone());
                }
            }
        }
    }
    if a.docs.is_empty() {
        if is_leading {
            invisibles(tr)
        } else {
            ignores(tr)
        }
    } else {
        a.flush();
        cat(a.docs)
    }
}
fn unwrap_item(n: &Node) -> &Node {
    if n.kind == SyntaxKind::ListItem {
        n.c(0)
    } else {
        n
    }
}
fn op(n: &Node) -> operator::Operator {
    operator::Operator::trailing_from_token(n.token.as_ref().unwrap().kind)
}
impl Formatter<'_> {
    fn list(&self, n: &Node, before: Doc, after: Doc, last: Doc) -> Doc {
        let xs = n.list();
        cat(xs
            .iter()
            .enumerate()
            .map(|(i, x)| {
                cat(vec![
                    before.clone(),
                    self.t(x),
                    if i + 1 == xs.len() {
                        last.clone()
                    } else {
                        after.clone()
                    },
                ])
            })
            .collect())
    }
    fn all(&self, n: &Node) -> Doc {
        self.list(n, Nil, Nil, Nil)
    }
    fn simple(&self, n: &Node) -> Doc {
        cat(n.children.iter().map(|c| self.t(c)).collect())
    }
    fn block(&self, l: &Node, r: &Node, contents: Doc, collapse: bool) -> Doc {
        if collapse && !contents.printable() && !l.trailing_newline() {
            cat(vec![self.t(l), self.t(r)])
        } else {
            let (tr, r) = r.without_leading();
            cat(vec![
                self.t(l),
                Newline,
                Block(vec![contents, trivia(&tr, true), Newline]),
                self.t(&r),
            ])
        }
    }
    fn compound(&self, n: &Node, collapse: bool, space: bool) -> Doc {
        if n.kind == SyntaxKind::CompoundStatement {
            cat(vec![
                Space,
                self.block(n.c(0), n.c(2), self.all(n.c(1)), collapse),
                if space { Space } else { Nil },
            ])
        } else if matches!(n.kind, SyntaxKind::Token(_)) {
            self.t(n)
        } else {
            cat(vec![Newline, Block(vec![self.t(n)])])
        }
    }
    fn nest(&self, r: &Node, contents: Doc, spaces: bool) -> Doc {
        let (tr, r) = r.without_leading();
        let inner = Nest(vec![contents, trivia(&tr, true)]);
        let split = if !inner.printable() {
            Nil
        } else if spaces {
            space_split()
        } else {
            Split
        };
        cat(vec![split.clone(), inner, split, self.t(&r)])
    }
    fn delimited(&self, l: &Node, r: &Node, contents: Doc, parental: bool, force: bool) -> Doc {
        Span(vec![
            self.t(l),
            rule(
                if force {
                    Always
                } else if parental {
                    Parental
                } else {
                    Simple(1)
                },
                self.nest(r, contents, false),
            ),
        ])
    }
    fn bad_braces(&self, n: &Node) -> bool {
        let n = unwrap_item(n);
        match n.kind {
            SyntaxKind::LambdaExpression if n.c(4).kind == SyntaxKind::CompoundStatement => false,
            SyntaxKind::FunctionCallExpression => {
                n.c(0).kind == SyntaxKind::MemberSelectionExpression
            }
            SyntaxKind::ConditionalExpression
            | SyntaxKind::BinaryExpression
            | SyntaxKind::MemberSelectionExpression
            | SyntaxKind::FieldSpecifier
            | SyntaxKind::FieldInitializer
            | SyntaxKind::ElementInitializer
            | SyntaxKind::LambdaExpression
            | SyntaxKind::XHPExpression
            | SyntaxKind::IsExpression
            | SyntaxKind::AsExpression
            | SyntaxKind::NullableAsExpression => true,
            _ => false,
        }
    }
    fn braced(&self, l: &Node, n: &Node, r: &Node) -> Doc {
        if l.trailing().is_empty()
            && n.leading().is_empty()
            && n.trailing().is_empty()
            && r.leading().is_empty()
            && !self.bad_braces(n)
        {
            cat(vec![self.t(l), self.t(n), self.t(r)])
        } else {
            self.delimited(l, r, self.t(n), true, false)
        }
    }
    fn arg_item(&self, n: &Node) -> Doc {
        match n.kind {
            SyntaxKind::ListItem => cat(vec![self.arg_item(n.c(0)), self.t(n.c(1))]),
            SyntaxKind::BinaryExpression if !n.c(1).is_token(".") => {
                self.binary(n.c(0), n.c(1), n.c(2), true)
            }
            _ => self.t(n),
        }
    }
    fn comma(&self, item: &Node, comma: &Node, allow: bool) -> Doc {
        let allow = allow && self.env.add_trailing_commas;
        if let Some(t) = &comma.token {
            cat(vec![
                self.arg_item(item),
                trivia(&t.leading, true),
                if allow { Comma(true) } else { Nil },
                Ignore(t.text.len()),
                trivia(&t.trailing, false),
            ])
        } else {
            let (item, tr) = item.without_trailing();
            cat(vec![
                self.arg_item(&item),
                if allow { Comma(false) } else { Nil },
                trivia(&tr, false),
            ])
        }
    }
    fn args(&self, n: &Node, allow: bool) -> Doc {
        let xs = n.list();
        cat(xs
            .iter()
            .enumerate()
            .map(|(i, x)| {
                if i + 1 == xs.len() {
                    cat(vec![self.comma(x.c(0), x.c(1), allow), Split])
                } else {
                    cat(vec![self.arg_item(x), space_split()])
                }
            })
            .collect())
    }
    fn argish(
        &self,
        l: &Node,
        args: &Node,
        r: &Node,
        allow: bool,
        force: bool,
        spaces: bool,
    ) -> Doc {
        let xs = args.list();
        let mut allow = allow;
        let mut parental = true;
        if let Some(last) = xs.last() {
            let last_item = unwrap_item(last);
            if last_item.kind == SyntaxKind::DecoratedExpression && last_item.c(0).is_token("...") {
                allow = false;
            }
            if !spaces {
                let doc_literal = last_item.kind == SyntaxKind::LiteralExpression
                    && last_item.first().is_some_and(|t| {
                        matches!(
                            t.kind,
                            TokenKind::HeredocStringLiteral
                                | TokenKind::NowdocStringLiteral
                                | TokenKind::HeredocStringLiteralHead
                        )
                    });
                let trailing_ok = args.trailing().is_empty()
                    || self.env.version(3)
                        && doc_literal
                        && r.leading().iter().all(|t| t.kind == TriviaKind::WhiteSpace);
                let leading_ok = if xs.len() == 1 {
                    l.trailing().is_empty()
                } else {
                    last.leading().is_empty()
                };
                if leading_ok && trailing_ok {
                    parental = self.bad_braces(last)
                        || xs[..xs.len() - 1].iter().any(|n| self.t(n).has_split());
                }
            }
        }
        self.delimited(l, r, self.args(args, allow), parental, force)
    }
    fn fn_args(&self, args: &Node, r: &Node) -> Doc {
        let allow = args.list().last().is_none_or(|x| {
            let x = unwrap_item(x);
            match x.kind {
                SyntaxKind::ParameterDeclaration => x.c(8).missing(),
                SyntaxKind::ClosureParameterTypeSpecifier => x.c(7).missing(),
                _ => true,
            }
        });
        rule(Parental, self.nest(r, self.args(args, allow), false))
    }
    fn attribute(&self, n: &Node) -> Doc {
        if n.missing() {
            Nil
        } else {
            self.argish(n.c(0), n.c(1), n.c(2), true, false, false)
        }
    }
    fn declarators(&self, n: &Node) -> Doc {
        let xs = n.list();
        match xs {
            [] => Nil,
            [x] => Nest(vec![Space, SplitWith(1), self.t(x)]),
            _ => rule(
                Parental,
                Nest(
                    xs.iter()
                        .map(|x| cat(vec![Space, Split, self.t(x)]))
                        .collect(),
                ),
            ),
        }
    }
    fn condition(&self, l: &Node, c: &Node, r: &Node) -> Doc {
        cat(vec![
            self.t(l),
            Split,
            rule(Parental, cat(vec![Nest(vec![self.t(c)]), Split, self.t(r)])),
        ])
    }
    fn lambda_body(&self, n: &Node) -> Doc {
        if n.kind == SyntaxKind::CompoundStatement {
            self.compound(n, true, false)
        } else if n.kind == SyntaxKind::XHPExpression {
            rule(Parental, cat(vec![Space, Split, Nest(vec![self.t(n)])]))
        } else {
            cat(vec![Space, SplitWith(1), Nest(vec![self.t(n)])])
        }
    }
    fn consequence(&self, body: &Node, previous: &Node) -> Doc {
        if body.kind == SyntaxKind::CompoundStatement {
            self.compound(body, false, true)
        } else {
            cat(vec![
                Space,
                if previous.trailing_newline() {
                    cat(vec![Newline, Nest(vec![self.t(body)])])
                } else {
                    rule(Parental, Nest(vec![Span(vec![Space, Split, self.t(body)])]))
                },
            ])
        }
    }
    fn binary(&self, l: &Node, o: &Node, r: &Node, nested: bool) -> Doc {
        let operator = op(o);
        if operator.is_comparison() {
            return lazy(
                Parental,
                cat(vec![self.t(l), Space, self.t(o)]),
                cat(vec![Space, Split, Nest(vec![self.t(r)])]),
            );
        }
        if operator.is_assignment() {
            return cat(vec![
                self.t(l),
                Space,
                self.t(o),
                Space,
                SplitWith(if self.env.version(1) { 1 } else { 2 }),
                Nest(vec![self.t(r)]),
            ]);
        }
        fn flatten<'a>(n: &'a Node, precedence: usize, out: &mut Vec<&'a Node>) {
            if n.kind == SyntaxKind::BinaryExpression
                && op(n.c(1)).precedence(&Default::default()) == precedence
            {
                flatten(n.c(0), precedence, out);
                out.push(n.c(1));
                flatten(n.c(2), precedence, out);
            } else {
                out.push(n);
            }
        }
        let p = operator.precedence(&Default::default());
        let mut xs = vec![];
        flatten(l, p, &mut xs);
        xs.push(o);
        flatten(r, p, &mut xs);
        let operand = |n: &Node| {
            if n.kind == SyntaxKind::BinaryExpression {
                self.binary(n.c(0), n.c(1), n.c(2), true)
            } else {
                self.t(n)
            }
        };
        let mut ds = vec![];
        let mut last = xs[0];
        for pair in xs[1..].chunks_exact(2) {
            let o = pair[0];
            let r = pair[1];
            let concat = o.is_token(".");
            let pipe = o.is_token("|>");
            ds.push(cat(vec![
                if pipe && last.trailing_newline() {
                    Newline
                } else if pipe {
                    space_split()
                } else if concat {
                    Nil
                } else {
                    Space
                },
                if concat {
                    ConcatOp(Box::new(self.t(o)))
                } else {
                    self.t(o)
                },
            ]));
            ds.push(cat(vec![
                if pipe {
                    Space
                } else if concat {
                    Split
                } else {
                    space_split()
                },
                operand(r),
            ]));
            last = r;
        }
        lazy(
            Parental,
            operand(xs[0]),
            if nested { Nest(ds) } else { Conditional(ds) },
        )
    }
    fn chain(&self, n: &Node) -> Doc {
        type Member<'a> = (
            &'a Node,
            &'a Node,
            Option<&'a Node>,
            Option<(&'a Node, &'a Node, &'a Node)>,
        );
        fn flatten(n: &Node) -> (&Node, Vec<Member<'_>>) {
            let (member, args, targs) = if n.kind == SyntaxKind::FunctionCallExpression {
                (
                    n.c(0),
                    Some((n.c(2), n.c(3), n.c(4))),
                    Some(n.c(1)).filter(|n| !n.missing()),
                )
            } else {
                (n, None, None)
            };
            if !matches!(
                member.kind,
                SyntaxKind::MemberSelectionExpression | SyntaxKind::SafeMemberSelectionExpression
            ) {
                return (n, vec![]);
            }
            let (first, mut chain) = flatten(member.c(0));
            chain.push((member.c(1), member.c(2), targs, args));
            (first, chain)
        }
        let (first, chain) = flatten(n);
        let first_doc = if first.kind == SyntaxKind::FunctionCallExpression {
            cat(vec![
                self.t(first.c(0)),
                self.t(first.c(1)),
                self.argish(first.c(2), first.c(3), first.c(4), true, false, false),
            ])
        } else {
            self.t(first)
        };
        let ds: Vec<_> = chain
            .iter()
            .map(|(o, m, ty, args)| {
                cat(vec![
                    self.t(o),
                    self.t(m),
                    ty.map_or(Nil, |n| self.t(n)),
                    args.map_or(Nil, |(l, a, r)| self.argish(l, a, r, true, false, false)),
                ])
            })
            .collect();
        match ds.len() {
            0 => first_doc,
            1 => cat(vec![
                Span(vec![first_doc]),
                if first.trailing_newline() {
                    Newline
                } else {
                    SplitWith(3)
                },
                Nest(ds),
            ]),
            _ => {
                let (_, m, ty, args) = chain[0];
                let trail = args.map(|(_, _, r)| r).or(ty).unwrap_or(m);
                let kind = if trail.trailing_newline() {
                    Always
                } else if first.trailing_newline() || ds[..ds.len() - 1].iter().any(Doc::has_split)
                {
                    Parental
                } else {
                    Simple(0)
                };
                Span(vec![lazy(
                    kind,
                    cat(vec![
                        first_doc,
                        if first.trailing_newline() {
                            Newline
                        } else {
                            SplitWith(1)
                        },
                    ]),
                    cat(vec![
                        Nest(vec![ds[0].clone()]),
                        Nest(
                            ds[1..]
                                .iter()
                                .map(|d| cat(vec![Split, d.clone()]))
                                .collect(),
                        ),
                    ]),
                )])
            }
        }
    }
    pub fn t(&self, n: &Node) -> Doc {
        if let Some(i) = n.leading().iter().position(|t| {
            matches!(
                t.kind,
                TriviaKind::DelimitedComment | TriviaKind::SingleLineComment
            ) && t.text.contains("hackfmt-ignore")
        }) {
            let leading = n.leading();
            let node = n.with_leading(leading[i..].to_vec());
            let (node, tr) = node.without_trailing();
            let text = node.text();
            return cat(vec![
                trivia(&leading[..i], true),
                Newline,
                string(text.clone(), text.len()),
                trivia(&tr, false),
                if has_nl(&tr) { Newline } else { Nil },
            ]);
        }
        if let Some(tok) = &n.token {
            let mut lead = tok.leading.as_slice();
            if tok.kind == TokenKind::EndOfFile {
                while lead.last().is_some_and(Trivia::invisible) {
                    lead = &lead[..lead.len() - 1];
                }
            }
            let text = if tok.kind == TokenKind::EndOfFile {
                Nil
            } else if matches!(
                tok.kind,
                TokenKind::SingleQuotedStringLiteral
                    | TokenKind::DoubleQuotedStringLiteral
                    | TokenKind::DoubleQuotedStringLiteralHead
                    | TokenKind::DoubleQuotedStringLiteralTail
                    | TokenKind::HeredocStringLiteral
                    | TokenKind::HeredocStringLiteralHead
                    | TokenKind::HeredocStringLiteralTail
                    | TokenKind::NowdocStringLiteral
                    | TokenKind::XHPStringLiteral
                    | TokenKind::StringLiteralBody
            ) && (tok.kind != TokenKind::XHPStringLiteral || self.env.version(2))
            {
                string(tok.text.clone(), tok.text.len())
            } else {
                Text(tok.text.clone(), tok.text.len())
            };
            return cat(vec![trivia(lead, true), text, trivia(&tok.trailing, false)]);
        }
        self.node(n)
    }
    fn node(&self, n: &Node) -> Doc {
        let c = |i| n.c(i);
        let t = |i| self.t(c(i));
        let all = |i| self.all(c(i));
        let sp = |i| present(c(i), Space);
        let nl = |i| present(c(i), Newline);
        let mods = |i| self.list(c(i), Nil, Space, Space);
        match n.kind {
            SyntaxKind::Missing => Nil,
            SyntaxKind::Script => all(0),
            SyntaxKind::EndOfFile => t(0),
            SyntaxKind::SyntaxList => self.all(n),
            SyntaxKind::SimpleTypeSpecifier
            | SyntaxKind::VariableExpression
            | SyntaxKind::PipeVariableExpression
            | SyntaxKind::PropertyDeclarator
            | SyntaxKind::ConstantDeclarator
            | SyntaxKind::ScopeResolutionExpression
            | SyntaxKind::EmbeddedMemberSelectionExpression
            | SyntaxKind::EmbeddedSubscriptExpression
            | SyntaxKind::PostfixUnaryExpression
            | SyntaxKind::XHPRequired
            | SyntaxKind::XHPLateinit
            | SyntaxKind::XHPSimpleClassAttribute
            | SyntaxKind::XHPClose
            | SyntaxKind::TypeConstant
            | SyntaxKind::GenericTypeSpecifier
            | SyntaxKind::NullableTypeSpecifier
            | SyntaxKind::LikeTypeSpecifier
            | SyntaxKind::VariablePattern
            | SyntaxKind::ShapeSplatSpecifier
            | SyntaxKind::ShapeSplatExpression
            | SyntaxKind::ListItem => self.simple(n),
            SyntaxKind::LiteralExpression => {
                let literal = t(0);
                match c(0).last().map(|t| t.kind).unwrap_or(TokenKind::EndOfFile) {
                    TokenKind::HeredocStringLiteral
                    | TokenKind::HeredocStringLiteralHead
                    | TokenKind::HeredocStringLiteralTail
                    | TokenKind::NowdocStringLiteral => Literal(Box::new(literal)),
                    TokenKind::DecimalLiteral
                    | TokenKind::OctalLiteral
                    | TokenKind::HexadecimalLiteral
                    | TokenKind::BinaryLiteral
                    | TokenKind::FloatingLiteral => Number(Box::new(literal)),
                    _ => literal,
                }
            }
            SyntaxKind::PrefixedStringExpression => self.simple(n),
            SyntaxKind::MarkupSection => cat(vec![t(0), nl(0), t(1)]),
            SyntaxKind::MarkupSuffix | SyntaxKind::ExpressionStatement => {
                cat(vec![self.simple(n), Newline])
            }
            SyntaxKind::ReifiedTypeArgument
            | SyntaxKind::FunctionCtxTypeSpecifier
            | SyntaxKind::NamespaceDeclarationHeader => cat(vec![t(0), Space, t(1)]),
            SyntaxKind::QualifiedName | SyntaxKind::ModuleName => all(0),
            SyntaxKind::EnumDeclaration => cat(vec![
                t(0),
                nl(0),
                mods(1),
                t(2),
                Space,
                t(3),
                t(4),
                Space,
                SplitWith(1),
                Nest(vec![Space, t(5), Space, t(6), Space]),
                self.block(c(7), c(10), cat(vec![all(8), all(9)]), true),
                Newline,
            ]),
            SyntaxKind::Enumerator => {
                let value = t(2);
                cat(vec![
                    t(0),
                    Space,
                    t(1),
                    Space,
                    if value.has_split() { SplitWith(1) } else { Nil },
                    Nest(vec![value]),
                    t(3),
                    Newline,
                ])
            }
            SyntaxKind::EnumUse | SyntaxKind::TraitUse => cat(vec![
                t(0),
                if c(1).list().len() > 1 {
                    rule(
                        Parental,
                        Nest(vec![self.list(c(1), space_split(), Nil, Nil)]),
                    )
                } else {
                    cat(vec![Space, all(1)])
                },
                t(2),
                Newline,
            ]),
            SyntaxKind::AliasDeclaration => cat(vec![
                t(0),
                nl(0),
                mods(1),
                t(2),
                sp(2),
                t(3),
                Space,
                t(4),
                t(5),
                Space,
                all(6),
                present(
                    c(7),
                    cat(vec![Space, t(7), Space, SplitWith(1), Nest(vec![t(8)])]),
                ),
                t(9),
                Newline,
            ]),
            SyntaxKind::ContextAliasDeclaration => cat(vec![
                t(0),
                nl(0),
                t(1),
                Space,
                t(2),
                t(3),
                Space,
                all(4),
                present(
                    c(5),
                    cat(vec![Space, t(5), Space, SplitWith(1), Nest(vec![t(6)])]),
                ),
                t(7),
                Newline,
            ]),
            SyntaxKind::CaseTypeDeclaration => {
                let bar = c(9)
                    .list()
                    .first()
                    .is_some_and(|x| x.kind == SyntaxKind::CaseTypeVariant && !x.c(0).missing());
                cat(vec![
                    t(0),
                    nl(0),
                    mods(1),
                    t(2),
                    Space,
                    t(3),
                    Space,
                    t(4),
                    t(5),
                    sp(6),
                    t(6),
                    sp(6),
                    all(7),
                    present(
                        c(8),
                        cat(vec![
                            Space,
                            t(8),
                            if bar { Newline } else { Space },
                            rule(
                                if bar { Always } else { Parental },
                                Nest(vec![self.list(c(9), Nil, space_split(), Nil)]),
                            ),
                        ]),
                    ),
                    t(10),
                    Newline,
                ])
            }
            SyntaxKind::CaseTypeVariant => Span(vec![t(0), sp(0), t(1), sp(2), t(2)]),
            SyntaxKind::ClassAliasDeclaration => cat(vec![
                t(0),
                nl(0),
                all(1),
                t(2),
                t(3),
                Space,
                t(4),
                t(5),
                Space,
                t(6),
                Space,
                t(7),
                t(8),
                t(9),
                Newline,
            ]),
            SyntaxKind::PropertyDeclaration => {
                let declaration = cat(vec![mods(1), t(2), self.declarators(c(3)), t(4), Newline]);
                if c(0).missing() {
                    declaration
                } else {
                    lazy(
                        Parental,
                        self.attribute(c(0)),
                        cat(vec![Space, Split, declaration]),
                    )
                }
            }
            SyntaxKind::NamespaceDeclaration => cat(vec![t(0), t(1), Newline]),
            SyntaxKind::NamespaceBody => cat(vec![Space, self.block(c(0), c(2), all(1), true)]),
            SyntaxKind::NamespaceEmptyBody => t(0),
            SyntaxKind::NamespaceUseDeclaration => cat(vec![
                t(0),
                Space,
                t(1),
                sp(1),
                rule(
                    Parental,
                    Nest(vec![self.list(c(2), Nil, space_split(), Split)]),
                ),
                t(3),
                Newline,
            ]),
            SyntaxKind::NamespaceGroupUseDeclaration => cat(vec![
                t(0),
                Space,
                t(1),
                sp(1),
                t(2),
                self.argish(c(3), c(4), c(5), true, false, false),
                t(6),
                Newline,
            ]),
            SyntaxKind::NamespaceUseClause => {
                cat(vec![t(0), sp(0), t(1), sp(2), t(2), sp(3), t(3)])
            }
            SyntaxKind::FunctionDeclaration => cat(vec![
                t(0),
                nl(0),
                t(1),
                self.compound(c(2), true, true),
                Newline,
            ]),
            SyntaxKind::FunctionDeclarationHeader => cat(vec![
                Span(vec![mods(0), t(1), Space, t(2), t(3), t(4), Split]),
                self.fn_args(c(5), c(6)),
                t(7),
                t(8),
                sp(8),
                t(9),
                sp(9),
                t(10),
                sp(11),
                t(11),
            ]),
            SyntaxKind::WhereClause => {
                let xs = c(1).list();
                let ds = xs
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        cat(vec![
                            space_split(),
                            if i + 1 == xs.len() {
                                self.comma(x.c(0), x.c(1), false)
                            } else {
                                self.arg_item(x)
                            },
                        ])
                    })
                    .collect();
                rule(Parental, cat(vec![Space, Split, t(0), Nest(ds)]))
            }
            SyntaxKind::WhereConstraint => cat(vec![t(0), Space, t(1), Space, t(2)]),
            SyntaxKind::TypeRefinement => {
                let sep = if c(3).list().len() > 1 {
                    Newline
                } else {
                    Space
                };
                cat(vec![
                    t(0),
                    Space,
                    t(1),
                    Space,
                    t(2),
                    space_split(),
                    Nest(vec![Span(vec![self.list(
                        c(3),
                        cat(vec![sep.clone(), SplitWith(2)]),
                        Nil,
                        sep,
                    )])]),
                    t(4),
                ])
            }
            SyntaxKind::TypeInRefinement | SyntaxKind::CtxInRefinement => Span(vec![
                t(0),
                Space,
                t(1),
                t(2),
                self.list(c(3), Space, Nil, Nil),
                sp(4),
                t(4),
                present(c(5), cat(vec![Space, SplitWith(3), Nest(vec![t(5)])])),
            ]),
            SyntaxKind::Contexts => self.argish(c(0), c(1), c(2), true, false, false),
            SyntaxKind::MethodishDeclaration => cat(vec![
                t(0),
                nl(0),
                t(1),
                if c(2).missing() {
                    Nil
                } else {
                    self.compound(c(2), true, true)
                },
                t(3),
                Newline,
            ]),
            SyntaxKind::MethodishTraitResolution => {
                cat(vec![t(0), nl(0), t(1), t(2), t(3), t(4), Newline])
            }
            SyntaxKind::ClassishDeclaration => {
                let ancestors = |kw: usize, list: usize| {
                    if c(kw).missing() {
                        Nil
                    } else {
                        Nest(vec![
                            Space,
                            Split,
                            t(kw),
                            rule(
                                Parental,
                                Nest(vec![Span(vec![
                                    Space,
                                    if c(list).list().len() == 1 {
                                        SplitWith(1)
                                    } else {
                                        Split
                                    },
                                    Nest(vec![self.list(c(list), Nil, space_split(), Nil)]),
                                ])]),
                            ),
                        ])
                    }
                };
                cat(vec![
                    t(0),
                    nl(0),
                    Span(vec![mods(1), t(2), sp(2), t(3), Space, t(4), t(5)]),
                    rule(Parental, cat(vec![ancestors(6, 7), ancestors(8, 9)])),
                    t(10),
                ])
            }
            SyntaxKind::ClassishBody => {
                cat(vec![Space, self.block(c(0), c(2), all(1), true), Newline])
            }
            SyntaxKind::RequireClause | SyntaxKind::RequireClauseConstraint => {
                let value_i = n.children.len() - 2;
                let value = t(value_i);
                let mut ds = vec![];
                for i in 0..value_i {
                    ds.extend([t(i), Space]);
                }
                ds.extend([
                    if value.has_split() { SplitWith(3) } else { Nil },
                    Nest(vec![value, t(value_i + 1)]),
                    Newline,
                ]);
                cat(ds)
            }
            SyntaxKind::ConstDeclaration => cat(vec![
                t(0),
                nl(0),
                mods(1),
                t(2),
                sp(3),
                t(3),
                self.declarators(c(4)),
                t(5),
                Newline,
            ]),
            SyntaxKind::TypeConstDeclaration => cat(vec![
                t(0),
                nl(0),
                mods(1),
                Space,
                t(2),
                Space,
                t(3),
                Space,
                t(4),
                t(5),
                self.list(c(6), Space, Nil, Nil),
                sp(7),
                t(7),
                present(c(8), cat(vec![Space, SplitWith(1), Nest(vec![t(8)])])),
                t(9),
                Newline,
            ]),
            SyntaxKind::ContextConstDeclaration => cat(vec![
                mods(0),
                Space,
                t(1),
                Space,
                t(2),
                Space,
                t(3),
                t(4),
                sp(5),
                self.list(c(5), Nil, Space, Nil),
                sp(6),
                t(6),
                present(c(7), cat(vec![Space, SplitWith(1), Nest(vec![t(7)])])),
                t(8),
                Newline,
            ]),
            SyntaxKind::ParameterDeclaration => {
                let mut ds = vec![
                    self.attribute(c(0)),
                    present(c(0), cat(vec![Space, SplitWith(1)])),
                ];
                for i in 1..=6 {
                    ds.extend([t(i), sp(i)]);
                }
                ds.push(t(7));
                if c(9).missing() {
                    // Variadic named parameters can omit the variable name.
                    ds.push(t(8));
                } else if c(1).missing() && c(3).missing() && c(7).missing() {
                    ds.extend([t(8), t(9)]);
                } else {
                    ds.extend([Space, SplitWith(2), Nest(vec![t(8), t(9)])]);
                }
                ds.push(t(10));
                cat(ds)
            }
            SyntaxKind::FileAttributeSpecification => cat(vec![
                t(0),
                t(1),
                t(2),
                sp(2),
                self.nest(c(4), self.args(c(3), false), false),
                Newline,
            ]),
            SyntaxKind::OldAttributeSpecification => self.attribute(n),
            SyntaxKind::AttributizedSpecifier => cat(vec![self.attribute(c(0)), Space, t(1)]),
            SyntaxKind::InclusionExpression => cat(vec![
                t(0),
                if c(1).kind == SyntaxKind::ParenthesizedExpression {
                    Nil
                } else {
                    Space
                },
                SplitWith(1),
                t(1),
            ]),
            SyntaxKind::InclusionDirective => cat(vec![t(0), t(1), Newline]),
            SyntaxKind::CompoundStatement => cat(vec![self.compound(n, false, false), Newline]),
            SyntaxKind::UnsetStatement => cat(vec![
                t(0),
                self.argish(c(1), c(2), c(3), false, false, false),
                t(4),
                Newline,
            ]),
            SyntaxKind::WhileStatement => cat(vec![
                t(0),
                Space,
                self.condition(c(1), c(2), c(3)),
                self.compound(c(4), false, true),
                Newline,
            ]),
            SyntaxKind::UsingStatementBlockScoped => cat(vec![
                t(0),
                sp(0),
                t(1),
                Space,
                t(2),
                Split,
                rule(
                    Parental,
                    cat(vec![
                        Nest(vec![self.list(c(3), Nil, space_split(), Nil)]),
                        Split,
                        t(4),
                    ]),
                ),
                self.compound(c(5), false, true),
                Newline,
            ]),
            SyntaxKind::UsingStatementFunctionScoped => {
                cat(vec![t(0), sp(0), t(1), Space, t(2), t(3), Newline])
            }
            SyntaxKind::IfStatement => cat(vec![
                t(0),
                Space,
                self.condition(c(1), c(2), c(3)),
                self.consequence(c(4), c(3)),
                t(5),
                Newline,
            ]),
            SyntaxKind::ElseClause => cat(vec![
                t(0),
                if c(1).kind == SyntaxKind::IfStatement {
                    cat(vec![Space, t(1), Space])
                } else {
                    self.consequence(c(1), c(0))
                },
            ]),
            SyntaxKind::TryStatement => cat(vec![
                t(0),
                self.compound(c(1), false, true),
                all(2),
                t(3),
                Newline,
            ]),
            SyntaxKind::CatchClause => cat(vec![
                t(0),
                Space,
                self.delimited(
                    c(1),
                    c(4),
                    cat(vec![t(2), Space, SplitWith(1), Nest(vec![t(3)])]),
                    true,
                    false,
                ),
                self.compound(c(5), false, true),
            ]),
            SyntaxKind::FinallyClause => cat(vec![t(0), Space, self.compound(c(1), false, true)]),
            SyntaxKind::DoStatement => cat(vec![
                t(0),
                Space,
                self.compound(c(1), false, true),
                t(2),
                Space,
                self.condition(c(3), c(4), c(5)),
                t(6),
                Newline,
            ]),
            SyntaxKind::ForStatement => cat(vec![
                t(0),
                Space,
                t(1),
                rule(
                    Parental,
                    cat(vec![
                        Split,
                        Nest(vec![
                            self.list(c(2), Nil, space_split(), Nil),
                            t(3),
                            Space,
                            Split,
                            self.list(c(4), Nil, space_split(), Nil),
                            t(5),
                            Space,
                            Split,
                            self.list(c(6), Nil, space_split(), Nil),
                        ]),
                        Split,
                        t(7),
                    ]),
                ),
                self.compound(c(8), false, true),
                Newline,
            ]),
            SyntaxKind::ForeachStatement => cat(vec![
                t(0),
                Space,
                self.delimited(
                    c(1),
                    c(8),
                    cat(vec![
                        t(2),
                        Space,
                        t(3),
                        Space,
                        t(4),
                        Space,
                        SplitWith(1),
                        Nest(vec![Span(vec![
                            t(5),
                            Space,
                            t(6),
                            Space,
                            SplitWith(1),
                            Nest(vec![t(7)]),
                        ])]),
                    ]),
                    true,
                    false,
                ),
                self.compound(c(9), false, true),
                Newline,
            ]),
            SyntaxKind::SwitchStatement => cat(vec![
                t(0),
                Space,
                self.delimited(c(1), c(3), t(2), true, false),
                Space,
                self.block(c(4), c(6), all(5), true),
                Newline,
            ]),
            SyntaxKind::SwitchSection => {
                let (tr, labels) = c(0).without_leading();
                let split = tr
                    .iter()
                    .rposition(|t| t.kind == TriviaKind::FallThrough)
                    .map_or(0, |i| i + 1);
                let scoped =
                    c(1).list().len() == 1 && c(1).list()[0].kind == SyntaxKind::CompoundStatement;
                cat(vec![
                    if split == 0 {
                        trivia(&tr, true)
                    } else {
                        cat(vec![
                            Block(vec![trivia(&tr[..split], true), Newline]),
                            trivia(&tr[split..], false),
                        ])
                    },
                    self.list(&labels, Nil, Newline, if scoped { Nil } else { Newline }),
                    if scoped { all(1) } else { Block(vec![all(1)]) },
                    t(2),
                ])
            }
            SyntaxKind::CaseLabel => cat(vec![t(0), Space, t(1), t(2)]),
            SyntaxKind::DefaultLabel | SyntaxKind::SwitchFallthrough => self.simple(n),
            SyntaxKind::MatchStatement => cat(vec![
                t(0),
                Space,
                self.delimited(c(1), c(3), t(2), true, false),
                Space,
                self.block(c(4), c(6), all(5), true),
                Newline,
            ]),
            SyntaxKind::MatchStatementArm => cat(vec![
                t(0),
                Space,
                t(1),
                Space,
                if c(2).kind == SyntaxKind::CompoundStatement {
                    self.compound(c(2), true, false)
                } else {
                    cat(vec![SplitWith(1), Nest(vec![t(2)])])
                },
                Newline,
            ]),
            SyntaxKind::ReturnStatement | SyntaxKind::ThrowStatement => cat(vec![
                t(0),
                present(
                    c(1),
                    cat(vec![
                        Space,
                        SplitWith(if self.env.version(1) { 1 } else { 2 }),
                        Nest(vec![t(1)]),
                    ]),
                ),
                t(2),
                Newline,
            ]),
            SyntaxKind::YieldBreakStatement => cat(vec![t(0), Space, t(1), t(2)]),
            SyntaxKind::BreakStatement | SyntaxKind::ContinueStatement => {
                cat(vec![self.simple(n), Newline])
            }
            SyntaxKind::EchoStatement => cat(vec![
                t(0),
                if c(1).list().len() == 1
                    && unwrap_item(&c(1).list()[0]).kind == SyntaxKind::ParenthesizedExpression
                {
                    self.t(unwrap_item(&c(1).list()[0]))
                } else {
                    self.declarators(c(1))
                },
                t(2),
                Newline,
            ]),
            SyntaxKind::ConcurrentStatement => {
                cat(vec![t(0), Space, self.compound(c(1), false, true), Newline])
            }
            SyntaxKind::SimpleInitializer => {
                cat(vec![Space, t(0), Space, SplitWith(1), Nest(vec![t(1)])])
            }
            SyntaxKind::AnonymousFunction => cat(vec![
                self.attribute(c(0)),
                sp(0),
                t(1),
                sp(1),
                t(2),
                t(3),
                t(4),
                present(c(5), Split),
                self.fn_args(c(5), c(6)),
                t(7),
                t(8),
                sp(8),
                t(9),
                sp(9),
                t(10),
                t(11),
                self.compound(c(12), true, false),
            ]),
            SyntaxKind::AnonymousFunctionUseClause => cat(vec![
                Space,
                t(0),
                Space,
                self.argish(c(1), c(2), c(3), true, false, false),
            ]),
            SyntaxKind::ConstructorPattern => cat(vec![
                t(0),
                self.argish(
                    c(1),
                    c(2),
                    c(3),
                    true,
                    c(1).trailing_newline() && !c(2).missing(),
                    false,
                ),
            ]),
            SyntaxKind::RefinementPattern => cat(vec![t(0), t(1), Space, t(2)]),
            SyntaxKind::LambdaExpression => cat(vec![
                self.attribute(c(0)),
                sp(0),
                t(1),
                sp(1),
                t(2),
                Space,
                t(3),
                self.lambda_body(c(4)),
            ]),
            SyntaxKind::LambdaSignature => cat(vec![
                t(0),
                t(1),
                t(2),
                present(c(3), Split),
                self.fn_args(c(3), c(4)),
                t(5),
                t(6),
                sp(6),
                t(7),
                sp(7),
                t(8),
            ]),
            SyntaxKind::CastExpression => Span(n.children.iter().map(|n| self.t(n)).collect()),
            SyntaxKind::MemberSelectionExpression
            | SyntaxKind::SafeMemberSelectionExpression
            | SyntaxKind::FunctionCallExpression => self.chain(n),
            SyntaxKind::YieldExpression => cat(vec![t(0), Space, SplitWith(1), Nest(vec![t(1)])]),
            SyntaxKind::PrefixUnaryExpression => cat(vec![
                t(0),
                if matches!(c(0).token_text(), "await" | "delay" | "readonly" | "clone")
                    || c(0).is_token("print") && c(1).kind != SyntaxKind::ParenthesizedExpression
                {
                    Space
                } else {
                    Nil
                },
                t(1),
            ]),
            SyntaxKind::BinaryExpression => self.binary(c(0), c(1), c(2), false),
            SyntaxKind::IsExpression
            | SyntaxKind::AsExpression
            | SyntaxKind::NullableAsExpression
            | SyntaxKind::UpcastExpression => cat(vec![
                t(0),
                Space,
                SplitWith(1),
                Nest(vec![t(1), Space, t(2)]),
            ]),
            SyntaxKind::ConditionalExpression => lazy(
                Parental,
                t(0),
                Nest(vec![
                    Space,
                    Split,
                    t(1),
                    present(
                        c(2),
                        cat(vec![
                            Space,
                            if self.env.indent_width == 2 {
                                Nest(vec![t(2)])
                            } else {
                                t(2)
                            },
                            Space,
                            Split,
                        ]),
                    ),
                    t(3),
                    Space,
                    if !c(2).missing() && self.env.indent_width == 2 {
                        Nest(vec![t(4)])
                    } else {
                        t(4)
                    },
                ]),
            ),
            SyntaxKind::FunctionPointerExpression => self.simple(n),
            SyntaxKind::EvalExpression => cat(vec![t(0), self.braced(c(1), c(2), c(3))]),
            SyntaxKind::IssetExpression => cat(vec![
                t(0),
                self.argish(c(1), c(2), c(3), false, false, false),
            ]),
            SyntaxKind::ParenthesizedExpression => self.condition(c(0), c(1), c(2)),
            SyntaxKind::NameofExpression | SyntaxKind::ObjectCreationExpression => {
                cat(vec![t(0), Space, t(1)])
            }
            SyntaxKind::ETSpliceExpression => cat(vec![t(0), self.condition(c(1), c(2), c(3))]),
            SyntaxKind::BracedExpression => cat(vec![
                t(0),
                Split,
                rule(
                    if c(0).trailing().is_empty() && c(1).trailing().is_empty() {
                        Simple(1)
                    } else {
                        Parental
                    },
                    cat(vec![Nest(vec![t(1)]), Split, t(2)]),
                ),
            ]),
            SyntaxKind::EmbeddedBracedExpression => cat(vec![t(0), Nest(vec![t(1)]), t(2)]),
            SyntaxKind::ListExpression | SyntaxKind::ConstructorCall => cat(vec![
                t(0),
                self.argish(c(1), c(2), c(3), true, false, false),
            ]),
            SyntaxKind::CollectionLiteralExpression => cat(vec![
                t(0),
                Space,
                self.argish(
                    c(1),
                    c(2),
                    c(3),
                    true,
                    c(1).trailing_newline() && !c(2).missing(),
                    false,
                ),
            ]),
            SyntaxKind::AnonymousClass => {
                let ancestors = |kw: usize, list: usize| {
                    present(
                        c(kw),
                        cat(vec![
                            Space,
                            Split,
                            rule(
                                Parental,
                                Nest(vec![Span(vec![
                                    t(kw),
                                    Space,
                                    Split,
                                    rule(
                                        Parental,
                                        Nest(vec![self.list(c(list), Nil, space_split(), Nil)]),
                                    ),
                                ])]),
                            ),
                        ]),
                    )
                };
                cat(vec![
                    t(0),
                    self.argish(c(1), c(2), c(3), true, false, false),
                    ancestors(4, 5),
                    ancestors(6, 7),
                    t(8),
                ])
            }
            SyntaxKind::DarrayIntrinsicExpression
            | SyntaxKind::DictionaryIntrinsicExpression
            | SyntaxKind::KeysetIntrinsicExpression
            | SyntaxKind::VarrayIntrinsicExpression
            | SyntaxKind::VectorIntrinsicExpression => cat(vec![
                t(0),
                t(1),
                self.argish(
                    c(2),
                    c(3),
                    c(4),
                    true,
                    c(2).trailing_newline() && !c(3).missing(),
                    false,
                ),
            ]),
            SyntaxKind::ElementInitializer => cat(vec![
                t(0),
                Space,
                t(1),
                Space,
                SplitWith(1),
                Nest(vec![t(2)]),
            ]),
            SyntaxKind::SubscriptExpression => cat(vec![t(0), self.braced(c(1), c(2), c(3))]),
            SyntaxKind::AwaitableCreationExpression => cat(vec![
                self.attribute(c(0)),
                sp(0),
                t(1),
                sp(1),
                self.compound(c(2), false, false),
            ]),
            SyntaxKind::XHPChildrenDeclaration => cat(vec![t(0), Space, t(1), t(2), Newline]),
            SyntaxKind::XHPChildrenParenthesizedList => {
                self.argish(c(0), c(1), c(2), false, false, false)
            }
            SyntaxKind::XHPCategoryDeclaration => cat(vec![
                t(0),
                rule(
                    Parental,
                    Nest(vec![self.list(c(1), space_split(), Nil, Nil)]),
                ),
                t(2),
                Newline,
            ]),
            SyntaxKind::XHPEnumType => cat(vec![
                t(0),
                t(1),
                Space,
                self.argish(c(2), c(3), c(4), true, false, false),
            ]),
            SyntaxKind::XHPClassAttributeDeclaration => cat(vec![
                t(0),
                match c(1).list() {
                    [] => Nil,
                    [x] => rule(Parental, Nest(vec![Space, Split, self.t(x)])),
                    _ => Nest(vec![self.list(c(1), Newline, Nil, Nil)]),
                },
                t(2),
                Newline,
            ]),
            SyntaxKind::XHPClassAttribute => cat(vec![t(0), Space, t(1), sp(2), t(2), sp(3), t(3)]),
            SyntaxKind::XHPSimpleAttribute => {
                Span(vec![t(0), t(1), SplitWith(1), Nest(vec![t(2)])])
            }
            SyntaxKind::XHPSpreadAttribute => {
                Span(vec![t(0), t(1), SplitWith(1), Nest(vec![t(2)]), t(3)])
            }
            SyntaxKind::XHPOpen => {
                let end = cat(vec![
                    if c(3).is_token("/>") {
                        cat(vec![Space, present(c(2), Split)])
                    } else {
                        Nil
                    },
                    t(3),
                ]);
                cat(vec![
                    t(0),
                    t(1),
                    if c(2).missing() {
                        end
                    } else {
                        cat(vec![
                            Space,
                            Split,
                            rule(
                                Parental,
                                cat(vec![
                                    Nest(vec![self.list(c(2), Nil, space_split(), Nil)]),
                                    end,
                                ]),
                            ),
                        ])
                    },
                ])
            }
            SyntaxKind::XHPExpression => {
                let body = if c(1).missing() {
                    present(c(2), Split)
                } else {
                    let mut prev = false;
                    let mut ds = vec![];
                    for node in c(1).list() {
                        let is_body = node
                            .token
                            .as_ref()
                            .is_some_and(|t| t.kind == TokenKind::XHPBody);
                        let (leading, node) = node.without_leading();
                        let preserved = |tr: &[Trivia]| {
                            if is_body {
                                if has_nl(tr) {
                                    Newline
                                } else if has_ws(tr) {
                                    Space
                                } else {
                                    Nil
                                }
                            } else {
                                Nil
                            }
                        };
                        let lead = if prev {
                            trivia(&leading, true)
                        } else {
                            let cut = leading
                                .iter()
                                .position(|t| t.kind == TriviaKind::EndOfLine)
                                .map_or(leading.len(), |i| i + 1);
                            cat(vec![
                                ignores(&leading[..cut]),
                                invisibles(
                                    &leading[cut..].iter().cloned().rev().collect::<Vec<_>>(),
                                ),
                            ])
                        };
                        ds.push(cat(vec![
                            if !prev && !is_body { Split } else { Nil },
                            lead,
                            preserved(&leading),
                            self.t(&node),
                            preserved(node.trailing()),
                        ]));
                        prev = is_body;
                    }
                    ds.push(if prev {
                        Nil
                    } else if has_nl(c(1).leading()) {
                        Newline
                    } else {
                        Split
                    });
                    cat(ds)
                };
                let close = if c(2).missing() {
                    Nil
                } else {
                    let (tr, close) = c(2).without_leading();
                    cat(vec![ignores(&tr), self.t(&close)])
                };
                Override(Box::new(cat(vec![t(0), Nest(vec![body]), close])))
            }
            SyntaxKind::VarrayTypeSpecifier
            | SyntaxKind::VectorTypeSpecifier
            | SyntaxKind::KeysetTypeSpecifier => {
                let contents = self.comma(c(2), c(3), false);
                let clean = c(1).trailing().is_empty()
                    && c(2).leading().is_empty()
                    && c(2).trailing().is_empty()
                    && c(3).leading().is_empty()
                    && c(3).trailing().is_empty()
                    && c(4).leading().is_empty();
                cat(vec![
                    t(0),
                    if clean && !self.bad_braces(c(2)) {
                        cat(vec![t(1), contents, t(4)])
                    } else {
                        self.delimited(c(1), c(4), contents, true, false)
                    },
                ])
            }
            SyntaxKind::TypeParameter => cat(vec![
                self.attribute(c(0)),
                sp(0),
                t(1),
                sp(1),
                t(2),
                t(3),
                sp(4),
                self.list(c(4), Nil, Space, Nil),
            ]),
            SyntaxKind::TypeConstraint | SyntaxKind::PackageExpression => {
                cat(vec![t(0), Space, t(1)])
            }
            SyntaxKind::ContextConstraint => cat(vec![Space, t(0), Space, t(1)]),
            SyntaxKind::DarrayTypeSpecifier => {
                let args = Node {
                    kind: SyntaxKind::SyntaxList,
                    children: vec![
                        Node {
                            kind: SyntaxKind::ListItem,
                            children: vec![c(2).clone(), c(3).clone()],
                            token: None,
                        },
                        Node {
                            kind: SyntaxKind::ListItem,
                            children: vec![c(4).clone(), c(5).clone()],
                            token: None,
                        },
                    ],
                    token: None,
                };
                cat(vec![
                    t(0),
                    self.argish(c(1), &args, c(6), true, false, false),
                ])
            }
            SyntaxKind::DictionaryTypeSpecifier | SyntaxKind::TupleTypeExplicitSpecifier => {
                cat(vec![
                    t(0),
                    self.argish(c(1), c(2), c(3), true, false, false),
                ])
            }
            SyntaxKind::ClosureTypeSpecifier => cat(vec![
                t(0),
                t(1),
                sp(1),
                t(2),
                t(3),
                t(4),
                present(c(5), Split),
                self.fn_args(c(5), c(6)),
                t(7),
                t(8),
                sp(8),
                t(9),
                sp(9),
                t(10),
                t(11),
            ]),
            SyntaxKind::ClosureParameterTypeSpecifier => cat(vec![
                t(0),
                sp(0),
                t(1),
                sp(1),
                t(2),
                sp(2),
                t(3),
                sp(3),
                t(4),
                t(5),
                sp(6),
                t(6),
                t(7),
            ]),
            SyntaxKind::TupleOrUnionOrIntersectionElementTypeSpecifier => {
                cat(vec![t(0), sp(0), t(1), t(2), t(3)])
            }
            SyntaxKind::ClassPtrTypeSpecifier | SyntaxKind::ClassnameTypeSpecifier => {
                let contents = self.comma(c(2), c(3), false);
                let clean = c(1).trailing().is_empty()
                    && c(2).leading().is_empty()
                    && c(2).trailing().is_empty()
                    && c(3).leading().is_empty()
                    && c(3).trailing().is_empty()
                    && c(4).leading().is_empty();
                cat(vec![
                    t(0),
                    if clean && !self.bad_braces(c(2)) {
                        cat(vec![t(1), contents, t(4)])
                    } else {
                        self.delimited(c(1), c(4), contents, true, false)
                    },
                ])
            }
            SyntaxKind::FieldSpecifier => cat(vec![
                t(0),
                t(1),
                Space,
                t(2),
                Space,
                SplitWith(1),
                Nest(vec![t(3)]),
            ]),
            SyntaxKind::FieldInitializer => cat(vec![
                t(0),
                Space,
                t(1),
                Space,
                SplitWith(1),
                Nest(vec![t(2)]),
            ]),
            SyntaxKind::AbsentFieldSpecifier => cat(vec![t(0), Space, t(1)]),
            SyntaxKind::ShapeTypeSpecifier => {
                let has_ellipsis = !c(3).missing() || !c(4).missing();
                let force = c(1).trailing_newline() && (!c(2).missing() || has_ellipsis);
                let contents = if has_ellipsis {
                    // The typed ellipsis is a trailing item, not a field:
                    // preserve the preceding comma and never add one after it.
                    let mut fields = c(2)
                        .list()
                        .iter()
                        .map(|field| cat(vec![self.arg_item(field), space_split()]))
                        .collect::<Vec<_>>();
                    fields.extend([t(3), t(4)]);
                    self.delimited(c(1), c(5), cat(fields), true, force)
                } else {
                    self.argish(c(1), c(2), c(5), true, force, false)
                };
                cat(vec![t(0), contents])
            }
            SyntaxKind::ShapeExpression | SyntaxKind::TupleExpression => {
                let mut fields = Node {
                    kind: SyntaxKind::SyntaxList,
                    children: c(2).list().to_vec(),
                    token: None,
                };
                if !c(3).missing() {
                    fields.children.push(Node {
                        kind: SyntaxKind::ListItem,
                        children: vec![
                            c(3).clone(),
                            Node {
                                kind: SyntaxKind::Missing,
                                children: vec![],
                                token: None,
                            },
                        ],
                        token: None,
                    });
                }
                cat(vec![
                    t(0),
                    self.argish(
                        c(1),
                        &fields,
                        c(4),
                        c(3).missing(),
                        n.kind == SyntaxKind::ShapeExpression
                            && c(1).trailing_newline()
                            && !fields.children.is_empty(),
                        false,
                    ),
                ])
            }
            SyntaxKind::TypeArguments
            | SyntaxKind::TypeParameters
            | SyntaxKind::TupleTypeSpecifier => self.argish(c(0), c(1), c(2), true, false, false),
            SyntaxKind::UnionTypeSpecifier | SyntaxKind::IntersectionTypeSpecifier => {
                let xs = c(1).list();
                let ds = xs
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        cat(vec![
                            if x.kind == SyntaxKind::ListItem {
                                cat(vec![self.t(x.c(0)), present(x.c(1), Space), self.t(x.c(1))])
                            } else {
                                self.t(x)
                            },
                            if i + 1 == xs.len() {
                                Split
                            } else {
                                space_split()
                            },
                        ])
                    })
                    .collect();
                self.delimited(c(0), c(2), cat(ds), true, false)
            }
            SyntaxKind::PrefixedCodeExpression => cat(vec![
                t(0),
                if c(2).kind == SyntaxKind::CompoundStatement {
                    cat(vec![
                        t(1),
                        self.block(c(2).c(0), c(2).c(2), self.all(c(2).c(1)), true),
                        t(3),
                    ])
                } else {
                    self.braced(c(1), c(2), c(3))
                },
            ]),
            SyntaxKind::DecoratedExpression => cat(vec![
                t(0),
                if c(0).is_token("inout") { Space } else { Nil },
                t(1),
            ]),
            SyntaxKind::NamedArgument | SyntaxKind::EnumClassLabelExpression => self.simple(n),
            SyntaxKind::EnumClassDeclaration => cat(vec![
                t(0),
                nl(0),
                mods(1),
                t(2),
                Space,
                t(3),
                Space,
                t(4),
                t(5),
                Space,
                SplitWith(1),
                Nest(vec![Space, t(6), Space]),
                present(
                    c(7),
                    Nest(vec![
                        Space,
                        Split,
                        t(7),
                        rule(
                            Parental,
                            Nest(vec![Span(vec![
                                Space,
                                if c(8).list().len() == 1 {
                                    SplitWith(1)
                                } else {
                                    Split
                                },
                                Nest(vec![self.list(c(8), Nil, space_split(), Nil)]),
                            ])]),
                        ),
                    ]),
                ),
                Space,
                self.block(c(9), c(11), all(10), true),
                Newline,
            ]),
            SyntaxKind::EnumClassEnumerator => {
                cat(vec![mods(0), t(1), sp(1), t(2), t(3), t(4), Newline])
            }
            SyntaxKind::ModuleDeclaration => cat(vec![
                t(0),
                nl(0),
                t(1),
                Space,
                t(2),
                Space,
                t(3),
                Space,
                self.block(c(4), c(5), Nil, true),
                Newline,
            ]),
            SyntaxKind::ModuleMembershipDeclaration => cat(vec![t(0), Space, t(1), t(2), Newline]),
            _ => panic!("unhandled formatter syntax {:?}", n.kind),
        }
    }
}
