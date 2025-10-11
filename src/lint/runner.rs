//! Runner for executing linters and migrators

use super::{LintContext, LintError, Linter, Migrator};
use parser_core_types::source_text::SourceText;
use relative_path::{Prefix, RelativePath};
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
}

impl LintResult {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
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
    let relative_path = Arc::new(RelativePath::make(
        Prefix::Root,
        PathBuf::from(file_path),
    ));
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

    // Run each linter
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
        all_errors.extend(errors);
    }

    // Apply auto-fixes if requested
    let mut fixes_applied = false;
    let mut modified_source = None;

    if config.apply_auto_fix && config.allow_auto_fix {
        let mut edits = super::EditSet::new();
        for error in &all_errors {
            if let Some(ref fix) = error.fix {
                for edit in fix.edits() {
                    edits.add(edit);
                }
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
    })
}

/// Run a migrator on a file
pub fn run_migrator(
    file_path: &Path,
    file_contents: &str,
    migrator: &dyn Migrator,
) -> Result<Option<String>, String> {
    let arena = bumpalo::Bump::new();
    let relative_path = Arc::new(RelativePath::make(
        Prefix::Root,
        PathBuf::from(file_path),
    ));
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
