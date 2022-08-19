use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::graph::WholeProgramKind;
use hakana_reflection_info::issue::IssueKind;
use rustc_hash::FxHashSet;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

pub trait TestRunner {
    fn run_test(&self, test_or_test_dir: String, had_error: &mut bool, build_checksum: &str) {
        let test_folders = get_all_test_folders(test_or_test_dir);

        println!("Running tests\n");

        let mut test_diagnostics = vec![];

        let mut test_results = vec![];

        let starter_codebase = if test_folders.len() > 1 {
            Some(hakana_workhorse::get_single_file_codebase(vec![
                "tests/stubs/stubs.hack",
            ]))
        } else {
            None
        };

        for test_folder in test_folders {
            let cache_dir = format!("{}/.hakana_cache", test_folder);

            if !Path::new(&cache_dir).is_dir() && fs::create_dir(&cache_dir).is_err() {
                panic!("could not create aast cache directory");
            }

            let is_xhp_test = test_folder.contains("xhp");

            test_results.push(self.run_test_in_dir(
                test_folder,
                Some(&cache_dir),
                had_error,
                &mut test_diagnostics,
                build_checksum,
                if let Some(starter_codebase) = &starter_codebase {
                    if is_xhp_test {
                        None
                    } else {
                        Some(starter_codebase.clone())
                    }
                } else {
                    None
                },
            ));
        }

        println!("{}\n", test_results.join(""));

        println!(
            "{}\n",
            test_diagnostics
                .into_iter()
                .map(|(folder, diag)| format!("Unexpected output for {}:\n\n{}", folder, diag))
                .collect::<Vec<_>>()
                .join("\n\n")
        );
    }

    fn get_config_for_test(&self, dir: &String) -> config::Config {
        let mut analysis_config = config::Config::new(dir.clone());
        analysis_config.find_unused_expressions =
            dir.contains("/unused/") || dir.contains("/fix/UnusedVariable/");
        analysis_config.find_unused_definitions = dir.contains("/unused/UnusedCode/");
        analysis_config.graph_kind = if dir.contains("/security/") {
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        } else if dir.contains("/query/") {
            GraphKind::WholeProgram(WholeProgramKind::Query)
        } else {
            GraphKind::FunctionBody
        };

        analysis_config.hooks = self.get_hooks_for_test(dir);

        let mut dir_parts = dir.split("/").collect::<Vec<_>>();

        while let Some(&"tests" | &"internal" | &"public") = dir_parts.first() {
            dir_parts = dir_parts[1..].to_vec();
        }

        if dir.contains("/migrations/") {
            let migration_name = dir_parts.get(1).unwrap().to_string();
            let replacements_path = dir.clone() + "/replacements.txt";
            let replacements = fs::read_to_string(replacements_path).unwrap().to_string();

            analysis_config.migration_symbols = replacements
                .lines()
                .map(|v| (migration_name.clone(), v.to_string()))
                .collect();
        } else if dir.contains("/fix/") {
            let issue_name = dir_parts.get(1).unwrap().to_string();

            analysis_config
                .issues_to_fix
                .insert(IssueKind::from_str(&issue_name).unwrap());
        }
        analysis_config
    }

    fn get_hooks_for_test(&self, _dir: &String) -> Vec<Box<dyn CustomHook>> {
        vec![]
    }

    fn run_test_in_dir(
        &self,
        dir: String,
        cache_dir: Option<&String>,
        had_error: &mut bool,
        test_diagnostics: &mut Vec<(String, String)>,
        build_checksum: &str,
        starter_codebase: Option<CodebaseInfo>,
    ) -> String {
        let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();

        let analysis_config = self.get_config_for_test(&dir);

        println!("running test {}", dir);

        let mut stub_dirs = vec![cwd.clone() + "/test/stubs"];

        if dir.contains("xhp") || dir.contains("XHP") {
            stub_dirs.push(cwd.clone() + "/third-party/xhp-lib/src");
        }

        let config = Arc::new(analysis_config);

        let result = hakana_workhorse::scan_and_analyze(
            starter_codebase.is_none(),
            stub_dirs,
            None,
            Some(FxHashSet::from_iter([
                "tests/stubs/stubs.hack".to_string(),
                format!("{}/third-party/xhp-lib/src", cwd),
            ])),
            config.clone(),
            if starter_codebase.is_none() {
                cache_dir
            } else {
                None
            },
            1,
            false,
            build_checksum,
            starter_codebase,
        );

        if dir.contains("/migrations/") || dir.contains("/fix/") {
            let input_file = format!("{}/input.hack", dir);
            let output_file = format!("{}/output.txt", dir);
            let input_contents = fs::read_to_string(&input_file).unwrap();
            let expected_output_contents = fs::read_to_string(&output_file).unwrap();

            let result = result.unwrap();

            let output_contents = if let Some(file_replacements) =
                result.replacements.get(&"input.hack".to_string())
            {
                crate::replace_contents(input_contents, file_replacements)
            } else {
                input_contents
            };

            return if output_contents == expected_output_contents {
                ".".to_string()
            } else {
                println!("expected: {}", expected_output_contents);
                println!("actual: {}", output_contents);
                "F".to_string()
            };
        } else {
            let test_output = match result {
                Ok(analysis_result) => {
                    let mut output = vec![];
                    for issues in analysis_result.emitted_issues.values() {
                        for issue in issues {
                            output.push(issue.format());
                        }
                    }

                    output
                }
                Err(error) => {
                    *had_error = true;
                    vec![error.to_string()]
                }
            };

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
                    test_output.len() == 1
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
                return ".".to_string();
            } else {
                if let Some(expected_output) = &expected_output {
                    test_diagnostics.push((
                        dir,
                        format!("- {}\n+ {}", expected_output, test_output.join("+ ")),
                    ));
                } else {
                    test_diagnostics.push((dir, format!("-\n+ {}", test_output.join("+ "))));
                }
                return "F".to_string();
            }
        }
    }
}

fn get_all_test_folders(test_or_test_dir: String) -> Vec<String> {
    let mut test_folders = vec![];
    if Path::new(&(test_or_test_dir.clone() + "/input.hack")).exists() {
        test_folders.push(test_or_test_dir);
    } else {
        for entry in WalkDir::new(test_or_test_dir)
            .sort_by(|a, b| a.file_name().cmp(b.file_name()))
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            let metadata = fs::metadata(&path).unwrap();

            if metadata.is_dir() {
                if let Some(path) = path.to_str() {
                    if Path::new(&(path.to_owned() + "/input.hack")).exists() {
                        test_folders.push(path.to_owned().to_string());
                    }
                }
            }
        }
    }
    test_folders
}
