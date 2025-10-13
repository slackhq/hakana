//! Linter: Detect trailing whitespace at end of lines
//!
//! This is a port of HHAST's NoWhitespaceAtEndOfLineLinter.
//! It detects and removes trailing whitespace characters (spaces, tabs) at the end of lines.

use crate::{Edit, EditSet, LintContext, LintError, Linter, Severity};

pub struct NoWhitespaceAtEndOfLineLinter;

impl Linter for NoWhitespaceAtEndOfLineLinter {
    fn name(&self) -> &'static str {
        "no-whitespace-at-end-of-line"
    }

    fn hhast_name(&self) -> Option<&'static str> {
        Some("Facebook\\HHAST\\NoWhitespaceAtEndOfLineLinter")
    }

    fn description(&self) -> &'static str {
        "Detects and removes trailing whitespace at the end of lines"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let source = ctx.source.text();
        let mut errors = Vec::new();

        let mut line_start = 0;
        for (line_num, line) in source.split(|&b| b == b'\n').enumerate() {
            let line_end = line_start + line.len();

            // Find trailing whitespace
            let trimmed_len = line
                .iter()
                .rev()
                .take_while(|&&b| b == b' ' || b == b'\t')
                .count();

            if trimmed_len > 0 {
                let ws_start = line_end - trimmed_len;
                let ws_end = line_end;

                let mut error = LintError::new(
                    Severity::Warning,
                    format!(
                        "Line {} has {} trailing whitespace character(s)",
                        line_num + 1,
                        trimmed_len
                    ),
                    ws_start,
                    ws_end,
                    self.name(),
                );

                // Add auto-fix
                if ctx.allow_auto_fix {
                    let mut fix = EditSet::new();
                    fix.add(Edit::delete(ws_start, ws_end));
                    error = error.with_fix(fix);
                }

                errors.push(error);
            }

            // Move to next line (account for newline character)
            line_start = line_end + 1;
        }

        errors
    }

    fn supports_auto_fix(&self) -> bool {
        true
    }
}

impl NoWhitespaceAtEndOfLineLinter {
    pub fn new() -> Self {
        Self
    }
}
