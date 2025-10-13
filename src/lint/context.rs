//! Context provided to linters during analysis

use parser_core_types::lexable_token::LexableToken;
use parser_core_types::source_text::SourceText;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_trait::SyntaxTrait;
use std::path::Path;

/// Context passed to linters containing the file being analyzed and utilities
pub struct LintContext<'a> {
    /// The source file being analyzed
    pub source: &'a SourceText<'a>,

    /// The full syntax tree for the file
    pub root: &'a PositionedSyntax<'a>,

    /// The file path being analyzed
    pub file_path: &'a Path,

    /// Whether auto-fixes should be generated
    pub allow_auto_fix: bool,
}

impl<'a> LintContext<'a> {
    /// Create a new lint context
    pub fn new(
        source: &'a SourceText<'a>,
        root: &'a PositionedSyntax<'a>,
        file_path: &'a Path,
        allow_auto_fix: bool,
    ) -> Self {
        Self {
            source,
            root,
            file_path,
            allow_auto_fix,
        }
    }

    /// Extract text from a syntax node
    pub fn node_text(&self, node: &PositionedSyntax<'a>) -> &'a str {
        node.text(self.source)
    }

    /// Get the byte offset range for a node including all trivia
    ///
    /// This returns the full range that corresponds to `node.text()`,
    /// including leading and trailing trivia (whitespace, comments, newlines).
    pub fn node_range(&self, node: &PositionedSyntax<'a>) -> (usize, usize) {
        let start = node.leading_start_offset();
        let end = start + node.full_width();
        (start, end)
    }

    /// Get line and column for a byte offset
    /// Note: This is a placeholder - actual line/column calculation
    /// would require walking the source text and counting newlines
    pub fn offset_to_line_column(&self, offset: usize) -> (usize, usize) {
        // Simple implementation: count newlines before offset
        let text = self.source.text();
        let line = text[..offset.min(text.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1;
        let column = text[..offset.min(text.len())]
            .iter()
            .rev()
            .take_while(|&&b| b != b'\n')
            .count()
            + 1;
        (line, column)
    }

    /// Get the byte offset range for a token, excluding leading trivia
    ///
    /// This is useful when you want to edit/replace just the token itself
    /// while preserving leading whitespace and comments.
    ///
    /// # Example
    /// ```text
    /// "  ; // comment"
    ///    ^
    /// token_range returns (2, 3) - just the semicolon
    /// node_range would return (0, 14) - including leading space and trailing comment
    /// ```
    pub fn token_range(&self, token: &PositionedToken<'a>) -> (usize, usize) {
        let leading_offset = token.leading_start_offset().unwrap_or(0);
        let token_start = leading_offset + token.leading_width();
        let token_end = token_start + token.width();
        (token_start, token_end)
    }

    /// Get the byte offset range for a token including leading trivia but excluding trailing
    ///
    /// This is useful when you want to preserve the visual positioning of code
    /// while replacing a token.
    ///
    /// # Example
    /// ```text
    /// "  ; // comment"
    ///    ^^^
    /// token_range_with_leading returns (0, 3) - includes leading space but not trailing comment
    /// ```
    pub fn token_range_with_leading(&self, token: &PositionedToken<'a>) -> (usize, usize) {
        let leading_offset = token.leading_start_offset().unwrap_or(0);
        let token_start = leading_offset + token.leading_width();
        let token_end = token_start + token.width();
        (leading_offset, token_end)
    }
}
