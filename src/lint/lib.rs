//! HHAST-equivalent linting and migration framework for Hakana
//!
//! This module provides a framework for building linters and migrators that work with
//! the full-fidelity AST from HHVM's parser. Unlike Hakana's main analysis which uses
//! the higher-level oxidized AST, this framework operates on the full-fidelity syntax
//! tree that preserves all trivia (whitespace, comments) and enables precise code
//! transformations.
//!
//! # Architecture
//!
//! - **Linters**: Analyze code and report issues, optionally with auto-fixes
//! - **Migrators**: Multi-pass code transformations for large-scale refactoring
//! - **Visitors**: Traverse the syntax tree with pattern matching on node types
//! - **Edits**: Collect and apply text replacements safely
//!
//! # Example
//!
//! ```rust,ignore
//! use hakana_lint::{Linter, LintError, LintContext};
//!
//! struct MyLinter;
//!
//! impl Linter for MyLinter {
//!     fn lint_function_declaration(
//!         &self,
//!         ctx: &LintContext,
//!         node: &FunctionDeclarationChildren,
//!     ) -> Vec<LintError> {
//!         // Check function and return errors
//!         vec![]
//!     }
//! }
//! ```

pub mod context;
pub mod edit;
pub mod error;
pub mod examples;
pub mod hhast_config;
pub mod linter;
pub mod migrator;
pub mod runner;
pub mod visitor;

pub use context::LintContext;
pub use edit::{Edit, EditSet};
pub use error::{LintError, Severity};
pub use hhast_config::{HhastLintConfig, map_hhast_linter_to_hakana};
pub use linter::Linter;
pub use migrator::Migrator;
pub use runner::{LintConfig, LintResult, run_linters};
pub use visitor::SyntaxVisitor;

use parser_core_types::source_text::SourceText;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;

/// Parse a Hack file into a full-fidelity syntax tree
pub fn parse_file<'a>(
    arena: &'a bumpalo::Bump,
    source: &SourceText<'a>,
) -> (
    PositionedSyntax<'a>,
    Vec<parser_core_types::syntax_error::SyntaxError>,
) {
    let mut env = parser_core_types::parser_env::ParserEnv::default();
    // Enable XHP support
    env.enable_xhp_class_modifier = true;
    env.disable_xhp_element_mangling = false;
    env.disable_xhp_children_declarations = false;

    let (root, errors, _state) = positioned_by_ref_parser::parse_script(arena, source, env);
    (root, errors)
}
