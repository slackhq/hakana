use super::positioned_token::PositionedToken;
use super::positioned_value::PositionedValue;
use super::syntax::Syntax;
use crate::source_text::SourceText;
use crate::syntax_trait::SyntaxTrait;

/// A node of the lossless syntax tree.
pub type PositionedSyntax<'a> = Syntax<'a, PositionedToken<'a>, PositionedValue<'a>>;

impl<'a> SyntaxTrait for PositionedSyntax<'a> {
    fn offset(&self) -> Option<usize> {
        Some(self.start_offset())
    }

    fn width(&self) -> usize {
        self.value.width()
    }

    fn leading_width(&self) -> usize {
        self.value.leading_width()
    }

    fn trailing_width(&self) -> usize {
        self.value.trailing_width()
    }

    fn full_width(&self) -> usize {
        self.leading_width() + self.width() + self.trailing_width()
    }

    fn leading_start_offset(&self) -> usize {
        self.value.start_offset()
    }

    fn extract_text<'src>(&self, source_text: &'src SourceText<'_>) -> Option<&'src str> {
        Some(self.text(source_text))
    }
}
