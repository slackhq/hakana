//! Runner for executing linters and migrators

use super::{LintContext, LintError, Linter, Migrator};
use parser_core_types::source_text::SourceText;
use relative_path::{Prefix, RelativePath};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Configuration for running linters
#[derive(Debug, Clone)]
pub struct LintConfig {
    /// Whether to generate auto-fixes
    pub allow_auto_fix: bool,

    /// Whether to apply auto-fixes immediately
    pub apply_auto_fix: bool,

    /// Specific linters to run (empty = all)
    pub enabled_linters: Vec<String>,

    /// Linters to skip
    pub disabled_linters: Vec<String>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            allow_auto_fix: true,
            apply_auto_fix: false,
            enabled_linters: Vec::new(),
            disabled_linters: Vec::new(),
        }
    }
}

/// Result of running linters on a file
#[derive(Debug)]
pub struct LintResult {
    /// The file that was linted
    pub file_path: PathBuf,

    /// All errors found
    pub errors: Vec<LintError>,

    /// Whether any auto-fixes were applied
    pub fixes_applied: bool,

    /// The modified source code (if fixes were applied)
    pub modified_source: Option<String>,

    /// File operations to perform (create/delete files)
    pub file_operations: Vec<super::FileOperation>,
}

impl LintResult {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

/// Suppression information
#[derive(Debug, Clone)]
struct SuppressionInfo {
    /// Linters suppressed for the entire file (from HHAST_IGNORE_ALL[Linter])
    whole_file: FxHashSet<String>,
    /// Linters suppressed for specific lines (from HHAST_FIXME[Linter] or HHAST_IGNORE_ERROR[Linter])
    /// Maps line number to set of suppressed linter names
    single_instance: FxHashMap<usize, FxHashSet<String>>,
}

/// Parse HHAST suppression comments from source code
fn parse_suppressions(source: &str) -> SuppressionInfo {
    let mut whole_file = FxHashSet::default();
    let mut single_instance: FxHashMap<usize, FxHashSet<String>> = FxHashMap::default();

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = line_idx + 1; // 1-indexed

        // Check for HHAST suppression directives
        // Supported formats:
        // - HHAST_IGNORE_ALL[Linter] -> suppress Linter for whole file
        // - HHAST_FIXME[Linter], HHAST_IGNORE_ERROR[Linter] -> suppress Linter for next line
        // - HHAST_IGNORE_ALL, HHAST_FIXME, HHAST_IGNORE_ERROR (without linter name) -> suppress all linters

        let is_ignore_all = line.contains("HHAST_IGNORE_ALL");
        let is_fixme = line.contains("HHAST_FIXME");
        let is_ignore_error = line.contains("HHAST_IGNORE_ERROR");

        if !is_ignore_all && !is_fixme && !is_ignore_error {
            continue;
        }

        // Check if there's a specific linter name in brackets
        if let Some(bracket_start) = line.find('[') {
            if let Some(bracket_end) = line.find(']') {
                if bracket_end > bracket_start {
                    let linter_name = &line[bracket_start + 1..bracket_end];

                    if is_ignore_all {
                        // HHAST_IGNORE_ALL[Linter] - suppress for whole file
                        whole_file.insert(linter_name.to_string());
                    } else {
                        // HHAST_FIXME[Linter] or HHAST_IGNORE_ERROR[Linter] - suppress for next line
                        single_instance
                            .entry(line_num)
                            .or_insert_with(FxHashSet::default)
                            .insert(linter_name.to_string());
                    }
                    continue;
                }
            }
        }

        // No specific linter name - suppress all linters for the next line
        // Store as a special marker (empty string means "all linters")
        single_instance
            .entry(line_num)
            .or_insert_with(FxHashSet::default)
            .insert(String::new());
    }

    SuppressionInfo {
        whole_file,
        single_instance,
    }
}

/// Check if an error should be suppressed
fn is_suppressed(
    error: &LintError,
    source: &str,
    suppressions: &SuppressionInfo,
    linter_name: &str,
    hhast_name: Option<&str>,
) -> bool {
    // Check if this linter is suppressed for the whole file
    if is_linter_in_set(&suppressions.whole_file, linter_name, hhast_name) {
        return true;
    }

    // Get the line range of the error
    // start_offset includes leading trivia, end_offset includes trailing trivia
    let (start_line, _) = offset_to_line_column(source, error.start_offset);
    let (end_line, _) = offset_to_line_column(source, error.end_offset);

    // The actual error token is somewhere in the range [start_line, end_line]
    // Check if any line in this range has a suppression:
    // 1. On the same line (inline comment: /* HHAST_IGNORE_ERROR[Linter] */)
    // 2. On the previous line (HHAST suppression comments on line N suppress errors on line N+1)
    for line in start_line..=end_line {
        // Check for suppression on the same line (inline comments)
        if let Some(linters) = suppressions.single_instance.get(&line) {
            // Empty string means suppress all linters
            if linters.contains("") {
                return true;
            }

            // Check if this specific linter is suppressed
            if is_linter_in_set(linters, linter_name, hhast_name) {
                return true;
            }
        }

        // Check for suppression on the previous line
        if line > 0 {
            let previous_line = line - 1;

            if let Some(linters) = suppressions.single_instance.get(&previous_line) {
                // Empty string means suppress all linters
                if linters.contains("") {
                    return true;
                }

                // Check if this specific linter is suppressed
                if is_linter_in_set(linters, linter_name, hhast_name) {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if a linter matches any name in the suppression set
fn is_linter_in_set(
    linters: &FxHashSet<String>,
    linter_name: &str,
    hhast_name: Option<&str>,
) -> bool {
    // Check against short name
    if linters.contains(linter_name) {
        return true;
    }

    // Check against HHAST name variants
    if let Some(hhast) = hhast_name {
        // Check full HHAST name
        if linters.contains(hhast) {
            return true;
        }

        // Extract last part of HHAST name (after last \)
        if let Some(last_part) = hhast.rsplit('\\').next() {
            // Check with full last part
            if linters.contains(last_part) {
                return true;
            }

            // Check without "Linter" suffix
            if let Some(without_suffix) = last_part.strip_suffix("Linter") {
                if linters.contains(without_suffix) {
                    return true;
                }
            }
        }
    }

    false
}

/// Convert byte offset to line and column number
fn offset_to_line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

/// Run linters on a file
pub fn run_linters(
    file_path: &Path,
    file_contents: &str,
    linters: &[&dyn Linter],
    config: &LintConfig,
) -> Result<LintResult, String> {
    // Parse the file
    let arena = bumpalo::Bump::new();
    let relative_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from(file_path)));
    let source_text = SourceText::make(relative_path, file_contents.as_bytes());

    let (root, parse_errors) = crate::parse_file(&arena, &source_text);

    if !parse_errors.is_empty() {
        return Err(format!(
            "Parse errors in {}: {:?}",
            file_path.display(),
            parse_errors
        ));
    }

    // Create lint context
    let ctx = LintContext::new(&source_text, &root, file_path, config.allow_auto_fix);

    // Parse suppression comments
    let suppressions = parse_suppressions(file_contents);

    // Run each linter and track which errors came from which linter
    let mut all_errors = Vec::new();
    for linter in linters {
        // Check if this linter should run
        if !config.enabled_linters.is_empty()
            && !config.enabled_linters.contains(&linter.name().to_string())
        {
            continue;
        }
        if config.disabled_linters.contains(&linter.name().to_string()) {
            continue;
        }

        let errors = linter.lint(&ctx);

        // Filter out suppressed errors
        let linter_name = linter.name();
        let hhast_name = linter.hhast_name();
        for error in errors {
            if !is_suppressed(
                &error,
                file_contents,
                &suppressions,
                linter_name,
                hhast_name,
            ) {
                all_errors.push(error);
            }
        }
    }

    // Apply auto-fixes if requested
    let mut fixes_applied = false;
    let mut modified_source = None;
    let mut file_operations = Vec::new();

    if config.apply_auto_fix && config.allow_auto_fix {
        let mut edits = super::EditSet::new();
        for error in &all_errors {
            if let Some(ref fix) = error.fix {
                for edit in fix.edits() {
                    edits.add(edit);
                }
                // Collect file operations
                file_operations.extend(fix.file_operations().iter().cloned());
            }
        }

        if !edits.is_empty() {
            match edits.apply(file_contents) {
                Ok(new_source) => {
                    modified_source = Some(new_source);
                    fixes_applied = true;
                }
                Err(e) => {
                    return Err(format!("Failed to apply fixes: {}", e));
                }
            }
        }
    }

    Ok(LintResult {
        file_path: file_path.to_path_buf(),
        errors: all_errors,
        fixes_applied,
        modified_source,
        file_operations,
    })
}

/// Run a migrator on a file
pub fn run_migrator(
    file_path: &Path,
    file_contents: &str,
    migrator: &dyn Migrator,
) -> Result<Option<String>, String> {
    let arena = bumpalo::Bump::new();
    let relative_path = Arc::new(RelativePath::make(Prefix::Root, PathBuf::from(file_path)));
    let source_text = SourceText::make(relative_path, file_contents.as_bytes());

    let (root, parse_errors) = crate::parse_file(&arena, &source_text);

    if !parse_errors.is_empty() {
        return Err(format!(
            "Parse errors in {}: {:?}",
            file_path.display(),
            parse_errors
        ));
    }

    let ctx = LintContext::new(&source_text, &root, file_path, true);

    if let Some(edits) = migrator.migrate(&ctx) {
        match edits.apply(file_contents) {
            Ok(new_source) => Ok(Some(new_source)),
            Err(e) => Err(format!("Failed to apply migration: {}", e)),
        }
    } else {
        Ok(None)
    }
}
