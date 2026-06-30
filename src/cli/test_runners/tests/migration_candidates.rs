use hakana_analyzer::custom_hook::CustomHook;
use hakana_str::Interner;
use rustc_hash::FxHashSet;

use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestArtifacts, TestContext};
use crate::test_runners::outputs::CandidatesSnapshot;
use crate::test_runners::utils::{augment_with_local_config, default_config_for_test};

/// Validates migration-candidate detection for `tests/migration-candidates/` directories.
///
/// Runs analysis with migration mode enabled, collects candidates reported by
/// custom hooks, and compares them against the expected list in `candidates.txt`.
/// Reports both unexpected and missing candidates on failure.
pub struct MigrationCandidatesTest;

impl IntegrationTest for MigrationCandidatesTest {
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
            config.clone(),
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

        let result = result.map_err(|error| error.to_string())?;

        let time_in_analysis = result.0.time_in_analysis;

        let mut migration_candidates = vec![];
        for config_hook in &config.hooks {
            let hook: &dyn CustomHook = &**config_hook;
            for candidate in hook.get_candidates(&result.1.codebase, &result.1.interner, &result.0)
            {
                migration_candidates.push(candidate);
            }
        }

        let candidates_file = format!("{}/candidates.txt", ctx.dir);

        Ok(TestArtifacts::new(
            Some(result.1),
            Some(result.0),
            time_in_analysis,
            vec![Box::new(CandidatesSnapshot {
                path: candidates_file,
                actual: migration_candidates,
            })],
        ))
    }
}
