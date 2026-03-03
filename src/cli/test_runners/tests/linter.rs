use std::fs;
use std::path::Path;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::format_diff;

/// Runs HHAST-style linter tests under `tests/hhast_tests/`.
///
/// Discovers `.php.in` / `.hack.in` files, runs the matching linter(s) from
/// `HooksProvider`, and compares reported errors against `.expect` JSON
/// snapshots. Also validates autofix output against `.autofix.expect` and
/// file-operation output against `.autofix.files.expect` when present.
pub struct LinterTest;

impl IntegrationTest for LinterTest {
    fn run(&self, ctx: TestContext) -> TestResult {
        let provided_linters = ctx.hooks_provider.get_linters_for_test(&ctx.dir);

        if provided_linters.is_empty() {
            return TestResult::fail(
                ctx.dir.clone(),
                format!("No matching linter found for directory: {}", ctx.dir),
                None,
                None,
                std::time::Duration::default(),
            );
        }

        let linters: Vec<&dyn hakana_lint::Linter> = provided_linters
            .iter()
            .map(|linter| linter.as_ref())
            .collect();

        let config = hakana_lint::LintConfig {
            allow_auto_fix: false,
            apply_auto_fix: false,
            add_fixmes: false,
            fixme_linters: Vec::new(),
            enabled_linters: Vec::new(),
            disabled_linters: Vec::new(),
            root_path: None,
        };

        // Check if dir is a specific test file (without extension) or a directory
        let in_files = if ctx.dir.ends_with(".php") || ctx.dir.ends_with(".hack") {
            let base_path = ctx.dir.trim_end_matches(".php").trim_end_matches(".hack");
            vec![
                format!("{}.php.in", base_path),
                format!("{}.hack.in", base_path),
            ]
            .into_iter()
            .filter(|p| Path::new(p).exists())
            .map(|p| std::path::PathBuf::from(p))
            .collect::<Vec<_>>()
        } else if Path::new(&format!("{}.php.in", ctx.dir)).exists() {
            vec![std::path::PathBuf::from(format!("{}.php.in", ctx.dir))]
        } else if Path::new(&format!("{}.hack.in", ctx.dir)).exists() {
            vec![std::path::PathBuf::from(format!("{}.hack.in", ctx.dir))]
        } else {
            let entries = match fs::read_dir(&ctx.dir) {
                Ok(entries) => entries,
                Err(e) => {
                    return TestResult::fail(
                        ctx.dir,
                        format!("Failed to read directory: {}", e),
                        None,
                        None,
                        std::time::Duration::default(),
                    );
                }
            };

            let mut in_files = vec![];
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let file_name = path.to_string_lossy().to_string();
                if file_name.ends_with(".php.in") || file_name.ends_with(".hack.in") {
                    in_files.push(path);
                }
            }

            in_files
        };

        if in_files.is_empty() {
            return TestResult::fail(
                ctx.dir,
                "No .in files found".to_string(),
                None,
                None,
                std::time::Duration::default(),
            );
        }

        let mut in_files = in_files;
        in_files.sort();
        let mut all_passed = true;
        let mut errors_output = String::new();

        for in_path in in_files {
            let test_name = in_path.file_name().unwrap().to_string_lossy().to_string();

            let input_contents = match fs::read_to_string(&in_path) {
                Ok(contents) => contents,
                Err(e) => {
                    errors_output.push_str(&format!(
                        "\n=== {} ===\nFailed to read input file: {}\n",
                        test_name, e
                    ));
                    all_passed = false;
                    continue;
                }
            };

            let expect_path = in_path.to_string_lossy().replace(".in", ".expect");
            let expected_errors: Vec<serde_json::Value> = if Path::new(&expect_path).exists() {
                match fs::read_to_string(&expect_path) {
                    Ok(json_str) => match serde_json::from_str(&json_str) {
                        Ok(json) => json,
                        Err(e) => {
                            errors_output.push_str(&format!(
                                "\n=== {} ===\nFailed to parse .expect file: {}\n",
                                test_name, e
                            ));
                            all_passed = false;
                            continue;
                        }
                    },
                    Err(_) => vec![],
                }
            } else {
                vec![]
            };

            let result =
                match hakana_lint::run_linters(&in_path, &input_contents, &linters, &config) {
                    Ok(r) => r,
                    Err(e) => {
                        errors_output.push_str(&format!(
                            "\n=== {} ===\nLinter error: {}\n",
                            test_name, e
                        ));
                        all_passed = false;
                        continue;
                    }
                };

            let actual_errors_json: Vec<serde_json::Value> = result
                .errors
                .iter()
                .map(|err| {
                    let blame_start = err.start_offset;
                    let blame_end = err.end_offset;
                    let blame = if blame_start < input_contents.len()
                        && blame_end <= input_contents.len()
                        && blame_start <= blame_end
                    {
                        input_contents[blame_start..blame_end].to_string()
                    } else {
                        String::new()
                    };

                    serde_json::json!({
                        "blame": blame,
                        "blame_pretty": blame,
                        "description": err.message,
                    })
                })
                .collect();

            if expected_errors != actual_errors_json {
                errors_output.push_str(&format!("\n=== {} ===\n", test_name));

                let expected_json_str = serde_json::to_string_pretty(&expected_errors)
                    .unwrap_or_else(|_| format!("{:?}", expected_errors));
                let actual_json_str = serde_json::to_string_pretty(&actual_errors_json)
                    .unwrap_or_else(|_| format!("{:?}", actual_errors_json));

                errors_output.push_str(&format_diff(&expected_json_str, &actual_json_str));
                all_passed = false;
            }

            // Test autofix if .autofix.expect file exists
            let autofix_expect_path = in_path.to_string_lossy().replace(".in", ".autofix.expect");
            if Path::new(&autofix_expect_path).exists() {
                let expected_autofix = match fs::read_to_string(&autofix_expect_path) {
                    Ok(content) => content,
                    Err(e) => {
                        errors_output.push_str(&format!(
                            "\n=== {} ===\nFailed to read .autofix.expect file: {}\n",
                            test_name, e
                        ));
                        all_passed = false;
                        continue;
                    }
                };

                let autofix_config = hakana_lint::LintConfig {
                    allow_auto_fix: true,
                    apply_auto_fix: true,
                    add_fixmes: false,
                    fixme_linters: Vec::new(),
                    enabled_linters: Vec::new(),
                    disabled_linters: Vec::new(),
                    root_path: None,
                };

                let autofix_result = match hakana_lint::run_linters(
                    &in_path,
                    &input_contents,
                    &linters,
                    &autofix_config,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        errors_output.push_str(&format!(
                            "\n=== {} ===\nAutofix linter error: {}\n",
                            test_name, e
                        ));
                        all_passed = false;
                        continue;
                    }
                };

                let actual_autofix = autofix_result
                    .modified_source
                    .unwrap_or(input_contents.clone());

                if expected_autofix != actual_autofix {
                    errors_output.push_str(&format!("\n=== {} (autofix) ===\n", test_name));
                    errors_output.push_str(&format_diff(&expected_autofix, &actual_autofix));
                    all_passed = false;
                }

                // Check for file operations if .autofix.files.expect exists
                let files_expect_path = in_path
                    .to_string_lossy()
                    .replace(".in", ".autofix.files.expect");
                if Path::new(&files_expect_path).exists() {
                    let expected_files_json = match fs::read_to_string(&files_expect_path) {
                        Ok(content) => content,
                        Err(e) => {
                            errors_output.push_str(&format!(
                                "\n=== {} ===\nFailed to read .autofix.files.expect: {}\n",
                                test_name, e
                            ));
                            all_passed = false;
                            continue;
                        }
                    };

                    let expected_files: serde_json::Value =
                        match serde_json::from_str(&expected_files_json) {
                            Ok(json) => json,
                            Err(e) => {
                                errors_output.push_str(&format!(
                                    "\n=== {} ===\nFailed to parse .autofix.files.expect: {}\n",
                                    test_name, e
                                ));
                                all_passed = false;
                                continue;
                            }
                        };

                    let mut actual_files = serde_json::json!({});
                    for file_op in &autofix_result.file_operations {
                        let file_name = file_op.path.to_string_lossy().to_string();
                        if let Some(ref content) = file_op.content {
                            actual_files[file_name] = serde_json::json!(content);
                        }
                    }

                    if expected_files != actual_files {
                        errors_output
                            .push_str(&format!("\n=== {} (autofix files) ===\n", test_name));
                        errors_output.push_str(&format_diff(
                            &serde_json::to_string_pretty(&expected_files).unwrap(),
                            &serde_json::to_string_pretty(&actual_files).unwrap(),
                        ));
                        all_passed = false;
                    }
                }
            }
        }

        if all_passed {
            TestResult::pass(None, None, std::time::Duration::default())
        } else {
            TestResult::fail(
                ctx.dir,
                errors_output,
                None,
                None,
                std::time::Duration::default(),
            )
        }
    }
}
