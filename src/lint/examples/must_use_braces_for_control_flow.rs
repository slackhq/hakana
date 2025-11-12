//! Linter: Require braces for control flow statements
//!
//! This is a port of HHAST's MustUseBracesForControlFlowLinter.
//! It ensures if/while/for/foreach/else statements always use braces.

use crate::{Edit, EditSet, LintContext, LintError, Linter, Severity, SyntaxVisitor};
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_by_ref::positioned_value::PositionedValue;
use parser_core_types::syntax_by_ref::syntax_variant_generated::*;
use parser_core_types::syntax_trait::SyntaxTrait;

pub struct MustUseBracesForControlFlowLinter;

impl Linter for MustUseBracesForControlFlowLinter {
    fn name(&self) -> &'static str {
        "must-use-braces-for-control-flow"
    }

    fn hhast_name(&self) -> Option<&'static str> {
        Some("Facebook\\HHAST\\MustUseBracesForControlFlowLinter")
    }

    fn description(&self) -> &'static str {
        "Requires braces for if, while, for, foreach, and else statements"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = MustUseBracesVisitor {
            ctx,
            errors: Vec::new(),
        };

        crate::visitor::walk(&mut visitor, ctx.root);

        visitor.errors
    }

    fn supports_auto_fix(&self) -> bool {
        true
    }
}

impl MustUseBracesForControlFlowLinter {
    pub fn new() -> Self {
        Self
    }
}

struct MustUseBracesVisitor<'a> {
    ctx: &'a LintContext<'a>,
    errors: Vec<LintError>,
}

impl<'a> MustUseBracesVisitor<'a> {
    fn check_body(
        &mut self,
        body: &'a PositionedSyntax<'a>,
        statement_name: &str,
        statement_start: usize,
    ) {
        // Check if body is a compound statement (has braces)
        let has_braces = matches!(&body.children, SyntaxVariant::CompoundStatement(_));

        if !has_braces {
            let (body_start, body_end) = self.ctx.node_range(body);

            let mut error = LintError::new(
                Severity::Warning,
                format!("{} without braces", statement_name),
                statement_start,
                body_end,
                "must-use-braces-for-control-flow",
            );

            // Add auto-fix to wrap body in braces
            if self.ctx.allow_auto_fix {
                let source = self.ctx.source.text();
                let mut fix = EditSet::new();

                // Find the newline/whitespace before the body
                let mut newline_pos = None;

                // Look backwards from body_start to find newline
                for i in (0..body_start).rev() {
                    if source[i] == b'\n' {
                        newline_pos = Some(i);
                        break;
                    } else if source[i] != b' ' && source[i] != b'\t' {
                        // Hit non-whitespace, body is on same line as if/else
                        break;
                    }
                }

                if let Some(newline_pos) = newline_pos {
                    // Multi-line format: body is on a new line
                    // Insert opening brace before the newline
                    fix.add(Edit::insert(newline_pos, " {"));

                    // The body already includes its trailing newline, so we just need to add the closing brace
                    // Extract the leading spaces from the source bytes to calculate closing brace indentation
                    let body_bytes = &source[body_start..body_end];
                    let body_leading_spaces = body_bytes
                        .iter()
                        .take_while(|&&b| b == b' ' || b == b'\t')
                        .count();
                    let closing_indent = if body_leading_spaces >= 2 {
                        " ".repeat(body_leading_spaces - 2)
                    } else {
                        String::new()
                    };

                    // Insert closing brace after the body (body already has trailing newline)
                    fix.add(Edit::insert(body_end, format!("{}}}\n", closing_indent)));
                } else {
                    // Single-line format: body is on same line as if/else
                    let body_text = self.ctx.node_text(body).trim();

                    // Find the actual start of the body (skipping leading whitespace)
                    let mut actual_body_start = body_start;
                    while actual_body_start < source.len()
                        && (source[actual_body_start] == b' ' || source[actual_body_start] == b'\t')
                    {
                        actual_body_start += 1;
                    }

                    // Preserve trailing whitespace (spaces/tabs) and newlines
                    // The body range includes trailing trivia, so we need to extract it
                    let body_bytes = &source[body_start..body_end];
                    let mut trailing = String::new();

                    // Walk backwards from the end to collect trailing whitespace
                    for &b in body_bytes.iter().rev() {
                        if b == b' ' || b == b'\t' || b == b'\n' {
                            trailing.insert(0, b as char);
                        } else {
                            break;
                        }
                    }

                    fix.add(Edit::new(
                        actual_body_start,
                        body_end,
                        format!("{{ {}; }}{}", body_text.trim_end_matches(';'), trailing),
                    ));
                }

                error = error.with_fix(fix);
            }

            self.errors.push(error);
        }
    }
}

impl<'a> SyntaxVisitor<'a> for MustUseBracesVisitor<'a> {
    fn visit_if_statement(
        &mut self,
        node: &'a IfStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        let start = node.keyword.offset().unwrap_or(0);
        self.check_body(&node.statement, "if statement", start);
    }

    fn visit_while_statement(
        &mut self,
        node: &'a WhileStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        let start = node.keyword.offset().unwrap_or(0);
        self.check_body(&node.body, "while statement", start);
    }

    fn visit_for_statement(
        &mut self,
        node: &'a ForStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        let start = node.keyword.offset().unwrap_or(0);
        self.check_body(&node.body, "for statement", start);
    }

    fn visit_foreach_statement(
        &mut self,
        node: &'a ForeachStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        let start = node.keyword.offset().unwrap_or(0);
        self.check_body(&node.body, "foreach statement", start);
    }

    fn visit_do_statement(
        &mut self,
        node: &'a DoStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        let start = node.keyword.offset().unwrap_or(0);
        self.check_body(&node.body, "do statement", start);
    }

    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) -> bool {
        // Check for else clauses
        if let SyntaxVariant::ElseClause(else_clause) = &node.children {
            let start = else_clause.keyword.offset().unwrap_or(0);

            // Allow "else if" without braces
            let is_else_if = matches!(
                &else_clause.statement.children,
                SyntaxVariant::IfStatement(_)
            );

            if !is_else_if {
                self.check_body(&else_clause.statement, "else clause", start);
            }
        }
        true
    }
}
