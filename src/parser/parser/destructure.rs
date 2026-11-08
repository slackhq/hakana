//! `shape(...)` and `tuple(...)` expressions, and their use as
//! destructuring patterns.
//!
//! Like HHVM, the parser reads a pattern (`shape('x' => $x, ...) = $point`)
//! as an ordinary expression and only learns that it is one when it meets
//! the `=` after it, or finds it as the variable of a `foreach`. Syntax that
//! only a pattern may have (`?` in front of a field, `optional` in front of
//! an entry, a final `...`) is therefore accepted everywhere and remembered;
//! what has not turned out to be a pattern by the end of the statement is
//! reported then.

use std::num::NonZeroU32;

use super::expr::{Shape, Term};
use super::{Parser, R};
use crate::ast::*;
use crate::checks::Feature;
use crate::cst::SyntaxKind as K;
use crate::span::Span;
use crate::token::T;

const INVALID_SHAPE_FIELD: &str =
    "Shape field name must be a nonempty single-quoted string or a class constant";

/// Whether something only a pattern may have is written directly in `e`.
fn has_pattern_syntax(e: &Expr<'_>) -> bool {
    match e.kind {
        ExprKind::Shape(fields, rest) => {
            rest.is_some() || fields.iter().any(ShapeField::is_optional)
        }
        ExprKind::Tuple(items, rest) => {
            rest.is_some()
                || items
                    .iter()
                    .any(|e| matches!(e.kind, ExprKind::OptionalEntry(_)))
        }
        _ => false,
    }
}

impl<'a> Parser<'a> {
    pub(super) fn parse_tuple(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        self.bump();
        self.require(T::LParen);
        let mut items = self.vec();
        let item_list = self.c_mark();
        while !matches!(self.tok.kind, T::RParen | T::DotDotDot | T::Eof) {
            if self.at(T::Optional) {
                let keyword = self.take();
                let inner = self.expr_reset()?.e;
                self.c_node(K::PrefixUnaryExpression);
                items.push(Expr {
                    span: Span::new(keyword.start, self.prev_end),
                    kind: ExprKind::OptionalEntry(self.alloc(inner)),
                });
            } else {
                items.push(self.expr_reset()?.e);
            }
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(item_list);
        let rest = self.parse_pattern_rest();
        self.require(T::RParen);
        self.c_node(K::TupleExpression);
        let term = self.term(
            start,
            ExprKind::Tuple(items.into_bump_slice(), rest),
            Shape::Other,
        );
        if has_pattern_syntax(&term.e) {
            self.pending_patterns.push(term.e);
        }
        Ok(term)
    }

    pub(super) fn parse_shape(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        self.bump();
        self.require(T::LParen);
        let mut fields = self.vec();
        let field_list = self.c_mark();
        while !self.at(T::RParen) && !self.at(T::Eof) {
            if self.at(T::Comma) {
                // No field where one is due.
                self.report_missing("A shape field name is expected here.");
                self.c_missing();
                self.bump();
                self.c_list_item(true);
                // The field that is not there has no name, as HHVM's
                // checks see it, and its translation to the AST gives up.
                let at = self.missing_pos();
                self.report(at, at, INVALID_SHAPE_FIELD);
                self.missing_syntax("shape field");
                continue;
            }
            if self.at(T::DotDotDot) {
                // On its own, `...` ends a pattern; in front of an
                // expression it adds the fields of another shape.
                if matches!(self.peek_kind(), T::RParen | T::Comma) {
                    break;
                }
                let ellipsis = self.take();
                let value = self.expr_reset()?.e;
                self.c_node(K::ShapeSplatExpression);
                self.require_feature(ellipsis.start, self.prev_end, Feature::ShapeSplatExpression);
                fields.push(ShapeField::Splat {
                    start: ellipsis.start,
                    value,
                });
            } else {
                fields.push(self.parse_shape_field()?);
            }
            if !self.eat_separator(T::Comma) {
                break;
            }
        }
        self.c_list(field_list);
        let rest = self.parse_pattern_rest();
        self.require(T::RParen);
        self.c_node(K::ShapeExpression);
        let term = self.term(
            start,
            ExprKind::Shape(fields.into_bump_slice(), rest),
            Shape::Other,
        );
        if has_pattern_syntax(&term.e) {
            self.pending_patterns.push(term.e);
        }
        Ok(term)
    }

    /// The `...` in front of the closing parenthesis, if there is one.
    fn parse_pattern_rest(&mut self) -> Rest {
        if self.at(T::DotDotDot) {
            NonZeroU32::new(self.take().start)
        } else {
            self.c_missing();
            None
        }
    }

    /// `'key' => value`, `$value`, or either with a `?` in front.
    fn parse_shape_field(&mut self) -> R<ShapeField<'a>> {
        let optional = if self.at(T::Question) {
            Some(self.take().start)
        } else {
            None
        };
        let (key, value) = if self.at(T::Variable)
            && matches!(self.peek_kind(), T::Comma | T::RParen | T::DotDotDot)
        {
            // `$x` stands for `'x' => $x`.
            let tok = self.take();
            self.c_node(K::VariableExpression);
            self.note_variable(tok);
            let span = Self::tok_span(tok);
            let name = self.tok_text(tok);
            if optional.is_none() {
                self.require_feature(span.start, span.end, Feature::ShapeFieldPunning);
            }
            let key = ShapeKey::Str {
                span,
                value: &name.as_bytes()[1..],
            };
            let value = Expr {
                span,
                kind: ExprKind::Lvar(name),
            };
            (key, value)
        } else {
            let key_start = self.tok.start;
            let key = self.expr_reset()?.e;
            let key = self.shape_key(key, key_start, false)?;
            self.require(T::FatArrow);
            let value = self.expr_reset()?.e;
            self.c_node(K::FieldInitializer);
            (key, value)
        };
        Ok(match optional {
            Some(question) => {
                self.c_node(K::PrefixUnaryExpression);
                ShapeField::Optional(self.alloc(OptionalShapeField {
                    question,
                    key,
                    value,
                }))
            }
            None => ShapeField::Field { key, value },
        })
    }

    /// `e` is assigned to with `=`, or is the key or value of a `foreach`:
    /// if it is a `shape(...)` or `tuple(...)`, it is a pattern. `direct`
    /// tells whether HHVM's syntax checks see it as one too: they do not
    /// look through parentheses, nor at the key of a `foreach`.
    pub(crate) fn check_pattern(&mut self, e: &Expr<'a>, direct: bool) {
        match e.kind {
            ExprKind::Shape(fields, _) => {
                self.forget_pending_pattern(e);
                for field in fields {
                    match field {
                        ShapeField::Field { value, .. } => {
                            let nested = direct && !self.is_parenthesized(value);
                            self.check_pattern_target(value, nested);
                        }
                        ShapeField::Optional(optional) => {
                            let (span, value) = (field.span(), &optional.value);
                            if !direct {
                                self.report(span.start, span.end, INVALID_SHAPE_FIELD);
                            } else if field.is_punned() {
                                self.require_feature(
                                    value.span.start,
                                    value.span.end,
                                    Feature::ShapeFieldPunning,
                                );
                            }
                            let nested = direct && !self.is_parenthesized(value);
                            self.check_pattern_target(value, nested);
                        }
                        ShapeField::Splat { .. } => {
                            // HHVM cannot translate this and gives up on
                            // the declaration.
                            let span = field.span();
                            self.abort_lowering(
                                span.start,
                                span.end,
                                format!(
                                    "Encountered unexpected text `{}`, was expecting a shape destructure field.",
                                    span.text(self.src)
                                ),
                            );
                        }
                    }
                }
            }
            ExprKind::Tuple(items, _) => {
                self.forget_pending_pattern(e);
                let mut seen_optional = false;
                for item in items {
                    let target = match item.kind {
                        ExprKind::OptionalEntry(inner) => {
                            seen_optional = true;
                            inner
                        }
                        _ => {
                            if seen_optional {
                                self.report_lowering(
                                    item.span.start,
                                    item.span.end,
                                    "There must be no required elements following an optional element",
                                );
                            }
                            item
                        }
                    };
                    let nested = direct && !self.is_parenthesized(target);
                    self.check_pattern_target(target, nested);
                }
            }
            _ => {}
        }
    }

    /// What a pattern binds a field or entry to: a variable, `_`, or a
    /// nested pattern.
    fn check_pattern_target(&mut self, target: &Expr<'a>, direct: bool) {
        match target.kind {
            ExprKind::Shape(..) | ExprKind::Tuple(..) => self.check_pattern(target, direct),
            ExprKind::Lvar(_) | ExprKind::Id("_") => {}
            _ => self.report_lowering(
                target.span.start,
                target.span.end,
                "Only variables, wildcards, and nested shape/tuple patterns are allowed in destructuring patterns",
            ),
        }
    }

    /// Whether `e`, the value of a shape field or an entry of a tuple, is
    /// wrapped in parentheses.
    fn is_parenthesized(&self, e: &Expr<'a>) -> bool {
        let before = &self.bytes[..e.span.start as usize];
        let Some(open) = before.iter().rposition(|b| !b.is_ascii_whitespace()) else {
            return false;
        };
        if before[open] != b'(' {
            return false;
        }
        // The first entry of a tuple follows the parenthesis of the tuple
        // itself.
        match before[..open]
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
        {
            Some(i) => !before[..=i].ends_with(b"tuple"),
            None => true,
        }
    }

    fn forget_pending_pattern(&mut self, e: &Expr<'a>) {
        if let Some(i) = self.pending_patterns.iter().rposition(|p| p.span == e.span) {
            self.pending_patterns.remove(i);
        }
    }

    /// Reports the shapes and tuples remembered since `mark` that use
    /// pattern syntax but have not turned out to be patterns.
    #[cold]
    pub(crate) fn report_pending_patterns(&mut self, mark: usize) {
        for i in mark..self.pending_patterns.len() {
            let e = self.pending_patterns[i];
            match e.kind {
                ExprKind::Shape(fields, rest) => {
                    if let Some(rest) = rest {
                        self.report_lowering(
                            rest.get(),
                            rest.get() + 3,
                            "Ellipsis `...` is only valid in shape destructuring patterns",
                        );
                    }
                    for field in fields {
                        if field.is_optional() {
                            let span = field.span();
                            self.report(span.start, span.end, INVALID_SHAPE_FIELD);
                            self.report_lowering(
                                span.start,
                                span.end,
                                "Optional field syntax `?` is only valid in shape destructuring patterns",
                            );
                        }
                    }
                }
                ExprKind::Tuple(items, rest) => {
                    if let Some(rest) = rest {
                        self.report_lowering(
                            rest.get(),
                            rest.get() + 3,
                            "Ellipsis `...` is only valid in tuple destructuring patterns",
                        );
                    }
                    for item in items {
                        if let ExprKind::OptionalEntry(_) = item.kind {
                            // HHVM cannot translate this and gives up on
                            // the declaration.
                            self.abort_lowering(
                                item.span.start,
                                item.span.end,
                                format!(
                                    "Encountered unexpected text `{}`, was expecting a unary operator.",
                                    item.span.text(self.src)
                                ),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        self.pending_patterns.truncate(mark);
    }
}
