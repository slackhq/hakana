//! Expressions.
//!
//! The control flow mirrors HHVM's `expression_parser.rs`: a term followed by
//! a "remaining expression" loop driven by operator precedence, including its
//! special cases (assignments binding tighter than their precedence suggests
//! when the left side is an lvalue, `<` starting generic arguments after a
//! name, ...). Keeping the same structure is what makes the resulting trees
//! agree with HHVM's on unusual operator mixes.

use super::{LoweringState, Parser, R};
use crate::ast::*;
use crate::checks::{Feature, LvalRoot, Nesting, ParamCtx};
use crate::cst::SyntaxKind as K;
use crate::span::Span;
use crate::token::T;

/// The syntactic shape of a term, where it affects parsing decisions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Shape {
    Other,
    /// A (possibly qualified) name.
    Name,
    Variable,
    Subscript,
    /// `a->b` / `a?->b`
    Member,
    /// `A::b`
    Scope,
    List,
    New,
    /// A token that is no expression, taken for one (see `repair.rs`).
    Token,
}

/// An expression plus the extent of its syntax, which unlike `e.span`
/// includes enclosing parentheses.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Term<'a> {
    pub e: Expr<'a>,
    pub start: u32,
    pub end: u32,
    pub shape: Shape,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Assoc {
    Left,
    Right,
    None,
}

const RIGHT_PAREN_EXPECTED: &str = "A right parenthesis `)` is expected here.";

const TOO_MANY_TYPE_ARGUMENTS: &str =
    "Collection expression must have less than three type arguments";

const PREC_ASSIGNMENT_IN_EXPRESSIONS: u8 = 25;
const PREC_CONDITIONAL: u8 = 8;
const PREC_CAST: u8 = 22;
const PREC_NEW: u8 = 27;
const PREC_CALL: u8 = 26;
const PREC_PACKAGE: u8 = 32;

/// Precedence and associativity of a token that can follow a term.
fn trailing_op(kind: T) -> Option<(u8, Assoc)> {
    Some(match kind {
        T::Eq
        | T::PlusEq
        | T::MinusEq
        | T::StarEq
        | T::SlashEq
        | T::StarStarEq
        | T::DotEq
        | T::PercentEq
        | T::AmpEq
        | T::BarEq
        | T::CaretEq
        | T::LtLtEq
        | T::GtGtEq
        | T::QQEq => (6, Assoc::Right),
        T::BarGt | T::BarQGt => (7, Assoc::Left),
        T::Question | T::QuestionColon => (8, Assoc::Left),
        T::QQ => (9, Assoc::Right),
        T::BarBar => (10, Assoc::Left),
        T::AmpAmp => (11, Assoc::Left),
        T::Bar => (12, Assoc::Left),
        T::Caret => (13, Assoc::Left),
        T::Amp => (14, Assoc::Left),
        T::EqEq | T::EqEqEq | T::BangEq | T::BangEqEq => (15, Assoc::None),
        T::Spaceship | T::Lt | T::LtEq | T::Gt | T::GtEq => (16, Assoc::None),
        T::LtLt | T::GtGt => (17, Assoc::Left),
        T::Plus | T::Minus | T::Dot => (18, Assoc::Left),
        T::Star | T::Slash | T::Percent => (19, Assoc::Left),
        T::Instanceof => (21, Assoc::None),
        T::Is | T::As | T::QuestionAs | T::Upcast => (21, Assoc::Left),
        T::StarStar => (22, Assoc::Right),
        T::PlusPlus | T::MinusMinus => (23, Assoc::Right),
        T::LParen | T::Hash => (PREC_CALL, Assoc::Left),
        T::Arrow | T::QArrow => (28, Assoc::Left),
        T::LBracket => (29, Assoc::Left),
        T::ColonColon => (30, Assoc::Left),
        _ => return None,
    })
}

pub(super) fn is_assignment_op(kind: T) -> bool {
    matches!(
        kind,
        T::Eq
            | T::PlusEq
            | T::MinusEq
            | T::StarEq
            | T::SlashEq
            | T::StarStarEq
            | T::DotEq
            | T::PercentEq
            | T::AmpEq
            | T::BarEq
            | T::CaretEq
            | T::LtLtEq
            | T::GtGtEq
            | T::QQEq
    )
}

fn binary_op(kind: T) -> Option<Bop> {
    Some(match kind {
        T::Plus | T::PlusEq => Bop::Plus,
        T::Minus | T::MinusEq => Bop::Minus,
        T::Star | T::StarEq => Bop::Star,
        T::Slash | T::SlashEq => Bop::Slash,
        T::StarStar | T::StarStarEq => Bop::StarStar,
        T::Dot | T::DotEq => Bop::Dot,
        T::Percent | T::PercentEq => Bop::Percent,
        T::Amp | T::AmpEq => Bop::Amp,
        T::Bar | T::BarEq => Bop::Bar,
        T::Caret | T::CaretEq => Bop::Xor,
        T::LtLt | T::LtLtEq => Bop::LtLt,
        T::GtGt | T::GtGtEq => Bop::GtGt,
        T::QQ | T::QQEq => Bop::QuestionQuestion,
        T::EqEq => Bop::EqEq,
        T::EqEqEq => Bop::EqEqEq,
        T::BangEq => Bop::Diff,
        T::BangEqEq => Bop::Diff2,
        T::Lt => Bop::Lt,
        T::LtEq => Bop::Lte,
        T::Gt => Bop::Gt,
        T::GtEq => Bop::Gte,
        T::Spaceship => Bop::Cmp,
        T::AmpAmp => Bop::AmpAmp,
        T::BarBar => Bop::BarBar,
        _ => return None,
    })
}

enum Prefix<'a> {
    None,
    Assignment,
    /// Generic arguments were parsed after the term.
    TypeArgs(CallTypeArgs<'a>),
}

/// Generic arguments written after a term (`foo<int>`).
#[derive(Clone, Copy)]
struct CallTypeArgs<'a> {
    hints: &'a [Hint<'a>],
    /// Where the `<` is.
    start: u32,
    /// What HHVM's translation to the AST had reported before they were
    /// parsed.
    before: LoweringState,
}

enum Step<'a> {
    Done(Term<'a>),
    Binary(Term<'a>, bool),
    Reparse(Term<'a>),
}

impl<'a> Parser<'a> {
    // ------------------------------------------------------------------
    // Entry points

    /// Parses an expression in a fresh context (statement, declaration or
    /// type position).
    pub(crate) fn parse_expr(&mut self) -> R<Expr<'a>> {
        Ok(self.parse_expr_term()?.e)
    }

    pub(crate) fn parse_expr_term(&mut self) -> R<Term<'a>> {
        let (old_prec, old_as) = (self.precedence, self.allow_as);
        self.precedence = 0;
        self.allow_as = true;
        let r = self.parse_expression();
        self.precedence = old_prec;
        self.allow_as = old_as;
        r
    }

    /// Like [`parse_expr`](Self::parse_expr) but with `as` disabled, for the
    /// collection of a `foreach`.
    pub(crate) fn parse_expr_no_as(&mut self) -> R<Expr<'a>> {
        let (old_prec, old_as) = (self.precedence, self.allow_as);
        self.precedence = 0;
        self.allow_as = false;
        let r = self.parse_expression();
        self.precedence = old_prec;
        self.allow_as = old_as;
        Ok(r?.e)
    }

    fn parse_expression(&mut self) -> R<Term<'a>> {
        self.enter()?;
        let term = self.parse_term()?;
        let r = self.parse_remaining_expression(term)?;
        self.leave();
        Ok(r)
    }

    /// Nested expression with the precedence reset (inside brackets etc.).
    pub(super) fn expr_reset(&mut self) -> R<Term<'a>> {
        let old = self.precedence;
        self.precedence = 0;
        let r = self.parse_expression();
        self.precedence = old;
        r
    }

    fn expr_with_precedence(&mut self, precedence: u8) -> R<Term<'a>> {
        let old = self.precedence;
        self.precedence = precedence;
        let r = self.parse_expression();
        self.precedence = old;
        r
    }

    #[inline]
    pub(super) fn term(&self, start: u32, kind: ExprKind<'a>, shape: Shape) -> Term<'a> {
        // Nothing may have been consumed since `start` (see `repair.rs`).
        let end = self.prev_end.max(start);
        Term {
            e: Expr {
                span: Span::new(start, end),
                kind,
            },
            start,
            end,
            shape,
        }
    }

    // ------------------------------------------------------------------
    // Terms

    fn parse_term(&mut self) -> R<Term<'a>> {
        self.retokenize_xhp_class_name();
        let start = self.tok.start;
        match self.tok.kind {
            T::Int => {
                let tok = self.take();
                // Long enough to overflow, or possibly octal.
                let len = tok.end - tok.start;
                if len > 17 || (len > 1 && self.bytes[tok.start as usize] == b'0') {
                    self.check_int_literal(tok);
                }
                self.c_node(K::LiteralExpression);
                Ok(self.term(start, ExprKind::Int(self.tok_text(tok)), Shape::Other))
            }
            T::Float => {
                let tok = self.take();
                self.c_node(K::LiteralExpression);
                Ok(self.term(start, ExprKind::Float(self.tok_text(tok)), Shape::Other))
            }
            T::SingleQuoted => {
                let tok = self.take();
                self.c_node(K::LiteralExpression);
                let end = self.single_quoted_end(tok);
                let value = self.unescape_single(tok.start + 1, end)?;
                Ok(self.term(start, ExprKind::String(value), Shape::Other))
            }
            T::DoubleQuoted => {
                let e = self.parse_double_quoted()?;
                Ok(Term {
                    e,
                    start,
                    end: self.prev_end,
                    shape: Shape::Other,
                })
            }
            T::Heredoc | T::Nowdoc => {
                let e = self.parse_heredoc()?;
                Ok(Term {
                    e,
                    start,
                    end: self.prev_end,
                    shape: Shape::Other,
                })
            }
            T::True => {
                self.bump();
                self.c_node(K::LiteralExpression);
                Ok(self.term(start, ExprKind::True, Shape::Other))
            }
            T::False => {
                self.bump();
                self.c_node(K::LiteralExpression);
                Ok(self.term(start, ExprKind::False, Shape::Other))
            }
            T::Null => {
                self.bump();
                self.c_node(K::LiteralExpression);
                Ok(self.term(start, ExprKind::Null, Shape::Other))
            }
            T::Variable => {
                if self.peek_kind() == T::LambdaArrow {
                    self.c_missing();
                    self.parse_lambda(start, &[], None)
                } else {
                    let tok = self.take();
                    self.c_node(K::VariableExpression);
                    self.note_variable(tok);
                    Ok(self.term(start, ExprKind::Lvar(self.tok_text(tok)), Shape::Variable))
                }
            }
            T::DollarDollar => {
                let tok = self.take();
                self.c_node(K::PipeVariableExpression);
                if !self.pipe_ok {
                    self.interest = true;
                }
                Ok(self.term(start, ExprKind::Lvar(self.tok_text(tok)), Shape::Other))
            }
            T::XhpClassName => {
                let tok = self.take();
                let name = self.id(tok);
                self.finish_name_term(name)
            }
            T::Name | T::Backslash | T::Namespace => {
                if self.tok.mixed_case_kw {
                    if self.static_as_name() {
                        let tok = self.take();
                        return Ok(self.term(
                            start,
                            ExprKind::Id(self.tok_text(tok)),
                            Shape::Other,
                        ));
                    }
                    self.lowercase_keyword();
                    return self.parse_term();
                }
                if self.tok.mixed_case_soft {
                    // `Self::` / `Parent::` are read as the keywords.
                    let text = self.tok_text(self.tok);
                    if (text.eq_ignore_ascii_case("self") || text.eq_ignore_ascii_case("parent"))
                        && self.peek_kind() == T::ColonColon
                    {
                        self.lowercase_keyword();
                        return self.parse_term();
                    }
                }
                let is_plain_name = self.at(T::Name);
                let (name, _) = self.parse_qualified_name()?;
                if is_plain_name && matches!(self.tok.kind, T::SingleQuoted | T::DoubleQuoted) {
                    return self.parse_prefixed_string(name);
                }
                if is_plain_name && matches!(self.tok.kind, T::Heredoc | T::Nowdoc) {
                    // In `f<<<__Soft>> T>()` what looks like the start of a
                    // heredoc opens type arguments, the first of which has
                    // an attribute.
                    let snap = self.snapshot();
                    let term = self.name_term(name);
                    self.at_type_lt();
                    return match self.try_parse_call_type_args(&term) {
                        Some(targs) => self.finish_call_with_type_args(term, targs),
                        None => {
                            // HHVM says so and carries on with the name.
                            self.restore(snap);
                            self.report_missing("Only double-quoted strings may be prefixed.");
                            self.finish_name_term(name)
                        }
                    };
                }
                self.finish_name_term(name)
            }
            T::SelfKw | T::Parent | T::Static => {
                // `static` may also start `static function` in old code; that
                // form is long gone, so treat everything as a name.
                let tok = self.take();
                // The keyword only in front of `::`.
                if !self.at(T::ColonColon) {
                    self.c_name();
                }
                Ok(self.term(start, ExprKind::Id(self.tok_text(tok)), Shape::Other))
            }
            T::Yield => self.parse_yield(),
            T::Bang
            | T::PlusPlus
            | T::MinusMinus
            | T::Tilde
            | T::Minus
            | T::Plus
            | T::Await
            | T::Delay
            | T::Readonly
            | T::Clone
            | T::Print => self.parse_prefix_unary(),
            T::At => {
                // The error suppression operator is gone; the operand is
                // still an expression.
                let at = self.take();
                self.report_lowering(
                    at.start,
                    at.end,
                    "The error suppression operator `@` is not allowed",
                );
                let operand = self.expr_with_precedence(PREC_CAST)?;
                self.c_node(K::PrefixUnaryExpression);
                Ok(Term {
                    start,
                    shape: Shape::Other,
                    ..operand
                })
            }
            T::Dollar => {
                if self.peek_kind() == T::LBrace {
                    return self.parse_et_splice();
                }
                // `$$x`: variable variables are not Hack.
                let operand = self.parse_dollar_operator(false)?;
                Ok(self.term(start, ExprKind::Dollar(operand), Shape::Variable))
            }
            T::LParen => self.parse_cast_or_parenthesized_or_lambda(),
            T::Lt => self.parse_xhp_term(),
            T::List => self.parse_list(),
            T::New => self.parse_new(),
            T::Varray | T::Vec | T::Darray | T::Dict | T::Keyset => {
                self.parse_collection_intrinsic()
            }
            T::Tuple => self.parse_tuple(),
            T::Shape => self.parse_shape(),
            T::Function => {
                self.c_missing();
                self.parse_anon_or_lambda(start, &[], self.tok)
            }
            T::LtLt | T::Async => {
                let first = self.tok;
                let has_attrs = self.at(T::LtLt);
                let before_attrs = has_attrs.then(|| self.snapshot());
                let attrs = if has_attrs {
                    self.parse_attribute_spec()?
                } else {
                    &[]
                };
                let after_async = if self.at(T::Async) {
                    self.peek_kind()
                } else {
                    self.tok.kind
                };
                if !has_attrs
                    && matches!(
                        after_async,
                        T::Function | T::LBrace | T::Variable | T::LParen
                    )
                {
                    self.c_missing();
                }
                match after_async {
                    T::Function => self.parse_anon_or_lambda(start, attrs, first),
                    T::LBrace => self.parse_async_block(start, attrs),
                    T::Variable | T::LParen => self.parse_lambda(start, attrs, None),
                    _ => {
                        if let Some(before_attrs) = before_attrs {
                            // No function for the attributes to be on:
                            // HHVM goes back and takes the `<<` on its own
                            // for the expression.
                            self.restore(before_attrs);
                            return Ok(self.token_term());
                        }
                        if !self.at(T::Async) {
                            return self.fail("expected function after attributes");
                        }
                        // `async` used as a plain name.
                        let tok = self.take();
                        self.c_name();
                        Ok(self.term(start, ExprKind::Id(self.tok_text(tok)), Shape::Name))
                    }
                }
            }
            T::Include | T::IncludeOnce | T::Require | T::RequireOnce => {
                let flavor = match self.tok.kind {
                    T::Include => ImportFlavor::Include,
                    T::IncludeOnce => ImportFlavor::IncludeOnce,
                    T::Require => ImportFlavor::Require,
                    _ => ImportFlavor::RequireOnce,
                };
                self.bump();
                let operand = self.expr_with_precedence(1)?;
                self.c_node(K::InclusionExpression);
                Ok(self.term(
                    start,
                    ExprKind::Import(flavor, self.alloc(operand.e)),
                    Shape::Other,
                ))
            }
            T::Isset | T::Eval => {
                if self.peek_kind() != T::LParen {
                    return self.fail("expected `(`");
                }
                let tok = self.take();
                let func = Expr {
                    span: Self::tok_span(tok),
                    kind: ExprKind::Id(self.tok_text(tok)),
                };
                let (args, unpacked) = if tok.kind == T::Eval {
                    // One expression in parentheses, not a list.
                    self.require(T::LParen);
                    let argument = self.expr_reset()?.e;
                    self.require(T::RParen);
                    self.c_node(K::EvalExpression);
                    let args: &[Expr<'a>] = if self.absent(&argument) {
                        &[]
                    } else {
                        self.arena.alloc_slice_copy(&[argument])
                    };
                    (args, None)
                } else {
                    let arguments = self.parse_arguments()?;
                    self.c_node(K::IssetExpression);
                    arguments
                };
                let call = Call {
                    func,
                    targs: &[],
                    args,
                    unpacked,
                };
                Ok(self.term(start, ExprKind::Call(self.alloc(call)), Shape::Other))
            }
            T::Hash => {
                // No qualifier.
                self.c_missing();
                self.bump();
                let label = self.expect_name_allow_keywords()?;
                self.c_node(K::EnumClassLabelExpression);
                let label = EnumClassLabel {
                    class: None,
                    label: label.name,
                };
                Ok(self.term(
                    start,
                    ExprKind::EnumClassLabel(self.alloc(label)),
                    Shape::Other,
                ))
            }
            T::Nameof => {
                self.bump();
                let yields = self.yields.len();
                let operand = self.expr_with_precedence(24)?;
                self.c_node(K::NameofExpression);
                // A name in parentheses is no name.
                let bare = operand.start == operand.e.span.start;
                match operand.e.kind {
                    ExprKind::Id(name)
                        if name != "string" && operand.shape != Shape::Scope && bare => {}
                    kind => {
                        // HHVM gives up on the declaration here, before it
                        // gets to what `nameof` is applied to.
                        self.yields.truncate(yields);
                        let string = matches!(kind, ExprKind::Id("string"))
                            && operand.shape != Shape::Scope
                            && bare;
                        self.abort_lowering(
                            operand.start,
                            operand.end,
                            if string {
                                "`nameof` cannot be used with string"
                            } else {
                                "`nameof` can only be used with a class"
                            },
                        );
                    }
                }
                Ok(self.term(start, ExprKind::Nameof(self.alloc(operand.e)), Shape::Other))
            }
            T::Package => self.parse_package(),
            T::Empty => {
                // HHVM says so in front of the keyword and takes it for a
                // name.
                self.report_missing("The `empty()` expression has been removed from Hack.");
                Ok(self.token_term())
            }
            T::Backtick => {
                // A literal without the name of its DSL.
                self.c_missing();
                self.parse_expression_tree(start, None)
            }
            kind if kind.is_keyword() && !kind.is_reserved() => {
                // Keywords HHVM does not reserve are ordinary names here.
                let name_mark = self.c_mark();
                let (name, _) = self.parse_qualified_name()?;
                self.c_name_unless_reserved(name_mark);
                self.finish_name_term(name)
            }
            _ => Ok(self.parse_unexpected_term()),
        }
    }

    /// A name has just been consumed in term position.
    fn finish_name_term(&mut self, name: Id<'a>) -> R<Term<'a>> {
        let start = name.span.start;
        match self.tok.kind {
            T::LBrace => {
                self.c_node(K::SimpleTypeSpecifier);
                self.parse_collection_literal(name, &[])
            }
            T::Lt => {
                // `Vector<int> {..}`
                let targs = self.speculate(|p| {
                    let targs = p.parse_type_arguments()?;
                    if !p.at(T::LBrace) {
                        return p.fail("not a collection literal");
                    }
                    Ok(targs)
                });
                match targs {
                    Some(targs) => {
                        self.c_node(K::GenericTypeSpecifier);
                        self.parse_collection_literal(name, targs)
                    }
                    None => Ok(self.name_term(name)),
                }
            }
            // Inside a literal this is a name in front of the closing
            // backtick; elsewhere it names the DSL of a literal.
            T::Backtick if !self.in_expression_tree => {
                self.c_node(K::SimpleTypeSpecifier);
                self.parse_expression_tree(start, Some(name))
            }
            _ => Ok(self.name_term(name)),
        }
    }

    fn name_term(&self, name: Id<'a>) -> Term<'a> {
        Term {
            e: Expr {
                span: name.span,
                kind: ExprKind::Id(name.name),
            },
            start: name.span.start,
            end: name.span.end,
            shape: Shape::Name,
        }
    }

    fn parse_collection_literal(&mut self, name: Id<'a>, targs: &'a [Hint<'a>]) -> R<Term<'a>> {
        // HHVM says so before it gets to the elements, and then does
        // without type arguments.
        let (targs, too_many) = if targs.len() > 2 {
            (
                &[][..],
                self.begin_lowering_error(name.span.start, TOO_MANY_TYPE_ARGUMENTS),
            )
        } else {
            (targs, None)
        };
        self.require(T::LBrace);
        let mut fields = self.vec();
        let initializers = self.c_mark();
        while !self.at(T::RBrace) {
            let first = self.expr_reset()?.e;
            if self.eat(T::FatArrow) {
                let second = self.expr_reset()?.e;
                self.c_node(K::ElementInitializer);
                fields.push(Afield::KeyValue(first, second));
            } else {
                fields.push(Afield::Value(first));
            }
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(initializers);
        self.require(T::RBrace);
        self.c_node(K::CollectionLiteralExpression);
        self.end_lowering_error(too_many, self.prev_end);
        let lit = Collection {
            name,
            targs,
            fields: fields.into_bump_slice(),
        };
        self.check_collection_literal(name.span.start, &lit);
        Ok(self.term(
            name.span.start,
            ExprKind::Collection(self.alloc(lit)),
            Shape::Other,
        ))
    }

    fn parse_prefixed_string(&mut self, name: Id<'a>) -> R<Term<'a>> {
        let start = name.span.start;
        let expr = if self.at(T::SingleQuoted) {
            let tok = self.take();
            // HHVM finds out about the string by reading it without what
            // follows it.
            self.c_drop_trailing();
            self.no_trailing = self.prev_end;
            self.c_node(K::LiteralExpression);
            let end = self.single_quoted_end(tok);
            let value = self.unescape_single(tok.start + 1, end)?;
            Expr {
                span: Self::tok_span(tok),
                kind: ExprKind::String(value),
            }
        } else {
            let quote = self.tok.start;
            let expr = self.parse_double_quoted()?;
            if self.c_lexed_in_one_go(quote, expr.span.end) {
                self.c_drop_trailing();
                (self.prev_start, self.prev_end) = (quote, expr.span.end);
                self.no_trailing = self.prev_end;
            }
            expr
        };
        self.c_node(K::PrefixedStringExpression);
        if name.name != "re" {
            self.report_lowering(start, self.prev_end, "Only `re`-prefixed strings allowed.");
        }
        let node = PrefixedString {
            prefix: name.name,
            expr,
        };
        Ok(self.term(
            start,
            ExprKind::PrefixedString(self.alloc(node)),
            Shape::Other,
        ))
    }

    /// `package name` / `package default`: whether the package is deployed.
    fn parse_package(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        self.bump();
        let name = if self.at(T::Default) {
            let tok = self.take();
            self.id(tok)
        } else {
            // HHVM's translation to the AST takes the text of whatever
            // stands there for the name, without looking at it.
            let untranslated = self.lowering_state();
            let operand = self.expr_with_precedence(PREC_PACKAGE)?;
            self.restore_lowering_state(untranslated);
            match operand.e.kind {
                ExprKind::Id(name)
                    if operand.shape == Shape::Name
                        && !name.contains([':', '\\'])
                        && operand.start == operand.e.span.start =>
                {
                    Id {
                        span: operand.e.span,
                        name,
                    }
                }
                _ => {
                    // Anything but a plain name is an error to its parser.
                    self.report_missing("A name is expected here.");
                    let span = Span::new(operand.start, operand.end);
                    Id {
                        span,
                        name: span.text(self.src),
                    }
                }
            }
        };
        self.c_node(K::PackageExpression);
        Ok(self.term(start, ExprKind::Package(self.alloc(name)), Shape::Other))
    }

    fn parse_yield(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        self.bump();
        self.yield_seen += 1;
        self.interest = true;
        if self.at(T::Semicolon) {
            self.c_missing();
            self.c_node(K::YieldExpression);
            let null = Expr {
                span: Span::new(start, self.prev_end),
                kind: ExprKind::Null,
            };
            self.note_yield(start);
            return Ok(self.term(
                start,
                ExprKind::Yield(self.alloc(Afield::Value(null))),
                Shape::Other,
            ));
        }
        let first = self.expr_reset()?.e;
        let field = if self.eat(T::FatArrow) {
            let second = self.expr_reset()?.e;
            self.c_node(K::ElementInitializer);
            Afield::KeyValue(first, second)
        } else {
            Afield::Value(first)
        };
        self.c_node(K::YieldExpression);
        self.note_yield(start);
        Ok(self.term(start, ExprKind::Yield(self.alloc(field)), Shape::Other))
    }

    /// Records the `yield` expression that started at `start` and has just
    /// been parsed, until it is known whether it stands where HHVM accepts
    /// one.
    fn note_yield(&mut self, start: u32) {
        let at = (!self.unit_aborted).then_some(self.lowering_list.len());
        self.yields.push((start, self.prev_end.max(start), at));
    }

    /// HHVM's translation to the AST complains about each `yield` that is
    /// neither a statement nor what a statement assigns: about those of
    /// the function just parsed, recorded from `first` on.
    #[cold]
    pub(super) fn report_misplaced_yields(&mut self, first: usize) {
        // Last one first, so that the places recorded for the others stay
        // right.
        for (start, end, at) in self.yields.drain(first..).rev() {
            let Some(at) = at else { continue };
            self.lowering_errors = true;
            let at = at.min(self.lowering_list.len());
            if self.first_only_at != usize::MAX && at <= self.first_only_at {
                self.first_only_at += 1;
            }
            self.lowering_list.insert(
                at,
                crate::SyntaxError {
                    start,
                    end,
                    message:
                        "`yield` can only appear as a statement or on the right of an assignment"
                            .into(),
                },
            );
        }
    }

    /// The `yield` expression at `start` stands where HHVM accepts one.
    pub(crate) fn accept_yield(&mut self, start: u32) {
        self.yield_ok += 1;
        if let Some(i) = self.yields.iter().rposition(|y| y.0 == start) {
            self.yields.remove(i);
        }
    }

    fn parse_prefix_unary(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        let op = self.take().kind;
        let precedence = match op {
            T::Bang | T::Tilde | T::Plus | T::Minus => 20,
            T::PlusPlus | T::MinusMinus => 22,
            T::Await | T::Delay | T::Readonly => 23,
            T::Clone => 24,
            _ => 5, // print
        };
        if op == T::Readonly {
            self.uses_readonly = true;
        }
        if op == T::Await {
            self.note_await(start);
        }
        let operand = self.expr_with_precedence(precedence)?;
        self.c_node(K::PrefixUnaryExpression);
        if matches!(op, T::PlusPlus | T::MinusMinus) {
            self.note_write(&operand.e, LvalRoot::IncrementOrDecrement);
        }
        let e = self.alloc(operand.e);
        let kind = match op {
            T::Bang => ExprKind::Unop(Uop::Not, e),
            T::Tilde => ExprKind::Unop(Uop::Tilde, e),
            T::Plus => ExprKind::Unop(Uop::Plus, e),
            T::Minus => ExprKind::Unop(Uop::Minus, e),
            T::PlusPlus => ExprKind::Unop(Uop::Incr, e),
            T::MinusMinus => ExprKind::Unop(Uop::Decr, e),
            T::Await => ExprKind::Await(e),
            T::Delay => ExprKind::Delay(e),
            T::Readonly => ExprKind::Readonly(e),
            T::Clone => ExprKind::Clone(e),
            _ => ExprKind::Print(e),
        };
        Ok(self.term(start, kind, Shape::Other))
    }

    pub(crate) fn parse_cast_or_parenthesized_or_lambda(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        // `(int)$x`
        let next = self.peek();
        if matches!(
            next.kind,
            T::Bool
                | T::Boolean
                | T::Double
                | T::Float_
                | T::Real
                | T::Int_
                | T::Integer
                | T::String_
                | T::Binary
        ) {
            let snap = self.snapshot();
            self.bump();
            let ty = self.take();
            if self.eat(T::RParen) {
                if ty.kind == T::Bool {
                    self.report_lowering(ty.start, ty.end, "(bool) casts are not allowed");
                }
                let operand = self.expr_with_precedence(PREC_CAST)?;
                self.c_node(K::CastExpression);
                let cast = Cast {
                    hint: Hint {
                        span: Self::tok_span(ty),
                        kind: HintKind::Name(self.tok_text(ty)),
                    },
                    expr: operand.e,
                };
                return Ok(self.term(start, ExprKind::Cast(self.alloc(cast)), Shape::Other));
            }
            self.restore(snap);
        }
        // `(...) ==> ...`
        let signature = self.c_mark();
        if let Some(sig) = self.speculate(|p| {
            let sig = p.parse_lambda_signature(false)?;
            if !p.at(T::LambdaArrow) {
                return p.fail("not a lambda");
            }
            Ok(sig)
        }) {
            // Neither attributes nor `async` in front of the signature.
            self.c_insert_missing_n(signature, 2);
            return self.parse_lambda(start, &[], Some(sig));
        }
        self.bump();
        if start == self.await_ok_at {
            self.await_ok_at = self.tok.start;
        }
        let old_as = self.allow_as;
        self.allow_as = true;
        let inner = self.expr_reset();
        self.allow_as = old_as;
        let inner = inner?;
        self.require(T::RParen);
        self.c_node(K::ParenthesizedExpression);
        Ok(Term {
            e: inner.e,
            start,
            end: self.prev_end,
            shape: Shape::Other,
        })
    }

    fn parse_list(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        self.bump();
        self.require(T::LParen);
        let omitted = Expr {
            span: Span::NONE,
            kind: ExprKind::Omitted,
        };
        let mut items = self.vec();
        // Slots may be left empty. A trailing comma adds a final empty slot
        // only when the slot before it was empty too (HHVM's list parser).
        let members = self.c_mark();
        if !self.at(T::RParen) {
            loop {
                if self.at(T::RParen) {
                    self.c_missing();
                    self.c_list_item(false);
                    items.push(omitted);
                    break;
                }
                if self.at(T::Comma) {
                    self.c_missing();
                    self.bump();
                    self.c_list_item(true);
                    items.push(omitted);
                    continue;
                }
                items.push(self.expr_reset()?.e);
                if !self.eat_separator(T::Comma) || self.at(T::RParen) {
                    break;
                }
            }
        }
        self.c_list(members);
        self.require(T::RParen);
        self.c_node(K::ListExpression);
        Ok(self.term(start, ExprKind::List(items.into_bump_slice()), Shape::List))
    }

    /// `vec[..]`, `dict[..]`, `keyset[..]`, `varray[..]`, `darray[..]`, or
    /// one of those words used as an ordinary name (`vec($x)`).
    fn parse_collection_intrinsic(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        let kw = self.tok;
        let parsed = self.speculate(|p| {
            p.bump();
            let targs_start = p.tok.start;
            let targs = if p.at(T::Lt) {
                p.parse_type_arguments()?
            } else {
                p.c_missing();
                &[]
            };
            let targs_end = p.prev_end;
            p.require(T::LBracket);
            Ok((targs, targs_start, targs_end))
        });
        let Some((targs, targs_start, targs_end)) = parsed else {
            let name_mark = self.c_mark();
            let (name, _) = self.parse_qualified_name()?;
            self.c_name_unless_reserved(name_mark);
            return self.finish_name_term(name);
        };
        let members = self.c_mark();
        let keyed = matches!(kw.kind, T::Dict | T::Darray);
        let legacy = matches!(kw.kind, T::Varray | T::Darray);
        // HHVM does without type arguments when their number is wrong. It
        // says so about a `vec`, `keyset` or `dict`; with `varray` and
        // `darray` it says that there are too many or, if the number is
        // that of the other, gives up - before it gets to the elements.
        let (mut too_many, mut unusable) = (None, None);
        let wanted = if keyed { 2 } else { 1 };
        if !targs.is_empty() && targs.len() != wanted {
            match kw.kind {
                T::Varray | T::Darray if targs.len() > 2 => {
                    too_many = self.begin_lowering_error(start, TOO_MANY_TYPE_ARGUMENTS);
                }
                T::Varray => {
                    unusable =
                        self.begin_lowering_abort(start, "VarrayIntrinsicExpression type args");
                }
                T::Darray => {
                    unusable =
                        self.begin_lowering_abort(start, "DarrayIntrinsicExpression type args");
                }
                _ => self.report_lowering(
                    targs_start,
                    targs_end,
                    match kw.kind {
                        T::Vec => "`vec` takes exactly one type argument",
                        T::Keyset => "`keyset` takes exactly one type argument",
                        _ => "`dict` takes exactly two type arguments",
                    },
                ),
            }
        }
        let kind = if keyed {
            let targs = match targs {
                [k, v] => Some((*k, *v)),
                _ => None,
            };
            let mut fields = self.vec();
            while !self.at(T::RBracket) {
                let key = self.expr_reset()?.e;
                self.require(T::FatArrow);
                let value = self.expr_reset()?.e;
                self.c_node(K::ElementInitializer);
                fields.push((key, value));
                if !self.eat_separator(T::Comma) {
                    break;
                }
            }
            self.c_list(members);
            self.require(T::RBracket);
            ExprKind::KeyValCollection(self.alloc(KeyValCollection {
                keyword: Self::tok_span(kw),
                targs,
                fields: fields.into_bump_slice(),
            }))
        } else {
            let targ = match targs {
                [t] => Some(*t),
                _ => None,
            };
            let mut items = self.vec();
            while !self.at(T::RBracket) {
                items.push(self.expr_reset()?.e);
                if !self.eat_separator(T::Comma) {
                    break;
                }
            }
            self.c_list(members);
            self.require(T::RBracket);
            ExprKind::ValCollection(self.alloc(ValCollection {
                kind: if kw.kind == T::Keyset {
                    VcKind::Keyset
                } else {
                    VcKind::Vec
                },
                keyword: Self::tok_span(kw),
                targ,
                items: items.into_bump_slice(),
            }))
        };
        if self.c_on() {
            self.c_node(match kw.kind {
                T::Vec => K::VectorIntrinsicExpression,
                T::Keyset => K::KeysetIntrinsicExpression,
                T::Varray => K::VarrayIntrinsicExpression,
                T::Dict => K::DictionaryIntrinsicExpression,
                _ => K::DarrayIntrinsicExpression,
            });
        }
        self.end_lowering_error(too_many, self.prev_end);
        self.end_lowering_abort(unusable, self.prev_end);
        if legacy {
            self.report_from(
                start,
                "varray and darray are obsolete. Use vec and dict instead.",
            );
        }
        Ok(self.term(start, kind, Shape::Other))
    }

    fn parse_new(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        self.bump();
        let class_start = self.tok.start;
        let mut targs: &'a [Hint<'a>] = &[];
        let kind = self.tok.kind;
        let class = match kind {
            T::Parent | T::SelfKw | T::Static if self.peek_kind() == T::LParen => {
                let tok = self.take();
                Expr {
                    span: Self::tok_span(tok),
                    kind: ExprKind::Id(self.tok_text(tok)),
                }
            }
            T::Parent | T::SelfKw if self.peek_kind() == T::Lt => {
                let tok = self.take();
                targs = self.parse_type_arguments()?;
                self.c_node(K::GenericTypeSpecifier);
                Expr {
                    span: Self::tok_span(tok),
                    kind: ExprKind::Id(self.tok_text(tok)),
                }
            }
            T::Name | T::Backslash => {
                // `new Foo(..)` / `new Foo<T>(..)`; anything else after the
                // name makes it an ordinary expression.
                let named = self.speculate(|p| {
                    p.check_case()?;
                    // HHVM reads a name spelled like a keyword in another
                    // case as an expression, not as a type.
                    let is_type = !p.tok.mixed_case_soft;
                    let (name, _) = p.parse_qualified_name()?;
                    let targs = match p.tok.kind {
                        T::LParen => {
                            if is_type {
                                p.c_node(K::SimpleTypeSpecifier);
                            }
                            &[][..]
                        }
                        T::Lt => {
                            let targs = p.parse_type_arguments()?;
                            p.c_node(K::GenericTypeSpecifier);
                            targs
                        }
                        _ => return p.fail("not a simple class name"),
                    };
                    Ok((name, targs))
                });
                match named {
                    Some((name, t)) => {
                        targs = t;
                        Expr {
                            span: name.span,
                            kind: ExprKind::Id(name.name),
                        }
                    }
                    None => self.parse_new_designator()?.e,
                }
            }
            _ => self.parse_new_designator()?.e,
        };
        let (args, unpacked) = if self.at(T::LParen) {
            self.parse_arguments()?
        } else {
            // `new Foo` without an argument list.
            let class_end = self.prev_end.max(class_start);
            let text = &self.src[class_start as usize..class_end as usize];
            self.report(
                class_start,
                class_end,
                format!(
                    "A constructor initializing an object must be passed a (possibly empty) list of arguments. Did you mean `new {}()`?",
                    text
                ),
            );
            self.c_missing_n(3);
            (&[][..], None)
        };
        self.c_node(K::ConstructorCall);
        self.c_node(K::ObjectCreationExpression);
        for arg in args {
            if let ExprKind::InoutArg(_) = arg.kind {
                self.report(
                    arg.span.start,
                    arg.span.end,
                    "Parameters may not be marked `inout` on constructors",
                );
            }
        }
        let new = New {
            class,
            targs,
            args,
            unpacked,
        };
        Ok(self.term(start, ExprKind::New(self.alloc(new)), Shape::New))
    }

    /// What follows `name` where the class of a `new` or of an attribute is
    /// expected and the name alone is not it: the rest of an expression
    /// that starts with the name. Returns the extent of the expression.
    pub(super) fn parse_new_designator_after(&mut self, name: Id<'a>) -> R<Span> {
        let term = self.name_term(name);
        let old = self.precedence;
        self.precedence = PREC_NEW;
        let term = self.parse_remaining_expression(term);
        self.precedence = old;
        let term = term?;
        Ok(Span::new(term.start, term.end))
    }

    /// The class of a constructor call that does not start with a name: any
    /// expression (see [`parse_new_designator`](Self::parse_new_designator)).
    /// Returns its extent.
    pub(super) fn parse_new_designator_span(&mut self) -> R<Span> {
        let term = self.parse_new_designator()?;
        Ok(Span::new(term.start, term.end.max(term.start)))
    }

    /// The class of a `new` expression when it is not a plain name: an
    /// expression, which has to be one that can denote a class.
    fn parse_new_designator(&mut self) -> R<Term<'a>> {
        let term = self.expr_with_precedence(PREC_NEW)?;
        if term.shape == Shape::Token {
            self.report(
                term.start,
                term.end,
                "`new` requires a class name or local variable, but got: `token`",
            );
        } else if matches!(term.e.kind, ExprKind::Id(name) if name.eq_ignore_ascii_case("namespace"))
            && term.start == term.e.span.start
        {
            // The keyword on its own is no name.
            self.report(
                term.start,
                term.end,
                "`new` requires a class name or local variable, but got: `token`",
            );
        } else if term.start == term.e.span.start && term.end == term.e.span.end {
            // Anything goes inside parentheses.
            self.check_new_designator(&term.e, false);
        }
        Ok(term)
    }

    /// `(a, inout $b, ...$rest)`; the current token must be `(`.
    pub(crate) fn parse_arguments(&mut self) -> R<(&'a [Expr<'a>], Option<&'a Expr<'a>>)> {
        self.parse_argument_list(false)
    }

    /// [`parse_arguments`](Self::parse_arguments) for the arguments of a
    /// function or method call (`call`), where nothing but named arguments
    /// may follow an unpacked one.
    fn parse_argument_list(&mut self, call: bool) -> R<(&'a [Expr<'a>], Option<&'a Expr<'a>>)> {
        self.require(T::LParen);
        let (old_prec, old_as) = (self.precedence, self.allow_as);
        self.precedence = 0;
        self.allow_as = true;
        let r = self.parse_arguments_inner(call);
        self.precedence = old_prec;
        self.allow_as = old_as;
        let r = r?;
        self.require(T::RParen);
        Ok(r)
    }

    fn parse_arguments_inner(&mut self, call: bool) -> R<(&'a [Expr<'a>], Option<&'a Expr<'a>>)> {
        let mut args = self.vec();
        let mut unpacked = None;
        // Whether to look for a positional argument after an unpacked one.
        let mut ordered = !call;
        let arguments = self.c_mark();
        // The item before was missing (see `repair.rs`), in which case the
        // `)` does not end the list just yet.
        let mut after_gap = false;
        while !self.at(T::RParen) || after_gap {
            let kind = self.tok.kind;
            if matches!(kind, T::Comma | T::Eof | T::RParen) {
                after_gap = self.missing_list_item(RIGHT_PAREN_EXPECTED);
                self.missing_expression();
                if after_gap {
                    continue;
                }
                break;
            }
            after_gap = false;
            let start = self.tok.start;
            let misplaced = !ordered
                && unpacked.is_some()
                && kind != T::DotDotDot
                && !(kind == T::Name && self.peek_kind() == T::Eq);
            match kind {
                T::DotDotDot => {
                    self.bump();
                    if unpacked.is_some() {
                        // HHVM's translation to the AST objects to a second
                        // `...` and leaves what it is applied to alone.
                        let untranslated = self.lowering_state();
                        self.parse_expression()?;
                        self.c_node(K::DecoratedExpression);
                        self.restore_lowering_state(untranslated);
                        self.report_lowering(
                            start,
                            self.prev_end,
                            "Too many splat operators `...`. An argument list can have at most one splat operator.",
                        );
                    } else {
                        let e = self.parse_expression()?.e;
                        self.c_node(K::DecoratedExpression);
                        unpacked = Some(self.alloc(e));
                    }
                }
                T::Inout => {
                    let kw = self.take();
                    let inner = self.parse_expression()?.e;
                    self.c_node(K::DecoratedExpression);
                    self.check_lvalue(&inner, LvalRoot::Inout, Nesting::None);
                    self.marks |= crate::checks::MARK_INOUT;
                    args.push(Expr {
                        span: Span::new(kw.start, self.prev_end),
                        kind: ExprKind::InoutArg(self.alloc(inner)),
                    });
                }
                T::Name if self.peek_kind() == T::Eq => {
                    self.check_case()?;
                    let name = self.take();
                    self.bump();
                    let value = self.parse_expression()?.e;
                    self.c_node(K::NamedArgument);
                    let node = NamedArg {
                        name: self.id(name),
                        value,
                    };
                    args.push(Expr {
                        span: Span::new(name.start, self.prev_end),
                        kind: ExprKind::NamedArg(self.alloc(node)),
                    });
                }
                _ => args.push(self.parse_expression()?.e),
            }
            if misplaced {
                self.report_from(
                    start,
                    "The splat operator `...` for unpacking variadic arguments may not appear before other positional arguments.",
                );
                ordered = true;
            }
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(arguments);
        Ok((args.into_bump_slice(), unpacked))
    }

    // ------------------------------------------------------------------
    // Functions as expressions

    /// `$x`, or `(params)[ctx]: ret` — everything before `==>`.
    fn parse_lambda_signature(&mut self, is_async: bool) -> R<LambdaSig<'a>> {
        if self.at(T::Variable) {
            let tok = self.take();
            let param = Param {
                name: self.id(tok),
                hint: None,
                default: None,
                attrs: &[],
                visibility: None,
                inout: None,
                flags: 0,
            };
            return Ok(LambdaSig {
                tparams: &[],
                params: self.arena.alloc_slice_copy(&[param]),
                ctxs: None,
                ret: None,
                readonly_ret: false,
            });
        }
        // Neither `function` nor type parameters.
        self.c_missing_n(2);
        let params = self.parse_params(ParamCtx {
            is_async,
            is_lambda: true,
            ..ParamCtx::default()
        })?;
        let ctxs = self.parse_contexts_opt()?;
        let (ret, readonly_ret) = self.parse_return_hint_opt()?;
        self.c_node(K::LambdaSignature);
        Ok(LambdaSig {
            tparams: &[],
            params,
            ctxs,
            ret,
            readonly_ret,
        })
    }

    /// `: T` / `: readonly T` if present.
    pub(crate) fn parse_return_hint_opt(&mut self) -> R<(Option<Hint<'a>>, bool)> {
        if !self.eat(T::Colon) {
            self.c_missing_n(3);
            return Ok((None, false));
        }
        let readonly = self.eat(T::Readonly);
        if !readonly {
            self.c_missing();
        }
        if readonly
            && matches!(
                self.tok.kind,
                T::LBrace | T::LambdaArrow | T::Use | T::Semicolon
            )
        {
            self.c_missing();
            return Ok((None, true));
        }
        // HHVM keeps a `noreturn` return type as the bare keyword.
        let noreturn = self.at(T::Noreturn);
        let hint = self.parse_hint()?;
        if noreturn {
            self.c_bare_type();
        }
        Ok((Some(hint), readonly))
    }

    /// Parses a lambda. `start` is where the expression began (attributes
    /// included); `sig` is the signature if it has been parsed already.
    fn parse_lambda(
        &mut self,
        start: u32,
        attrs: &'a [Attribute<'a>],
        sig: Option<LambdaSig<'a>>,
    ) -> R<Term<'a>> {
        let mut flags = 0u8;
        let sig = match sig {
            Some(sig) => sig,
            None => {
                if self.eat(T::Async) {
                    flags |= fun_flags::ASYNC;
                } else {
                    self.c_missing();
                }
                self.parse_lambda_signature(flags & fun_flags::ASYNC != 0)?
            }
        };
        self.finish_lambda(start, attrs, sig, flags)
    }

    /// Parses `==> body` and builds the lambda.
    fn finish_lambda(
        &mut self,
        start: u32,
        attrs: &'a [Attribute<'a>],
        sig: LambdaSig<'a>,
        mut flags: u8,
    ) -> R<Term<'a>> {
        self.require(T::LambdaArrow);
        if sig.readonly_ret {
            flags |= fun_flags::READONLY_RETURN;
        }
        let outer_strict = self.enter_closure(sig.ctxs.is_some());
        // A lambda in the body of another is three levels down.
        self.enter()?;
        self.enter()?;
        let r = self.with_function_body(|p| {
            if p.at(T::LBrace) {
                p.parse_compound_statement()
            } else {
                let body_start = p.tok.start;
                let e = p.parse_root_expr()?;
                // HHVM puts up with a lambda that has no body.
                p.absent(&e);
                if matches!(e.kind, ExprKind::AsyncBlock(_)) && e.span.start == body_start {
                    p.report(
                        e.span.start,
                        e.span.end,
                        "Don't use `() ==> async { ... }`. Instead, use: `async () ==> { ... }`",
                    );
                }
                let stmt = Stmt {
                    span: e.span,
                    kind: StmtKind::Return(Some(p.alloc(e))),
                };
                Ok(&*p.arena.alloc_slice_copy(&[stmt]))
            }
        });
        self.leave();
        self.leave();
        self.strict_co = outer_strict;
        let (body, has_yield) = r?;
        self.c_node(K::LambdaExpression);
        if has_yield {
            flags |= fun_flags::HAS_YIELD;
        }
        self.check_closure_signature(attrs, flags, sig.ret.as_ref());
        let fun = Fun {
            span: self.span_from(start),
            attrs,
            tparams: sig.tparams,
            params: sig.params,
            ret: sig.ret,
            ctxs: sig.ctxs,
            where_constraints: &[],
            body,
            flags,
            doc_comment: None,
        };
        Ok(self.term(start, ExprKind::Lfun(self.alloc(fun)), Shape::Other))
    }

    /// `[async] function (...) [use (...)] {...}`, or the `function` spelling
    /// of a lambda.
    fn parse_anon_or_lambda(
        &mut self,
        start: u32,
        attrs: &'a [Attribute<'a>],
        first: crate::lexer::Token,
    ) -> R<Term<'a>> {
        // The doc comment comes from in front of the expression, or failing
        // that from the enclosing statement.
        let own_doc = self.doc_before(first);
        let mut flags = 0u8;
        if self.eat(T::Async) {
            flags |= fun_flags::ASYNC;
        } else {
            self.c_missing();
        }
        self.require(T::Function);
        let errors_before = (self.errors.len(), self.syntax_error_reported);
        let tparams = self.parse_type_params(true)?;
        let params = self.parse_params(ParamCtx {
            is_async: flags & fun_flags::ASYNC != 0,
            is_lambda: true,
            ..ParamCtx::default()
        })?;
        let ctxs = self.parse_contexts_opt()?;
        let (ret, readonly_ret) = self.parse_return_hint_opt()?;
        if readonly_ret {
            flags |= fun_flags::READONLY_RETURN;
        }
        if self.at(T::LambdaArrow) {
            // `function(..) ==> ..` is simply another spelling of a lambda.
            // HHVM's syntax checks skip the signature of one that has type
            // parameters.
            if !tparams.is_empty() && errors_before.1 == self.syntax_error_reported {
                self.errors.truncate(errors_before.0);
            }
            let sig = LambdaSig {
                tparams,
                params,
                ctxs,
                ret,
                readonly_ret,
            };
            self.c_node(K::LambdaSignature);
            return self.finish_lambda(start, attrs, sig, flags);
        }
        let mut uses = self.vec();
        if self.eat(T::Use) {
            self.require(T::LParen);
            let variables = self.c_mark();
            let mut after_gap = false;
            while !self.at(T::RParen) || after_gap {
                if matches!(self.tok.kind, T::Comma | T::Eof | T::RParen) {
                    after_gap = self.missing_list_item(RIGHT_PAREN_EXPECTED);
                    if after_gap {
                        continue;
                    }
                    break;
                }
                after_gap = false;
                uses.push(self.expect_variable()?);
                if !self.eat_separator(T::Comma) {
                    break;
                }
            }
            self.c_list(variables);
            self.require(T::RParen);
            self.c_node(K::AnonymousFunctionUseClause);
            if self.at(T::Colon) {
                // `function() use (..): T`: the type goes first.
                self.report_missing("Bad signature: use(...) should occur after the type");
            }
        } else {
            self.c_missing();
        }
        let stmt_doc = self.stmt_doc;
        let outer_strict = self.enter_closure(ctxs.is_some());
        let r = self.with_function_body(|p| p.parse_compound_statement());
        self.strict_co = outer_strict;
        let (body, has_yield) = r?;
        self.c_node(K::AnonymousFunction);
        if has_yield {
            flags |= fun_flags::HAS_YIELD;
        }
        self.check_closure_signature(attrs, flags, ret.as_ref());
        let doc_comment =
            own_doc.or_else(|| stmt_doc.and_then(|(trivia, tok)| self.doc_comment(trivia, tok)));
        let efun = Efun {
            fun: Fun {
                span: self.span_from(start),
                attrs,
                tparams,
                params,
                ret,
                ctxs,
                where_constraints: &[],
                body,
                flags,
                doc_comment,
            },
            uses: uses.into_bump_slice(),
        };
        Ok(self.term(start, ExprKind::Efun(self.alloc(efun)), Shape::Other))
    }

    fn parse_async_block(&mut self, start: u32, attrs: &'a [Attribute<'a>]) -> R<Term<'a>> {
        self.require(T::Async);
        let outer_strict = self.enter_closure(false);
        let r = self.with_function_body(|p| p.parse_compound_statement());
        self.strict_co = outer_strict;
        let (body, has_yield) = r?;
        self.c_node(K::AwaitableCreationExpression);
        let mut flags = fun_flags::ASYNC;
        if has_yield {
            flags |= fun_flags::HAS_YIELD;
        }
        self.check_closure_signature(attrs, flags, None);
        let fun = Fun {
            span: self.span_from(start),
            attrs,
            tparams: &[],
            params: &[],
            ret: None,
            ctxs: None,
            where_constraints: &[],
            body,
            flags,
            doc_comment: None,
        };
        Ok(self.term(start, ExprKind::AsyncBlock(self.alloc(fun)), Shape::Other))
    }

    /// Runs `f` to parse a function body with fresh per-function state and
    /// reports whether the body contained a `yield`.
    pub(crate) fn with_function_body<X>(
        &mut self,
        f: impl FnOnce(&mut Self) -> R<X>,
    ) -> R<(X, bool)> {
        let saved = (
            self.precedence,
            self.allow_as,
            self.yield_seen,
            self.yield_ok,
            self.pipe_ok,
        );
        self.precedence = 0;
        self.allow_as = true;
        self.yield_seen = 0;
        self.yield_ok = 0;
        self.pipe_ok = false;
        // A function in the code of an expression tree literal is part of
        // the literal; anywhere else it knows nothing of the literals
        // around it.
        let literals = (self.et_depth, self.et_active);
        if !self.et_active {
            self.et_depth = 0;
        }
        let outer_yields = self.yields.len();
        let r = f(self);
        (self.et_depth, self.et_active) = literals;
        let seen = self.yield_seen;
        (
            self.precedence,
            self.allow_as,
            self.yield_seen,
            self.yield_ok,
            self.pipe_ok,
        ) = saved;
        let x = r?;
        if self.yields.len() > outer_yields {
            self.report_misplaced_yields(outer_yields);
        }
        Ok((x, seen > 0))
    }

    // ------------------------------------------------------------------
    // Operators

    fn parse_remaining_expression(&mut self, mut term: Term<'a>) -> R<Term<'a>> {
        // Each operator applied to the term so far puts it one level
        // further down the tree.
        let outer_depth = self.depth;
        let mut operands = 1;
        loop {
            match self.remaining_step(term)? {
                Step::Done(t) => {
                    self.depth = outer_depth;
                    return Ok(t);
                }
                Step::Binary(t, is_assignment) => {
                    term = self.parse_remaining_binary(t, is_assignment)?;
                    if matches!(term.e.kind, ExprKind::Binop(_)) && self.depth_step == 1 {
                        operands += 1;
                        self.check_operands(operands)?;
                    } else {
                        self.enter()?;
                    }
                }
                Step::Reparse(t) => {
                    term = t;
                    self.enter()?;
                }
            }
        }
    }

    fn can_take_type_args(term: &Term<'a>) -> bool {
        matches!(term.shape, Shape::Name | Shape::Member | Shape::Scope)
    }

    fn is_lvalue(term: &Term<'a>) -> bool {
        matches!(
            term.shape,
            Shape::Variable | Shape::Subscript | Shape::Member | Shape::Scope
        )
    }

    /// Generic arguments in expression position: `foo<int>(..)`, `foo<>`.
    fn try_parse_call_type_args(&mut self, term: &Term<'a>) -> Option<CallTypeArgs<'a>> {
        if !self.at(T::Lt) || !Self::can_take_type_args(term) {
            return None;
        }
        let start = self.tok.start;
        let before = self.lowering_state();
        let found = |hints| CallTypeArgs {
            hints,
            start,
            before,
        };
        // Cheap rejection of ordinary comparisons such as `$a->b < 3`.
        match self.peek_kind() {
            T::Variable | T::Int | T::Float | T::SingleQuoted | T::DoubleQuoted | T::Minus => {
                return None;
            }
            T::Gt => {
                // `foo<>`
                self.bump();
                self.c_missing();
                self.bump();
                self.c_node(K::TypeArguments);
                return Some(found(&[]));
            }
            _ => {}
        }
        self.speculate(|p| p.parse_type_arguments()).map(found)
    }

    fn override_precedence(&mut self, term: &Term<'a>, op: T, left_precedence: u8) -> Prefix<'a> {
        if let Some(targs) = self.try_parse_call_type_args(term) {
            return Prefix::TypeArgs(targs);
        }
        if left_precedence >= PREC_ASSIGNMENT_IN_EXPRESSIONS {
            return Prefix::None;
        }
        if op == T::Eq && term.shape == Shape::List {
            return Prefix::Assignment;
        }
        if is_assignment_op(op) && Self::is_lvalue(term) {
            return Prefix::Assignment;
        }
        Prefix::None
    }

    fn remaining_step(&mut self, term: Term<'a>) -> R<Step<'a>> {
        let op = self.tok.kind;
        let Some((op_precedence, _)) = trailing_op(op) else {
            return Ok(Step::Done(term));
        };
        let prefix = self.override_precedence(&term, op, self.precedence);
        let is_assignment = match prefix {
            Prefix::TypeArgs(targs) => {
                let t = self.finish_call_with_type_args(term, targs)?;
                return Ok(Step::Reparse(t));
            }
            Prefix::Assignment => true,
            Prefix::None => {
                if op_precedence < self.precedence {
                    return Ok(Step::Done(term));
                }
                false
            }
        };
        match op {
            T::Instanceof => {
                // HHVM reports the operator, drops it and ends the
                // expression there.
                self.report_missing(
                    "The `instanceof` operator is not supported in Hack; use the `is` operator or `is_a()`",
                );
                self.drop_token();
                Ok(Step::Done(term))
            }
            T::Is | T::QuestionAs | T::Upcast => Ok(Step::Reparse(self.parse_type_test(term)?)),
            T::As => {
                if self.allow_as {
                    Ok(Step::Reparse(self.parse_type_test(term)?))
                } else {
                    Ok(Step::Done(term))
                }
            }
            T::Arrow | T::QArrow => Ok(Step::Reparse(self.parse_member_selection(term)?)),
            T::ColonColon => Ok(Step::Reparse(self.parse_scope_resolution(term)?)),
            T::PlusPlus | T::MinusMinus => {
                self.bump();
                let uop = if op == T::PlusPlus {
                    Uop::PostIncr
                } else {
                    Uop::PostDecr
                };
                self.c_node(K::PostfixUnaryExpression);
                self.note_write(&term.e, LvalRoot::IncrementOrDecrement);
                let e = self.alloc(term.e);
                Ok(Step::Reparse(self.term(
                    term.start,
                    ExprKind::Unop(uop, e),
                    Shape::Other,
                )))
            }
            T::Hash => {
                // A name, which is what `self`, `parent` and `static` are
                // here; an XHP class name is none.
                let bare = term.start == term.e.span.start && term.end == term.e.span.end;
                let class = match term.e.kind {
                    ExprKind::Id(name)
                        if match term.shape {
                            Shape::Name => !name.starts_with(':'),
                            _ => bare && matches!(name, "self" | "parent" | "static"),
                        } =>
                    {
                        Id {
                            span: term.e.span,
                            name,
                        }
                    }
                    _ => {
                        // HHVM's translation to the AST says so and takes
                        // the text of the qualifier for a name.
                        self.report_lowering(
                            term.start,
                            term.end,
                            "Invalid label qualifier. Only names or qualified names are allowed at this position.",
                        );
                        let span = Span::new(term.start, term.end);
                        Id {
                            span,
                            name: span.text(self.src),
                        }
                    }
                };
                self.bump();
                let label = self.expect_name_allow_keywords()?;
                self.c_node(K::EnumClassLabelExpression);
                let label = EnumClassLabel {
                    class: Some(class),
                    label: label.name,
                };
                Ok(Step::Reparse(self.term(
                    term.start,
                    ExprKind::EnumClassLabel(self.alloc(label)),
                    Shape::Other,
                )))
            }
            T::LParen => {
                // No type arguments.
                self.c_missing();
                self.check_call_receiver(&term);
                let (args, unpacked) = self.parse_argument_list(true)?;
                Ok(Step::Reparse(self.make_call(term, &[], args, unpacked)?))
            }
            T::LBracket => {
                self.bump();
                let index = if self.at(T::RBracket) {
                    self.c_missing();
                    None
                } else {
                    let old_as = self.allow_as;
                    self.allow_as = true;
                    let index = self.expr_reset();
                    self.allow_as = old_as;
                    Some(index?.e)
                };
                self.require(T::RBracket);
                self.c_node(K::SubscriptExpression);
                let node = ArrayGet {
                    base: term.e,
                    index,
                };
                Ok(Step::Reparse(self.term(
                    term.start,
                    ExprKind::ArrayGet(self.alloc(node)),
                    Shape::Subscript,
                )))
            }
            T::Question => {
                self.bump();
                Ok(Step::Reparse(self.parse_conditional(term)?))
            }
            _ => Ok(Step::Binary(term, is_assignment)),
        }
    }

    /// The `$` operator, which the current token is, and its operand: no
    /// way to name a variable in Hack, which HHVM says once it has
    /// translated the operand. With `braces` the operand may be an
    /// expression in braces (where the name of a member goes).
    fn parse_dollar_operator(&mut self, braces: bool) -> R<&'a Expr<'a>> {
        let dollar = self.take();
        let operand;
        let valid = if braces && self.at(T::LBrace) {
            self.bump();
            operand = self.expr_reset()?.e;
            self.require(T::RBrace);
            self.c_node(K::BracedExpression);
            true
        } else {
            let term = self.expr_with_precedence(31)?;
            operand = term.e;
            term.start == term.e.span.start
                && matches!(
                    term.e.kind,
                    ExprKind::Lvar(_)
                        | ExprKind::Omitted
                        | ExprKind::Dollar(_)
                        | ExprKind::ArrayGet(_)
                        | ExprKind::Null
                        | ExprKind::True
                        | ExprKind::False
                        | ExprKind::Int(_)
                        | ExprKind::Float(_)
                        | ExprKind::String(_)
                        | ExprKind::String2(_)
                )
        };
        self.c_node(K::PrefixUnaryExpression);
        self.report_lowering(
            dollar.start,
            dollar.end,
            "A valid variable name starts with a letter or underscore, followed by any number of letters, numbers, or underscores",
        );
        if !valid {
            self.report_from(
                dollar.start,
                "The dollar sign `$` cannot be used as a unary operator",
            );
        }
        Ok(self.alloc(operand))
    }

    /// Where the `{` of the braced name of a scope resolution is, given
    /// its qualifier: past any parentheses that close the qualifier, the
    /// `::` and whatever trivia surrounds them.
    pub(super) fn braced_name_start(&self, qualifier: &Expr<'a>) -> u32 {
        let mut at = qualifier.span.end as usize;
        loop {
            match self.bytes.get(at) {
                Some(b')' | b':') => at += 1,
                Some(b'{') | None => return at as u32,
                Some(_) => match crate::cst::trivia::scan(self.bytes, at) {
                    Some(piece) => at += piece.width,
                    None => return at as u32,
                },
            }
        }
    }

    /// What HHVM's translation to the AST says of a braced expression
    /// after `::` (`start` is where its `{` is, and it has just ended)
    /// that names no static property or method.
    #[cold]
    fn not_a_static_member_name(&mut self, start: u32, end: u32) {
        let text = &self.src[start as usize..end as usize];
        self.abort_lowering(
            start,
            end,
            format!(
                "Encountered unexpected text `{text}`, was expecting a static property or method name."
            ),
        );
    }

    /// Called when `receiver` turns out to be what a call is made on, before
    /// its arguments are parsed: HHVM gives up on a call to a static method
    /// named by a variable before it gets to the arguments.
    fn check_call_receiver(&mut self, receiver: &Term<'a>) {
        let (Shape::Scope, ExprKind::ClassGet(access)) = (receiver.shape, receiver.e.kind) else {
            return;
        };
        if self.bytes[receiver.end as usize - 1] == b'}' {
            let start = self.braced_name_start(&access.class);
            self.not_a_static_member_name(start, receiver.end);
        } else {
            self.abort_lowering(
                receiver.start,
                receiver.end,
                "Dynamic calls to static methods are banned.",
            );
        }
    }

    fn finish_call_with_type_args(
        &mut self,
        term: Term<'a>,
        targs: CallTypeArgs<'a>,
    ) -> R<Term<'a>> {
        let CallTypeArgs {
            hints: targs,
            start: targs_start,
            before,
        } = targs;
        match self.tok.kind {
            T::ColonColon => {
                // `A<T>::b`: to HHVM's parser the qualifier is a generic
                // type; its translation to the AST rejects the type
                // arguments without looking at them.
                self.c_node(K::GenericTypeSpecifier);
                self.restore_lowering_state(before);
                self.report_lowering(
                    targs_start,
                    self.prev_end,
                    "Type arguments are not allowed in this position",
                );
                self.parse_scope_resolution(term)
            }
            T::LParen => {
                self.check_call_receiver(&term);
                let (args, unpacked) = self.parse_argument_list(true)?;
                self.make_call(term, targs, args, unpacked)
            }
            _ => {
                // Function pointer: `foo<>`, `Foo::bar<>`.
                self.c_node(K::FunctionPointerExpression);
                let ok = match term.e.kind {
                    ExprKind::Id(_) => true,
                    ExprKind::ClassConst(access) => matches!(access.class.kind, ExprKind::Id(_)),
                    _ => false,
                };
                if !ok {
                    // Reported, and HHVM gives up on the declaration.
                    self.report_lowering(
                        term.start,
                        self.prev_end,
                        "Function pointers `<>` can only be created with toplevel functions and explicitly named static methods. Use lambdas `(...) ==> {...}` for other cases.",
                    );
                    self.unit_aborted = true;
                    return Ok(Term {
                        end: self.prev_end,
                        shape: Shape::Other,
                        ..term
                    });
                }
                let node = FunctionPointer {
                    target: term.e,
                    targs,
                };
                Ok(self.term(
                    term.start,
                    ExprKind::FunctionPointer(self.alloc(node)),
                    Shape::Other,
                ))
            }
        }
    }

    fn make_call(
        &mut self,
        receiver: Term<'a>,
        targs: &'a [Hint<'a>],
        args: &'a [Expr<'a>],
        unpacked: Option<&'a Expr<'a>>,
    ) -> R<Term<'a>> {
        let mut func = receiver.e;
        if let ExprKind::Id(name) = func.kind
            && name.strip_prefix('\\').unwrap_or(name) == "HH\\Readonly\\as_mut"
        {
            self.uses_readonly = true;
        }
        match (receiver.shape, func.kind) {
            (Shape::Member, ExprKind::ObjGet(get)) if !get.is_method => {
                // An unparenthesised `a->b(..)` is a method call. Member
                // selection normally sees the `(` coming and sets the flag
                // itself; this covers `a->b<T>(..)`.
                func.kind = ExprKind::ObjGet(self.alloc(ObjGet {
                    is_method: true,
                    ..*get
                }));
            }
            _ => {}
        }
        if let (Shape::Member, ExprKind::ObjGet(get)) = (receiver.shape, func.kind) {
            self.check_xhp_method_call(&receiver, get);
        }
        self.c_node(K::FunctionCallExpression);
        let call = Call {
            func,
            targs,
            args,
            unpacked,
        };
        self.check_call(receiver.start, &call);
        Ok(self.term(
            receiver.start,
            ExprKind::Call(self.alloc(call)),
            Shape::Other,
        ))
    }

    fn parse_type_test(&mut self, term: Term<'a>) -> R<Term<'a>> {
        let op = self.take().kind;
        let hint = self.parse_hint()?;
        if self.c_on() {
            self.c_node(match op {
                T::Is => K::IsExpression,
                T::As => K::AsExpression,
                T::QuestionAs => K::NullableAsExpression,
                _ => K::UpcastExpression,
            });
        }
        if matches!(op, T::Is | T::As) && matches!(hint.kind, HintKind::Fun(_) | HintKind::Soft(_))
        {
            self.check_type_test_hint(op == T::Is, &hint);
        }
        if op == T::Upcast {
            self.require_feature(term.start, self.prev_end, Feature::UpcastExpression);
        }
        let node = self.alloc(TypeTest { expr: term.e, hint });
        let kind = match op {
            T::Is => ExprKind::Is(node),
            T::As => ExprKind::As(node),
            T::QuestionAs => ExprKind::NullableAs(node),
            _ => ExprKind::Upcast(node),
        };
        Ok(self.term(term.start, kind, Shape::Other))
    }

    fn parse_member_selection(&mut self, term: Term<'a>) -> R<Term<'a>> {
        if term.shape == Shape::New {
            // `new Foo()->bar` has to be written `(new Foo())->bar`.
            self.report_lowering(
                term.start,
                term.end,
                "Method call following immediate constructor call requires parentheses around constructor call.",
            );
        }
        let nullsafe = self.take().kind == T::QArrow;
        self.retokenize_xhp_class_name();
        let member = match self.tok.kind {
            T::XhpClassName => {
                let tok = self.take();
                Expr {
                    span: Self::tok_span(tok),
                    kind: ExprKind::Id(self.tok_text(tok)),
                }
            }
            T::Variable => {
                let tok = self.take();
                Expr {
                    span: Self::tok_span(tok),
                    kind: ExprKind::Lvar(self.tok_text(tok)),
                }
            }
            T::LBrace => {
                self.bump();
                let inner = self.expr_reset()?;
                self.require(T::RBrace);
                self.c_node(K::BracedExpression);
                inner.e
            }
            T::Dollar => {
                let start = self.tok.start;
                let operand = self.parse_dollar_operator(true)?;
                Expr {
                    span: self.span_from(start),
                    kind: ExprKind::Dollar(operand),
                }
            }
            kind if kind.is_name_like() => {
                let name = if self.peek_kind() == T::Backslash {
                    // `$a->b\c`: HHVM reads a qualified name.
                    let parts = self.c_mark();
                    let (name, _) = self.parse_qualified_name()?;
                    self.c_name_at(parts);
                    name
                } else {
                    let tok = self.take();
                    self.c_name();
                    self.id(tok)
                };
                Expr {
                    span: name.span,
                    kind: ExprKind::Id(name.name),
                }
            }
            _ => {
                // No member: HHVM says so and leaves what is there alone.
                self.report_missing("A name or variable name is expected here.");
                self.missing_term().e
            }
        };
        self.c_node(if nullsafe {
            K::SafeMemberSelectionExpression
        } else {
            K::MemberSelectionExpression
        });
        // `a->b(` is a method call unless we are inside `new a->b(..)`,
        // where the parentheses belong to `new`.
        let is_method = self.at(T::LParen) && self.precedence <= PREC_CALL;
        let node = ObjGet {
            object: term.e,
            member,
            nullsafe,
            is_method,
        };
        Ok(self.term(
            term.start,
            ExprKind::ObjGet(self.alloc(node)),
            Shape::Member,
        ))
    }

    /// `receiver`, a member that is being called, belongs to an XHP element
    /// written without parentheses or is one of its attributes.
    fn check_xhp_method_call(&mut self, receiver: &Term<'a>, get: &ObjGet<'a>) {
        if matches!(get.object.kind, ExprKind::Xml(_)) && get.object.span.start == receiver.start {
            self.report(
                receiver.start,
                receiver.end,
                "Please add parentheses around the XHP component",
            );
        }
        if matches!(get.member.kind, ExprKind::Id(name) if name.starts_with(':')) {
            self.report(
                receiver.start,
                receiver.end,
                "Method calls are not allowed on XHP attributes",
            );
        }
    }

    fn parse_scope_resolution(&mut self, term: Term<'a>) -> R<Term<'a>> {
        self.bump();
        if self.at(T::Name) && self.tok_text(self.tok).eq_ignore_ascii_case("class") {
            // `CLASS` is the keyword misspelt, as HHVM's lexer says.
            let (start, end) = (self.tok.start, self.tok.end);
            let text = &self.src[start as usize..end as usize];
            self.report_syntax(
                start,
                end,
                format!("Keyword `{text}` must be written in lowercase"),
            );
            self.tok.kind = T::Class;
        }
        let kind = match self.tok.kind {
            T::Variable => {
                let tok = self.take();
                ExprKind::ClassGet(self.alloc(ClassAccess {
                    class: term.e,
                    name: self.id(tok),
                }))
            }
            // Only `class` is a keyword after `::`.
            T::Class => {
                let tok = self.take();
                ExprKind::ClassConst(self.alloc(ClassAccess {
                    class: term.e,
                    name: self.id(tok),
                }))
            }
            kind if kind.is_name_like() => {
                let name = if self.peek_kind() == T::Backslash {
                    // `A::b\c`: HHVM reads a qualified name.
                    let parts = self.c_mark();
                    let (name, _) = self.parse_qualified_name()?;
                    self.c_name_at(parts);
                    name
                } else {
                    let tok = self.take();
                    self.c_name();
                    self.id(tok)
                };
                ExprKind::ClassConst(self.alloc(ClassAccess {
                    class: term.e,
                    name,
                }))
            }
            T::Dollar => {
                // The name HHVM's translation to the AST leaves out (see
                // `parse_dollar_operator`) is no name to it.
                let start = self.tok.start;
                self.parse_dollar_operator(true)?;
                self.not_a_static_member_name(start, self.prev_end);
                ExprKind::ClassConst(self.alloc(ClassAccess {
                    class: term.e,
                    name: Id {
                        span: self.span_from(start),
                        name: "",
                    },
                }))
            }
            T::LBrace => {
                // `Foo::{'name'}`, `Foo::{$name}`
                let brace = self.tok.start;
                self.bump();
                let inner = self.expr_reset()?.e;
                self.require(T::RBrace);
                self.c_node(K::BracedExpression);
                let access = |name| ClassAccess {
                    class: term.e,
                    name: Id {
                        span: inner.span,
                        name,
                    },
                };
                match inner.kind {
                    ExprKind::String(value) => match std::str::from_utf8(value) {
                        Ok(name) => ExprKind::ClassConst(self.alloc(access(name))),
                        Err(_) => return self.fail_at(inner.span.start, "invalid UTF-8 in a name"),
                    },
                    ExprKind::Id(name) => ExprKind::ClassConst(self.alloc(access(name))),
                    ExprKind::Lvar(name) => ExprKind::ClassGet(self.alloc(access(name))),
                    _ => {
                        self.not_a_static_member_name(brace, self.prev_end);
                        ExprKind::ClassConst(self.alloc(access("")))
                    }
                }
            }
            _ => {
                // No name: HHVM says so and leaves what is there alone.
                // Its translation to the AST finds no expression there,
                // and so nothing a static member could be named by.
                self.report_missing("A name, variable name or `class` is expected here.");
                let at = self.missing_pos();
                self.c_missing();
                self.missing_expression();
                self.abort_lowering(
                    at,
                    at + 1,
                    "Encountered unexpected text ``, was expecting a static property or method name.",
                );
                ExprKind::ClassConst(self.alloc(ClassAccess {
                    class: term.e,
                    name: Id {
                        span: Span::new(at, at),
                        name: "",
                    },
                }))
            }
        };
        self.c_node(K::ScopeResolutionExpression);
        self.note_scope_resolution(&term.e, &kind);
        if term.start != term.e.span.start {
            // A qualifier in parentheses is for the later checks.
            self.interest = true;
        }
        Ok(self.term(term.start, kind, Shape::Scope))
    }

    fn parse_conditional(&mut self, test: Term<'a>) -> R<Term<'a>> {
        self.enter()?;
        let r = self.parse_conditional_inner(test);
        self.leave();
        r
    }

    fn parse_conditional_inner(&mut self, test: Term<'a>) -> R<Term<'a>> {
        let then = if self.at(T::Colon) && !self.lexer.is_xhp_class_name_at(self.tok.start as usize)
        {
            self.c_missing();
            None
        } else {
            let then = self.expr_reset()?.e;
            (!self.absent(&then)).then_some(then)
        };
        self.require(T::Colon);
        let colon_end = self.prev_end;
        let term = self.parse_term()?;
        let mut otherwise = self.parse_remaining_binary_helper(term, PREC_CONDITIONAL)?;
        if otherwise.start == otherwise.end {
            // Nothing there (see `repair.rs`): to HHVM the expression ends
            // with the `:`.
            otherwise.end = colon_end.max(test.end);
        }
        self.c_node(K::ConditionalExpression);
        if then.is_none() {
            // `$a ? : $b` with the operator split in two.
            self.report(
                test.start,
                otherwise.end,
                "An Elvis operator `?:` is expected here.",
            );
        }
        // A conditional directly (without parentheses) in the condition, or
        // as the body of a lambda in the last operand, is ambiguous.
        let is_conditional = |e: &Expr<'a>| matches!(e.kind, ExprKind::Eif(c) if c.then.is_some());
        let nested_test = is_conditional(&test.e)
            && test.start == test.e.span.start
            && test.end == test.e.span.end;
        let nested_lambda = otherwise.start == otherwise.e.span.start
            && matches!(otherwise.e.kind, ExprKind::Lfun(f)
                if matches!(f.body, [Stmt { span, kind: StmtKind::Return(Some(body)) }]
                    if span.start == body.span.start
                        && f.span.end == body.span.end
                        && is_conditional(body)));
        if nested_test || nested_lambda {
            self.report(
                test.start,
                otherwise.end,
                "Nested ternary expressions inside ternary expressions are ambiguous. Please add parentheses",
            );
        }
        let node = Eif {
            cond: test.e,
            then,
            otherwise: otherwise.e,
        };
        Ok(self.term(test.start, ExprKind::Eif(self.alloc(node)), Shape::Other))
    }

    fn parse_remaining_binary(&mut self, left: Term<'a>, is_assignment: bool) -> R<Term<'a>> {
        self.enter()?;
        let r = self.parse_remaining_binary_inner(left, is_assignment);
        self.leave();
        r
    }

    fn parse_remaining_binary_inner(&mut self, left: Term<'a>, is_assignment: bool) -> R<Term<'a>> {
        let op = self.take().kind;
        let op_end = self.prev_end;
        let (precedence, _) = trailing_op(op).unwrap();
        // What an assignment that roots `await`s assigns does so too.
        if left.start == self.await_ok_at && is_assignment_op(op) && op != T::QQEq {
            self.await_ok_at = self.tok.start;
        }
        // `$$` stands for the left-hand side throughout the right-hand side
        // of a pipe.
        let outer_pipe = self.pipe_ok;
        if matches!(op, T::BarGt | T::BarQGt) {
            self.pipe_ok = true;
        }
        let right = if is_assignment {
            let old = self.precedence;
            self.precedence = 0;
            let r = self.parse_term();
            self.precedence = old;
            r
        } else {
            self.parse_term()
        };
        let right = match right {
            Ok(right) => self.parse_remaining_binary_helper(right, precedence),
            Err(e) => Err(e),
        };
        self.pipe_ok = outer_pipe;
        let mut right = right?;
        if right.start == right.end {
            // Nothing there (see `repair.rs`): to HHVM the expression ends
            // with the operator.
            right.end = op_end;
        }
        self.make_binary(left, op, right)
    }

    fn parse_remaining_binary_helper(
        &mut self,
        mut right: Term<'a>,
        left_precedence: u8,
    ) -> R<Term<'a>> {
        loop {
            let kind = self.tok.kind;
            let Some((right_precedence, assoc)) = trailing_op(kind) else {
                return Ok(right);
            };
            if kind == T::As && !self.allow_as {
                return Ok(right);
            }
            match self.override_precedence(&right, kind, left_precedence) {
                Prefix::TypeArgs(targs) => {
                    right = self.finish_call_with_type_args(right, targs)?;
                }
                Prefix::Assignment => {
                    let old = self.precedence;
                    self.precedence = 0;
                    let r = self.parse_remaining_expression(right);
                    self.precedence = old;
                    right = r?;
                }
                Prefix::None => {
                    if right_precedence > left_precedence
                        || (assoc == Assoc::Right && right_precedence == left_precedence)
                    {
                        let old = self.precedence;
                        self.precedence = right_precedence;
                        let r = self.parse_remaining_expression(right);
                        self.precedence = old;
                        right = r?;
                    } else {
                        return Ok(right);
                    }
                }
            }
        }
    }

    fn make_binary(&mut self, left: Term<'a>, op: T, right: Term<'a>) -> R<Term<'a>> {
        self.c_node(K::BinaryExpression);
        let kind = if is_assignment_op(op) {
            if op == T::Eq && matches!(left.e.kind, ExprKind::Shape(..) | ExprKind::Tuple(..)) {
                let direct = left.start == left.e.span.start && left.end == left.e.span.end;
                self.check_pattern(&left.e, direct);
            }
            self.note_write(&left.e, LvalRoot::Assignment);
            ExprKind::Assign(self.alloc(Assign {
                lhs: left.e,
                op: if op == T::Eq { None } else { binary_op(op) },
                rhs: right.e,
            }))
        } else {
            match op {
                T::BarGt | T::BarQGt => ExprKind::Pipe(self.alloc(Pipe {
                    lhs: left.e,
                    rhs: right.e,
                    nullsafe: op == T::BarQGt,
                })),
                T::QuestionColon => ExprKind::Eif(self.alloc(Eif {
                    cond: left.e,
                    then: None,
                    otherwise: right.e,
                })),
                _ => {
                    let Some(bop) = binary_op(op) else {
                        return self.fail("unexpected operator");
                    };
                    ExprKind::Binop(self.alloc(Binop {
                        op: bop,
                        lhs: left.e,
                        rhs: right.e,
                    }))
                }
            }
        };
        Ok(Term {
            e: Expr {
                span: Span::new(left.start, right.end),
                kind,
            },
            start: left.start,
            end: right.end,
            shape: Shape::Other,
        })
    }
}

/// The part of a lambda in front of `==>`.
#[derive(Clone, Copy)]
pub(crate) struct LambdaSig<'a> {
    tparams: &'a [Tparam<'a>],
    params: &'a [Param<'a>],
    ctxs: Option<Contexts<'a>>,
    ret: Option<Hint<'a>>,
    readonly_ret: bool,
}
