//! Linter: Detect discarded new expressions
//!
//! This is a port of HHAST's DontDiscardNewExpressionsLinter.
//! It detects when you create an object with `new` but don't assign or use it.

use crate::{LintContext, LintError, Linter, Severity, SyntaxVisitor};
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_by_ref::positioned_value::PositionedValue;
use parser_core_types::syntax_by_ref::syntax_variant_generated::{
    ExpressionStatementChildren, SyntaxVariant,
};

pub struct DontDiscardNewExpressionsLinter;

impl Linter for DontDiscardNewExpressionsLinter {
    fn name(&self) -> &'static str {
        "dont-discard-new-expressions"
    }

    fn hhast_name(&self) -> Option<&'static str> {
        Some("Facebook\\HHAST\\DontDiscardNewExpressionsLinter")
    }

    fn description(&self) -> &'static str {
        "Detects when new expressions are created but not used"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = DontDiscardNewVisitor {
            ctx,
            errors: Vec::new(),
        };

        crate::visitor::walk(&mut visitor, ctx.root);

        visitor.errors
    }

    fn supports_auto_fix(&self) -> bool {
        false // This requires manual refactoring
    }
}

impl DontDiscardNewExpressionsLinter {
    pub fn new() -> Self {
        Self
    }
}

struct DontDiscardNewVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: Vec<LintError>,
}

impl<'a> SyntaxVisitor<'a> for DontDiscardNewVisitor<'a> {
    fn visit_expression_statement(
        &mut self,
        node: &'a ExpressionStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        // Check if the expression is an object creation
        if let SyntaxVariant::ObjectCreationExpression(obj_creation) = &node.expression.children {
            let (start, end) = self.ctx.node_full_range(&node.expression);

            // Check if it looks like an Exception
            let obj_text = self.ctx.node_text(&obj_creation.object);
            let is_exception = obj_text.contains("Exception")
                || obj_text.contains("Error")
                || obj_text.ends_with("Error");

            let message = if is_exception {
                "You are discarding the result of a `new` expression. \
                It looks like you are constructing an Exception. Did you intend to throw it?"
            } else {
                "You are discarding the result of a `new` expression. \
                If you are intentionally discarding it, consider assigning it to `$_`. \
                If you are running this constructor for its side-effects, \
                consider restructuring that class/constructor."
            };

            self.errors.push(LintError::new(
                Severity::Warning,
                message,
                start,
                end,
                "dont-discard-new-expressions",
            ));
        }
    }
}
