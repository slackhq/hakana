use hakana_analyzer::custom_hook::CustomHook;
use hakana_str::Interner;
use rustc_hash::FxHashSet;

use std::fs;
use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::{augment_with_local_config, default_config_for_test};

/// Validates migration-candidate detection for `tests/migration-candidates/` directories.
///
/// Runs analysis with migration mode enabled, collects candidates reported by
/// custom hooks, and compares them against the expected list in `candidates.txt`.
/// Reports both unexpected and missing candidates on failure.
pub struct MigrationCandidatesTest;

impl IntegrationTest for MigrationCandidatesTest {
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
            config.clone(),
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

        let candidates_file = format!("{}/candidates.txt", ctx.dir);
        let expected_candidates = fs::read_to_string(candidates_file)
            .unwrap()
            .lines()
            .map(String::from)
            .collect::<Vec<String>>();

        let result = result.unwrap();

        let time_in_analysis = result.0.time_in_analysis;

        let mut migration_candidates = vec![];
        for config_hook in &config.hooks {
            let hook: &dyn CustomHook = &**config_hook;
            for candidate in
                hook.get_candidates(&result.1.codebase, &result.1.interner, &result.0)
            {
                migration_candidates.push(candidate);
            }
        }

        let missing_candidates = expected_candidates
            .iter()
            .filter(|item| !migration_candidates.contains(item))
            .cloned()
            .collect::<Vec<String>>();
        let unexpected_candidates = migration_candidates
            .iter()
            .filter(|item| !expected_candidates.contains(item))
            .cloned()
            .collect::<Vec<String>>();

        let mut diagnostics = vec![];
        if !unexpected_candidates.is_empty() {
            diagnostics.push(format!(
                "Found unexpected candidates: {}",
                unexpected_candidates.join("\n")
            ));
        }
        if !missing_candidates.is_empty() {
            diagnostics.push(format!(
                "Missing expected candidates: {}",
                missing_candidates.join("\n")
            ));
        }

        if diagnostics.is_empty() {
            TestResult::pass(Some(result.1), Some(result.0), time_in_analysis)
        } else {
            TestResult::fail(
                ctx.dir,
                diagnostics.join("\n"),
                Some(result.1),
                Some(result.0),
                time_in_analysis,
            )
        }
    }
}
