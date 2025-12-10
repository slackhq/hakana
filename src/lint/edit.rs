//! Lint-specific text edit wrappers
//!
//! This module provides a `usize`-based API on top of the unified edit types
//! from `hakana_code_info::edit`. The lint system uses `usize` for byte offsets
//! (idiomatic in Rust), while the core edit types use `u32` for memory efficiency.

use hakana_code_info::edit as core_edit;
use std::fmt;

// Re-export core types that don't need wrapping
pub use core_edit::{FileOpType, FileOperation};

/// A single text edit (replacement at a byte range)
///
/// This is a wrapper around the core Edit type that uses `usize` offsets
/// for ergonomic use in the lint system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Start byte offset
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
    /// Replacement text
    pub replacement: String,
}

impl Edit {
    /// Create a new edit
    pub fn new(start: usize, end: usize, replacement: impl Into<String>) -> Self {
        Self {
            start,
            end,
            replacement: replacement.into(),
        }
    }

    /// Create an insertion at an offset
    pub fn insert(offset: usize, text: impl Into<String>) -> Self {
        Self::new(offset, offset, text)
    }

    /// Create a deletion of a range
    pub fn delete(start: usize, end: usize) -> Self {
        Self::new(start, end, "")
    }

    /// Convert to a core Edit
    fn to_core_edit(&self) -> core_edit::Edit {
        core_edit::Edit::new(self.start as u32, self.end as u32, self.replacement.clone())
    }
}

impl fmt::Display for Edit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{} -> {:?}", self.start, self.end, self.replacement)
    }
}

/// A set of edits that can be applied to a file
#[derive(Debug, Clone, Default)]
pub struct EditSet {
    edits: Vec<Edit>,
    file_operations: Vec<FileOperation>,
}

impl EditSet {
    /// Create an empty edit set
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an edit to the set
    pub fn add(&mut self, edit: Edit) {
        self.edits.push(edit);
    }

    /// Get all edits, sorted by position (for safe application)
    pub fn edits(&self) -> Vec<Edit> {
        let mut edits = self.edits.clone();
        edits.sort_by_key(|e| (e.start, e.end));
        edits
    }

    /// Check if the edit set is empty
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.file_operations.is_empty()
    }

    /// Add a file operation to the set
    pub fn add_file_operation(&mut self, operation: FileOperation) {
        self.file_operations.push(operation);
    }

    /// Get all file operations
    pub fn file_operations(&self) -> &[FileOperation] {
        &self.file_operations
    }

    /// Check if there are any file operations
    pub fn has_file_operations(&self) -> bool {
        !self.file_operations.is_empty()
    }

    /// Apply all edits to a source string using the unified edit system
    pub fn apply(&self, source: &str) -> Result<String, String> {
        // Convert to core EditSet and apply
        let mut core_set = core_edit::EditSet::new();
        for edit in &self.edits {
            core_set.add(edit.to_core_edit());
        }
        core_set.apply(source)
    }
}

impl From<Edit> for EditSet {
    fn from(edit: Edit) -> Self {
        let mut set = EditSet::new();
        set.add(edit);
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_single_edit() {
        let source = "hello world";
        let mut edits = EditSet::new();
        edits.add(Edit::new(6, 11, "rust"));

        let result = edits.apply(source).unwrap();
        assert_eq!(result, "hello rust");
    }

    #[test]
    fn test_apply_multiple_edits() {
        let source = "hello world";
        let mut edits = EditSet::new();
        edits.add(Edit::new(0, 5, "goodbye"));
        edits.add(Edit::new(6, 11, "rust"));

        let result = edits.apply(source).unwrap();
        assert_eq!(result, "goodbye rust");
    }

    #[test]
    fn test_apply_insertion() {
        let source = "hello world";
        let mut edits = EditSet::new();
        edits.add(Edit::insert(5, " beautiful"));

        let result = edits.apply(source).unwrap();
        assert_eq!(result, "hello beautiful world");
    }

    #[test]
    fn test_apply_deletion() {
        let source = "hello beautiful world";
        let mut edits = EditSet::new();
        edits.add(Edit::delete(5, 16)); // Delete " beautiful"

        let result = edits.apply(source).unwrap();
        assert_eq!(result, "helloworld");
    }
}
