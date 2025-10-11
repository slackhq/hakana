//! Lint error representation

use super::edit::EditSet;
use std::fmt;

/// Severity level of a lint error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Error that should block code from running
    Error,
    /// Warning that should be fixed but doesn't block
    Warning,
    /// Informational suggestion
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "ERROR"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

/// A lint error with optional auto-fix
#[derive(Debug, Clone)]
pub struct LintError {
    /// The severity level
    pub severity: Severity,

    /// Human-readable error message
    pub message: String,

    /// Start byte offset in the file
    pub start_offset: usize,

    /// End byte offset in the file
    pub end_offset: usize,

    /// Optional auto-fix edits
    pub fix: Option<EditSet>,

    /// The linter that generated this error
    pub linter_name: &'static str,
}

impl LintError {
    /// Create a new lint error
    pub fn new(
        severity: Severity,
        message: impl Into<String>,
        start_offset: usize,
        end_offset: usize,
        linter_name: &'static str,
    ) -> Self {
        Self {
            severity,
            message: message.into(),
            start_offset,
            end_offset,
            fix: None,
            linter_name,
        }
    }

    /// Add an auto-fix to this error
    pub fn with_fix(mut self, fix: EditSet) -> Self {
        self.fix = Some(fix);
        self
    }
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({}..{}): {}",
            self.linter_name, self.severity, self.start_offset, self.end_offset, self.message
        )
    }
}
