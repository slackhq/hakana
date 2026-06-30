use hakana_str::Interner;
use rustc_hash::FxHashSet;

use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestArtifacts, TestContext};
use crate::test_runners::outputs::IssueSnapshot;
use crate::test_runners::utils::{augment_with_local_config, default_config_for_test};

/// The default integration test type.
///
/// Runs a full scan-and-analyze pass on the test's `input.hack`, collects
/// reported issues, and compares them against `output.txt`.
pub struct StandardAnalysisTest;

impl IntegrationTest for StandardAnalysisTest {
    fn run(&self, ctx: TestContext) -> Result<TestArtifacts, String> {
        let cwd = &ctx.cwd;

        let mut analysis_config = default_config_for_test(&ctx.dir, ctx.hooks_provider);
        augment_with_local_config(&ctx.dir, &mut analysis_config);

        log::debug!("running test {}", ctx.dir);

        let config = Arc::new(analysis_config);

        let mut stub_dirs = vec![cwd.clone() + "/tests/stubs"];

        if ctx.dir.to_ascii_lowercase().contains("xhp") {
            stub_dirs.push(cwd.clone() + "/third-party/xhp-lib/src");
        }

        let interner = Arc::new(Interner::default());

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
            false,
            ctx.build_checksum,
            interner,
            ctx.previous_scan_data,
            ctx.previous_analysis_result,
            None,
            || {},
        );

        match result {
            Ok((analysis_result, run_data)) => {
                let time_in_analysis = analysis_result.time_in_analysis;

                let mut output = vec![];
                for (file_path, issues) in
                    analysis_result.get_all_issues(&run_data.interner, &ctx.dir, true)
                {
                    for issue in issues {
                        output.push(issue.format(&file_path));
                    }
                }

                Ok(TestArtifacts::new(
                    Some(run_data),
                    Some(analysis_result),
                    time_in_analysis,
                    vec![Box::new(IssueSnapshot {
                        dir: ctx.dir,
                        actual: output,
                    })],
                ))
            }
            Err(error) => Err(error.to_string()),
        }
    }
}
