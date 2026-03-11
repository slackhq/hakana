use hakana_str::Interner;
use rustc_hash::FxHashSet;

use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::{
    augment_with_local_config, compare_issues_to_expected, default_config_for_test,
};

/// Runs cyclomatic complexity analysis on `tests/cyclomatic-complexity/` directories.
///
/// Enables cyclomatic complexity analysis with a threshold of 0 (to capture all
/// functions), then formats and compares the results against `output.txt`.
pub struct CyclomaticComplexityTest;

impl IntegrationTest for CyclomaticComplexityTest {
    fn run(&self, ctx: TestContext) -> TestResult {
        let cwd = &ctx.cwd;

        let mut analysis_config = default_config_for_test(&ctx.dir, ctx.hooks_provider);
        augment_with_local_config(&ctx.dir, &mut analysis_config);

        analysis_config.analyze_cyclomatic_complexity = true;
        analysis_config.cyclomatic_complexity_threshold = 0;

        let config = Arc::new(analysis_config);

        let stub_dirs = vec![cwd.clone() + "/tests/stubs"];

        let interner = Interner::default();

        let result = hakana_orchestrator::scan_and_analyze(
            stub_dirs,
            None,
            Some(FxHashSet::from_iter([format!(
                "{}/tests/stubs/stubs.hack",
                cwd
            )])),
            config,
            None,
            1,
            ctx.logger,
            ctx.build_checksum,
            interner,
            None,
            None,
            None,
            || {},
        );

        match result {
            Ok((analysis_result, run_data)) => {
                let time_in_analysis = analysis_result.time_in_analysis;

                let mut results = analysis_result.cyclomatic_complexity;
                results.sort_by(|a, b| b.cmp(&run_data.interner, &a));

                let output: Vec<String> = results
                    .iter()
                    .map(|c| c.to_string(&run_data.interner) + "\n")
                    .collect();

                let (passed, diagnostic) = compare_issues_to_expected(&ctx.dir, &output);

                if passed {
                    TestResult::pass(None, None, time_in_analysis)
                } else {
                    TestResult::fail(
                        ctx.dir,
                        diagnostic.unwrap_or_default(),
                        None,
                        None,
                        time_in_analysis,
                    )
                }
            }
            Err(error) => TestResult::fail(
                ctx.dir,
                error.to_string(),
                None,
                None,
                std::time::Duration::default(),
            ),
        }
    }
}
