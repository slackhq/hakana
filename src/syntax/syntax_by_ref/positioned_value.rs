use super::positioned_token::PositionedToken;
use crate::lexable_token::LexableToken;
use crate::token_kind::TokenKind;

/// The extent of a node, in terms of its first and last tokens.
#[derive(Debug, Clone)]
pub enum PositionedValue<'a> {
    /// A token.
    TokenValue(PositionedToken<'a>),
    /// A node from its first token to its last.
    TokenSpan(PositionedToken<'a>, PositionedToken<'a>),
    /// A node without text: where it would be.
    Missing { offset: usize },
}

impl<'a> PositionedValue<'a> {
    pub fn width(&self) -> usize {
        match self {
            PositionedValue::TokenValue(t) => t.width(),
            PositionedValue::TokenSpan(left, right) => {
                (right.end_offset() - left.start_offset()) + 1
            }
            PositionedValue::Missing { .. } => 0,
        }
    }

    /// Where the leading trivia starts.
    pub fn start_offset(&self) -> usize {
        match self {
            PositionedValue::TokenValue(t) | PositionedValue::TokenSpan(t, _) => t.offset(),
            PositionedValue::Missing { offset } => *offset,
        }
    }

    pub fn leading_width(&self) -> usize {
        match self {
            PositionedValue::TokenValue(t) | PositionedValue::TokenSpan(t, _) => t.leading_width(),
            PositionedValue::Missing { .. } => 0,
        }
    }

    pub fn trailing_width(&self) -> usize {
        match self {
            PositionedValue::TokenValue(t) | PositionedValue::TokenSpan(_, t) => t.trailing_width(),
            PositionedValue::Missing { .. } => 0,
        }
    }

    pub fn leading_token(&self) -> Option<PositionedToken<'a>> {
        match self {
            PositionedValue::TokenValue(t) | PositionedValue::TokenSpan(t, _) => Some(*t),
            PositionedValue::Missing { .. } => None,
        }
    }

    pub fn trailing_token(&self) -> Option<PositionedToken<'a>> {
        match self {
            PositionedValue::TokenValue(t) | PositionedValue::TokenSpan(_, t) => Some(*t),
            PositionedValue::Missing { .. } => None,
        }
    }

    /// The value of a token node. The end of the file, and tokens with
    /// nothing in them, count as missing.
    pub(crate) fn from_token(token: PositionedToken<'a>) -> Self {
        if token.kind() == TokenKind::EndOfFile || token.full_width() == 0 {
            PositionedValue::Missing {
                offset: token.end_offset(),
            }
        } else {
            PositionedValue::TokenValue(token)
        }
    }

    /// The value of a node with the given children: from the first child
    /// that has tokens to the last one that has. A node whose children are
    /// all missing is missing where the first one is.
    pub(crate) fn from_values<'b>(children: impl Iterator<Item = &'b Self>, fallback: usize) -> Self
    where
        'a: 'b,
    {
        let mut first_missing = None;
        let mut first = None;
        let mut last = None;
        for value in children {
            match value {
                PositionedValue::Missing { offset } => {
                    first_missing.get_or_insert(*offset);
                }
                _ => {
                    first.get_or_insert(value);
                    last = Some(value);
                }
            }
        }
        match (first, last) {
            (Some(first), Some(last)) => {
                let left = first.leading_token().unwrap();
                let right = last.trailing_token().unwrap();
                if PositionedToken::inner_ptr_eq(&left, &right) {
                    PositionedValue::TokenValue(left)
                } else {
                    PositionedValue::TokenSpan(left, right)
                }
            }
            _ => PositionedValue::Missing {
                offset: first_missing.unwrap_or(fallback),
            },
        }
    }
}
