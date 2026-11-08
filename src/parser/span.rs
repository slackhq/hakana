//! Source positions.
//!
//! AST nodes carry byte offsets only. Line and column information is derived
//! on demand from a [`LineIndex`], which costs four bytes per line instead of
//! six machine words per node.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const NONE: Span = Span {
        start: u32::MAX,
        end: u32::MAX,
    };

    #[inline]
    pub fn new(start: u32, end: u32) -> Span {
        Span { start, end }
    }

    #[inline]
    pub fn is_none(self) -> bool {
        self.start == u32::MAX
    }

    #[inline]
    pub fn text(self, src: &str) -> &str {
        &src[self.start as usize..self.end as usize]
    }
}

/// Maps byte offsets to lines.
///
/// The table matches HHVM's `LineBreakMap` entry for entry, including its
/// habit of appending the file length as a final pseudo line start when the
/// file does not end with a newline, so positions computed here are identical
/// to the ones HHVM reports.
pub struct LineIndex {
    line_starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &[u8]) -> LineIndex {
        let len = text.len();
        let mut line_starts = Vec::with_capacity(len / 40 + 2);
        line_starts.push(0u32);
        let mut i = 0;
        while i < len {
            let b = text[i];
            if b == b'\n' || (b == b'\r' && text.get(i + 1) != Some(&b'\n')) {
                line_starts.push((i + 1) as u32);
            }
            i += 1;
        }
        if line_starts.len() > 1 && *line_starts.last().unwrap() as usize != len {
            line_starts.push(len as u32);
        }
        LineIndex { line_starts }
    }

    /// The offset at which each line starts.
    pub fn line_starts(&self) -> &[u32] {
        &self.line_starts
    }

    /// Returns `(line, line_start_offset)`; lines are 1-based.
    #[inline]
    pub fn line_of(&self, offset: u32) -> (u32, u32) {
        let idx = self.line_starts.partition_point(|&s| s <= offset) - 1;
        ((idx + 1) as u32, self.line_starts[idx])
    }

    /// Like [`line_of`](Self::line_of), but starts the search from a hint.
    /// Lookups during a tree walk are nearly sorted, so this is usually O(1).
    #[inline]
    pub fn line_of_hinted(&self, offset: u32, hint: &mut usize) -> (u32, u32) {
        let starts = &self.line_starts;
        let mut idx = (*hint).min(starts.len() - 1);
        if starts[idx] <= offset {
            // Walk forward a few lines before falling back to a binary search.
            let mut steps = 0;
            while idx + 1 < starts.len() && starts[idx + 1] <= offset {
                idx += 1;
                steps += 1;
                if steps > 8 {
                    idx = starts.partition_point(|&s| s <= offset) - 1;
                    break;
                }
            }
        } else {
            idx = starts.partition_point(|&s| s <= offset) - 1;
        }
        *hint = idx;
        ((idx + 1) as u32, starts[idx])
    }

    /// 1-based `(line, column)` for an offset.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let (line, start) = self.line_of(offset);
        (line, offset - start + 1)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}
