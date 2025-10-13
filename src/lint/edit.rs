//! Text edit representation for auto-fixes

use std::fmt;

/// A single text edit (replacement at a byte range)
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
        self.edits.is_empty()
    }

    /// Apply all edits to a source string
    pub fn apply(&self, source: &str) -> Result<String, String> {
        let mut result = String::new();
        let mut last_pos = 0;

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

        for edit in edits {
            if edit.start < last_pos {
                return Err(format!("Edit starts before previous edit ended: {}", edit));
            }

            // Add unchanged text before this edit
            result.push_str(&source[last_pos..edit.start]);

            // Add replacement text
            result.push_str(&edit.replacement);

            last_pos = edit.end;
        }

        // Add remaining text
        result.push_str(&source[last_pos..]);

        Ok(result)
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
        edits.add(Edit::delete(5, 16));  // Delete " beautiful"

        let result = edits.apply(source).unwrap();
        assert_eq!(result, "helloworld");
    }
}
