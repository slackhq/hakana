//! Runner for executing linters and migrators

use crate::collect_statements;

use super::{LintContext, LintError, Linter, Migrator};
use line_break_map::LineBreakMap;
use parser_core_types::source_text::SourceText;
use relative_path::{Prefix, RelativePath};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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

    /// Time spent in each linter (linter name -> duration)
    pub linter_times: FxHashMap<String, Duration>,
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
        // - HHAST_FIXME[Linter], HHAST_IGNORE_ERROR[Linter] -> suppress Linter on same line or following lines in statement
        // - HHAST_IGNORE_ALL, HHAST_FIXME, HHAST_IGNORE_ERROR (without linter name) -> suppress all linters
        // Multiple suppressions can appear on the same line (e.g., /* HHAST_FIXME[A] */ ... /* HHAST_IGNORE_ERROR[B] */)

        let is_ignore_all = line.contains("HHAST_IGNORE_ALL");
        let is_fixme = line.contains("HHAST_FIXME");
        let is_ignore_error = line.contains("HHAST_IGNORE_ERROR");

        if !is_ignore_all && !is_fixme && !is_ignore_error {
            continue;
        }

        // Check for all linter names in brackets on this line
        // We need to find all bracket pairs, not just the first one
        let mut found_any_brackets = false;
        let mut search_start = 0;

        while let Some(bracket_start) = line[search_start..].find('[') {
            let absolute_bracket_start = search_start + bracket_start;
            if let Some(bracket_end_relative) = line[absolute_bracket_start..].find(']') {
                let absolute_bracket_end = absolute_bracket_start + bracket_end_relative;
                if absolute_bracket_end > absolute_bracket_start {
                    let linter_name = &line[absolute_bracket_start + 1..absolute_bracket_end];
                    found_any_brackets = true;

                    if is_ignore_all {
                        // HHAST_IGNORE_ALL[Linter] - suppress for whole file
                        whole_file.insert(linter_name.to_string());
                    } else {
                        // HHAST_FIXME[Linter] or HHAST_IGNORE_ERROR[Linter] - suppress for this line
                        single_instance
                            .entry(line_num)
                            .or_insert_with(FxHashSet::default)
                            .insert(linter_name.to_string());
                    }

                    // Continue searching after this closing bracket
                    search_start = absolute_bracket_end + 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // If no brackets found, suppress all linters for this line
        if !found_any_brackets {
            single_instance
                .entry(line_num)
                .or_insert_with(FxHashSet::default)
                .insert(String::new());
        }
    }

    SuppressionInfo {
        whole_file,
        single_instance,
    }
}

/// Parse suppressions and collect statement boundaries for better suppression handling
fn parse_suppressions_with_statements<'a>(
    source: &str,
    root: &'a parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax<'a>,
    line_break_map: &LineBreakMap,
) -> (SuppressionInfo, Vec<(usize, usize)>) {
    use parser_core_types::syntax_trait::SyntaxTrait;

    let suppression_info = parse_suppressions(source);

    // Collect statement boundaries
    let statements = collect_statements(root);
    let mut statement_boundaries = Vec::new();

    for stmt in statements {
        if let Some(start_offset) = stmt.offset() {
            let (start_line, _) =
                crate::syntax_utils::offset_to_line_column(line_break_map, start_offset);
            let width = stmt.full_width();
            let end_offset = start_offset + width;
            let (end_line, _) =
                crate::syntax_utils::offset_to_line_column(line_break_map, end_offset);

            statement_boundaries.push((start_line, end_line));
        }
    }

    (suppression_info, statement_boundaries)
}

/// Check if an error should be suppressed, considering statement boundaries
fn is_suppressed_with_boundaries(
    error: &LintError,
    line_break_map: &LineBreakMap,
    suppressions: &SuppressionInfo,
    linter_name: &str,
    hhast_name: Option<&str>,
    statement_boundaries: &[(usize, usize)],
) -> bool {
    // Check if this linter is suppressed for the whole file
    if is_linter_in_set(&suppressions.whole_file, linter_name, hhast_name) {
        return true;
    }

    // Get the line range of the error
    // start_offset includes leading trivia, end_offset includes trailing trivia
    let (start_line, _) =
        crate::syntax_utils::offset_to_line_column(line_break_map, error.start_offset);
    let (end_line, _) =
        crate::syntax_utils::offset_to_line_column(line_break_map, error.end_offset);

    // Find the smallest (innermost) statement containing this error
    // When statements are nested, we want the most specific one
    let containing_statement = statement_boundaries
        .iter()
        .filter(|(stmt_start, stmt_end)| start_line >= *stmt_start && end_line <= *stmt_end)
        .min_by_key(|(stmt_start, stmt_end)| stmt_end - stmt_start);

    // The actual error token is somewhere in the range [start_line, end_line]
    // Check if any line in this range has a suppression:
    // 1. On the same line (inline comment: /* HHAST_IGNORE_ERROR[Linter] */)
    // 2. On the previous line (HHAST suppression comments on line N suppress errors on line N+1)
    // 3. On any line before the statement starts (statement-level suppression)
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

    // Check for statement-level suppression: if a suppression appears on a line
    // before the statement starts, it suppresses errors throughout the statement
    if let Some(&(stmt_start, _)) = containing_statement {
        if stmt_start > 0 {
            if let Some(linters) = suppressions.single_instance.get(&(stmt_start - 1)) {
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

    // Create line break map for efficient offset-to-line conversions
    let line_break_map = LineBreakMap::new(file_contents.as_bytes());

    // Parse suppression comments and collect statement boundaries
    let (suppressions, statement_boundaries) =
        parse_suppressions_with_statements(file_contents, &root, &line_break_map);

    // Run each linter and track which errors came from which linter
    let mut all_errors = Vec::new();
    let mut linter_times = FxHashMap::default();
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

        let start_time = std::time::Instant::now();
        let errors = linter.lint(&ctx);
        let elapsed = start_time.elapsed();

        linter_times.insert(linter.name().to_string(), elapsed);

        // Filter out suppressed errors
        let linter_name = linter.name();
        let hhast_name = linter.hhast_name();
        for error in errors {
            if !is_suppressed_with_boundaries(
                &error,
                &line_break_map,
                &suppressions,
                linter_name,
                hhast_name,
                &statement_boundaries,
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
        linter_times,
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
