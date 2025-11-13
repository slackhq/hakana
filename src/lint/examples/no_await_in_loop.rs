//! Example linter: Detect await expressions inside loops
//!
//! This is a port of HHAST's NoAwaitInLoopLinter demonstrating how to implement
//! a linter using the Hakana lint framework.

use crate::{LintContext, LintError, Linter, Severity, SyntaxVisitor};
use parser_core_types::lexable_token::LexableToken;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_by_ref::positioned_value::PositionedValue;
use parser_core_types::syntax_by_ref::syntax_variant_generated::*;
use parser_core_types::token_kind::TokenKind;
use std::cell::RefCell;

pub struct NoAwaitInLoopLinter;

impl Linter for NoAwaitInLoopLinter {
    fn name(&self) -> &'static str {
        "no-await-in-loop"
    }

    fn hhast_name(&self) -> Option<&'static str> {
        Some("Facebook\\HHAST\\DontAwaitInALoopLinter")
    }

    fn description(&self) -> &'static str {
        "Detects await expressions inside loops, which can cause performance issues"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = NoAwaitVisitor {
            ctx,
            errors: RefCell::new(Vec::new()),
            in_loop_depth: 0,
        };

        crate::visitor::walk(&mut visitor, ctx.root);

        visitor.errors.into_inner()
    }

    fn supports_auto_fix(&self) -> bool {
        false // This requires manual refactoring
    }
}

struct NoAwaitVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: RefCell<Vec<LintError>>,
    in_loop_depth: usize,
}

impl<'a> SyntaxVisitor<'a> for NoAwaitVisitor<'a> {
    fn visit_while_statement(
        &mut self,
        _node: &'a WhileStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.in_loop_depth += 1;
    }

    fn visit_for_statement(
        &mut self,
        _node: &'a ForStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.in_loop_depth += 1;
    }

    fn visit_foreach_statement(
        &mut self,
        _node: &'a ForeachStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.in_loop_depth += 1;
    }

    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) -> bool {
        // Check if this is an await expression
        if self.in_loop_depth > 0 {
            if let Some(token) = node.get_token() {
                if token.kind() == TokenKind::Await {
                    let (start, end) = self.ctx.node_full_range(node);
                    self.errors.borrow_mut().push(
                        LintError::new(
                            Severity::Warning,
                            "Await expression found inside loop. Consider using concurrent operations instead.",
                            start,
                            end,
                            "no-await-in-loop",
                        )
                    );
                }
            }
        }

        // After processing children, decrement depth for loop statements
        match &node.children {
            SyntaxVariant::WhileStatement(_)
            | SyntaxVariant::ForStatement(_)
            | SyntaxVariant::ForeachStatement(_) => {
                // Walk children first
                for child in node.iter_children() {
                    crate::visitor::walk(self, child);
                }
                if self.in_loop_depth > 0 {
                    self.in_loop_depth -= 1;
                }
            }
            _ => {}
        }
        true
    }
}

impl NoAwaitInLoopLinter {
    pub fn new() -> Self {
        Self
    }
}
