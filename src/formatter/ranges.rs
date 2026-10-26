// Ported from HHVM hackfmt. Copyright (c) Meta Platforms, Inc. MIT.
use crate::{
    Config,
    engine::{self, Group, Range},
    syntax::Node,
};
use parser_core_types::{syntax_kind::SyntaxKind, trivia_kind::TriviaKind};
use std::sync::LazyLock;

static BEGIN_MANUAL_SECTION: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"/\* BEGIN MANUAL SECTION [^*]*\*/").unwrap());
pub fn lines(text: &str) -> Vec<Range> {
    let mut start = 0;
    let mut result = vec![];
    for line in text.split_terminator('\n') {
        let end = start + line.len() + 1;
        result.push((start, end));
        start = end;
    }
    if result.is_empty() {
        result.push((0, 1));
    }
    result
}
pub fn expand_lines(text: &str, r: Range) -> Range {
    let bytes = text.as_bytes();
    let mut end = r.1.max(r.0).min(text.len());
    let start = bytes[..r.0.min(end)]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |i| i + 1);
    // An end already at a line boundary stays there. Like lines(), count a
    // virtual newline after an unterminated final line.
    if end > 0 && bytes[end - 1] != b'\n' {
        end = bytes[end..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(text.len() + 1, |i| end + i + 1);
    }
    (start, end)
}
pub fn line_range(text: &str, start: usize, end: usize) -> Result<Range, String> {
    let bounds = lines(text);
    if start > end {
        return Err(format!("Illegal line interval: {start},{end}"));
    }
    if start < 1 || end < 1 || start > bounds.len() || end > bounds.len() {
        return Err(format!(
            "Can't format line interval {start},{end} in file with {} lines",
            bounds.len()
        ));
    }
    Ok((bounds[start - 1].0, bounds[end - 1].1))
}
fn union(mut ranges: Vec<Range>) -> Vec<Range> {
    ranges.sort();
    let mut result: Vec<Range> = vec![];
    for r in ranges {
        if let Some(prev) = result.last_mut().filter(|prev| engine::overlaps(**prev, r)) {
            prev.1 = prev.1.max(r.1);
        } else {
            result.push(r);
        }
    }
    result
}
fn subtract(mut ranges: Vec<Range>, remove: &[Range]) -> Vec<Range> {
    for &(s, e) in remove {
        ranges = ranges
            .into_iter()
            .flat_map(|(a, b)| {
                if !engine::overlaps((a, b), (s, e)) {
                    vec![(a, b)]
                } else {
                    let mut out = vec![];
                    if a < s {
                        out.push((a, s));
                    }
                    if b > e {
                        out.push((e, b));
                    }
                    out
                }
            })
            .collect();
    }
    ranges
}
fn suppressed(env: &Config, root: &Node, text: &str) -> Vec<Range> {
    let mut tokens = vec![];
    root.all_tokens(&mut tokens);
    let mut fixmes = vec![];
    let mut manual = vec![];
    let mut start = None;
    for token in tokens {
        for tr in token.leading.iter().chain(&token.trailing) {
            if tr.is_fixme_comment() {
                let (s, e) = expand_lines(text, (tr.start, tr.end()));
                fixmes.push(expand_lines(text, (s, e + 1)));
            }
            if tr.kind == TriviaKind::DelimitedComment {
                if BEGIN_MANUAL_SECTION.is_match(&tr.text) {
                    start = Some(tr.end());
                } else if tr.text == "/* END MANUAL SECTION */"
                    && let Some(s) = start.take()
                {
                    manual.push((s, tr.start));
                }
            }
        }
    }
    if !env.format_generated_code {
        if text.contains("@generated") {
            fixmes.push((0, text.len()));
        } else if text.contains("@partially-generated") {
            fixmes.extend(
                subtract(vec![(0, text.len())], &manual)
                    .into_iter()
                    .map(|r| expand_lines(text, r)),
            );
        }
    }
    union(fixmes)
}

pub fn atoms(groups: &[Group]) -> Vec<Range> {
    groups
        .iter()
        .flat_map(|g| &g.chunks)
        .flat_map(|c| &c.atoms)
        .map(|a| a.range)
        .collect()
}
pub fn expand_atoms(bounds: &[Range], r: Range) -> Range {
    (
        bounds
            .iter()
            .rev()
            .find(|b| b.0 <= r.0)
            .map_or(r.0, |b| b.0),
        bounds.iter().find(|b| b.1 >= r.1).map_or(r.1, |b| b.1),
    )
}
fn replace(text: &str, range: Range, replacements: Vec<(Range, String)>) -> String {
    let mut out = String::new();
    let mut seen = range.0;
    for ((s, e), replacement) in replacements {
        out.push_str(text.get(seen..s).unwrap_or(""));
        out.push_str(&replacement);
        seen = e;
    }
    out.push_str(text.get(seen..range.1).unwrap_or(""));
    out
}
pub fn format_range(
    env: &Config,
    root: &Node,
    text: &str,
    groups: &[Group],
    range: Range,
) -> String {
    let ranges = subtract(vec![range], &suppressed(env, root, text));
    let replacements = ranges
        .into_iter()
        .map(|r| (r, engine::solve(env, groups, Some(r), text, true, true)))
        .collect();
    replace(text, range, replacements)
}
pub fn intervals(
    env: &Config,
    root: &Node,
    text: &str,
    groups: &[Group],
    intervals: &[Range],
) -> Result<String, String> {
    let bounds = atoms(groups);
    let mut ranges = vec![];
    for &(s, e) in intervals {
        let (s, e) = line_range(text, s, e)?;
        ranges.push(expand_atoms(&bounds, (s.saturating_sub(1), e + 1)));
    }
    let ranges = subtract(union(ranges), &suppressed(env, root, text));
    let replacements = ranges
        .into_iter()
        .map(|r| {
            (
                r,
                engine::solve(
                    env,
                    groups,
                    Some(r),
                    text,
                    !bounds.iter().any(|b| b.0 == r.0),
                    !bounds.iter().any(|b| b.1 == r.1),
                ),
            )
        })
        .collect();
    let mut out = replace(text, (0, text.len()), replacements);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}
pub fn at_offset(
    env: &Config,
    root: &Node,
    text: &str,
    groups: &[Group],
    offset: usize,
) -> Result<(Range, String), String> {
    fn find<'a>(n: &'a Node, offset: usize, path: &mut Vec<&'a Node>) -> bool {
        path.push(n);
        if let Some(t) = &n.token {
            if offset >= t.start && offset < t.start + t.text.len() {
                return true;
            }
        } else {
            for c in &n.children {
                if find(c, offset, path) {
                    return true;
                }
            }
        }
        path.pop();
        false
    }
    let mut path = vec![];
    if !find(root, offset, &mut path) || path.len() < 2 {
        return Err("Invalid offset".into());
    }
    let token = path.pop().unwrap();
    if token.end() != offset + 1 {
        return Err("Offset must point to the last character of a token".into());
    }
    let mut parent = path.pop().unwrap();
    if parent.kind == SyntaxKind::CompoundStatement
        && let Some(grand) = path.last().filter(|n| n.kind != SyntaxKind::SyntaxList)
    {
        parent = grand;
    }
    let end = offset + 1;
    let start = expand_lines(text, (parent.start(), end)).0;
    let start = expand_atoms(&atoms(groups), (start, end)).0;
    let range = (start, end);
    Ok((
        range,
        engine::solve(env, groups, Some(range), text, false, false),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_expansion_matches_line_table_at_every_byte_boundary() {
        for text in ["", "\n", "abc", "abc\n", "\nabc\n\nx", "a\r\nb", "α🙂\nz"] {
            let bounds = lines(text);
            for start in 0..=text.len() + 2 {
                for end in 0..=text.len() + 2 {
                    let mut expected_end = end.max(start).min(text.len());
                    let mut expected_start = start.min(expected_end);
                    for &(s, e) in &bounds {
                        if s < expected_start && expected_start < e {
                            expected_start = s;
                        }
                        if s < expected_end && expected_end < e {
                            expected_end = e;
                        }
                    }
                    assert_eq!(
                        expand_lines(text, (start, end)),
                        (expected_start, expected_end),
                        "{text:?} at {start}..{end}"
                    );
                }
            }
        }
    }
}
