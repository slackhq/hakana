//! Utilities for working with symbol references from analysis results.
//!
//! This module provides functions to extract and format symbol reference data
//! from analysis results. It's used by both the MCP server and test runners.

use crate::analysis_result::AnalysisResult;
use hakana_str::{Interner, StrId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single reference location for a symbol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferenceLocation {
    pub file: String,
    pub start_offset: u32,
    pub end_offset: u32,
}

/// All references for a single symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolReferences {
    pub symbol: String,
    pub references: Vec<ReferenceLocation>,
}

/// Extract the relative filename from a full path.
/// The file path typically starts with the project root, so we return the full path
/// and let the caller construct the absolute path by prepending the root dir.
fn get_relative_filename(original_path: &str) -> String {
    // If the path looks like a relative path (no leading /), return as-is
    if !original_path.starts_with('/') {
        return original_path.to_string();
    }
    // For absolute paths, return the full path - the caller will know how to handle it
    original_path.to_string()
}

/// Format a symbol name from its StrId components
fn format_symbol_name(symbol_id: StrId, member_id: StrId, interner: &Interner) -> String {
    let symbol_name = interner.lookup(&symbol_id);
    if member_id == StrId::EMPTY {
        symbol_name.to_string()
    } else {
        let member_name = interner.lookup(&member_id);
        format!("{}::{}", symbol_name, member_name)
    }
}

/// Get all references grouped by symbol name.
///
/// Returns a map from symbol name to list of reference locations.
/// This is useful for finding all usages of a particular symbol.
pub fn get_references_by_symbol(
    analysis_result: &AnalysisResult,
    interner: &Interner,
) -> BTreeMap<String, Vec<ReferenceLocation>> {
    let mut references_by_symbol: BTreeMap<String, Vec<ReferenceLocation>> = BTreeMap::new();

    for (file_path, locations) in &analysis_result.definition_locations {
        let file_path_str = get_relative_filename(interner.lookup(&file_path.0));

        for ((start_offset, end_offset), (symbol_id, member_id)) in locations {
            let name = format_symbol_name(*symbol_id, *member_id, interner);

            let location = ReferenceLocation {
                file: file_path_str.clone(),
                start_offset: *start_offset,
                end_offset: *end_offset,
            };

            references_by_symbol.entry(name).or_default().push(location);
        }
    }

    // Sort locations within each symbol by file then offset
    for locations in references_by_symbol.values_mut() {
        locations.sort_by(|a, b| a.file.cmp(&b.file).then(a.start_offset.cmp(&b.start_offset)));
    }

    references_by_symbol
}

/// Get references for a specific symbol.
///
/// Takes a symbol name (e.g., "MyClass" or "MyClass::method") and returns
/// all locations where it is referenced.
pub fn get_references_for_symbol(
    symbol_name: &str,
    analysis_result: &AnalysisResult,
    interner: &Interner,
) -> Option<Vec<ReferenceLocation>> {
    let references = get_references_by_symbol(analysis_result, interner);
    references.get(symbol_name).cloned()
}

/// Generate JSON representation of all symbol references.
///
/// Returns a JSON array where each element contains a symbol name
/// and its reference locations.
pub fn generate_references_json(analysis_result: &AnalysisResult, interner: &Interner) -> String {
    let references_by_symbol = get_references_by_symbol(analysis_result, interner);

    let output: Vec<SymbolReferences> = references_by_symbol
        .into_iter()
        .map(|(symbol, references)| SymbolReferences { symbol, references })
        .collect();

    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_relative_filename() {
        // Absolute paths are returned as-is
        assert_eq!(
            get_relative_filename("/some/path/workdir/file.hack"),
            "/some/path/workdir/file.hack"
        );
        assert_eq!(
            get_relative_filename("/absolute/path/to/file.hack"),
            "/absolute/path/to/file.hack"
        );
        // Relative paths are returned as-is
        assert_eq!(get_relative_filename("file.hack"), "file.hack");
    }
}
