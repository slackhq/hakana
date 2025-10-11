//! Migrator trait for multi-pass code transformations

use super::{EditSet, LintContext};

/// Trait for implementing a migrator
///
/// Migrators are similar to linters but focus on code transformations.
/// They can perform multiple passes over the codebase and collect all
/// necessary edits before applying them.
pub trait Migrator: Send + Sync {
    /// The unique name of this migrator
    fn name(&self) -> &'static str;

    /// Analyze a file and collect edits
    ///
    /// Returns the set of edits to apply to this file, or None if no changes needed.
    fn migrate<'a>(&self, ctx: &LintContext<'a>) -> Option<EditSet>;

    /// Number of passes this migrator needs
    ///
    /// Multi-pass migrators are run multiple times over the codebase.
    /// This is useful for migrations that need to see the results of previous changes.
    fn num_passes(&self) -> usize {
        1
    }

    /// Short description of what this migrator does
    fn description(&self) -> &'static str {
        ""
    }

    /// Whether this migrator is safe to run automatically
    ///
    /// Some migrators may require manual review before running.
    fn is_safe(&self) -> bool {
        true
    }
}

/// Registry of all available migrators
pub struct MigratorRegistry {
    migrators: Vec<Box<dyn Migrator>>,
}

impl MigratorRegistry {
    pub fn new() -> Self {
        Self {
            migrators: Vec::new(),
        }
    }

    pub fn register(&mut self, migrator: Box<dyn Migrator>) {
        self.migrators.push(migrator);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Migrator> {
        self.migrators
            .iter()
            .find(|m| m.name() == name)
            .map(|m| m.as_ref())
    }

    pub fn all(&self) -> &[Box<dyn Migrator>] {
        &self.migrators
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.migrators.iter().map(|m| m.name()).collect()
    }
}

impl Default for MigratorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
