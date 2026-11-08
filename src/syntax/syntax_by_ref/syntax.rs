use std::iter::{empty, once};

use super::syntax_variant_generated::SyntaxVariant;
use crate::lexable_token::LexableToken;
use crate::syntax_kind::SyntaxKind;
use crate::token_kind::TokenKind;

/// A node: what it is (`children`) and where (`value`).
#[derive(Debug, Clone)]
pub struct Syntax<'a, T, V> {
    pub children: SyntaxVariant<'a, T, V>,
    pub value: V,
}

impl<'a, T, V> Syntax<'a, T, V> {
    pub fn make(children: SyntaxVariant<'a, T, V>, value: V) -> Self {
        Self { children, value }
    }

    pub fn get_token(&self) -> Option<&T> {
        match &self.children {
            SyntaxVariant::Token(t) => Some(t),
            _ => None,
        }
    }

    /// The node's children, in order.
    pub fn iter_children(&'a self) -> std::slice::Iter<'a, Syntax<'a, T, V>> {
        self.children.children().iter()
    }

    /// The items of a list. A node that is not a list counts as a list of
    /// itself, a missing one as an empty list.
    pub fn syntax_node_to_list(
        &self,
    ) -> Box<dyn DoubleEndedIterator<Item = &Syntax<'a, T, V>> + '_> {
        match &self.children {
            SyntaxVariant::SyntaxList(items) => Box::new(items.iter()),
            SyntaxVariant::Missing => Box::new(empty()),
            _ => Box::new(once(self)),
        }
    }

    /// Like [`Syntax::syntax_node_to_list`], with each item taken out of the
    /// `ListItem` that pairs it with its separator.
    pub fn syntax_node_to_list_skip_separator(
        &self,
    ) -> Box<dyn DoubleEndedIterator<Item = &Syntax<'a, T, V>> + '_> {
        match &self.children {
            SyntaxVariant::SyntaxList(items) => Box::new(items.iter().map(|n| match &n.children {
                SyntaxVariant::ListItem(i) => &i.item,
                _ => n,
            })),
            SyntaxVariant::Missing => Box::new(empty()),
            _ => Box::new(once(self)),
        }
    }
}

impl<'a, T: LexableToken, V> Syntax<'a, T, V> {
    pub fn kind(&self) -> SyntaxKind {
        match &self.children {
            SyntaxVariant::Token(t) => SyntaxKind::Token(t.kind()),
            other => other.node_kind().unwrap(),
        }
    }

    pub fn is_specific_token(&self, kind: TokenKind) -> bool {
        match &self.children {
            SyntaxVariant::Token(t) => t.kind() == kind,
            _ => false,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self.children, SyntaxVariant::Missing)
    }

    pub fn is_syntax_list(&self) -> bool {
        matches!(self.children, SyntaxVariant::SyntaxList(_))
    }
}
