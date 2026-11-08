//! Hashing a piece of source text so that reformatting it does not change the
//! hash.

/// Hashes `src[start..end]`, ignoring comments and treating any run of
/// whitespace as a single separator (dropped altogether next to punctuation).
/// String literals are hashed verbatim.
///
/// Used to tell whether a definition changed between two runs: moving it,
/// re-indenting it or editing a comment inside it gives the same hash.
pub fn source_hash(src: &str, start: usize, end: usize) -> u64 {
    let b = src.as_bytes();
    let end = end.min(b.len());
    let mut h = Fnv::new();
    let mut i = start.min(end);
    // Whether whitespace or a comment was skipped since the last hashed byte.
    let mut gap = false;
    let mut prev = 0u8;
    while i < end {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => {
                gap = true;
                i += 1;
            }
            b'/' if i + 1 < end && b[i + 1] == b'/' => {
                while i < end && b[i] != b'\n' {
                    i += 1;
                }
                gap = true;
            }
            b'#' => {
                while i < end && b[i] != b'\n' {
                    i += 1;
                }
                gap = true;
            }
            b'/' if i + 1 < end && b[i + 1] == b'*' => {
                i += 2;
                while i < end && !(b[i] == b'*' && i + 1 < end && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(end);
                gap = true;
            }
            b'\'' | b'"' | b'`' => {
                if gap && joins(prev, c) {
                    h.byte(b' ');
                }
                gap = false;
                h.byte(c);
                i += 1;
                while i < end && b[i] != c {
                    if b[i] == b'\\' && i + 1 < end {
                        h.byte(b[i]);
                        i += 1;
                    }
                    h.byte(b[i]);
                    i += 1;
                }
                if i < end {
                    h.byte(c);
                    i += 1;
                }
                prev = c;
            }
            _ => {
                if gap && joins(prev, c) {
                    h.byte(b' ');
                }
                gap = false;
                h.byte(c);
                prev = c;
                i += 1;
            }
        }
    }
    h.finish()
}

/// Whether removing the whitespace between `a` and `b` could change how the
/// text is tokenised.
fn joins(a: u8, b: u8) -> bool {
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80;
    let op = |c: u8| b"+-*/<>=&|?:!.%^~\\".contains(&c);
    (word(a) && word(b)) || (op(a) && op(b))
}

/// FNV-1a: tiny, and stable across compiler versions and platforms.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    #[inline]
    fn byte(&mut self, b: u8) {
        self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::source_hash;

    fn h(s: &str) -> u64 {
        source_hash(s, 0, s.len())
    }

    #[test]
    fn ignores_layout_and_comments() {
        assert_eq!(h("const int A = 1;"), h("const   int A=1 ; // one"));
        assert_eq!(h("a + b"), h("a+b"));
    }

    #[test]
    fn keeps_meaningful_differences() {
        assert_ne!(h("const int A = 1;"), h("const int A = 2;"));
        assert_ne!(h("const string A = 'a b';"), h("const string A = 'ab';"));
        assert_ne!(h("newtype T = int;"), h("new type T = int;"));
        assert_ne!(h("$a - -$b"), h("$a --$b"));
    }
}
