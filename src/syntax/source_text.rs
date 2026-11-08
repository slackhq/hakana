use std::sync::Arc;

/// The text of a file, and where it came from.
#[derive(Clone, Debug)]
pub struct SourceText<'a> {
    file_path: Arc<str>,
    text: &'a [u8],
}

impl<'a> SourceText<'a> {
    pub fn make(file_path: impl Into<Arc<str>>, text: &'a [u8]) -> Self {
        SourceText {
            file_path: file_path.into(),
            text,
        }
    }

    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn text(&self) -> &'a [u8] {
        self.text
    }

    pub fn text_as_str(&self) -> &'a str {
        // The parser only accepts UTF-8; anything else never gets a tree.
        std::str::from_utf8(self.text).unwrap_or("")
    }

    pub fn length(&self) -> usize {
        self.text.len()
    }

    /// `length` bytes from `start`, clamped to the text.
    pub fn sub(&self, start: usize, length: usize) -> &'a [u8] {
        let len = self.text.len();
        if start >= len {
            b""
        } else if start + length > len {
            &self.text[start..]
        } else {
            &self.text[start..start + length]
        }
    }

    pub fn sub_as_str(&self, start: usize, length: usize) -> &'a str {
        std::str::from_utf8(self.sub(start, length)).unwrap_or("")
    }
}
