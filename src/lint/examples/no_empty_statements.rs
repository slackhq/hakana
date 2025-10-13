//! Linter: Detect empty statements and expressions with no effect
//!
//! This is a port of HHAST's NoEmptyStatementsLinter.
//! It detects:
//! 1. Empty statements (just semicolons)
//! 2. Expression statements that have no side effects

use crate::{Edit, EditSet, LintContext, LintError, Linter, Severity, SyntaxVisitor};
use parser_core_types::lexable_token::LexableToken;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_by_ref::positioned_value::PositionedValue;
use parser_core_types::syntax_by_ref::syntax_variant_generated::*;
use parser_core_types::token_kind::TokenKind;
use rustc_hash::FxHashSet;

pub struct NoEmptyStatementsLinter;

impl Linter for NoEmptyStatementsLinter {
    fn name(&self) -> &'static str {
        "no-empty-statements"
    }

    fn hhast_name(&self) -> Option<&'static str> {
        Some("Facebook\\HHAST\\NoEmptyStatementsLinter")
    }

    fn description(&self) -> &'static str {
        "Detects empty statements and expressions with no effect"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = NoEmptyStatementsVisitor {
            ctx,
            errors: Vec::new(),
            handled_statements: FxHashSet::default(),
        };

        crate::visitor::walk(&mut visitor, ctx.root);

        visitor.errors
    }

    fn supports_auto_fix(&self) -> bool {
        true
    }
}

impl NoEmptyStatementsLinter {
    pub fn new() -> Self {
        Self
    }
}

struct NoEmptyStatementsVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: Vec<LintError>,
    handled_statements: FxHashSet<usize>, // Track statement start offsets we've already handled
}

impl<'a> NoEmptyStatementsVisitor<'a> {
    /// Handle empty statement that is the body of a control flow statement
    /// Replaces ; with {}
    fn handle_control_flow_empty_body(&mut self, body: &'a PositionedSyntax<'a>) {
        if let SyntaxVariant::ExpressionStatement(stmt) = &body.children {
            if matches!(&stmt.expression.children, SyntaxVariant::Missing) {
                let (start, _) = self.ctx.node_range(body);

                // Mark this statement as handled so we don't process it again
                self.handled_statements.insert(start);

                // Get error range from the semicolon (including trivia)
                let (error_start, error_end) = self.ctx.node_range(&stmt.semicolon);

                let mut error = LintError::new(
                    Severity::Warning,
                    "This statement is empty",
                    error_start,
                    error_end,
                    "no-empty-statements",
                );

                // Add auto-fix to replace with empty compound statement {}
                if self.ctx.allow_auto_fix {
                    // Get the semicolon token
                    if let Some(token) = stmt.semicolon.get_token() {
                        let mut fix = EditSet::new();
                        // Replace just the semicolon with {}, preserving leading whitespace
                        let (token_start, token_end) = self.ctx.token_range(token);
                        fix.add(Edit::new(token_start, token_end, "{}"));
                        error = error.with_fix(fix);
                    }
                }

                self.errors.push(error);
            }
        }
    }

    /// Check if an expression has no side effects
    fn is_empty_expression(&self, expr: &'a PositionedSyntax<'a>) -> bool {
        match &expr.children {
            // Literal values
            SyntaxVariant::LiteralExpression(_) => true,

            // Variable references (reading a variable has no effect)
            SyntaxVariant::VariableExpression(_) => true,

            // Names/qualified names
            SyntaxVariant::QualifiedName(_) | SyntaxVariant::SimpleTypeSpecifier(_) => true,

            // Binary expressions (unless they're assignments or have side effects)
            SyntaxVariant::BinaryExpression(bin) => {
                self.is_operator_without_side_effects(&bin.operator)
            }

            // Cast expressions
            SyntaxVariant::CastExpression(_) => true,

            // Collection literals
            SyntaxVariant::VectorIntrinsicExpression(_)
            | SyntaxVariant::DictionaryIntrinsicExpression(_)
            | SyntaxVariant::KeysetIntrinsicExpression(_)
            | SyntaxVariant::VarrayIntrinsicExpression(_)
            | SyntaxVariant::DarrayIntrinsicExpression(_)
            | SyntaxVariant::CollectionLiteralExpression(_) => true,

            // Lambdas and anonymous functions
            SyntaxVariant::LambdaExpression(_) | SyntaxVariant::AnonymousFunction(_) => true,

            // isset()
            SyntaxVariant::IssetExpression(_) => true,

            // is expressions
            SyntaxVariant::IsExpression(_) => true,

            // Subscript (array access - reading has no effect)
            SyntaxVariant::SubscriptExpression(_) => true,

            // Parenthesized expressions - check the inner expression
            SyntaxVariant::ParenthesizedExpression(paren) => {
                self.is_empty_expression(&paren.expression)
            }

            _ => false,
        }
    }

    /// Check if an operator has no side effects (not an assignment operator)
    fn is_operator_without_side_effects(&self, op: &'a PositionedSyntax<'a>) -> bool {
        if let Some(token) = op.get_token() {
            let kind = token.kind();

            // Assignment operators have side effects
            match kind {
                TokenKind::Equal
                | TokenKind::PlusEqual
                | TokenKind::MinusEqual
                | TokenKind::StarEqual
                | TokenKind::SlashEqual
                | TokenKind::PercentEqual
                | TokenKind::DotEqual
                | TokenKind::AmpersandEqual
                | TokenKind::BarEqual
                | TokenKind::CaratEqual
                | TokenKind::LessThanLessThanEqual
                | TokenKind::GreaterThanGreaterThanEqual
                | TokenKind::QuestionQuestionEqual
                | TokenKind::StarStarEqual => false,

                // Pipe operator (|>) typically implies function invocation
                TokenKind::BarGreaterThan => false,

                // All other binary operators have no side effects
                _ => true,
            }
        } else {
            true
        }
    }
}

impl<'a> SyntaxVisitor<'a> for NoEmptyStatementsVisitor<'a> {
    fn visit_if_statement(
        &mut self,
        node: &'a IfStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.handle_control_flow_empty_body(&node.statement);
    }

    fn visit_while_statement(
        &mut self,
        node: &'a WhileStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.handle_control_flow_empty_body(&node.body);
    }

    fn visit_for_statement(
        &mut self,
        node: &'a ForStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.handle_control_flow_empty_body(&node.body);
    }

    fn visit_foreach_statement(
        &mut self,
        node: &'a ForeachStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.handle_control_flow_empty_body(&node.body);
    }

    fn visit_do_statement(
        &mut self,
        node: &'a DoStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        self.handle_control_flow_empty_body(&node.body);
    }

    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
        // Check for else clauses
        if let SyntaxVariant::ElseClause(else_clause) = &node.children {
            self.handle_control_flow_empty_body(&else_clause.statement);
        }
    }

    fn visit_expression_statement(
        &mut self,
        node: &'a ExpressionStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        // Check if this statement was already handled as a control flow body
        let (stmt_start, _) = self.ctx.node_range(&node.semicolon);
        if self.handled_statements.contains(&stmt_start) {
            return;
        }

        // Check if the expression is missing (just a semicolon)
        let is_empty = matches!(&node.expression.children, SyntaxVariant::Missing);

        if is_empty {
            // Get the range including trivia for error reporting
            let (error_start, error_end) = self.ctx.node_range(&node.semicolon);

            // Get the token itself to access its offset and width (excluding trivia)
            if let Some(token) = node.semicolon.get_token() {
                let mut error = LintError::new(
                    Severity::Warning,
                    "This statement is empty",
                    error_start,
                    error_end,
                    "no-empty-statements",
                );

                // Add auto-fix to remove just the semicolon character (keep surrounding trivia)
                if self.ctx.allow_auto_fix {
                    let mut fix = EditSet::new();
                    // Delete only the semicolon itself, preserving surrounding whitespace
                    let (token_start, token_end) = self.ctx.token_range(token);
                    fix.add(Edit::delete(token_start, token_end));
                    error = error.with_fix(fix);
                }

                self.errors.push(error);
            }
            return;
        }

        // Check if the expression has no effect
        if self.is_empty_expression(&node.expression) {
            let (start, _) = self.ctx.node_range(&node.expression);
            let (_, end) = self.ctx.node_range(&node.semicolon);

            let error = LintError::new(
                Severity::Warning,
                "This statement includes an expression that has no effect",
                start,
                end,
                "no-empty-statements",
            );

            self.errors.push(error);
        }
    }
}
