use hakana_analyzer::custom_hook::CustomHook;
use hakana_logger::Logger;
use hakana_orchestrator::SuccessfulScanData;
use hakana_orchestrator::wasm::get_single_file_codebase;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rustc_hash::FxHashMap;
use walkdir::WalkDir;

use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::integration_test::{TestContext, select_test_type};

pub trait HooksProvider {
    fn get_hooks_for_test(&self, dir: &str) -> Vec<Box<dyn CustomHook>>;
    fn get_linters_for_test(&self, dir: &str) -> Vec<Box<dyn hakana_lint::Linter>>;
}

pub struct TestRunner(pub Box<dyn HooksProvider>);

impl TestRunner {
    pub fn run_test(
        &self,
        test_or_test_dir: String,
        logger: Arc<Logger>,
        use_cache: bool,
        reuse_codebase: bool,
        had_error: &mut bool,
        build_checksum: &str,
        repeat: u16,
        random_seed: Option<u64>,
    ) {
        let candidate_test_folders = match get_all_test_folders(test_or_test_dir.clone()) {
            Ok(folders) => folders,
            Err(error) => {
                eprintln!("Error: {}", error);
                *had_error = true;
                return;
            }
        };

        let mut test_diagnostics = vec![];

        let starter_data =
            if candidate_test_folders.len() > 1 && !test_or_test_dir.ends_with("/diff") {
                let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();
                let stub_path = format!("{}/tests/stubs/stubs.hack", cwd);
                let (codebase, interner, file_system) =
                    get_single_file_codebase(vec![&stub_path]);

                Some(SuccessfulScanData {
                    codebase,
                    interner,
                    file_system,
                    resolved_names: FxHashMap::default(),
                })
            } else {
                None
            };

        let mut last_scan_data = None;
        let mut last_analysis_result = None;

        let mut time_in_analysis = Duration::default();

        let mut test_folders = candidate_test_folders.clone();

        if let Some(random_seed) = random_seed {
            println!("Running with random seed: {}\n", random_seed);
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(random_seed);
            test_folders.shuffle(&mut rng);
        }

        for _ in 0..(repeat + 1) {
            for test_folder in test_folders.clone() {
                // Only create cache directory for non-hhast tests and actual directories
                let cache_dir =
                    if !test_folder.contains("/hhast_tests/") && Path::new(&test_folder).is_dir() {
                        let cache_dir = format!("{}/.hakana_cache", test_folder);
                        if !Path::new(&cache_dir).is_dir() && fs::create_dir(&cache_dir).is_err() {
                            panic!("could not create aast cache directory");
                        }
                        Some(cache_dir)
                    } else {
                        None
                    };

                let needs_fresh_codebase = test_folder.to_ascii_lowercase().contains("xhp");

                let previous_scan_data = last_scan_data
                    .filter(|_| reuse_codebase)
                    .or_else(|| starter_data.clone())
                    .filter(|_| !needs_fresh_codebase);

                let previous_analysis_result =
                    last_analysis_result.filter(|_| reuse_codebase);

                let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();

                let test = select_test_type(&test_folder);
                let ctx = TestContext {
                    dir: test_folder,
                    cwd,
                    logger: logger.clone(),
                    cache_dir: cache_dir.as_ref().filter(|_| use_cache),
                    build_checksum,
                    previous_scan_data,
                    previous_analysis_result,
                    hooks_provider: &*self.0,
                };
                let result = test.run(ctx);

                time_in_analysis += result.time_in_analysis;

                if let Some(diagnostic) = result.diagnostic {
                    test_diagnostics.push(diagnostic);
                    *had_error = true;
                }

                last_scan_data = result.scan_data;
                last_analysis_result = result.analysis_result;

                print!("{}", result.status_char);
                io::stdout().flush().unwrap();
            }
        }

        println!("\n\nTotal analysis time:  {:.2?}", time_in_analysis);

        println!(
            "\n{}",
            test_diagnostics
                .into_iter()
                .map(|(folder, diag)| format!("Unexpected output for {}:\n\n{}", folder, diag))
                .collect::<Vec<_>>()
                .join("\n\n")
        );
    }
}

fn get_all_test_folders(test_or_test_dir: String) -> Result<Vec<String>, String> {
    let mut test_folders = vec![];
    let normalized_test_dir = test_or_test_dir.trim_end_matches('/');

    // Check if this is a specific HHAST test file (with or without extension)
    if normalized_test_dir.contains("/hhast_tests/") {
        // Try to find a matching .in file
        let php_in_path = format!("{}.php.in", normalized_test_dir);
        let hack_in_path = format!("{}.hack.in", normalized_test_dir);

        if Path::new(&php_in_path).exists() || Path::new(&hack_in_path).exists() {
            // This is a specific test file - return just this one
            return Ok(vec![normalized_test_dir.to_owned()]);
        }
    }

    // Check if the specified test directory exists
    if !Path::new(&test_or_test_dir).exists() {
        return Err(format!(
            "Test directory does not exist: {}",
            test_or_test_dir
        ));
    }

    let input_hack_path = normalized_test_dir.to_owned() + "/input.hack";
    let output_txt_path = normalized_test_dir.to_owned() + "/output.txt";

    if Path::new(&input_hack_path).exists() {
        // This looks like a single test directory
        if !Path::new(&output_txt_path).exists()
            && !test_or_test_dir.contains("/goto-definition/")
            && !test_or_test_dir.contains("/references/")
        {
            return Err(format!(
                "Test directory is missing required output.txt file: {}",
                test_or_test_dir
            ));
        }
        test_folders.push(normalized_test_dir.to_owned());
    } else {
        // Walk the directory to find test folders
        for entry in WalkDir::new(&test_or_test_dir)
            .sort_by(|a, b| a.file_name().cmp(b.file_name()))
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            let metadata = fs::metadata(path).unwrap();

            if metadata.is_dir() {
                if let Some(path_str) = path.to_str() {
                    let input_hack = path_str.to_owned() + "/input.hack";
                    let output_txt = path_str.to_owned() + "/output.txt";
                    let candidates_txt = path_str.to_owned() + "/candidates.txt";

                    if path_str.contains("/diff/") {
                        if Path::new(&(path_str.to_owned() + "/a")).is_dir() {
                            // Found a diff test directory - check if output.txt exists
                            if !Path::new(&output_txt).exists() {
                                return Err(format!(
                                    "Diff test directory is missing required output.txt file: {}",
                                    path_str
                                ));
                            }
                            test_folders.push(path_str.to_owned());
                        }
                    } else if path_str.contains("/hhast_tests/") {
                        // Skip directories that start with "skipped-"
                        if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                            if dir_name.starts_with("skipped-") {
                                continue;
                            }
                        }

                        // For HHAST tests, enumerate individual test files (not directories)
                        if let Ok(entries) = fs::read_dir(path_str) {
                            let mut in_files: Vec<String> = entries
                                .filter_map(|e| e.ok())
                                .filter_map(|e| {
                                    let file_path = e.path();
                                    let file_name = file_path.to_string_lossy().to_string();
                                    if file_name.ends_with(".php.in")
                                        || file_name.ends_with(".hack.in")
                                    {
                                        // Remove the ".in" extension to get the base test name
                                        let base_name = file_name
                                            .trim_end_matches(".php.in")
                                            .trim_end_matches(".hack.in");
                                        Some(base_name.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            in_files.sort();
                            test_folders.extend(in_files);
                        }
                    } else if Path::new(&input_hack).exists() {
                        // Migration candidates tests use candidates.txt instead of output.txt
                        if path_str.contains("/migration-candidates/")
                            && !Path::new(&candidates_txt).exists()
                        {
                            return Err(format!(
                                "Migration candidates test directory is missing required candidates.txt file: {}",
                                path_str
                            ));
                        } else if !Path::new(&output_txt).exists()
                            && !path_str.contains("/goto-definition/")
                            && !path_str.contains("/references/")
                        {
                            // Found a regular test directory - check if output.txt exists
                            return Err(format!(
                                "Test directory is missing required output.txt file: {}",
                                path_str
                            ));
                        }
                        test_folders.push(path_str.to_owned());
                    }
                }
            }
        }

        // If no test folders were found in the directory, it's an error
        if test_folders.is_empty() {
            return Err(format!(
                "No test directories found in: {}",
                test_or_test_dir
            ));
        }
    }
    Ok(test_folders)
}
