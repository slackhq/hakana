//! [`TestOutput`] implementations: one per kind of snapshot a test can produce.
//!
//! Each type owns the path to its committed expectation file plus the actual
//! output to compare against it. `verify` reads the expectation lazily and
//! reports a mismatch; `update` rewrites the expectation with the actual output
//! (used by `--update-snapshots`).

use std::fs;
use std::path::Path;

use similar::{ChangeTag, TextDiff};

use crate::test_runners::integration_test::TestOutput;

/// Issue output compared against `<dir>/output.txt` using the fuzzy
/// (trim-exact, single-line substring) rules in [`compare_issues_to_expected`].
///
/// A missing `output.txt` is treated as "expect no issues" — matching the
/// long-standing behavior of standard analysis tests.
pub struct IssueSnapshot {
    pub dir: String,
    pub actual: Vec<String>,
}

impl TestOutput for IssueSnapshot {
    fn expect_path(&self) -> String {
        format!("{}/output.txt", self.dir)
    }

    fn verify(&self) -> Result<(), String> {
        let expected_output_path = self.dir.clone() + "/output.txt";
        let expected_output = if Path::new(&expected_output_path).exists() {
            let expected = fs::read_to_string(expected_output_path)
                .unwrap()
                .trim()
                .to_string();
            Some(expected)
        } else {
            None
        };

        let passed = if let Some(expected_output) = &expected_output {
            if expected_output == self.actual.join("").trim() {
                true
            } else {
                !expected_output.is_empty()
                    && self.actual.len() == 1
                    && expected_output
                        .as_bytes()
                        .iter()
                        .filter(|&&c| c == b'\n')
                        .count()
                        == 0
                    && self.actual.iter().any(|s| s.contains(expected_output))
            }
        } else {
            self.actual.is_empty()
        };

        if passed {
            return Ok(());
        }
        let diagnostic = if let Some(expected_output) = &expected_output {
            format_diff(expected_output, &self.actual.join(""))
        } else {
            format_diff("", &self.actual.join(""))
        };
        Err(diagnostic)
    }

    fn update(&self) -> Result<(), String> {
        let contents = self.actual.join("");
        write_snapshot(&self.expect_path(), contents.trim())
    }
}

/// Output compared byte-for-byte against an expectation file. Used for
/// code-transformation tests where whitespace is significant.
pub struct ExactSnapshot {
    pub path: String,
    pub actual: String,
}

impl TestOutput for ExactSnapshot {
    fn expect_path(&self) -> String {
        self.path.clone()
    }

    fn verify(&self) -> Result<(), String> {
        let expected = fs::read_to_string(&self.path).unwrap_or_default();
        if expected == self.actual {
            Ok(())
        } else {
            Err(format_diff(&expected, &self.actual))
        }
    }

    fn update(&self) -> Result<(), String> {
        write_snapshot(&self.path, &self.actual)
    }
}

/// A JSON value compared structurally against an expectation file, with a
/// pretty-printed diff on mismatch. Used for `definition_locations.json`,
/// `references.json`, the executable-code-finder, and the linter snapshots.
///
/// A missing expectation file is a verification failure: snapshots are only
/// written under `--update-snapshots`. Tests that expect no output commit an
/// empty-array (`[]`) expectation file.
pub struct JsonValueSnapshot {
    pub path: String,
    pub actual: serde_json::Value,
}

impl JsonValueSnapshot {
    fn pretty(value: &serde_json::Value) -> String {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    }
}

impl TestOutput for JsonValueSnapshot {
    fn expect_path(&self) -> String {
        self.path.clone()
    }

    fn verify(&self) -> Result<(), String> {
        if !Path::new(&self.path).exists() {
            return Err(format_diff("", &Self::pretty(&self.actual)));
        }

        let contents = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        let expected: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse {}: {}", self.path, e))?;

        if expected == self.actual {
            Ok(())
        } else {
            Err(format_diff(
                &Self::pretty(&expected),
                &Self::pretty(&self.actual),
            ))
        }
    }

    fn update(&self) -> Result<(), String> {
        write_snapshot(&self.path, &Self::pretty(&self.actual))
    }
}

/// Migration candidates compared as a set against `<dir>/candidates.txt`,
/// reporting unexpected and missing entries separately.
pub struct CandidatesSnapshot {
    pub path: String,
    pub actual: Vec<String>,
}

impl TestOutput for CandidatesSnapshot {
    fn expect_path(&self) -> String {
        self.path.clone()
    }

    fn verify(&self) -> Result<(), String> {
        let expected = fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect::<Vec<String>>();

        let missing = expected
            .iter()
            .filter(|item| !self.actual.contains(item))
            .cloned()
            .collect::<Vec<String>>();
        let unexpected = self
            .actual
            .iter()
            .filter(|item| !expected.contains(item))
            .cloned()
            .collect::<Vec<String>>();

        let mut diagnostics = vec![];
        if !unexpected.is_empty() {
            diagnostics.push(format!(
                "Found unexpected candidates: {}",
                unexpected.join("\n")
            ));
        }
        if !missing.is_empty() {
            diagnostics.push(format!(
                "Missing expected candidates: {}",
                missing.join("\n")
            ));
        }

        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics.join("\n"))
        }
    }

    fn update(&self) -> Result<(), String> {
        write_snapshot(&self.path, &self.actual.join("\n"))
    }
}

fn write_snapshot(path: &str, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|e| format!("Failed to write {}: {}", path, e))
}

fn format_diff(expected: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut output = String::new();

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        output.push_str(&format!("{}{}", sign, change));
    }

    output
}
