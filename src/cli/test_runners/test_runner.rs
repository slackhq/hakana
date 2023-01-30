use hakana_analyzer::config;
use hakana_analyzer::config::Verbosity;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::graph::WholeProgramKind;
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::Interner;
use hakana_workhorse::wasm::get_single_file_codebase;
use rustc_hash::FxHashSet;
use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

pub trait TestRunner {
    fn run_test(
        &self,
        test_or_test_dir: String,
        verbosity: Verbosity,
        use_cache: bool,
        had_error: &mut bool,
        build_checksum: &str,
        repeat: u16,
    ) {
        let test_folders = get_all_test_folders(test_or_test_dir);

        let mut test_diagnostics = vec![];

        let starter_codebase = if test_folders.len() > 1 {
            Some(get_single_file_codebase(vec!["tests/stubs/stubs.hack"]))
        } else {
            None
        };

        for _ in 0..(repeat + 1) {
            for test_folder in test_folders.clone() {
                let cache_dir = format!("{}/.hakana_cache", test_folder);

                if !Path::new(&cache_dir).is_dir() && fs::create_dir(&cache_dir).is_err() {
                    panic!("could not create aast cache directory");
                }

                let needs_fresh_codebase =
                    test_folder.contains("xhp") || test_folder.contains("/diff/");

                let test_result = self.run_test_in_dir(
                    test_folder,
                    verbosity,
                    if use_cache { Some(&cache_dir) } else { None },
                    had_error,
                    &mut test_diagnostics,
                    build_checksum,
                    if let Some(starter_codebase) = &starter_codebase {
                        if needs_fresh_codebase {
                            None
                        } else {
                            Some(starter_codebase.clone())
                        }
                    } else {
                        None
                    },
                );

                print!("{}", test_result);
                io::stdout().flush().unwrap();
            }
        }

        println!(
            "\n{}\n",
            test_diagnostics
                .into_iter()
                .map(|(folder, diag)| format!("Unexpected output for {}:\n\n{}", folder, diag))
                .collect::<Vec<_>>()
                .join("\n\n")
        );
    }

    fn get_config_for_test(&self, dir: &String) -> config::Config {
        let mut analysis_config = config::Config::new(dir.clone(), FxHashSet::default());
        analysis_config.find_unused_expressions =
            dir.contains("/unused/") || dir.contains("/fix/UnusedAssignment/");
        analysis_config.find_unused_definitions =
            dir.contains("/unused/UnusedCode/") || dir.contains("/migrations/unused_symbol/");
        analysis_config.graph_kind = if dir.contains("/security/") {
            GraphKind::WholeProgram(WholeProgramKind::Taint)
        } else if dir.contains("/find-paths/") {
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
                .insert(IssueKind::from_str_custom(&issue_name, &FxHashSet::default()).unwrap());
        } else if dir.contains("/add-fixmes/") {
            let issue_name = dir_parts.get(1).unwrap().to_string();

            analysis_config
                .issues_to_fix
                .insert(IssueKind::from_str_custom(&issue_name, &FxHashSet::default()).unwrap());

            analysis_config.add_fixmes = true;
        } else if dir.contains("/remove-unused-fixmes/") {
            analysis_config.remove_fixmes = true;
        }
        analysis_config
    }

    fn get_hooks_for_test(&self, _dir: &String) -> Vec<Box<dyn CustomHook>> {
        vec![]
    }

    fn run_test_in_dir(
        &self,
        dir: String,
        verbosity: Verbosity,
        cache_dir: Option<&String>,
        had_error: &mut bool,
        test_diagnostics: &mut Vec<(String, String)>,
        build_checksum: &str,
        starter_data: Option<(CodebaseInfo, Interner)>,
    ) -> String {
        if dir.contains("skipped-") || dir.contains("SKIPPED-") {
            return "S".to_string();
        }

        if dir.contains("/diff/") {
            return self.run_diff_test(
                dir,
                verbosity,
                cache_dir,
                had_error,
                test_diagnostics,
                build_checksum,
                starter_data,
            );
        }

        let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();

        let analysis_config = self.get_config_for_test(&dir);

        if matches!(verbosity, Verbosity::Debugging) {
            println!("running test {}", dir);
        }

        let mut stub_dirs = vec![cwd.clone() + "/test/stubs"];

        if dir.contains("xhp") || dir.contains("XHP") {
            stub_dirs.push(cwd.clone() + "/third-party/xhp-lib/src");
        }

        let config = Arc::new(analysis_config);

        let result = hakana_workhorse::scan_and_analyze(
            starter_data.is_none(),
            stub_dirs,
            None,
            Some(FxHashSet::from_iter([
                "tests/stubs/stubs.hack".to_string(),
                format!("{}/third-party/xhp-lib/src", cwd),
            ])),
            config.clone(),
            if starter_data.is_none() {
                cache_dir
            } else {
                None
            },
            1,
            verbosity,
            build_checksum,
            starter_data,
        );

        if dir.contains("/migrations/")
            || dir.contains("/fix/")
            || dir.contains("/add-fixmes/")
            || dir.contains("/remove-unused-fixmes/")
        {
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
                test_diagnostics.push((
                    dir,
                    format!("- {}\n+ {}", expected_output_contents, output_contents),
                ));
                "F".to_string()
            };
        } else {
            let test_output = match result {
                Ok(analysis_result) => {
                    let mut output = vec![];
                    for (file_path, issues) in &analysis_result.emitted_issues {
                        for issue in issues {
                            output.push(issue.format(&file_path));
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

    fn run_diff_test(
        &self,
        dir: String,
        verbosity: Verbosity,
        cache_dir: Option<&String>,
        had_error: &mut bool,
        test_diagnostics: &mut Vec<(String, String)>,
        build_checksum: &str,
        starter_data: Option<(CodebaseInfo, Interner)>,
    ) -> String {
        let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();

        if matches!(verbosity, Verbosity::Debugging) {
            println!("running test {}", dir);
        }

        if let Some(cache_dir) = cache_dir {
            fs::remove_dir_all(&cache_dir).unwrap();
            fs::create_dir(&cache_dir).unwrap();
        }

        let workdir_base = dir.clone() + "/.workdir";

        copy_recursively(dir.clone() + "/a", workdir_base.clone()).unwrap();

        let mut config = self.get_config_for_test(&workdir_base);
        config.ast_diff = true;
        config.find_unused_definitions = true;
        let config = Arc::new(config);

        let stub_dirs = vec![cwd.clone() + "/test/stubs"];

        hakana_workhorse::scan_and_analyze(
            starter_data.is_none(),
            stub_dirs.clone(),
            None,
            Some(FxHashSet::from_iter([
                "tests/stubs/stubs.hack".to_string(),
                format!("{}/third-party/xhp-lib/src", cwd),
            ])),
            config.clone(),
            if starter_data.is_none() {
                cache_dir
            } else {
                None
            },
            1,
            verbosity,
            build_checksum,
            starter_data.clone(),
        )
        .unwrap();

        copy_recursively(dir.clone() + "/b", workdir_base.clone()).unwrap();

        let b_result = hakana_workhorse::scan_and_analyze(
            starter_data.is_none(),
            stub_dirs,
            None,
            Some(FxHashSet::from_iter([
                "tests/stubs/stubs.hack".to_string(),
                format!("{}/third-party/xhp-lib/src", cwd),
            ])),
            config.clone(),
            if starter_data.is_none() {
                cache_dir
            } else {
                None
            },
            1,
            verbosity,
            build_checksum,
            starter_data,
        );

        fs::remove_dir_all(&workdir_base).unwrap();

        let test_output = match b_result {
            Ok(analysis_result) => {
                let mut output = vec![];
                for (file_path, issues) in &analysis_result.emitted_issues {
                    for issue in issues {
                        output.push(issue.format(&file_path));
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

fn get_all_test_folders(test_or_test_dir: String) -> Vec<String> {
    let mut test_folders = vec![];
    if Path::new(&(test_or_test_dir.clone() + "/input.hack")).exists()
        || Path::new(&(test_or_test_dir.clone() + "/output.txt")).exists()
    {
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
                    if (Path::new(&(path.to_owned() + "/input.hack")).exists()
                        && !path.contains("/diff/"))
                        || Path::new(&(path.to_owned() + "/output.txt")).exists()
                    {
                        test_folders.push(path.to_owned().to_string());
                    }
                }
            }
        }
    }
    test_folders
}
