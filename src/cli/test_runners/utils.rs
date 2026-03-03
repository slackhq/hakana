use hakana_analyzer::config;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::data_flow::graph::WholeProgramKind;
use hakana_code_info::issue::IssueKind;
use hakana_str::{Interner, StrId};
use rustc_hash::FxHashSet;
use similar::{ChangeTag, TextDiff};

use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

use super::test_runner::HooksProvider;

pub fn format_diff(expected: &str, actual: &str) -> String {
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

pub fn augment_with_local_config(dir: &str, analysis_config: &mut config::Config) {
    let config_path_str = format!("{}/config.json", dir);
    let config_path = Path::new(&config_path_str);

    if config_path.exists() {
        let Ok(test_config) = super::config::read_from_file(config_path) else {
            panic!("invalid test config file {}", config_path_str);
        };

        if let Some(max_changes_allowed) = test_config.max_changes_allowed {
            analysis_config.max_changes_allowed = max_changes_allowed;
        }
    }
}

pub fn copy_recursively(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            copy_recursively(entry.path(), destination.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), destination.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub fn generate_definition_locations_json(
    analysis_result: &AnalysisResult,
    interner: &Interner,
) -> String {
    use serde_json::json;

    let mut all_locations = Vec::new();

    for (file_path, locations) in &analysis_result.definition_locations {
        let original_file_path_str = interner.lookup(&file_path.0);

        let file_path_str = if let Some(workdir_pos) = original_file_path_str.find("/workdir/") {
            let file_name = &original_file_path_str[workdir_pos + 9..];
            file_name.to_string()
        } else {
            original_file_path_str
                .split('/')
                .last()
                .unwrap_or(original_file_path_str)
                .to_string()
        };

        for ((start_offset, end_offset), (symbol_id, member_id)) in locations {
            let symbol_name = interner.lookup(symbol_id);
            let member_name = if *member_id == StrId::EMPTY {
                ""
            } else {
                interner.lookup(member_id)
            };

            let name = if member_name.is_empty() {
                symbol_name.to_string()
            } else {
                format!("{}::{}", symbol_name, member_name)
            };

            all_locations.push(json!({
                "name": name,
                "file": file_path_str,
                "start_offset": start_offset,
                "end_offset": end_offset
            }));
        }
    }

    all_locations.sort_by(|a, b| {
        let start_a = a["start_offset"].as_u64().unwrap();
        let start_b = b["start_offset"].as_u64().unwrap();
        let end_a = a["end_offset"].as_u64().unwrap();
        let end_b = b["end_offset"].as_u64().unwrap();

        start_a.cmp(&start_b).then(end_a.cmp(&end_b))
    });

    serde_json::to_string_pretty(&all_locations).unwrap_or_else(|_| "[]".to_string())
}

pub fn default_config_for_test(dir: &str, hooks_provider: &dyn HooksProvider) -> config::Config {
    let mut analysis_config = config::Config::new(dir.to_string(), FxHashSet::default());
    analysis_config.add_date_comments = false;

    let mut dir_parts = dir.split('/').collect::<Vec<_>>();

    while let Some(&"tests" | &"internal" | &"public") = dir_parts.first() {
        dir_parts = dir_parts[1..].to_vec();
    }

    let maybe_issue_name = dir_parts.get(1).unwrap().to_string();

    let dir_issue = IssueKind::from_str(&maybe_issue_name);
    analysis_config.find_unused_expressions = if let Ok(dir_issue) = &dir_issue {
        dir_issue.requires_dataflow_analysis()
    } else {
        dir.contains("/unused/")
    };
    analysis_config.find_unused_definitions = if let Ok(dir_issue) = &dir_issue {
        dir_issue.is_unused_definition()
    } else {
        dir.to_ascii_lowercase().contains("unused") && !dir.contains("UnusedExpression")
    };
    analysis_config.graph_kind = if dir.contains("/security/") {
        GraphKind::WholeProgram(WholeProgramKind::Taint)
    } else if dir.contains("/find-paths/") {
        GraphKind::WholeProgram(WholeProgramKind::Query)
    } else {
        GraphKind::FunctionBody
    };

    analysis_config.hooks = hooks_provider
        .get_hooks_for_test(dir)
        .into_iter()
        .map(std::sync::Arc::from)
        .collect();

    if dir.contains("/migrations/") {
        let replacements_path = dir.to_string() + "/replacements.txt";
        let replacements = fs::read_to_string(replacements_path).unwrap().to_string();

        analysis_config.migration_symbols = replacements
            .lines()
            .map(|v| {
                let mut parts = v.split(',').collect::<Vec<_>>();
                let first_part = parts.remove(0);
                (first_part.to_string(), parts.join(","))
            })
            .collect();
        analysis_config.in_migration = true;
    } else if dir.contains("/fix/") {
        let issue_name = dir_parts.get(1).unwrap().to_string();

        analysis_config
            .issues_to_fix
            .insert(IssueKind::from_str_custom(&issue_name, &FxHashSet::default()).unwrap());
    } else if dir.contains("/add-fixmes/") {
        let issue_name = dir_parts.get(1).unwrap().to_string();

        analysis_config
            .issues_to_fix
            .insert(IssueKind::from_str_custom(&issue_name, &FxHashSet::default()).unwrap());

        analysis_config.add_fixmes = true;
        analysis_config.find_unused_expressions = true;
    } else if dir.contains("/remove-unused-fixmes/") {
        analysis_config.remove_fixmes = true;
        analysis_config.find_unused_expressions = true;
    } else if dir.contains("/migration-candidates/") {
        analysis_config.in_migration = true;
    }

    if dir.contains("/goto-definition/")
        || dir.contains("/references/")
        || dir.contains("/diff/")
    {
        analysis_config.collect_goto_definition_locations = true;
    }

    analysis_config
}

pub fn compare_issues_to_expected(
    dir: &str,
    test_output: &[String],
) -> (bool, Option<String>) {
    let expected_output_path = dir.to_string() + "/output.txt";
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
        if expected_output == test_output.join("").trim() {
            true
        } else {
            !expected_output.is_empty()
                && test_output.len() == 1
                && expected_output
                    .as_bytes()
                    .iter()
                    .filter(|&&c| c == b'\n')
                    .count()
                    == 0
                && test_output.iter().any(|s| s.contains(expected_output))
        }
    } else {
        test_output.is_empty()
    };

    if passed {
        (true, None)
    } else {
        let diagnostic = if let Some(expected_output) = &expected_output {
            format_diff(expected_output, &test_output.join(""))
        } else {
            format_diff("", &test_output.join(""))
        };
        (false, Some(diagnostic))
    }
}
