//! `match` statements and the patterns of their arms.

use super::{Parser, R};
use crate::ast::*;
use crate::checks::Feature;
use crate::cst::SyntaxKind as K;
use crate::token::T;

impl<'a> Parser<'a> {
    /// `match (subject) { pattern => statement ... }`
    pub(super) fn parse_match(&mut self) -> R<Stmt<'a>> {
        let start = self.tok.start;
        self.bump();
        self.require(T::LParen);
        let subject = self.parse_expr()?;
        self.require(T::RParen);
        self.require(T::LBrace);
        let mut arms = self.vec();
        let arm_list = self.c_mark();
        while !self.at(T::RBrace) && !self.at(T::Eof) {
            let arm_start = self.tok.start;
            if self.at(T::Case) {
                // HHVM reads past a `case` in front of a pattern.
                self.skip_unexpected_token();
            }
            let pattern = self.parse_pattern(true)?;
            self.require(T::FatArrow);
            let body = self.parse_arm_body()?;
            self.c_node(K::MatchStatementArm);
            arms.push(MatchArm { pattern, body });
            if self.tok.start == arm_start {
                break;
            }
        }
        self.c_list(arm_list);
        if arms.is_empty() {
            self.report_missing("`match` statements need to have at least one arm");
        }
        self.require(T::RBrace);
        self.c_node(K::MatchStatement);
        self.require_feature(start, self.prev_end, Feature::MatchStatements);
        let node = Match {
            subject,
            arms: arms.into_bump_slice(),
        };
        Ok(Stmt {
            span: self.span_from(start),
            kind: StmtKind::Match(self.alloc(node)),
        })
    }

    /// The statement of an arm: a block, or a single statement.
    fn parse_arm_body(&mut self) -> R<Block<'a>> {
        if self.at(T::LBrace) {
            self.parse_compound_statement()
        } else {
            let stmt = self.parse_statement()?;
            Ok(self.arena.alloc_slice_copy(&[stmt]))
        }
    }

    /// `$x`, `_`, `$x: T`, `_: T`, or `Name(pattern, ...)`. Only `_` and
    /// `_: T` mean something so far; HHVM reports the others, without
    /// looking at the patterns inside one (`top_level` is unset for those).
    fn parse_pattern(&mut self, top_level: bool) -> R<Pattern<'a>> {
        let start = self.tok.start;
        let (binding, qualified) = match self.tok.kind {
            T::Variable => {
                let tok = self.take();
                (PatternBinding::Variable(self.tok_text(tok)), false)
            }
            T::Name => {
                let (name, _) = self.parse_qualified_name()?;
                let plain = name.name.len() == (name.span.end - name.span.start) as usize
                    && !name.name.contains('\\');
                (PatternBinding::Wildcard(name.name), !plain)
            }
            _ => return self.fail("A pattern is expected here."),
        };
        let mut has_members = false;
        let hint = if self.eat(T::Colon) {
            let hint = self.parse_hint_ex(false)?;
            self.c_node(K::RefinementPattern);
            Some(hint)
        } else {
            match binding {
                PatternBinding::Variable(_) => self.c_node(K::VariablePattern),
                PatternBinding::Wildcard(_) => {
                    if self.eat(T::LParen) {
                        let members = self.c_mark();
                        while !self.at(T::RParen) {
                            self.enter()?;
                            self.parse_pattern(false)?;
                            self.leave();
                            has_members = true;
                            if !self.eat_separator(T::Comma) {
                                break;
                            }
                        }
                        self.c_list(members);
                        self.expect(T::RParen)?;
                    } else {
                        self.c_missing_n(3);
                    }
                    self.c_node(K::ConstructorPattern);
                }
            }
            None
        };
        let pattern = Pattern {
            span: self.span_from(start),
            binding,
            hint,
        };
        if !top_level {
            return Ok(pattern);
        }
        let end = self.prev_end;
        if has_members {
            self.report_lowering(
                start,
                end,
                "Destructuring values in patterns is not yet supported.",
            );
        }
        match binding {
            PatternBinding::Variable(_) => self.report_lowering(
                start,
                end,
                "Binding a variable in a pattern is not yet supported.",
            ),
            PatternBinding::Wildcard(_) if qualified && hint.is_none() => self.report_lowering(
                start,
                end,
                "Constructor patterns are not yet supported. Use a type refinement pattern `_: T` instead.",
            ),
            PatternBinding::Wildcard(name) => {
                if !name.starts_with('_') {
                    self.report_lowering(
                        start,
                        end,
                        format!(
                            "Wildcard patterns must begin with the `_` character. To bind a variable, use a `$` character (`${name}`).",
                        ),
                    );
                }
            }
        }
        Ok(pattern)
    }
}
