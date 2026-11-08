//! Converting between byte offsets and line/column positions.

/// The offsets at which the lines of a text start.
#[derive(Debug)]
pub struct LineBreakMap {
    /// Starts with 0; if the text has any line break, ends with its length.
    lines: Box<[usize]>,
}

impl LineBreakMap {
    pub fn new(text: &[u8]) -> Self {
        let len = text.len();
        let mut lines = Vec::with_capacity(len / 80 + 1);
        lines.push(0);
        for (i, &byte) in text.iter().enumerate() {
            // `\r\n` is one line break.
            if byte == b'\n' || (byte == b'\r' && text.get(i + 1) != Some(&b'\n')) {
                lines.push(i + 1);
            }
        }
        if lines.len() > 1 && lines.last() != Some(&len) {
            lines.push(len);
        }
        LineBreakMap {
            lines: lines.into_boxed_slice(),
        }
    }

    /// `(line number from 1, offset of the start of that line, offset)`.
    pub fn offset_to_file_pos_triple(&self, offset: usize) -> (usize, usize, usize) {
        let index = self
            .lines
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        (index + 1, self.lines[index], offset)
    }

    /// `(line, column)`, both counted from 1.
    pub fn offset_to_position(&self, offset: isize) -> (isize, isize) {
        if offset < 0 {
            return (1, offset + 1);
        }
        let (line, line_start, offset) = self.offset_to_file_pos_triple(offset as usize);
        (line as isize, (offset - line_start) as isize + 1)
    }

    /// The offset of a position (line and column counted from 1). With
    /// `existing`, only if the position is within the text.
    pub fn position_to_offset(&self, existing: bool, line: isize, column: isize) -> Option<isize> {
        let len = self.lines.len() as isize;
        if line <= 0 || line > len {
            return None;
        }
        let line_start = self.lines[(line - 1) as usize] as isize;
        let offset = line_start + column - 1;
        let line_end = self.lines[std::cmp::min(len - 1, line) as usize] as isize;
        if !existing || (offset >= 0 && offset <= line_end) {
            Some(offset)
        } else {
            None
        }
    }

    pub fn offset_to_line_start_offset(&self, offset: isize) -> isize {
        offset - self.offset_to_position(offset).1 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::LineBreakMap;

    #[test]
    fn line_starts() {
        assert_eq!(*LineBreakMap::new(b"<?hh \r\nFoo Bar!").lines, [0, 7, 15]);
        assert_eq!(*LineBreakMap::new(b"<?hh \nFoo Bar!").lines, [0, 6, 14]);
        assert_eq!(*LineBreakMap::new(b"<?hh \r").lines, [0, 6]);
    }

    #[test]
    fn positions() {
        let map = LineBreakMap::new(b"ab\ncd\n");
        assert_eq!(map.offset_to_position(0), (1, 1));
        assert_eq!(map.offset_to_position(2), (1, 3));
        assert_eq!(map.offset_to_position(3), (2, 1));
        assert_eq!(map.position_to_offset(true, 2, 1), Some(3));
        assert_eq!(map.position_to_offset(true, 9, 1), None);
    }
}
