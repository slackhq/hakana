use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestArtifacts, TestContext};
use crate::test_runners::outputs::JsonValueSnapshot;
use crate::test_runners::utils::{
    augment_with_local_config, default_config_for_test, generate_definition_locations_json,
};

/// Validates go-to-definition results for `tests/goto-definition/` directories.
///
/// Runs analysis with `collect_goto_definition_locations` enabled, then
/// compares the generated `definition_locations.json` against the expected
/// snapshot. Run with `--update-snapshots` to (re)generate the snapshot.
pub struct GotoDefinitionTest;

impl IntegrationTest for GotoDefinitionTest {
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

        let definition_locations_json =
            generate_definition_locations_json(&result.0, &result.1.interner);
        let definition_locations_path = ctx.dir.clone() + "/definition_locations.json";
        let actual = serde_json::from_str(&definition_locations_json)
            .map_err(|e| format!("Failed to serialize definition locations: {}", e))?;

        Ok(TestArtifacts::new(
            Some(result.1),
            Some(result.0),
            time_in_analysis,
            vec![Box::new(JsonValueSnapshot {
                path: definition_locations_path,
                actual,
            })],
        ))
    }
}
