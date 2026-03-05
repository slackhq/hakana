use hakana_code_info::symbol_references_utils::generate_references_json;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::{augment_with_local_config, default_config_for_test, format_diff};

/// Validates symbol-reference results for `tests/references/` directories.
///
/// Runs analysis and generates a `references.json` that groups usages by
/// symbol name, then compares it against the expected snapshot. If no
/// snapshot exists yet, one is written automatically.
pub struct ReferencesTest;

impl IntegrationTest for ReferencesTest {
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

        let references_json = generate_references_json(&result.0, &result.1.interner);
        let references_path = ctx.dir.clone() + "/references.json";

        if Path::new(&references_path).exists() {
            let expected_references = fs::read_to_string(&references_path)
                .unwrap()
                .trim()
                .to_string();

            if expected_references.trim() != references_json.trim() {
                return TestResult::fail(
                    ctx.dir,
                    format_diff(&expected_references, &references_json),
                    Some(result.1),
                    Some(result.0),
                    time_in_analysis,
                );
            }
        } else if let Err(e) = fs::write(&references_path, references_json) {
            eprintln!("Warning: Failed to write references.json: {}", e);
        }

        TestResult::pass(Some(result.1), Some(result.0), time_in_analysis)
    }
}
