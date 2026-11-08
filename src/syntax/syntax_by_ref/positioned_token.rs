use crate::lexable_token::LexableToken;
use crate::source_text::SourceText;
use crate::token_kind::TokenKind;
use crate::trivia_kind::TriviaKind;

#[derive(Debug)]
pub struct PositionedTokenImpl {
    pub kind: TokenKind,
    /// Where the leading trivia starts.
    pub offset: usize,
    pub leading_width: usize,
    /// The width of the token's own text.
    pub width: usize,
    pub trailing_width: usize,
    /// The kinds of trivia before and after the token, one bit per
    /// [`TriviaKind`].
    pub leading_kinds: u16,
    pub trailing_kinds: u16,
}

/// A token of the tree: a cheap handle to arena data.
#[derive(Debug, Clone, Copy)]
pub struct PositionedToken<'a>(pub(crate) &'a PositionedTokenImpl);

impl<'a> PositionedToken<'a> {
    /// Where the token's own text starts.
    pub fn start_offset(&self) -> usize {
        self.0.offset + self.0.leading_width
    }

    /// The offset of the last byte of the token's text.
    pub fn end_offset(&self) -> usize {
        self.start_offset() + self.0.width.saturating_sub(1)
    }

    /// Where the token's leading trivia starts.
    pub fn offset(&self) -> usize {
        self.0.offset
    }

    pub fn inner_ptr_eq(x: &Self, y: &Self) -> bool {
        std::ptr::eq(x.0, y.0)
    }

    pub fn text<'b>(&self, source_text: &'b SourceText<'_>) -> &'b str {
        source_text.sub_as_str(self.start_offset(), self.0.width)
    }

    pub fn text_raw<'b>(&self, source_text: &'b SourceText<'_>) -> &'b [u8] {
        source_text.sub(self.start_offset(), self.0.width)
    }
}

impl LexableToken for PositionedToken<'_> {
    fn kind(&self) -> TokenKind {
        self.0.kind
    }

    fn leading_start_offset(&self) -> Option<usize> {
        Some(self.0.offset)
    }

    fn width(&self) -> usize {
        self.0.width
    }

    fn leading_width(&self) -> usize {
        self.0.leading_width
    }

    fn trailing_width(&self) -> usize {
        self.0.trailing_width
    }

    fn full_width(&self) -> usize {
        self.0.leading_width + self.0.width + self.0.trailing_width
    }

    fn has_leading_trivia_kind(&self, kind: TriviaKind) -> bool {
        self.0.leading_kinds & (1 << kind as u8) != 0
    }

    fn has_trailing_trivia_kind(&self, kind: TriviaKind) -> bool {
        self.0.trailing_kinds & (1 << kind as u8) != 0
    }
}
