use hakana_code_info::code_location::FilePath;
use hakana_str::Interner;
use rustc_hash::FxHashSet;

use std::fs;
use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::{
    augment_with_local_config, default_config_for_test, format_diff,
};

/// Handles code-transformation tests under `tests/fix/`, `tests/migrations/`,
/// `tests/add-fixmes/`, and `tests/remove-unused-fixmes/`.
///
/// Runs analysis to produce an edit set, applies the edits to `input.hack`,
/// writes the result to `actual.txt`, and compares it against `output.txt`.
pub struct CodeTransformTest;

impl IntegrationTest for CodeTransformTest {
    fn run(&self, ctx: TestContext) -> TestResult {
        let cwd = &ctx.cwd;

        let mut analysis_config = default_config_for_test(&ctx.dir, ctx.hooks_provider);
        augment_with_local_config(&ctx.dir, &mut analysis_config);

        ctx.logger
            .log_debug_sync(&format!("running test {}", ctx.dir));

        let config = Arc::new(analysis_config);

        let mut stub_dirs = vec![cwd.clone() + "/tests/stubs"];

        if ctx.dir.to_ascii_lowercase().contains("xhp") {
            stub_dirs.push(cwd.clone() + "/third-party/xhp-lib/src");
        }

        let interner = Interner::default();

        let result = hakana_orchestrator::scan_and_analyze(
            stub_dirs,
            None,
            Some(FxHashSet::from_iter([
                format!("{}/tests/stubs/stubs.hack", cwd),
                format!("{}/third-party/xhp-lib/src", cwd),
            ])),
            config,
            if ctx.previous_scan_data.is_none() {
                ctx.cache_dir
            } else {
                None
            },
            1,
            ctx.logger,
            ctx.build_checksum,
            interner,
            ctx.previous_scan_data,
            ctx.previous_analysis_result,
            None,
            || {},
        );

        let input_file = format!("{}/input.hack", ctx.dir);
        let output_file = format!("{}/output.txt", ctx.dir);
        let actual_file = format!("{}/actual.txt", ctx.dir);
        let input_contents = fs::read_to_string(&input_file).unwrap();
        let expected_output_contents = fs::read_to_string(output_file).unwrap();

        let mut result = result.unwrap();

        let time_in_analysis = result.0.time_in_analysis;

        let input_file_path = FilePath(result.1.interner.get(&input_file).unwrap());

        let edit_set = result.0.take_edits_for_file(&input_file_path);

        let output_contents = if !edit_set.is_empty() {
            edit_set
                .apply(&input_contents)
                .unwrap_or_else(|e| panic!("Failed to apply edits: {}", e))
        } else {
            input_contents
        };

        fs::write(actual_file, &output_contents).unwrap();

        if output_contents == expected_output_contents {
            TestResult::pass(Some(result.1), Some(result.0), time_in_analysis)
        } else {
            TestResult::fail(
                ctx.dir,
                format_diff(&expected_output_contents, &output_contents),
                Some(result.1),
                Some(result.0),
                time_in_analysis,
            )
        }
    }
}
