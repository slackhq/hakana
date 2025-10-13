//! Linter trait and base implementations

use super::{LintContext, LintError, SyntaxVisitor};

/// Trait for implementing a linter
///
/// Linters analyze code and report issues. They can optionally provide auto-fixes.
///
/// # Example
///
/// ```rust,ignore
/// struct NoEchoLinter;
///
/// impl Linter for NoEchoLinter {
///     fn name(&self) -> &'static str {
///         "no-echo"
///     }
///
///     fn hhast_name(&self) -> Option<&'static str> {
///         Some("Facebook\\HHAST\\NoEchoLinter")
///     }
///
///     fn lint(&self, ctx: &LintContext) -> Vec<LintError> {
///         let mut visitor = NoEchoVisitor { ctx, errors: vec![] };
///         visitor::walk(&mut visitor, ctx.root);
///         visitor.errors
///     }
/// }
/// ```
pub trait Linter: Send + Sync {
    /// The unique name of this linter (kebab-case)
    fn name(&self) -> &'static str;

    /// The legacy HHAST linter class name, if this is a port of an HHAST linter
    fn hhast_name(&self) -> Option<&'static str> {
        None
    }

    /// Run the linter on a file and return any errors found
    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError>;

    /// Whether this linter supports auto-fixing
    fn supports_auto_fix(&self) -> bool {
        false
    }

    /// Short description of what this linter checks
    fn description(&self) -> &'static str {
        ""
    }
}

/// Helper for building visitor-based linters
///
/// This struct implements `SyntaxVisitor` and collects errors as it walks the tree.
pub struct VisitorBasedLinter<'a, V: SyntaxVisitor<'a>> {
    pub visitor: V,
    pub errors: Vec<LintError>,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a, V: SyntaxVisitor<'a>> VisitorBasedLinter<'a, V> {
    pub fn new(visitor: V) -> Self {
        Self {
            visitor,
            errors: Vec::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn add_error(&mut self, error: LintError) {
        self.errors.push(error);
    }

    pub fn into_errors(self) -> Vec<LintError> {
        self.errors
    }
}

/// Registry of all available linters
pub struct LinterRegistry {
    linters: Vec<Box<dyn Linter>>,
}

impl LinterRegistry {
    pub fn new() -> Self {
        Self {
            linters: Vec::new(),
        }
    }

    pub fn register(&mut self, linter: Box<dyn Linter>) {
        self.linters.push(linter);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Linter> {
        self.linters
            .iter()
            .find(|l| l.name() == name)
            .map(|l| l.as_ref())
    }

    pub fn get_by_hhast_name(&self, hhast_name: &str) -> Option<&dyn Linter> {
        self.linters
            .iter()
            .find(|l| l.hhast_name() == Some(hhast_name))
            .map(|l| l.as_ref())
    }

    pub fn all(&self) -> &[Box<dyn Linter>] {
        &self.linters
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.linters.iter().map(|l| l.name()).collect()
    }

    pub fn hhast_names(&self) -> Vec<&'static str> {
        self.linters.iter().filter_map(|l| l.hhast_name()).collect()
    }
}

impl Default for LinterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
