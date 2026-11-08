use std::fmt::Debug;

use crate::token_kind::TokenKind;
use crate::trivia_kind::TriviaKind;

/// What can be asked of a token.
pub trait LexableToken: Clone + Debug {
    fn kind(&self) -> TokenKind;

    /// Where the token's leading trivia starts.
    fn leading_start_offset(&self) -> Option<usize>;

    /// The width of the token's own text.
    fn width(&self) -> usize;
    fn leading_width(&self) -> usize;
    fn trailing_width(&self) -> usize;
    fn full_width(&self) -> usize;

    fn leading_is_empty(&self) -> bool {
        self.leading_width() == 0
    }

    fn trailing_is_empty(&self) -> bool {
        self.trailing_width() == 0
    }

    fn has_leading_trivia_kind(&self, kind: TriviaKind) -> bool;
    fn has_trailing_trivia_kind(&self, kind: TriviaKind) -> bool;

    fn has_trivia_kind(&self, kind: TriviaKind) -> bool {
        self.has_leading_trivia_kind(kind) || self.has_trailing_trivia_kind(kind)
    }
}
