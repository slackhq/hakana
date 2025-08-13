use executable_finder::ExecutableLines;
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::code_location::FilePath;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::data_flow::graph::WholeProgramKind;
use hakana_code_info::issue::IssueKind;
use hakana_logger::Logger;
use hakana_orchestrator::wasm::get_single_file_codebase;
use hakana_orchestrator::SuccessfulScanData;
use hakana_str::{Interner, StrId};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use serde_json;
use similar::{ChangeTag, TextDiff};

use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use walkdir::WalkDir;

pub trait HooksProvider {
    fn get_hooks_for_test(&self, dir: &str) -> Vec<Box<dyn CustomHook>>;
}

fn format_diff(expected: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut output = String::new();

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        output.push_str(&format!("{}{}", sign, change));
    }

    output
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
                let (codebase, interner, file_system) =
                    get_single_file_codebase(vec!["tests/stubs/stubs.hack"]);

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
                let cache_dir = format!("{}/.hakana_cache", test_folder);

                if !Path::new(&cache_dir).is_dir() && fs::create_dir(&cache_dir).is_err() {
                    panic!("could not create aast cache directory");
                }

                let needs_fresh_codebase = test_folder.to_ascii_lowercase().contains("xhp");

                let test_result = self.run_test_in_dir(
                    test_folder,
                    logger.clone(),
                    if use_cache { Some(&cache_dir) } else { None },
                    had_error,
                    &mut test_diagnostics,
                    build_checksum,
                    if let Some(last_run_data) = last_scan_data {
                        if reuse_codebase {
                            Some(last_run_data)
                        } else if !needs_fresh_codebase {
                            starter_data.clone()
                        } else {
                            None
                        }
                    } else if !needs_fresh_codebase {
                        starter_data.clone()
                    } else {
                        None
                    },
                    last_analysis_result.filter(|_last_analysis_result| reuse_codebase),
                    &mut time_in_analysis,
                );

                last_scan_data = test_result.1;
                last_analysis_result = test_result.2;

                print!("{}", test_result.0);
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

    fn get_config_for_test(&self, dir: &str) -> config::Config {
        let mut analysis_config = config::Config::new(dir.to_string(), FxHashSet::default());
        analysis_config.add_date_comments = false;

        let mut dir_parts = dir.split('/').collect::<Vec<_>>();

        while let Some(&"tests" | &"internal" | &"public") = dir_parts.first() {
            dir_parts = dir_parts[1..].to_vec();
        }

        let maybe_issue_name = dir_parts.get(1).unwrap().to_string();

        let dir_issue = IssueKind::from_str(&maybe_issue_name);
        analysis_config.find_unused_expressions = if let Ok(dir_issue) = &dir_issue {
            dir_issue.requires_dataflow_analysis()
        } else {
            dir.contains("/unused/")
        };
        analysis_config.find_unused_definitions = if let Ok(dir_issue) = &dir_issue {
            dir_issue.is_unused_definition()
        } else {
            dir.to_ascii_lowercase().contains("unused") && !dir.contains("UnusedExpression")
        };
        analysis_config.graph_kind = if dir.contains("/security/") {
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        } else if dir.contains("/find-paths/") {
            GraphKind::WholeProgram(WholeProgramKind::Query)
        } else {
            GraphKind::FunctionBody
        };

        analysis_config.hooks = self.0.get_hooks_for_test(dir);

        if dir.contains("/migrations/") {
            let replacements_path = dir.to_string() + "/replacements.txt";
            let replacements = fs::read_to_string(replacements_path).unwrap().to_string();

            analysis_config.migration_symbols = replacements
                .lines()
                .map(|v| {
                    let mut parts = v.split(',').collect::<Vec<_>>();
                    let first_part = parts.remove(0);
                    (first_part.to_string(), parts.join(","))
                })
                .collect();
            analysis_config.in_migration = true;
        } else if dir.contains("/fix/") {
            let issue_name = dir_parts.get(1).unwrap().to_string();

            analysis_config
                .issues_to_fix
                .insert(IssueKind::from_str_custom(&issue_name, &FxHashSet::default()).unwrap());
        } else if dir.contains("/add-fixmes/") {
            let issue_name = dir_parts.get(1).unwrap().to_string();

            analysis_config
                .issues_to_fix
                .insert(IssueKind::from_str_custom(&issue_name, &FxHashSet::default()).unwrap());

            analysis_config.add_fixmes = true;
            analysis_config.find_unused_expressions = true;
        } else if dir.contains("/remove-unused-fixmes/") {
            analysis_config.remove_fixmes = true;
            analysis_config.find_unused_expressions = true;
        } else if dir.contains("/migration-candidates/") {
            analysis_config.in_migration = true;
        }

        // Enable go-to-definition collection for goto-definition and diff tests
        if dir.contains("/goto-definition/") || dir.contains("/diff/") {
            analysis_config.collect_goto_definition_locations = true;
        }

        analysis_config
    }

    fn run_test_in_dir(
        &self,
        dir: String,
        logger: Arc<Logger>,
        cache_dir: Option<&String>,
        had_error: &mut bool,
        test_diagnostics: &mut Vec<(String, String)>,
        build_checksum: &str,
        previous_scan_data: Option<SuccessfulScanData>,
        previous_analysis_result: Option<AnalysisResult>,
        total_time_in_analysis: &mut Duration,
    ) -> (String, Option<SuccessfulScanData>, Option<AnalysisResult>) {
        if dir.contains("skipped-") || dir.contains("SKIPPED-") {
            return (
                "S".to_string(),
                previous_scan_data,
                previous_analysis_result,
            );
        }

        if dir.contains("/diff/") {
            return self.run_diff_test(
                dir,
                logger,
                cache_dir,
                had_error,
                test_diagnostics,
                build_checksum,
            );
        }

        let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();

        let mut analysis_config = self.get_config_for_test(&dir);

        augment_with_local_config(&dir, &mut analysis_config);

        logger.log_debug_sync(&format!("running test {}", dir));

        let config = Arc::new(analysis_config);

        if dir.contains("/goto-definition/") {
            return self.run_goto_definition_test(
                dir,
                config,
                logger,
                cache_dir,
                had_error,
                test_diagnostics,
                build_checksum,
                previous_scan_data,
                previous_analysis_result,
                total_time_in_analysis,
            );
        }

        if dir.contains("/executable-code-finder/") {
            return match executable_finder::scan_files(
                &vec![dir.clone()],
                None,
                &config.clone(),
                1,
                logger,
            ) {
                Ok(test_output) => {
                    let expected_output_path = dir.clone() + "/output.txt";
                    let expected_output = if Path::new(&expected_output_path).exists() {
                        let file_contents = fs::read_to_string(expected_output_path)
                            .unwrap()
                            .trim()
                            .to_string();
                        let j: Vec<ExecutableLines> = serde_json::from_str(&file_contents).unwrap();
                        Some(j)
                    } else {
                        None
                    };
                    if let Some(expected_output) = &expected_output {
                        if test_output == *expected_output {
                            (".".to_string(), None, None)
                        } else {
                            let expected_output_str =
                                serde_json::to_string_pretty(&expected_output).unwrap();
                            let test_output_str =
                                serde_json::to_string_pretty(&test_output).unwrap();
                            test_diagnostics
                                .push((dir, format_diff(&expected_output_str, &test_output_str)));
                            *had_error = true;
                            ("F".to_string(), None, None)
                        }
                    } else {
                        *had_error = true;
                        ("F".to_string(), None, None)
                    }
                }
                Err(_) => {
                    *had_error = true;
                    ("F".to_string(), None, None)
                }
            };
        }

        let mut stub_dirs = vec![cwd.clone() + "/tests/stubs"];

        if dir.to_ascii_lowercase().contains("xhp") {
            stub_dirs.push(cwd.clone() + "/third-party/xhp-lib/src");
        }

        let interner = Interner::default();

        let result = hakana_orchestrator::scan_and_analyze(
            stub_dirs,
            None,
            Some(FxHashSet::from_iter([
                "tests/stubs/stubs.hack".to_string(),
                format!("{}/third-party/xhp-lib/src", cwd),
            ])),
            config.clone(),
            if previous_scan_data.is_none() {
                cache_dir
            } else {
                None
            },
            1,
            logger,
            build_checksum,
            interner,
            previous_scan_data,
            previous_analysis_result,
            None,
            || {},
        );

        if dir.contains("/migrations/")
            || dir.contains("/fix/")
            || dir.contains("/add-fixmes/")
            || dir.contains("/remove-unused-fixmes/")
        {
            let input_file = format!("{}/input.hack", dir);
            let output_file = format!("{}/output.txt", dir);
            let actual_file = format!("{}/actual.txt", dir);
            let input_contents = fs::read_to_string(&input_file).unwrap();
            let expected_output_contents = fs::read_to_string(output_file).unwrap();

            let mut result = result.unwrap();

            *total_time_in_analysis += result.0.time_in_analysis;

            let input_file_path = FilePath(result.1.interner.get(&input_file).unwrap());

            let replacements = result
                .0
                .replacements
                .remove(&input_file_path)
                .unwrap_or_default();
            let insertions = result
                .0
                .insertions
                .remove(&input_file_path)
                .unwrap_or_default();

            let output_contents = if !replacements.is_empty() || !insertions.is_empty() {
                crate::replace_contents(input_contents, replacements, insertions)
            } else {
                input_contents
            };

            fs::write(actual_file, &output_contents).unwrap();

            if output_contents == expected_output_contents {
                (".".to_string(), Some(result.1), Some(result.0))
            } else {
                test_diagnostics.push((
                    dir,
                    format_diff(&expected_output_contents, &output_contents),
                ));
                ("F".to_string(), Some(result.1), Some(result.0))
            }
        } else if dir.contains("/migration-candidates/") {
            let candidates_file = format!("{}/candidates.txt", dir);
            let expected_candidates = fs::read_to_string(candidates_file)
                .unwrap()
                .lines()
                .map(String::from)
                .collect::<Vec<String>>();

            let result = result.unwrap();

            *total_time_in_analysis += result.0.time_in_analysis;

            let mut migration_candidates = vec![];
            for config_hook in &config.hooks {
                for candidate in
                    config_hook.get_candidates(&result.1.codebase, &result.1.interner, &result.0)
                {
                    migration_candidates.push(candidate);
                }
            }

            let missing_candidates = expected_candidates
                .iter()
                .filter(|item| !migration_candidates.contains(item))
                .map(String::from)
                .collect::<Vec<String>>();
            let unexpected_candidates = migration_candidates
                .iter()
                .filter(|item| !expected_candidates.contains(item))
                .map(String::from)
                .collect::<Vec<String>>();

            if !unexpected_candidates.is_empty() {
                test_diagnostics.push((
                    dir.clone(),
                    format!(
                        "Found unexpected candidates: {}",
                        unexpected_candidates.join("\n")
                    ),
                ));
            }
            if !missing_candidates.is_empty() {
                test_diagnostics.push((
                    dir.clone(),
                    format!(
                        "Missing expected candidates: {}",
                        missing_candidates.join("\n")
                    ),
                ));
            }

            if !unexpected_candidates.is_empty() || !missing_candidates.is_empty() {
                ("F".to_string(), Some(result.1), Some(result.0))
            } else {
                (".".to_string(), Some(result.1), Some(result.0))
            }
        } else {
            match result {
                Ok((analysis_result, run_data)) => {
                    *total_time_in_analysis += analysis_result.time_in_analysis;

                    let mut output = vec![];
                    for (file_path, issues) in
                        analysis_result.get_all_issues(&run_data.interner, &dir, true)
                    {
                        for issue in issues {
                            output.push(issue.format(&file_path));
                        }
                    }

                    let test_output = output;

                    let expected_output_path = dir.clone() + "/output.txt";
                    let expected_output = if Path::new(&expected_output_path).exists() {
                        let expected = fs::read_to_string(expected_output_path)
                            .unwrap()
                            .trim()
                            .to_string();
                        Some(expected)
                    } else {
                        None
                    };

                    if if let Some(expected_output) = &expected_output {
                        if expected_output == test_output.join("").trim() {
                            true
                        } else {
                            !expected_output.is_empty()
                                && test_output.len() == 1
                                && expected_output
                                    .as_bytes()
                                    .iter()
                                    .filter(|&&c| c == b'\n')
                                    .count()
                                    == 0
                                && test_output.iter().any(|s| s.contains(expected_output))
                        }
                    } else {
                        test_output.is_empty()
                    } {
                        (".".to_string(), Some(run_data), Some(analysis_result))
                    } else {
                        if let Some(expected_output) = &expected_output {
                            test_diagnostics
                                .push((dir, format_diff(expected_output, &test_output.join(""))));
                        } else {
                            test_diagnostics.push((dir, format_diff("", &test_output.join(""))));
                        }
                        ("F".to_string(), Some(run_data), Some(analysis_result))
                    }
                }
                Err(error) => {
                    *had_error = true;
                    test_diagnostics.push((dir, error.to_string()));
                    ("F".to_string(), None, None)
                }
            }
        }
    }

    fn run_diff_test(
        &self,
        dir: String,
        logger: Arc<Logger>,
        cache_dir: Option<&String>,
        had_error: &mut bool,
        test_diagnostics: &mut Vec<(String, String)>,
        build_checksum: &str,
    ) -> (String, Option<SuccessfulScanData>, Option<AnalysisResult>) {
        let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();

        logger.log_debug_sync(&format!("running test {}", dir));

        if let Some(cache_dir) = cache_dir {
            fs::remove_dir_all(cache_dir).unwrap();
            fs::create_dir(cache_dir).unwrap();
        }

        let workdir_base = dir.clone() + "/workdir";

        let mut folders = vec![(dir.clone() + "/a", false)];

        if Path::new(&(dir.clone() + "/a-before-analysis")).exists() {
            folders[0].1 = true;
        }

        if Path::new(&(dir.clone() + "/b")).exists() {
            folders.push((dir.clone() + "/b", false));
        }

        if Path::new(&(dir.clone() + "/c")).exists() {
            folders.push((dir.clone() + "/c", false));
        }

        if Path::new(&(dir.clone() + "/d")).exists() {
            folders.push((dir.clone() + "/d", false));
        }

        let mut previous_scan_data = None;
        let mut previous_analysis_result = None;

        let mut config = self.get_config_for_test(&workdir_base);
        augment_with_local_config(&dir, &mut config);

        config.ast_diff = true;
        config.find_unused_definitions = true;
        let interner = Interner::default();
        let config = Arc::new(config);
        let mut stub_dirs = vec![cwd.clone() + "/tests/stubs"];

        if dir.to_ascii_lowercase().contains("xhp") {
            stub_dirs.push(cwd.clone() + "/third-party/xhp-lib/src");
        }

        for (folder, change_after_scan) in folders {
            copy_recursively(folder.clone(), workdir_base.clone()).unwrap();

            let run_result = hakana_orchestrator::scan_and_analyze(
                stub_dirs.clone(),
                None,
                Some(FxHashSet::from_iter([
                    "tests/stubs/stubs.hack".to_string(),
                    format!("{}/third-party/xhp-lib/src", cwd),
                ])),
                config.clone(),
                None,
                1,
                logger.clone(),
                build_checksum,
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

            fs::remove_dir_all(&workdir_base).unwrap();

            match run_result {
                Ok(run_result) => {
                    previous_scan_data = Some(run_result.1);
                    previous_analysis_result = Some(run_result.0);
                }
                Err(error) => {
                    *had_error = true;
                    test_diagnostics.push((dir, error.to_string()));
                    return ("F".to_string(), None, None);
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
        let definition_locations_json = generate_definition_locations_json(&analysis_result, &run_data.interner);
        let definition_locations_path = dir.clone() + "/definition_locations.json";
        
        // Check if expected definition_locations.json exists for validation
        if Path::new(&definition_locations_path).exists() {
            let expected_definition_locations = fs::read_to_string(&definition_locations_path)
                .unwrap()
                .trim()
                .to_string();
            
            if expected_definition_locations.trim() != definition_locations_json.trim() {
                test_diagnostics.push((dir.clone(), format_diff(&expected_definition_locations, &definition_locations_json)));
                return ("F".to_string(), Some(run_data), Some(analysis_result));
            }
        } else {
            // Write definition_locations.json file if it doesn't exist
            if let Err(e) = fs::write(&definition_locations_path, definition_locations_json) {
                eprintln!("Warning: Failed to write definition_locations.json: {}", e);
            }
        }

        let expected_output_path = dir.clone() + "/output.txt";
        let expected_output = if Path::new(&expected_output_path).exists() {
            let expected = fs::read_to_string(expected_output_path)
                .unwrap()
                .trim()
                .to_string();
            Some(expected)
        } else {
            None
        };

        if if let Some(expected_output) = &expected_output {
            if expected_output.trim() == test_output.join("").trim() {
                true
            } else {
                !expected_output.is_empty()
                    && test_output.len() == 1
                    && expected_output
                        .as_bytes()
                        .iter()
                        .filter(|&&c| c == b'\n')
                        .count()
                        == 0
                    && test_output.iter().any(|s| s.contains(expected_output))
            }
        } else {
            test_output.is_empty()
        } {
            (".".to_string(), Some(run_data), Some(analysis_result))
        } else {
            if let Some(expected_output) = &expected_output {
                test_diagnostics.push((dir, format_diff(expected_output, &test_output.join(""))));
            } else {
                test_diagnostics.push((dir, format_diff("", &test_output.join(""))));
            }
            ("F".to_string(), Some(run_data), Some(analysis_result))
        }
    }

    fn run_goto_definition_test(
        &self,
        dir: String,
        config: Arc<config::Config>,
        logger: Arc<Logger>,
        _cache_dir: Option<&String>,
        had_error: &mut bool,
        test_diagnostics: &mut Vec<(String, String)>,
        build_checksum: &str,
        previous_scan_data: Option<SuccessfulScanData>,
        previous_analysis_result: Option<AnalysisResult>,
        total_time_in_analysis: &mut Duration,
    ) -> (String, Option<SuccessfulScanData>, Option<AnalysisResult>) {
        use hakana_orchestrator::scan_and_analyze;

        let result = scan_and_analyze(
            Vec::new(),
            Some(dir.clone()),
            None,
            config.clone(),
            None,
            1,
            logger,
            build_checksum,
            hakana_str::Interner::default(),
            previous_scan_data,
            previous_analysis_result,
            None,
            || {},
        );

        let result = match result {
            Ok(result) => result,
            Err(_) => {
                *had_error = true;
                return ("F".to_string(), None, None);
            }
        };

        *total_time_in_analysis += result.0.time_in_analysis;

        // Generate definition_locations.json
        let definition_locations_json = generate_definition_locations_json(&result.0, &result.1.interner);
        let definition_locations_path = dir.clone() + "/definition_locations.json";
        
        // Check if expected definition_locations.json exists for validation
        if Path::new(&definition_locations_path).exists() {
            let expected_definition_locations = fs::read_to_string(&definition_locations_path)
                .unwrap()
                .trim()
                .to_string();
            
            if expected_definition_locations.trim() != definition_locations_json.trim() {
                test_diagnostics.push((dir.clone(), format_diff(&expected_definition_locations, &definition_locations_json)));
                return ("F".to_string(), Some(result.1), Some(result.0));
            }
        } else {
            // Write definition_locations.json file if it doesn't exist
            if let Err(e) = fs::write(&definition_locations_path, definition_locations_json) {
                eprintln!("Warning: Failed to write definition_locations.json: {}", e);
            }
        }

        (".".to_string(), Some(result.1), Some(result.0))
    }

}

fn augment_with_local_config(dir: &String, analysis_config: &mut config::Config) {
    let config_path_str = format!("{}/config.json", dir);
    let config_path = Path::new(&config_path_str);

    if config_path.exists() {
        let Ok(test_config) = super::config::read_from_file(config_path) else {
            panic!("invalid test config file {}", config_path_str);
        };

        if let Some(max_changes_allowed) = test_config.max_changes_allowed {
            analysis_config.max_changes_allowed = max_changes_allowed;
        }
    }
}

fn copy_recursively(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            copy_recursively(entry.path(), destination.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), destination.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn generate_definition_locations_json(analysis_result: &AnalysisResult, interner: &Interner) -> String {
    use serde_json::json;

    let mut all_locations = Vec::new();

    for (file_path, locations) in &analysis_result.definition_locations {
        let original_file_path_str = interner.lookup(&file_path.0);
        
        // Extract just the filename for cleaner output
        let file_path_str = if let Some(workdir_pos) = original_file_path_str.find("/workdir/") {
            let file_name = &original_file_path_str[workdir_pos + 9..]; // Skip "/workdir/"
            file_name.to_string()
        } else {
            // Extract filename from any path
            original_file_path_str.split('/').last().unwrap_or(original_file_path_str).to_string()
        };
        
        for ((start_offset, end_offset), (symbol_id, member_id)) in locations {
            let symbol_name = interner.lookup(symbol_id);
            let member_name = if *member_id == StrId::EMPTY {
                ""
            } else {
                interner.lookup(member_id)
            };
            
            let name = if member_name.is_empty() {
                symbol_name.to_string()
            } else {
                format!("{}::{}", symbol_name, member_name)
            };
            
            all_locations.push(json!({
                "name": name,
                "file": file_path_str,
                "start_offset": start_offset,
                "end_offset": end_offset
            }));
        }
    }

    // Sort by start_offset, then by end_offset
    all_locations.sort_by(|a, b| {
        let start_a = a["start_offset"].as_u64().unwrap();
        let start_b = b["start_offset"].as_u64().unwrap();
        let end_a = a["end_offset"].as_u64().unwrap();
        let end_b = b["end_offset"].as_u64().unwrap();
        
        start_a.cmp(&start_b).then(end_a.cmp(&end_b))
    });

    serde_json::to_string_pretty(&all_locations).unwrap_or_else(|_| "[]".to_string())
}

fn get_all_test_folders(test_or_test_dir: String) -> Result<Vec<String>, String> {
    // Check if the specified test directory exists
    if !Path::new(&test_or_test_dir).exists() {
        return Err(format!(
            "Test directory does not exist: {}",
            test_or_test_dir
        ));
    }

    let mut test_folders = vec![];
    let input_hack_path = test_or_test_dir.clone() + "/input.hack";
    let output_txt_path = test_or_test_dir.clone() + "/output.txt";

    if Path::new(&input_hack_path).exists() {
        // This looks like a single test directory
        if !Path::new(&output_txt_path).exists() && !test_or_test_dir.contains("/goto-definition/")
        {
            return Err(format!(
                "Test directory is missing required output.txt file: {}",
                test_or_test_dir
            ));
        }
        test_folders.push(test_or_test_dir);
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

                    if Path::new(&input_hack).exists() && !path_str.contains("/diff/") {
                        // Found a regular test directory - check if output.txt exists
                        if !Path::new(&output_txt).exists()
                            && !path_str.contains("/goto-definition/")
                        {
                            return Err(format!(
                                "Test directory is missing required output.txt file: {}",
                                path_str
                            ));
                        }
                        test_folders.push(path_str.to_owned());
                    } else if path_str.contains("/diff/")
                        && Path::new(&(path_str.to_owned() + "/a")).is_dir()
                    {
                        // Found a diff test directory - check if output.txt exists
                        if !Path::new(&output_txt).exists() {
                            return Err(format!(
                                "Diff test directory is missing required output.txt file: {}",
                                path_str
                            ));
                        }
                        test_folders.push(path_str.to_owned());
                    } else if path_str.contains("/migration-candidates/")
                        && Path::new(&input_hack).exists()
                    {
                        // Migration candidates tests use candidates.txt instead of output.txt
                        if !Path::new(&candidates_txt).exists() {
                            return Err(format!("Migration candidates test directory is missing required candidates.txt file: {}", path_str));
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
