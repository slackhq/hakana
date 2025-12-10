//! Unified text edit representation for auto-fixes, migrations, and code transformations.
//!
//! This module provides a single edit system used by both the linter and analyzer
//! for all code modifications.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

/// The kind of edit operation to perform
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditKind {
    /// Simple text substitution
    Substitute(String),

    /// Remove the text at the specified range
    Remove,

    /// Remove the text and also trim preceding whitespace back to line_start offset.
    /// If the preceding content (from line_start to start) is all whitespace,
    /// the whitespace and any preceding newline are also removed.
    TrimPrecedingWhitespace {
        /// The offset of the beginning of the line
        line_start: u32,
    },

    /// Like TrimPrecedingWhitespace but also removes a trailing comma if present
    TrimPrecedingWhitespaceAndTrailingComma {
        /// The offset of the beginning of the line
        line_start: u32,
    },

    /// Remove the text but preserve non-whitespace content between end and line_end
    TrimTrailingWhitespace {
        /// The offset of the end of the line
        line_end: u32,
    },
}

/// A single text edit (replacement at a byte range)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Start byte offset
    pub start: u32,
    /// End byte offset (exclusive)
    pub end: u32,
    /// The kind of edit operation
    pub kind: EditKind,
}

impl Edit {
    /// Create a new substitution edit
    pub fn new(start: u32, end: u32, replacement: impl Into<String>) -> Self {
        Self {
            start,
            end,
            kind: EditKind::Substitute(replacement.into()),
        }
    }

    /// Create an insertion at an offset
    pub fn insert(offset: u32, text: impl Into<String>) -> Self {
        Self::new(offset, offset, text)
    }

    /// Create a deletion of a range
    pub fn delete(start: u32, end: u32) -> Self {
        Self {
            start,
            end,
            kind: EditKind::Remove,
        }
    }

    /// Create a removal that also trims preceding whitespace
    pub fn delete_with_preceding_whitespace(start: u32, end: u32, line_start: u32) -> Self {
        Self {
            start,
            end,
            kind: EditKind::TrimPrecedingWhitespace { line_start },
        }
    }

    /// Create a removal that trims preceding whitespace and trailing comma
    pub fn delete_with_preceding_whitespace_and_trailing_comma(
        start: u32,
        end: u32,
        line_start: u32,
    ) -> Self {
        Self {
            start,
            end,
            kind: EditKind::TrimPrecedingWhitespaceAndTrailingComma { line_start },
        }
    }

    /// Create a removal that trims trailing whitespace
    pub fn delete_with_trailing_whitespace(start: u32, end: u32, line_end: u32) -> Self {
        Self {
            start,
            end,
            kind: EditKind::TrimTrailingWhitespace { line_end },
        }
    }

    /// Check if this edit overlaps with another
    pub fn overlaps(&self, other: &Edit) -> bool {
        // Check if ranges overlap
        (self.start >= other.start && self.start < other.end)
            || (self.end > other.start && self.end <= other.end)
            || (other.start >= self.start && other.start < self.end)
            || (other.end > self.start && other.end <= self.end)
    }
}

impl fmt::Display for Edit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            EditKind::Substitute(s) => write!(f, "{}..{} -> {:?}", self.start, self.end, s),
            EditKind::Remove => write!(f, "{}..{} -> (remove)", self.start, self.end),
            EditKind::TrimPrecedingWhitespace { line_start } => {
                write!(
                    f,
                    "{}..{} -> (remove, trim preceding from {})",
                    self.start, self.end, line_start
                )
            }
            EditKind::TrimPrecedingWhitespaceAndTrailingComma { line_start } => {
                write!(
                    f,
                    "{}..{} -> (remove, trim preceding from {}, trim trailing comma)",
                    self.start, self.end, line_start
                )
            }
            EditKind::TrimTrailingWhitespace { line_end } => {
                write!(
                    f,
                    "{}..{} -> (remove, trim trailing to {})",
                    self.start, self.end, line_end
                )
            }
        }
    }
}

/// A file operation (creation, deletion, etc.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOperation {
    /// Type of operation
    pub op_type: FileOpType,
    /// Target file path (relative to the original file or absolute)
    pub path: PathBuf,
    /// Content for file creation
    pub content: Option<String>,
}

/// Type of file operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOpType {
    /// Create a new file
    Create,
    /// Delete an existing file
    Delete,
}

impl FileOperation {
    /// Create a new file operation
    pub fn create_file(path: PathBuf, content: String) -> Self {
        Self {
            op_type: FileOpType::Create,
            path,
            content: Some(content),
        }
    }

    /// Delete a file operation
    pub fn delete_file(path: PathBuf) -> Self {
        Self {
            op_type: FileOpType::Delete,
            path,
            content: None,
        }
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

    /// Try to add an edit, returning false if it would overlap with existing edits
    pub fn try_add(&mut self, edit: Edit) -> bool {
        for existing in &self.edits {
            if edit.overlaps(existing) {
                return false;
            }
        }
        self.edits.push(edit);
        true
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

    /// Get the number of edits
    pub fn len(&self) -> usize {
        self.edits.len()
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

    /// Apply all edits to a source string.
    ///
    /// This applies edits in reverse order (from end to start) to maintain
    /// correct offsets as the string is modified.
    pub fn apply(&self, source: &str) -> Result<String, String> {
        let edits = self.edits();

        // Check for overlapping edits
        for window in edits.windows(2) {
            if window[0].end > window[1].start {
                return Err(format!(
                    "Overlapping edits: {} and {}",
                    window[0], window[1]
                ));
            }
        }

        // Apply in reverse order to maintain correct offsets
        let mut result = source.to_string();
        for edit in edits.iter().rev() {
            result = apply_single_edit(&result, edit)?;
        }

        Ok(result)
    }

    /// Merge insertions at the same offset into the edit set.
    /// Multiple insertions at the same point are concatenated.
    pub fn merge_insertions(&mut self, insertions: BTreeMap<u32, Vec<String>>) {
        for (offset, texts) in insertions {
            // Insertions at same point are applied in reverse order per original behavior
            for text in texts.into_iter().rev() {
                self.edits.push(Edit::insert(offset, text));
            }
        }
    }
}

impl From<Edit> for EditSet {
    fn from(edit: Edit) -> Self {
        let mut set = EditSet::new();
        set.add(edit);
        set
    }
}

/// Apply a single edit to the source string
fn apply_single_edit(source: &str, edit: &Edit) -> Result<String, String> {
    let start = edit.start as usize;
    let end = edit.end as usize;

    if start > source.len() || end > source.len() {
        return Err(format!(
            "Edit offset out of bounds: {} (source length: {})",
            edit,
            source.len()
        ));
    }

    match &edit.kind {
        EditKind::Substitute(replacement) => {
            Ok(source[..start].to_string() + replacement + &source[end..])
        }

        EditKind::Remove => Ok(source[..start].to_string() + &source[end..]),

        EditKind::TrimPrecedingWhitespace { line_start } => {
            let mut actual_start = start;
            let line_start_usize = *line_start as usize;

            // Check if content between line_start and start is all whitespace
            let potential_whitespace = &source[line_start_usize..start];
            if potential_whitespace.chars().all(|c| c.is_whitespace()) {
                actual_start = line_start_usize;

                // Also remove preceding newline if present
                if line_start_usize > 0 && &source[line_start_usize - 1..line_start_usize] == "\n" {
                    actual_start = line_start_usize - 1;
                }
            }

            Ok(source[..actual_start].to_string() + &source[end..])
        }

        EditKind::TrimPrecedingWhitespaceAndTrailingComma { line_start } => {
            let mut actual_start = start;
            let mut actual_end = end;
            let line_start_usize = *line_start as usize;

            // Check if content between line_start and start is all whitespace
            let potential_whitespace = &source[line_start_usize..start];
            if potential_whitespace.chars().all(|c| c.is_whitespace()) {
                actual_start = line_start_usize;

                // Also remove preceding newline if present
                if line_start_usize > 0 && &source[line_start_usize - 1..line_start_usize] == "\n" {
                    actual_start = line_start_usize - 1;
                }
            }

            // Remove trailing comma if present
            if actual_end < source.len() && &source[actual_end..actual_end + 1] == "," {
                actual_end += 1;
            }

            Ok(source[..actual_start].to_string() + &source[actual_end..])
        }

        EditKind::TrimTrailingWhitespace { line_end } => {
            let line_end_usize = *line_end as usize;

            // Get content between end and line_end, trim it
            let potential_whitespace = &source[end..line_end_usize];
            let trimmed = potential_whitespace.trim();

            Ok(source[..start].to_string() + trimmed + &source[line_end_usize..])
        }
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

    #[test]
    fn test_trim_preceding_whitespace() {
        let source = "line1\n    to_remove\nline3";
        let mut edits = EditSet::new();
        // Remove "to_remove" but also the preceding whitespace and newline
        edits.add(Edit::delete_with_preceding_whitespace(10, 19, 6)); // line_start=6 (after \n)

        let result = edits.apply(source).unwrap();
        assert_eq!(result, "line1\nline3");
    }

    #[test]
    fn test_overlapping_edits_rejected() {
        let source = "hello world";
        let mut edits = EditSet::new();
        edits.add(Edit::new(0, 8, "goodbye"));
        edits.add(Edit::new(6, 11, "rust"));

        let result = edits.apply(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_try_add_prevents_overlap() {
        let mut edits = EditSet::new();
        assert!(edits.try_add(Edit::new(0, 8, "goodbye")));
        assert!(!edits.try_add(Edit::new(6, 11, "rust"))); // Should be rejected
        assert_eq!(edits.len(), 1);
    }
}
