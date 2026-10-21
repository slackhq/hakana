//! File and directory discovery shared by command-line tools.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub struct DiscoveryOptions {
    pub sort_by_file_name: bool,
    pub deduplicate: bool,
    /// Preserve non-directory arguments even if missing or rejected by the
    /// filter, so the caller can report them in its normal processing order.
    pub keep_explicit_files: bool,
    /// Skip traversal errors, including missing paths and symlink loops.
    pub ignore_errors: bool,
}

/// Expand files and directories, following symlinks. `include` selects files
/// within directories and, unless preserved, explicit file arguments.
///
/// Deduplication uses canonical paths and preserves the first spelling seen.
/// Explicit files rejected by `include` do not hide their supported targets.
pub fn discover_files(
    paths: &[impl AsRef<Path>],
    options: DiscoveryOptions,
    include: impl Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>, walkdir::Error> {
    let mut files = vec![];
    let mut seen = HashSet::new();
    let mut add = |path: &Path| {
        if !options.deduplicate
            || !include(path)
            || seen.insert(fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        {
            files.push(path.to_path_buf());
        }
    };
    for path in paths {
        let path = path.as_ref();
        if options.keep_explicit_files && !path.is_dir() {
            add(path);
            continue;
        }
        let mut walker = WalkDir::new(path).follow_links(true);
        if options.sort_by_file_name {
            walker = walker.sort_by_file_name();
        }
        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) if options.ignore_errors => continue,
                Err(error) => return Err(error),
            };
            if entry.file_type().is_file() && include(entry.path()) {
                add(entry.path());
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
