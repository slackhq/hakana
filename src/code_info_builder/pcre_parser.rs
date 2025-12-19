/// A simple PCRE pattern parser that extracts capture group information.
/// This only parses enough to identify capture groups - it does not validate
/// or compile the regex.

/// Represents a capture group found in a PCRE pattern.
pub struct CaptureGroup {
    /// The index of this capture group (0-based).
    pub index: usize,
    /// The name of this capture group, if it's a named group.
    pub name: Option<String>,
}

/// Parses a PCRE pattern and returns a list of capture groups.
///
/// This handles:
/// - Numbered capture groups: `(...)`
/// - Named capture groups: `(?<name>...)`, `(?P<name>...)`, `(?'name'...)`
/// - Non-capturing groups: `(?:...)` (skipped)
/// - Lookahead/lookbehind: `(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)` (skipped)
/// - Comments: `(?#...)` (skipped)
/// - Atomic groups: `(?>...)` (skipped - they don't capture in PCRE)
/// - Escaped characters: `\(`, `\)`, `\\`
/// - Character classes: `[...]` (parentheses inside don't count)
///
/// Note: Group 0 is always the full match (implicit in PCRE), and numbered
/// capture groups start at 1.
pub fn parse_capture_groups(pattern: &str) -> Vec<CaptureGroup> {
    let mut groups = Vec::new();

    // Group 0 is always the full match
    groups.push(CaptureGroup {
        index: 0,
        name: None,
    });

    // Capture groups are numbered starting from 1
    let mut group_index: usize = 1;
    let chars: Vec<char> = pattern.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut escaped = false;
    let mut in_char_class = false;

    while i < len {
        let c = chars[i];

        if escaped {
            escaped = false;
            i += 1;
            continue;
        }

        if c == '\\' {
            escaped = true;
            i += 1;
            continue;
        }

        if in_char_class {
            if c == ']' {
                in_char_class = false;
            }
            i += 1;
            continue;
        }

        if c == '[' {
            in_char_class = true;
            i += 1;
            continue;
        }

        if c == '(' {
            // Check what follows the opening paren
            if i + 1 < len && chars[i + 1] == '?' {
                // Special group - check what kind
                if i + 2 < len {
                    let next = chars[i + 2];

                    if next == ':' || next == '=' || next == '!' || next == '>' {
                        // Non-capturing (?:...), lookahead (?=...), (?!...), atomic (?>...)
                        i += 1;
                        continue;
                    }

                    if next == '<' {
                        // Could be named group (?<name>...) or lookbehind (?<=...), (?<!...)
                        if i + 3 < len {
                            let after_lt = chars[i + 3];
                            if after_lt == '=' || after_lt == '!' {
                                // Lookbehind - not a capture group
                                i += 1;
                                continue;
                            }

                            // Named group (?<name>...)
                            if let Some(name) = extract_name(&chars, i + 3, '>') {
                                groups.push(CaptureGroup {
                                    index: group_index,
                                    name: Some(name),
                                });
                                group_index += 1;
                            }
                        }
                        i += 1;
                        continue;
                    }

                    if next == 'P' {
                        // Python-style named group (?P<name>...)
                        if i + 3 < len && chars[i + 3] == '<' {
                            if let Some(name) = extract_name(&chars, i + 4, '>') {
                                groups.push(CaptureGroup {
                                    index: group_index,
                                    name: Some(name),
                                });
                                group_index += 1;
                            }
                        }
                        i += 1;
                        continue;
                    }

                    if next == '\'' {
                        // Perl-style named group (?'name'...)
                        if let Some(name) = extract_name(&chars, i + 3, '\'') {
                            groups.push(CaptureGroup {
                                index: group_index,
                                name: Some(name),
                            });
                            group_index += 1;
                        }
                        i += 1;
                        continue;
                    }

                    if next == '#' {
                        // Comment (?#...) - skip until closing paren
                        i += 1;
                        continue;
                    }

                    // Other special constructs (flags, etc.) - not a capture group
                    // e.g., (?i), (?m), (?s), (?x), (?-i), etc.
                    i += 1;
                    continue;
                }
            } else {
                // Regular capture group
                groups.push(CaptureGroup {
                    index: group_index,
                    name: None,
                });
                group_index += 1;
            }
        }

        i += 1;
    }

    groups
}

/// Extracts a name starting at position `start` until the `delimiter` character.
fn extract_name(chars: &[char], start: usize, delimiter: char) -> Option<String> {
    let mut name = String::new();
    let mut i = start;

    while i < chars.len() {
        let c = chars[i];
        if c == delimiter {
            if !name.is_empty() {
                return Some(name);
            }
            return None;
        }
        // Valid name characters: alphanumeric and underscore
        if c.is_alphanumeric() || c == '_' {
            name.push(c);
        } else {
            // Invalid character in name
            return None;
        }
        i += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_numbered_groups() {
        let groups = parse_capture_groups("(a)(b)(c)");
        // Group 0 is full match, groups 1-3 are capture groups
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0].index, 0);
        assert!(groups[0].name.is_none());
        assert_eq!(groups[1].index, 1);
        assert!(groups[1].name.is_none());
        assert_eq!(groups[2].index, 2);
        assert!(groups[2].name.is_none());
        assert_eq!(groups[3].index, 3);
        assert!(groups[3].name.is_none());
    }

    #[test]
    fn test_named_groups_angle_bracket() {
        let groups = parse_capture_groups("(?<foo>a)(?<bar>b)");
        // Group 0 is full match, groups 1-2 are named captures
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].index, 0);
        assert!(groups[0].name.is_none());
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[1].name, Some("foo".to_string()));
        assert_eq!(groups[2].index, 2);
        assert_eq!(groups[2].name, Some("bar".to_string()));
    }

    #[test]
    fn test_named_groups_python_style() {
        let groups = parse_capture_groups("(?P<foo>a)(?P<bar>b)");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].index, 0);
        assert!(groups[0].name.is_none());
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[1].name, Some("foo".to_string()));
        assert_eq!(groups[2].index, 2);
        assert_eq!(groups[2].name, Some("bar".to_string()));
    }

    #[test]
    fn test_named_groups_perl_style() {
        let groups = parse_capture_groups("(?'foo'a)(?'bar'b)");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].index, 0);
        assert!(groups[0].name.is_none());
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[1].name, Some("foo".to_string()));
        assert_eq!(groups[2].index, 2);
        assert_eq!(groups[2].name, Some("bar".to_string()));
    }

    #[test]
    fn test_mixed_groups() {
        let groups = parse_capture_groups("(a)(?<name>b)(c)");
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0].index, 0);
        assert!(groups[0].name.is_none());
        assert_eq!(groups[1].index, 1);
        assert!(groups[1].name.is_none());
        assert_eq!(groups[2].index, 2);
        assert_eq!(groups[2].name, Some("name".to_string()));
        assert_eq!(groups[3].index, 3);
        assert!(groups[3].name.is_none());
    }

    #[test]
    fn test_non_capturing_groups() {
        let groups = parse_capture_groups("(?:a)(b)(?:c)(d)");
        // Group 0 is full match, groups 1-2 are captures (non-capturing groups don't count)
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[2].index, 2);
    }

    #[test]
    fn test_lookahead_lookbehind() {
        let groups = parse_capture_groups("(?=a)(b)(?!c)(d)(?<=e)(f)(?<!g)(h)");
        // Group 0 is full match, lookahead/lookbehind don't count
        assert_eq!(groups.len(), 5);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[2].index, 2);
        assert_eq!(groups[3].index, 3);
        assert_eq!(groups[4].index, 4);
    }

    #[test]
    fn test_escaped_parentheses() {
        let groups = parse_capture_groups(r"\(a\)(b)\(c\)(d)");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[2].index, 2);
    }

    #[test]
    fn test_character_classes() {
        let groups = parse_capture_groups("[(](a)[)](b)");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[2].index, 2);
    }

    #[test]
    fn test_nested_groups() {
        let groups = parse_capture_groups("((a)(b))");
        // Group 0 is full match, groups 1-3 are the nested captures
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
        assert_eq!(groups[2].index, 2);
        assert_eq!(groups[3].index, 3);
    }

    #[test]
    fn test_atomic_groups() {
        let groups = parse_capture_groups("(?>a)(b)");
        // Atomic groups don't capture, only (b) does
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
    }

    #[test]
    fn test_comments() {
        let groups = parse_capture_groups("(?#comment)(a)");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
    }

    #[test]
    fn test_empty_pattern() {
        let groups = parse_capture_groups("");
        // Even empty patterns have group 0
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].index, 0);
    }

    #[test]
    fn test_no_groups() {
        let groups = parse_capture_groups("abc");
        // Group 0 is always present
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].index, 0);
    }

    #[test]
    fn test_escaped_backslash_before_paren() {
        // \\( is escaped backslash followed by real open paren
        let groups = parse_capture_groups(r"\\(a)");
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_complex_pattern() {
        // A realistic regex pattern
        let groups = parse_capture_groups(r"^(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})$");
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0].index, 0);
        assert!(groups[0].name.is_none());
        assert_eq!(groups[1].name, Some("year".to_string()));
        assert_eq!(groups[2].name, Some("month".to_string()));
        assert_eq!(groups[3].name, Some("day".to_string()));
    }

    #[test]
    fn test_flags() {
        // (?i) is a flag, not a capture group
        let groups = parse_capture_groups("(?i)(a)");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].index, 0);
        assert_eq!(groups[1].index, 1);
    }

    #[test]
    fn test_website_example() {
        // The pattern from the website example test
        let groups = parse_capture_groups("^(positional)and(?<named>foo)$");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].index, 0);
        assert!(groups[0].name.is_none()); // Full match
        assert_eq!(groups[1].index, 1);
        assert!(groups[1].name.is_none()); // (positional)
        assert_eq!(groups[2].index, 2);
        assert_eq!(groups[2].name, Some("named".to_string())); // (?<named>foo)
    }
}
