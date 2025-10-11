//! Example linters demonstrating the framework
//!
//! # Token Range Helpers
//!
//! When creating auto-fixes, use the `LintContext` helper methods for working with tokens:
//!
//! - `ctx.token_range(token)` - Returns byte range of just the token, excluding leading/trailing trivia.
//!   Use this when you want to delete or replace a token while preserving whitespace.
//!   Example: Removing a semicolon while keeping indentation.
//!
//! - `ctx.token_range_with_leading(token)` - Returns byte range including leading trivia.
//!   Use this when you want to preserve the visual alignment of code.
//!
//! - `ctx.node_range(node)` - Returns the full byte range including all trivia.
//!   Use this for error reporting or when you need the complete span.
//!
//! See `no_empty_statements.rs` for examples of using these helpers.

pub mod dont_discard_new_expressions;
pub mod must_use_braces_for_control_flow;
pub mod no_await_in_loop;
pub mod no_empty_statements;
pub mod no_whitespace_at_end_of_line;
pub mod use_statement_without_kind;

pub use dont_discard_new_expressions::DontDiscardNewExpressionsLinter;
pub use must_use_braces_for_control_flow::MustUseBracesForControlFlowLinter;
pub use no_await_in_loop::NoAwaitInLoopLinter;
pub use no_empty_statements::NoEmptyStatementsLinter;
pub use no_whitespace_at_end_of_line::NoWhitespaceAtEndOfLineLinter;
pub use use_statement_without_kind::UseStatementWithoutKindLinter;

/// Get all built-in example linters
pub fn all_example_linters() -> Vec<Box<dyn crate::Linter>> {
    vec![
        Box::new(DontDiscardNewExpressionsLinter::new()),
        Box::new(MustUseBracesForControlFlowLinter::new()),
        Box::new(NoAwaitInLoopLinter::new()),
        Box::new(NoEmptyStatementsLinter::new()),
        Box::new(NoWhitespaceAtEndOfLineLinter::new()),
        Box::new(UseStatementWithoutKindLinter::new()),
    ]
}
