use executable_finder::ExecutableLines;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::{augment_with_local_config, default_config_for_test, format_diff};

/// Runs the executable-code-finder on `tests/executable-code-finder/` directories.
///
/// Scans the test input for executable lines and compares the JSON output
/// against `output.txt`.
pub struct ExecutableCodeFinderTest;

impl IntegrationTest for ExecutableCodeFinderTest {
    fn run(&self, ctx: TestContext) -> TestResult {
        let mut analysis_config = default_config_for_test(&ctx.dir, ctx.hooks_provider);
        augment_with_local_config(&ctx.dir, &mut analysis_config);

        let config = Arc::new(analysis_config);

        match executable_finder::scan_files(&vec![ctx.dir.clone()], None, &config, 1, ctx.logger) {
            Ok(test_output) => {
                let expected_output_path = ctx.dir.clone() + "/output.txt";
                let expected_output = if Path::new(&expected_output_path).exists() {
                    let file_contents = fs::read_to_string(expected_output_path)
                        .unwrap()
                        .trim()
                        .to_string();
                    let j: Vec<ExecutableLines> = serde_json::from_str(&file_contents).unwrap();
                    Some(j)
                } else {
                    None
                };
                if let Some(expected_output) = &expected_output {
                    if test_output == *expected_output {
                        TestResult::pass(None, None, std::time::Duration::default())
                    } else {
                        let expected_output_str =
                            serde_json::to_string_pretty(&expected_output).unwrap();
                        let test_output_str = serde_json::to_string_pretty(&test_output).unwrap();
                        TestResult::fail(
                            ctx.dir,
                            format_diff(&expected_output_str, &test_output_str),
                            None,
                            None,
                            std::time::Duration::default(),
                        )
                    }
                } else {
                    TestResult::fail(
                        ctx.dir,
                        "No output.txt found".to_string(),
                        None,
                        None,
                        std::time::Duration::default(),
                    )
                }
            }
            Err(_) => TestResult::fail(
                ctx.dir,
                "executable code finder failed".to_string(),
                None,
                None,
                std::time::Duration::default(),
            ),
        }
    }
}
