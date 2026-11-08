//! Node sizes are part of the design: the tree is meant to stay compact.
//! If one of these fails, a change made a hot node fatter; either undo that
//! or move the new data behind a reference.

use hakana_parser::ast::*;
use std::mem::size_of;

#[test]
fn hot_nodes_stay_small() {
    assert_eq!(size_of::<Expr>(), 32);
    assert_eq!(size_of::<Stmt>(), 32);
    assert_eq!(size_of::<Hint>(), 32);
    assert_eq!(size_of::<Id>(), 24);
    assert!(size_of::<Call>() <= 72);
    assert!(size_of::<ShapeField>() <= 64);
}

/// The analysis tree keeps the shape of HHVM's, but not its weight: a
/// position is a byte range (lines are looked up when needed), and the
/// payloads of statements and expressions sit behind references.
#[test]
fn analysis_tree_nodes_stay_small() {
    use hakana_parser::aast;
    assert_eq!(size_of::<aast::Pos>(), 8);
    assert_eq!(size_of::<aast::Expr>(), 32);
    assert_eq!(size_of::<aast::Stmt>(), 32);
    assert_eq!(size_of::<aast::Hint>(), 16);
    assert_eq!(size_of::<aast::Id>(), 24);
}
