//! Utility functions for working with syntax nodes

use line_break_map::LineBreakMap;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_by_ref::positioned_value::PositionedValue;
use parser_core_types::syntax_by_ref::syntax_variant_generated::SyntaxVariant;

/// Check if a SyntaxVariant represents a statement
///
/// A statement is any variant that:
/// - Ends with "Statement" (e.g., ExpressionStatement, ReturnStatement)
/// - Is one of the special statement types: InclusionDirective, SwitchFallthrough,
///   UsingStatementBlockScoped, UsingStatementFunctionScoped
pub fn is_statement<'a>(
    variant: &SyntaxVariant<'a, PositionedToken<'a>, PositionedValue<'a>>,
) -> bool {
    matches!(
        variant,
        // All *Statement variants
        SyntaxVariant::ExpressionStatement(_)
            | SyntaxVariant::UnsetStatement(_)
            | SyntaxVariant::DeclareLocalStatement(_)
            | SyntaxVariant::WhileStatement(_)
            | SyntaxVariant::IfStatement(_)
            | SyntaxVariant::TryStatement(_)
            | SyntaxVariant::DoStatement(_)
            | SyntaxVariant::ForStatement(_)
            | SyntaxVariant::ForeachStatement(_)
            | SyntaxVariant::SwitchStatement(_)
            | SyntaxVariant::MatchStatement(_)
            | SyntaxVariant::ReturnStatement(_)
            | SyntaxVariant::YieldBreakStatement(_)
            | SyntaxVariant::ThrowStatement(_)
            | SyntaxVariant::BreakStatement(_)
            | SyntaxVariant::ContinueStatement(_)
            | SyntaxVariant::EchoStatement(_)
            | SyntaxVariant::ConcurrentStatement(_)
            // Special statement types
            | SyntaxVariant::InclusionDirective(_)
            | SyntaxVariant::SwitchFallthrough(_)
            | SyntaxVariant::UsingStatementBlockScoped(_)
            | SyntaxVariant::UsingStatementFunctionScoped(_)
    )
}

/// Collect all statement nodes from a syntax tree
pub fn collect_statements<'a>(root: &'a PositionedSyntax<'a>) -> Vec<&'a PositionedSyntax<'a>> {
    let mut collector = StatementCollector {
        statements: Vec::new(),
    };
    crate::visitor::walk(&mut collector, root);
    collector.statements
}

struct StatementCollector<'a> {
    statements: Vec<&'a PositionedSyntax<'a>>,
}

impl<'a> crate::visitor::SyntaxVisitor<'a> for StatementCollector<'a> {
    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) -> bool {
        if is_statement(&node.children) {
            self.statements.push(node);
        }
        true
    }
}

/// Convert byte offset to line and column number using a precalculated LineBreakMap
///
/// This is much more efficient than iterating through the source string each time,
/// especially when converting many offsets in the same file.
///
/// # Arguments
/// * `line_break_map` - Precalculated line break map for the source file
/// * `offset` - The byte offset in the source
///
/// # Returns
/// A tuple of (line, column) where both are 1-indexed
pub fn offset_to_line_column(line_break_map: &LineBreakMap, offset: usize) -> (usize, usize) {
    let (line, column) = line_break_map.offset_to_position(offset as isize);
    (line as usize, column as usize)
}
