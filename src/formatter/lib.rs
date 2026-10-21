//! Hackfmt-compatible formatting using HHVM's full-fidelity Rust parser.
pub mod cli;
mod doc;
mod engine;
mod ranges;
mod syntax;
mod transform;
/// Hackfmt options, with the same defaults as HHVM's `Format_env`.
#[derive(Clone, Debug)]
pub struct Config {
    pub indent_width: usize,
    pub line_width: usize,
    pub tabs: bool,
    pub add_trailing_commas: bool,
    pub format_generated_code: bool,
    pub formatting_version: Option<u32>,
    pub enable_xhp_class_modifier: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            indent_width: 2,
            line_width: 80,
            tabs: false,
            add_trailing_commas: true,
            format_generated_code: false,
            formatting_version: None,
            enable_xhp_class_modifier: false,
        }
    }
}
impl Config {
    pub(crate) fn version(&self, n: u32) -> bool {
        self.formatting_version.is_none_or(|v| v >= n)
    }
}
/// A parse or argument error. Exit codes follow hackfmt's command-line protocol.
#[derive(Debug)]
pub struct FormatError {
    pub message: String,
    pub exit_code: i32,
}
impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for FormatError {}
pub(crate) fn parse(
    text: &str,
    config: &Config,
) -> Result<(syntax::Node, Vec<engine::Group>), FormatError> {
    let (root, mut errors) = syntax::parse(text, config.enable_xhp_class_modifier);
    if !errors.is_empty() {
        errors.reverse();
        errors.sort_by_key(|e| (e.start_offset, e.end_offset));
        errors.dedup();
        let position = |offset: usize| {
            let prefix = &text.as_bytes()[..offset.min(text.len())];
            (
                1 + prefix.iter().filter(|b| **b == b'\n').count(),
                prefix
                    .iter()
                    .rposition(|b| *b == b'\n')
                    .map_or(prefix.len() + 1, |i| prefix.len() - i),
            )
        };
        let mut messages = vec![];
        for e in errors {
            let (sl, sc) = position(e.start_offset);
            let (el, ec) = position(e.end_offset);
            messages.push(format!("({sl},{sc})-({el},{ec}) {}", e.message));
        }
        messages.push("File failed to parse without errors".into());
        return Err(FormatError {
            message: messages.join("\n"),
            exit_code: 2,
        });
    }
    let doc = transform::Formatter { env: config }.t(&root);
    let groups = engine::build(config, &doc);
    Ok((root, groups))
}
/// Format a complete file, preserving generated sections and suppression comments.
/// Source may include `<?hh` or use Hakana's tagless `.hack` convention.
pub fn format(text: &str, config: &Config) -> Result<String, FormatError> {
    let (root, groups) = parse(text, config)?;
    Ok(ranges::format_range(
        config,
        &root,
        text,
        &groups,
        (0, text.len()),
    ))
}
/// Format a zero-based, half-open byte range and return only its replacement text.
/// Tokens crossing a boundary are indivisible. Unlike the CLI, this does not
/// expand the selection to complete lines or append a terminal newline.
pub fn format_range(
    text: &str,
    config: &Config,
    start: usize,
    end: usize,
) -> Result<String, FormatError> {
    if start > end || end > text.len() {
        return Err(FormatError {
            message: "Invalid byte range".into(),
            exit_code: 3,
        });
    }
    let (root, groups) = parse(text, config)?;
    Ok(ranges::format_range(
        config,
        &root,
        text,
        &groups,
        (start, end),
    ))
}
/// Format the construct ending at `offset`, the last byte of a token.
/// Returns the source byte range to replace and its formatted contents.
pub fn format_at_offset(
    text: &str,
    config: &Config,
    offset: usize,
) -> Result<((usize, usize), String), FormatError> {
    let (root, groups) = parse(text, config)?;
    ranges::at_offset(config, &root, text, &groups, offset).map_err(|message| FormatError {
        message,
        exit_code: 3,
    })
}
/// Format inclusive, one-based line intervals, returning the complete file.
/// Text outside the expanded token boundaries is preserved.
pub fn format_intervals(
    text: &str,
    config: &Config,
    intervals: &[(usize, usize)],
) -> Result<String, FormatError> {
    let (root, groups) = parse(text, config)?;
    ranges::intervals(config, &root, text, &groups, intervals).map_err(|message| FormatError {
        message,
        exit_code: 4,
    })
}
