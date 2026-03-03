use hakana_str::Interner;
use rustc_hash::FxHashSet;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::test_runners::integration_test::{IntegrationTest, TestContext, TestResult};
use crate::test_runners::utils::{
    augment_with_local_config, compare_issues_to_expected, copy_recursively,
    default_config_for_test, format_diff, generate_definition_locations_json,
};

/// Runs incremental (diff) analysis tests under `tests/diff/`.
///
/// Iterates through stage subdirectories (`a/`, `b/`, `c/`, `d/`) copying
/// each into a temporary `workdir/`, running scan-and-analyze incrementally,
/// and feeding the previous result into the next stage. After all stages,
/// validates both `definition_locations.json` and issue output against
/// expected snapshots.
pub struct DiffTest;

impl IntegrationTest for DiffTest {
    fn run(&self, ctx: TestContext) -> TestResult {
        let cwd = &ctx.cwd;

        ctx.logger
            .log_debug_sync(&format!("running test {}", ctx.dir));

        if let Some(cache_dir) = ctx.cache_dir {
            fs::remove_dir_all(cache_dir).unwrap();
            fs::create_dir(cache_dir).unwrap();
        }

        let workdir_base = ctx.dir.clone() + "/workdir";

        let mut folders = vec![(ctx.dir.clone() + "/a", false)];

        if Path::new(&(ctx.dir.clone() + "/a-before-analysis")).exists() {
            folders[0].1 = true;
        }

        if Path::new(&(ctx.dir.clone() + "/b")).exists() {
            folders.push((ctx.dir.clone() + "/b", false));
        }

        if Path::new(&(ctx.dir.clone() + "/c")).exists() {
            folders.push((ctx.dir.clone() + "/c", false));
        }

        if Path::new(&(ctx.dir.clone() + "/d")).exists() {
            folders.push((ctx.dir.clone() + "/d", false));
        }

        let mut previous_scan_data = None;
        let mut previous_analysis_result = None;

        let mut config = default_config_for_test(&workdir_base, ctx.hooks_provider);
        augment_with_local_config(&ctx.dir, &mut config);

        config.ast_diff = true;
        config.find_unused_definitions = true;
        let interner = Interner::default();
        let config = Arc::new(config);
        let mut stub_dirs = vec![cwd.clone() + "/tests/stubs"];

        if ctx.dir.to_ascii_lowercase().contains("xhp") {
            stub_dirs.push(cwd.clone() + "/third-party/xhp-lib/src");
        }

        for (folder, change_after_scan) in folders {
            copy_recursively(folder.clone(), workdir_base.clone()).unwrap();

            let run_result = hakana_orchestrator::scan_and_analyze(
                stub_dirs.clone(),
                None,
                Some(FxHashSet::from_iter([
                    format!("{}/tests/stubs/stubs.hack", cwd),
                    format!("{}/third-party/xhp-lib/src", cwd),
                ])),
                config.clone(),
                None,
                1,
                ctx.logger.clone(),
                ctx.build_checksum,
                interner.clone(),
                previous_scan_data,
                previous_analysis_result,
                None,
                || {
                    if change_after_scan {
                        copy_recursively(folder.clone() + "-before-analysis", workdir_base.clone())
                            .unwrap();
                    }
                },
            );

            let _ = fs::remove_dir_all(&workdir_base);

            match run_result {
                Ok(run_result) => {
                    previous_scan_data = Some(run_result.1);
                    previous_analysis_result = Some(run_result.0);
                }
                Err(error) => {
                    return TestResult::fail(
                        ctx.dir,
                        error.to_string(),
                        None,
                        None,
                        std::time::Duration::default(),
                    );
                }
            }
        }

        let run_data = previous_scan_data.unwrap();
        let analysis_result = previous_analysis_result.unwrap();

        let mut output = vec![];
        for (file_path, issues) in
            analysis_result.get_all_issues(&run_data.interner, &workdir_base, true)
        {
            for issue in issues {
                output.push(issue.format(&file_path));
            }
        }

        let test_output = output;

        // Generate definition_locations.json
        let definition_locations_json =
            generate_definition_locations_json(&analysis_result, &run_data.interner);
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
                    Some(run_data),
                    Some(analysis_result),
                    std::time::Duration::default(),
                );
            }
        } else if let Err(e) = fs::write(&definition_locations_path, definition_locations_json) {
            eprintln!("Warning: Failed to write definition_locations.json: {}", e);
        }

        let (passed, diagnostic) = compare_issues_to_expected(&ctx.dir, &test_output);

        if passed {
            TestResult::pass(
                Some(run_data),
                Some(analysis_result),
                std::time::Duration::default(),
            )
        } else {
            TestResult::fail(
                ctx.dir,
                diagnostic.unwrap_or_default(),
                Some(run_data),
                Some(analysis_result),
                std::time::Duration::default(),
            )
        }
    }
}
