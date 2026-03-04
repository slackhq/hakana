use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::{
    augment_with_local_config, default_config_for_test, format_diff,
    generate_definition_locations_json,
};

/// Validates go-to-definition results for `tests/goto-definition/` directories.
///
/// Runs analysis with `collect_goto_definition_locations` enabled, then
/// compares the generated `definition_locations.json` against the expected
/// snapshot. If no snapshot exists yet, one is written automatically.
pub struct GotoDefinitionTest;

impl IntegrationTest for GotoDefinitionTest {
    fn run(&self, ctx: TestContext) -> TestResult {
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
            ctx.logger,
            ctx.build_checksum,
            hakana_str::Interner::default(),
            ctx.previous_scan_data,
            ctx.previous_analysis_result,
            None,
            || {},
        );

        let result = match result {
            Ok(result) => result,
            Err(e) => {
                return TestResult::fail(
                    ctx.dir,
                    e.to_string(),
                    None,
                    None,
                    std::time::Duration::default(),
                );
            }
        };

        let time_in_analysis = result.0.time_in_analysis;

        let definition_locations_json =
            generate_definition_locations_json(&result.0, &result.1.interner);
        let definition_locations_path = ctx.dir.clone() + "/definition_locations.json";

        if Path::new(&definition_locations_path).exists() {
            let expected_definition_locations = fs::read_to_string(&definition_locations_path)
                .unwrap()
                .trim()
                .to_string();

            if expected_definition_locations.trim() != definition_locations_json.trim() {
                return TestResult::fail(
                    ctx.dir,
                    format_diff(&expected_definition_locations, &definition_locations_json),
                    Some(result.1),
                    Some(result.0),
                    time_in_analysis,
                );
            }
        } else if let Err(e) = fs::write(&definition_locations_path, definition_locations_json) {
            eprintln!("Warning: Failed to write definition_locations.json: {}", e);
        }

        TestResult::pass(Some(result.1), Some(result.0), time_in_analysis)
    }
}
