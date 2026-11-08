//! The lossless syntax tree with typed, named children: what linters and
//! other syntax-level tools read.
//!
//! The parser records a plain tree ([`hakana_parser::cst`]); this crate puts
//! the API of HHVM's full-fidelity tree on top of it — `SyntaxVariant` with a
//! struct of named children per node kind, positioned tokens, and the offset
//! arithmetic of `SyntaxTrait` — because that is the API the HHAST-style
//! linters are written against. The node types are generated from HHVM's
//! definitions (`src/parser/cst/generate.py`); nothing here depends on HHVM.
//!
//! ```ignore
//! let arena = bumpalo::Bump::new();
//! let source = SourceText::make("a.hack", contents.as_bytes());
//! let (root, errors) = hakana_syntax::parse_script(&arena, &source, ParserEnv::default());
//! if let SyntaxVariant::Script(script) = &root.children { /* ... */ }
//! ```

mod convert;
pub mod lexable_token;
pub mod line_break_map;
pub mod parser_env;
pub mod source_text;
pub mod syntax_error;
pub mod syntax_trait;

pub mod syntax_kind {
    pub use hakana_parser::cst::SyntaxKind;
}

pub mod token_kind {
    pub use hakana_parser::cst::TokenKind;
}

pub mod trivia_kind {
    pub use hakana_parser::cst::TriviaKind;
}

pub mod syntax_by_ref {
    pub mod positioned_syntax;
    pub mod positioned_token;
    pub mod positioned_value;
    pub mod syntax;
    pub mod syntax_variant_generated;
}

pub use convert::{from_cst, parse_script};
