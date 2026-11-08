use crate::source_text::SourceText;

/// Where a node is: the extent of its text and of the trivia around it.
pub trait SyntaxTrait {
    fn offset(&self) -> Option<usize>;
    fn width(&self) -> usize;
    fn leading_width(&self) -> usize;
    fn trailing_width(&self) -> usize;
    fn full_width(&self) -> usize;
    fn leading_start_offset(&self) -> usize;
    fn extract_text<'a>(&self, source_text: &'a SourceText<'_>) -> Option<&'a str>;

    /// Where the node's own text starts, after its leading trivia.
    fn start_offset(&self) -> usize {
        self.leading_start_offset() + self.leading_width()
    }

    /// The offset of the *last byte* of the node's text (not one past it).
    fn end_offset(&self) -> usize {
        self.start_offset() + self.width().saturating_sub(1)
    }

    fn text<'a>(&self, source_text: &'a SourceText<'_>) -> &'a str {
        source_text.sub_as_str(self.start_offset(), self.width())
    }

    fn full_text<'a>(&self, source_text: &'a SourceText<'_>) -> &'a [u8] {
        source_text.sub(self.leading_start_offset(), self.full_width())
    }

    fn leading_text<'a>(&self, source_text: &'a SourceText<'_>) -> &'a str {
        source_text.sub_as_str(self.leading_start_offset(), self.leading_width())
    }
}
