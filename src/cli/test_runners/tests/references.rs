use hakana_code_info::symbol_references_utils::generate_references_json;

use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestArtifacts, TestContext};
use crate::test_runners::outputs::JsonValueSnapshot;
use crate::test_runners::utils::{augment_with_local_config, default_config_for_test};

/// Validates symbol-reference results for `tests/references/` directories.
///
/// Runs analysis and generates a `references.json` that groups usages by
/// symbol name, then compares it against the expected snapshot. Run with
/// `--update-snapshots` to (re)generate the snapshot.
pub struct ReferencesTest;

impl IntegrationTest for ReferencesTest {
    fn run(&self, ctx: TestContext) -> Result<TestArtifacts, String> {
        let mut analysis_config = default_config_for_test(&ctx.dir, ctx.hooks_provider);
        augment_with_local_config(&ctx.dir, &mut analysis_config);

        let config = Arc::new(analysis_config);

        let result = hakana_orchestrator::scan_and_analyze(
            Vec::new(),
            Some(ctx.dir.clone()),
            None,
            config,
            None,
            1,
            false,
            ctx.build_checksum,
            Arc::new(hakana_str::Interner::default()),
            ctx.previous_scan_data,
            ctx.previous_analysis_result,
            None,
            || {},
        );

        let result = result.map_err(|e| e.to_string())?;

        let time_in_analysis = result.0.time_in_analysis;

        let references_json = generate_references_json(&result.0, &result.1.interner);
        let references_path = ctx.dir.clone() + "/references.json";
        let actual = serde_json::from_str(&references_json)
            .map_err(|e| format!("Failed to serialize references: {}", e))?;

        Ok(TestArtifacts::new(
            Some(result.1),
            Some(result.0),
            time_in_analysis,
            vec![Box::new(JsonValueSnapshot {
                path: references_path,
                actual,
            })],
        ))
    }
}
