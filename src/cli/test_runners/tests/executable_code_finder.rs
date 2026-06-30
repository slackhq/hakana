use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestArtifacts, TestContext};
use crate::test_runners::outputs::JsonValueSnapshot;
use crate::test_runners::utils::{augment_with_local_config, default_config_for_test};

/// Runs the executable-code-finder on `tests/executable-code-finder/` directories.
///
/// Scans the test input for executable lines and compares the JSON output
/// against `output.txt`.
pub struct ExecutableCodeFinderTest;

impl IntegrationTest for ExecutableCodeFinderTest {
    fn run(&self, ctx: TestContext) -> Result<TestArtifacts, String> {
        let mut analysis_config = default_config_for_test(&ctx.dir, ctx.hooks_provider);
        augment_with_local_config(&ctx.dir, &mut analysis_config);

        let config = Arc::new(analysis_config);

        let test_output =
            executable_finder::scan_files(&vec![ctx.dir.clone()], None, &config, 1, false)
                .map_err(|_| "executable code finder failed".to_string())?;

        Ok(TestArtifacts::new(
            None,
            None,
            std::time::Duration::default(),
            vec![Box::new(JsonValueSnapshot {
                path: ctx.dir.clone() + "/output.txt",
                actual: serde_json::to_value(&test_output).unwrap(),
            })],
        ))
    }
}
