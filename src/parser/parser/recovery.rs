//! Error recovery.
//!
//! A file that fails to parse is parsed a second time in a forgiving mode,
//! so that what it declares stays known while it is broken (which, in an
//! editor, is most of the time). In that mode:
//!
//! * function and method bodies are not parsed at all, just skipped;
//! * a declaration or class member that fails to parse is dropped, and
//!   parsing resumes at the next one.
//!
//! Both rely on layout rather than on tokens — a broken file is exactly the
//! one whose braces cannot be trusted. A body ends at the first `}` indented
//! like the line its `{` is on, and declarations and members start a line at
//! a known indentation. That is how formatted Hack looks; for anything else
//! recovery finds less, never something wrong enough to matter, since the
//! file is reported as invalid either way.

use super::{Parser, R};
use crate::ast::Block;
use crate::token::T;

/// Words that start a declaration at the top level of a file.
const TOP_LEVEL_STARTS: &[&str] = &[
    "function",
    "async",
    "class",
    "abstract",
    "final",
    "interface",
    "trait",
    "enum",
    "type",
    "newtype",
    "namespace",
    "use",
    "const",
    "module",
    "case",
    "newctx",
    "new",
    "internal",
    "public",
    "xhp",
    "<<",
];

/// Words that start a member of a class.
const MEMBER_STARTS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "static",
    "abstract",
    "final",
    "readonly",
    "async",
    "function",
    "const",
    "use",
    "require",
    "attribute",
    "children",
    "category",
    "case",
    "<<",
];

impl<'a> Parser<'a> {
    /// The offset of the start of the line containing `pos`.
    fn line_start_of(&self, pos: usize) -> usize {
        self.bytes[..pos.min(self.bytes.len())]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1)
    }

    /// The offset just past the line containing `pos` (past its newline).
    fn next_line_start(&self, pos: usize) -> usize {
        let pos = pos.min(self.bytes.len());
        self.bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(self.bytes.len(), |i| pos + i + 1)
    }

    /// The whitespace a line starts with.
    fn indent_at(&self, line_start: usize) -> &'a [u8] {
        let bytes: &'a [u8] = self.bytes;
        let len = bytes[line_start..]
            .iter()
            .take_while(|&&b| b == b' ' || b == b'\t')
            .count();
        &bytes[line_start..line_start + len]
    }

    fn starts_with_any(&self, at: usize, words: &[&str]) -> bool {
        let rest = &self.bytes[at..];
        words.iter().any(|word| {
            rest.starts_with(word.as_bytes())
                && (word.ends_with('<')
                    || !rest
                        .get(word.len())
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_'))
        })
    }

    /// Skips a function body without parsing it. The current token is its
    /// `{`.
    pub(crate) fn skip_function_body(&mut self) -> R<Block<'a>> {
        if !self.at(T::LBrace) {
            return self.fail("A left brace `{` is expected here.");
        }
        let open = self.tok.start as usize;
        let line_start = self.line_start_of(open);
        let line_end = self.next_line_start(open);

        // `{ ... }` on one line.
        let mut depth = 0usize;
        for (i, &b) in self.bytes[open..line_end].iter().enumerate() {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.jump_to(open + i + 1);
                        return Ok(&[]);
                    }
                }
                _ => {}
            }
        }

        // Otherwise the body ends at a `}` indented like the line of its `{`.
        let indent = self.indent_at(line_start);
        let mut line = line_end;
        while line < self.bytes.len() {
            let line_indent = self.indent_at(line);
            if line_indent == indent && self.bytes.get(line + indent.len()) == Some(&b'}') {
                self.jump_to(line + indent.len() + 1);
                return Ok(&[]);
            }
            line = self.next_line_start(line);
        }
        self.jump_to(self.bytes.len());
        Ok(&[])
    }

    /// After a top-level declaration starting at `decl_start` failed to
    /// parse: resumes at the next line that starts one.
    pub(crate) fn resync_top_level(&mut self, decl_start: usize) {
        let failed_at = self.error.map_or(0, |(at, _)| at as usize);
        self.clear_failure();
        // Always move past the line the declaration started on.
        let mut line = self.next_line_start(failed_at.max(decl_start));
        while line < self.bytes.len() {
            if self.starts_with_any(line, TOP_LEVEL_STARTS) {
                break;
            }
            line = self.next_line_start(line);
        }
        self.jump_to(line.min(self.bytes.len()));
    }

    /// After a class member starting at `member_start` failed to parse:
    /// resumes at the next member, or at the `}` that ends the class.
    pub(crate) fn resync_member(&mut self, member_start: usize) {
        let failed_at = self.error.map_or(0, |(at, _)| at as usize);
        self.clear_failure();
        let indent = self.indent_at(self.line_start_of(member_start));
        let mut line = self.next_line_start(failed_at.max(member_start));
        while line < self.bytes.len() {
            let line_indent = self.indent_at(line);
            let content = line + line_indent.len();
            if line_indent.len() < indent.len() && self.bytes.get(content) == Some(&b'}') {
                // The end of the class.
                self.jump_to(content);
                return;
            }
            if line_indent == indent && self.starts_with_any(content, MEMBER_STARTS) {
                self.jump_to(content);
                return;
            }
            // A member's own closing brace: the next member follows it.
            if line_indent == indent && self.bytes.get(content) == Some(&b'}') {
                self.jump_to(content + 1);
                return;
            }
            line = self.next_line_start(line);
        }
        self.jump_to(self.bytes.len());
    }
}
