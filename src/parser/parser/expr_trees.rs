//! Expression tree literals (`` Dsl`1 + ${$x}` ``) and the splices in them.
//!
//! The literal is parsed as an ordinary expression (or block); what it
//! means is worked out when the tree is lowered (see `expr_tree.rs`).

use super::expr::{Shape, Term};
use super::{Parser, R};
use crate::ast::*;
use crate::cst::SyntaxKind as K;
use crate::span::Span;
use crate::token::T;

impl<'a> Parser<'a> {
    /// A literal from its opening backtick on. `prefix` is the name of the
    /// DSL in front of it, which has been consumed; `start` is where the
    /// expression begins.
    pub(super) fn parse_expression_tree(
        &mut self,
        start: u32,
        prefix: Option<Id<'a>>,
    ) -> R<Term<'a>> {
        // HHVM tells an unnamed literal that is not inside another apart
        // from one inside a splice of another.
        let (outermost, was_active) = (self.et_depth == 0, self.et_active);
        let backtick = self.tok.start;
        self.require(T::Backtick);
        let outer_in_literal = std::mem::replace(&mut self.in_expression_tree, true);
        let enclosed = std::mem::replace(&mut self.et_enclosed, true);
        self.et_active = true;
        self.et_depth += 1;
        // Working out what the literal stands for takes more stack per
        // level of it than anything else does.
        let outer_step = std::mem::replace(&mut self.depth_step, 2);
        let body = self.parse_expression_tree_body();
        self.depth_step = outer_step;
        self.in_expression_tree = outer_in_literal;
        self.et_enclosed = enclosed;
        self.et_active = false;
        self.et_depth -= 1;
        let (body, body_span) = body?;
        self.require(T::Backtick);
        self.c_node(K::PrefixedCodeExpression);
        if prefix.is_none() {
            const NAME: &str = "Expression trees must provide the DSL name.";
            if outermost {
                self.report(start, self.prev_end, NAME);
            } else if !was_active {
                self.report(backtick, backtick, NAME);
            }
        }
        let node = ExpressionTree {
            prefix,
            body,
            body_span,
        };
        Ok(self.term(
            start,
            ExprKind::ExpressionTree(self.alloc(node)),
            Shape::Other,
        ))
    }

    /// What is between the backticks: a block or an expression.
    fn parse_expression_tree_body(&mut self) -> R<(EtBody<'a>, Span)> {
        if self.at(T::LBrace) {
            let start = self.tok.start;
            let block = self.parse_compound_statement()?;
            Ok((EtBody::Block(block), self.span_from(start)))
        } else {
            let body = self.expr_reset()?;
            Ok((EtBody::Expr(body.e), Span::new(body.start, body.end)))
        }
    }

    /// `${ expr }`; the current token is the `$`.
    pub(super) fn parse_et_splice(&mut self) -> R<Term<'a>> {
        let start = self.tok.start;
        self.bump();
        self.require(T::LBrace);
        // Inside the braces is ordinary code again.
        let outer_in_literal = std::mem::replace(&mut self.in_expression_tree, false);
        let was_active = std::mem::replace(&mut self.et_active, false);
        let tree_errors = self.tree_errors.len();
        let inner = self.expr_reset();
        self.in_expression_tree = outer_in_literal;
        self.et_active = was_active;
        let inner = inner?;
        self.require(T::RBrace);
        self.c_node(K::ETSpliceExpression);
        if !was_active {
            let end = self.prev_end;
            if self.et_depth > 0 {
                self.report(start, end, "Splice syntax `${...}` cannot be nested.");
            } else {
                self.report(
                    start,
                    end,
                    "Splicing can only occur inside expression tree literals (between backticks)",
                );
                if !self.et_enclosed {
                    // A splice outside any literal survives into HHVM's
                    // AST, where another check finds it; that one does not
                    // look inside the splices it reports.
                    const STRAY: &str =
                        "Splice syntax ${...} can only occur inside expression trees Foo``...``.";
                    let mut index = 0;
                    self.tree_errors.retain(|(_, error)| {
                        index += 1;
                        index <= tree_errors || error.message != STRAY
                    });
                    self.report_tree_pass(2, start, end, STRAY);
                }
            }
        }
        let node = EtSplice {
            expr: inner.e,
            span: Span::new(inner.start, inner.end),
        };
        Ok(self.term(start, ExprKind::EtSplice(self.alloc(node)), Shape::Other))
    }
}
