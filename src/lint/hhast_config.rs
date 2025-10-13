//! Parser for hhast-lint.json configuration files
//!
//! This module provides support for reading and parsing HHAST lint configuration files,
//! allowing Hakana to respect existing HHAST linter configurations in projects.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Configuration for HHAST linting
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HhastLintConfig {
    /// Root directories to lint
    #[serde(default)]
    pub roots: Vec<String>,

    /// Whether to use builtin linters ("all", "none", or omitted)
    #[serde(default)]
    pub builtin_linters: Option<String>,

    /// Namespace aliases for linters
    #[serde(default)]
    pub namespace_aliases: HashMap<String, String>,

    /// Additional linters to enable globally
    #[serde(default)]
    pub extra_linters: Vec<String>,

    /// Linters to disable globally
    #[serde(default)]
    pub disabled_linters: Vec<String>,

    /// Auto-fixes to disable
    #[serde(default)]
    pub disabled_auto_fixes: Vec<String>,

    /// Whether to disable all auto-fixes
    #[serde(default)]
    pub disable_all_auto_fixes: bool,

    /// Pattern-based overrides
    #[serde(default)]
    pub overrides: Vec<LintOverride>,
}

/// Pattern-based linter override
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LintOverride {
    /// Glob patterns to match files
    pub patterns: Vec<String>,

    /// Additional linters to enable for these patterns
    #[serde(default)]
    pub extra_linters: Vec<String>,

    /// Linters to disable for these patterns
    #[serde(default)]
    pub disabled_linters: Vec<String>,

    /// Auto-fixes to disable for these patterns
    #[serde(default)]
    pub disabled_auto_fixes: Vec<String>,
}

impl HhastLintConfig {
    /// Load configuration from a file
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read hhast-lint.json: {}", e))?;

        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse hhast-lint.json: {}", e))
    }

    /// Check if a linter is enabled for a given file path
    pub fn is_linter_enabled(&self, linter_name: &str, file_path: &str) -> bool {
        // Start with global configuration
        let mut enabled = !self.disabled_linters.contains(&linter_name.to_string());
        let mut explicitly_enabled = self.extra_linters.contains(&linter_name.to_string());

        // Apply overrides based on pattern matching
        for override_config in &self.overrides {
            if self.matches_any_pattern(file_path, &override_config.patterns) {
                // If disabled in override, disable
                if override_config
                    .disabled_linters
                    .contains(&linter_name.to_string())
                {
                    enabled = false;
                    explicitly_enabled = false;
                }

                // If enabled in override, enable
                if override_config
                    .extra_linters
                    .contains(&linter_name.to_string())
                {
                    enabled = true;
                    explicitly_enabled = true;
                }
            }
        }

        // If builtin_linters is "none", only enable explicitly enabled linters
        if let Some(ref builtin) = self.builtin_linters {
            if builtin == "none" {
                return explicitly_enabled;
            }
        }

        enabled
    }

    /// Check if a file path matches any of the given glob patterns
    fn matches_any_pattern(&self, file_path: &str, patterns: &[String]) -> bool {
        patterns.iter().any(|pattern| {
            // Convert glob pattern to simple matching
            // Support * wildcards and ** for directory traversal
            self.matches_glob(file_path, pattern)
        })
    }

    /// Simple glob pattern matching
    fn matches_glob(&self, path: &str, pattern: &str) -> bool {
        // Handle ** for recursive directory matching first
        if pattern.contains("**") {
            let parts: Vec<&str> = pattern.split("**").collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1].trim_start_matches('/');
                return path.starts_with(prefix) && path.contains(suffix);
            }
        }

        // Handle * wildcards (including multiple wildcards)
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            let mut current_pos = 0;

            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    // Empty parts from leading/trailing/consecutive * are skipped
                    continue;
                }

                if i == 0 {
                    // First non-empty part must match from start
                    if !path[current_pos..].starts_with(part) {
                        return false;
                    }
                    current_pos += part.len();
                } else if i == parts.len() - 1 {
                    // Last non-empty part must be found somewhere
                    if path[current_pos..].contains(part) {
                        return true;
                    } else {
                        return false;
                    }
                } else {
                    // Middle parts must be found
                    if let Some(pos) = path[current_pos..].find(part) {
                        current_pos += pos + part.len();
                    } else {
                        return false;
                    }
                }
            }
            // If we only had empty parts (pattern was all *), match anything
            return true;
        }

        // Exact match
        path == pattern
    }

    /// Get all linters that should be enabled for a file
    pub fn get_enabled_linters_for_file(&self, file_path: &str) -> Vec<String> {
        // Collect all possible linter names from global and overrides
        let mut all_linters: std::collections::HashSet<String> = std::collections::HashSet::new();
        all_linters.extend(self.extra_linters.iter().cloned());

        for override_config in &self.overrides {
            if self.matches_any_pattern(file_path, &override_config.patterns) {
                all_linters.extend(override_config.extra_linters.iter().cloned());
            }
        }

        // Filter based on enabled status
        all_linters
            .into_iter()
            .filter(|linter| self.is_linter_enabled(linter, file_path))
            .collect()
    }
}

impl Default for HhastLintConfig {
    fn default() -> Self {
        Self {
            roots: vec![],
            builtin_linters: None,
            namespace_aliases: HashMap::new(),
            extra_linters: vec![],
            disabled_linters: vec![],
            disabled_auto_fixes: vec![],
            disable_all_auto_fixes: false,
            overrides: vec![],
        }
    }
}

/// Map HHAST linter class names to Hakana linter names
pub fn map_hhast_linter_to_hakana(hhast_name: &str) -> Option<&'static str> {
    match hhast_name {
        "Facebook\\HHAST\\NoEmptyStatementsLinter" => Some("no-empty-statements"),
        "Facebook\\HHAST\\NoWhitespaceAtEndOfLineLinter" => Some("no-whitespace-at-end-of-line"),
        "Facebook\\HHAST\\UseStatementWithoutKindLinter" => Some("use-statement-without-kind"),
        "Facebook\\HHAST\\DontDiscardNewExpressionsLinter" => Some("dont-discard-new-expressions"),
        "Facebook\\HHAST\\MustUseBracesForControlFlowLinter" => {
            Some("must-use-braces-for-control-flow")
        }
        "Facebook\\HHAST\\MustUseOverrideAttributeLinter" => Some("must-use-override-attribute"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matching() {
        let config = HhastLintConfig::default();

        // Test exact match
        assert!(config.matches_glob("foo/bar.hack", "foo/bar.hack"));

        // Test prefix wildcard (leading *)
        assert!(config.matches_glob("foo/bar/baz.hack", "*baz.hack"));
        assert!(config.matches_glob("foo/bar/baz.hack", "*/bar/baz.hack"));

        // Test suffix wildcard (trailing *)
        assert!(config.matches_glob("foo/bar/baz.hack", "foo/bar/*"));
        assert!(config.matches_glob("foo/bar/baz.hack", "foo/*"));

        // Test ** wildcard (recursive directory matching)
        assert!(config.matches_glob("foo/bar/baz/qux.hack", "foo/**/qux.hack"));

        // Test middle wildcard
        assert!(config.matches_glob("foo/bar.hack", "foo/*.hack"));

        // Test wildcards at both beginning and end (like *gen-hack/*)
        assert!(config.matches_glob("gen-hack/api_types/Foo.hack", "*gen-hack/*"));
        assert!(config.matches_glob("foo/gen-hack/bar.hack", "*gen-hack/*"));
        assert!(config.matches_glob("foo/bar/gen-hack/baz/qux.hack", "*gen-hack/*"));
        assert!(!config.matches_glob("foo/genhack/bar.hack", "*gen-hack/*"));

        // Test other patterns from the config
        assert!(config.matches_glob("codegen/foo.hack", "codegen/*"));
        assert!(config.matches_glob("foo/vendor/bar.hack", "*vendor/*"));
        assert!(config.matches_glob("srv/test.hack", "*srv/*"));
    }

    #[test]
    fn test_linter_filtering() {
        let config = HhastLintConfig {
            extra_linters: vec!["Facebook\\HHAST\\NoEmptyStatementsLinter".to_string()],
            disabled_linters: vec![],
            overrides: vec![LintOverride {
                patterns: vec!["*tests/*".to_string()],
                extra_linters: vec![],
                disabled_linters: vec!["Facebook\\HHAST\\NoEmptyStatementsLinter".to_string()],
                disabled_auto_fixes: vec![],
            }],
            ..Default::default()
        };

        // Should be enabled for non-test files
        assert!(config.is_linter_enabled(
            "Facebook\\HHAST\\NoEmptyStatementsLinter",
            "src/foo.hack"
        ));

        // Should be disabled for test files
        assert!(!config.is_linter_enabled(
            "Facebook\\HHAST\\NoEmptyStatementsLinter",
            "tests/foo.hack"
        ));
    }
}
