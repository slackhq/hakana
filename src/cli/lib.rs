use clap::{arg, Command};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_reflection_info::analysis_result::{AnalysisResult, CheckPointEntry};
use hakana_reflection_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_reflection_info::issue::IssueKind;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use test_runners::test_runner::TestRunner;
pub mod test_runners;

pub fn init(
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    header: &str,
    test_runner: Box<dyn TestRunner>,
) {
    println!("{}\n", header);

    let matches = Command::new("hakana")
        .about("Another static analysis tool for Hack")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("analyze")
                .alias("analyse")
                .about("Analyzes code in the current directory")
                .arg(arg!(--"root" <PATH>).required(false).help(
                    "The root directory that Hakana runs in. Defaults to the current directory",
                ))
                .arg(
                    arg!(--"config" <PATH>)
                        .required(false)
                        .help("Hakana config path — defaults to ./hakana.json"),
                )
                .arg(
                    arg!(--"filter" <PATH>)
                        .required(false)
                        .help("Filter the files that are analyzed"),
                )
                .arg(
                    arg!(--"ignore" <PATH>)
                        .required(false)
                        .multiple(true)
                        .help("Ignore certain files during analysis"),
                )
                .arg(
                    arg!(--"threads" <PATH>)
                        .required(false)
                        .help("How many threads to use"),
                )
                .arg(
                    arg!(--"find-unused-expressions")
                        .required(false)
                        .help("Find unused expressions"),
                )
                .arg(
                    arg!(--"find-unused-definitions")
                        .required(false)
                        .help("Find unused definitions — classes, functions, methods etc."),
                )
                .arg(
                    arg!(--"show-issue" <PATH>)
                        .required(false)
                        .multiple(true)
                        .help("Only output issues of this/these type(s)"),
                )
                .arg(
                    arg!(--"ignore-mixed-issues")
                        .required(false)
                        .help("Ignore mixed/any issues"),
                )
                .arg(
                    arg!(--"show-mixed-function-counts")
                        .required(false)
                        .help("Show which functions we lead to mixed types"),
                )
                .arg(
                    arg!(--"show-symbol-map")
                        .required(false)
                        .help("Output a map of all symbols"),
                )
                .arg(
                    arg!(--"debug")
                        .required(false)
                        .help("Add output for debugging"),
                )
                .arg(
                    arg!(--"no-cache")
                        .required(false)
                        .help("Whether to use cache"),
                )
                .arg(
                    arg!(--"output" <PATH>)
                        .required(false)
                        .help("File to save output to"),
                ),
        )
        .subcommand(
            Command::new("migrate")
                .about("Migrates code in the current directory")
                .arg(arg!(--"root" <PATH>).required(false).help(
                    "The root directory that Hakana runs in. Defaults to the current directory",
                ))
                .arg(
                    arg!(--"config" <PATH>)
                        .required(false)
                        .help("Hakana config path — defaults to ./hakana.json"),
                )
                .arg(
                    arg!(--"migration" <PATH>)
                        .required(true)
                        .help("The migration you want to perform"),
                )
                .arg(
                    arg!(--"symbols" <PATH>)
                        .required(true)
                        .help("The path to a list of symbols, separated by newlines"),
                )
                .arg(
                    arg!(--"threads" <PATH>)
                        .required(false)
                        .help("How many threads to use"),
                )
                .arg(
                    arg!(--"debug")
                        .required(false)
                        .help("Add output for debugging"),
                ),
        )
        .subcommand(
            Command::new("fix")
                .about("Fixes issues in the codebase")
                .arg(arg!(--"root" <PATH>).required(false).help(
                    "The root directory that Hakana runs in. Defaults to the current directory",
                ))
                .arg(
                    arg!(--"config" <PATH>)
                        .required(false)
                        .help("Hakana config path — defaults to ./hakana.json"),
                )
                .arg(
                    arg!(--"issue" <PATH>)
                        .required(true)
                        .help("The issue to fix"),
                )
                .arg(
                    arg!(--"filter" <PATH>)
                        .required(false)
                        .help("Filter the files that are fixed"),
                )
                .arg(
                    arg!(--"threads" <PATH>)
                        .required(false)
                        .help("How many threads to use"),
                )
                .arg(
                    arg!(--"debug")
                        .required(false)
                        .help("Add output for debugging"),
                ),
        )
        .subcommand(
            Command::new("security-check")
                .about("Looks for vulnerabilities in the codebase")
                .arg(arg!(--"root" <PATH>).required(false).help(
                    "The root directory that Hakana runs in. Defaults to the current directory",
                ))
                .arg(
                    arg!(--"config" <PATH>)
                        .required(false)
                        .help("Hakana config path — defaults to ./hakana.json"),
                )
                .arg(
                    arg!(--"threads" <PATH>)
                        .required(false)
                        .help("How many threads to use"),
                )
                .arg(
                    arg!(--"max-depth" <PATH>)
                        .required(false)
                        .help("Length of the longest allowable path"),
                )
                .arg(
                    arg!(--"debug")
                        .required(false)
                        .help("Add output for debugging"),
                )
                .arg(
                    arg!(--"output" <PATH>)
                        .required(false)
                        .help("File to save output to"),
                ),
        )
        .subcommand(
            Command::new("find-paths")
                .about("Does whole-program analysis querying")
                .arg(arg!(--"root" <PATH>).required(false).help(
                    "The root directory that Hakana runs in. Defaults to the current directory",
                ))
                .arg(
                    arg!(--"config" <PATH>)
                        .required(false)
                        .help("Hakana config path — defaults to ./hakana.json"),
                )
                .arg(
                    arg!(--"threads" <PATH>)
                        .required(false)
                        .help("How many threads to use"),
                )
                .arg(
                    arg!(--"max-depth" <PATH>)
                        .required(false)
                        .help("Length of the longest allowable path"),
                )
                .arg(
                    arg!(--"debug")
                        .required(false)
                        .help("Add output for debugging"),
                ),
        )
        .subcommand(
            Command::new("test")
                .about("Runs one or more Hakana tests")
                .arg(arg!(<TEST> "The test to run"))
                .arg_required_else_help(true),
        )
        .get_matches();

    let cwd = (env::current_dir()).unwrap().to_str().unwrap().to_string();

    let threads = match matches.subcommand() {
        Some(("test", _)) => 1,
        Some((_, sub_matches)) => {
            if let Some(val) = sub_matches.value_of("threads").map(|f| f.to_string()) {
                val.parse::<u8>().unwrap()
            } else {
                8
            }
        }
        _ => 8,
    };

    let debug = match matches.subcommand() {
        Some(("test", _)) => false,
        Some((_, sub_matches)) => sub_matches.is_present("debug"),
        _ => false,
    };

    let root_dir = match matches.subcommand() {
        Some(("test", _)) => cwd.as_str().to_string(),
        Some((_, sub_matches)) => sub_matches
            .value_of("root")
            .unwrap_or(cwd.as_str())
            .to_string(),
        _ => panic!(),
    };

    let config_path = match matches.subcommand() {
        Some(("test", _)) => None,
        Some((_, sub_matches)) => Some(
            sub_matches
                .value_of("config")
                .unwrap_or(format!("{}/hakana.json", root_dir).as_str())
                .to_string(),
        ),
        _ => panic!(),
    };

    let config_path = if let Some(config_path) = &config_path {
        Some(Path::new(config_path))
    } else {
        None
    };

    let cache_dir = format!("{}/.hakana_cache", root_dir);

    if !Path::new(&cache_dir).is_dir() && fs::create_dir(&cache_dir).is_err() {
        panic!("could not create aast cache directory");
    }

    let mut had_error = false;

    match matches.subcommand() {
        Some(("analyze", sub_matches)) => {
            let filter = sub_matches.value_of("filter").map(|f| f.to_string());

            let output_file = sub_matches.value_of("output").map(|f| f.to_string());

            let ignored = sub_matches
                .values_of("ignore")
                .map(|values| values.map(|f| f.to_string()).collect::<FxHashSet<_>>());
            let find_unused_expressions = sub_matches.is_present("find-unused-expressions");
            let find_unused_definitions = sub_matches.is_present("find-unused-definitions");
            let show_mixed_function_counts = sub_matches.is_present("show-mixed-function-counts");
            let show_symbol_map = sub_matches.is_present("show-symbol-map");
            let ignore_mixed_issues = sub_matches.is_present("ignore-mixed-issues");

            let mut issue_kinds_filter = FxHashSet::default();

            let filter_issue_strings = sub_matches
                .values_of("show-issue")
                .map(|values| values.collect::<FxHashSet<_>>());

            if let Some(filter_issue_strings) = filter_issue_strings {
                for filter_issue_string in filter_issue_strings {
                    if let Ok(issue_kind) = IssueKind::from_str(filter_issue_string) {
                        issue_kinds_filter.insert(issue_kind);
                    } else {
                        println!("Invalid issue type {}", filter_issue_string);
                        exit(1);
                    }
                }
            }

            let mut analysis_config = config::Config::new(root_dir.clone());
            analysis_config.find_unused_expressions = find_unused_expressions;
            analysis_config.find_unused_definitions = find_unused_definitions;
            analysis_config.issue_filter = if !issue_kinds_filter.is_empty() {
                Some(issue_kinds_filter)
            } else {
                None
            };
            analysis_config.ignore_mixed_issues = ignore_mixed_issues;

            analysis_config.hooks = analysis_hooks;

            let config_path = config_path.unwrap();

            if config_path.exists() {
                analysis_config.update_from_file(&cwd, config_path);
            }

            let result = hakana_workhorse::scan_and_analyze(
                true,
                Vec::new(),
                filter,
                ignored,
                Arc::new(analysis_config),
                if sub_matches.is_present("no-cache") {
                    None
                } else {
                    Some(&cache_dir)
                },
                threads,
                debug,
                &header,
                None,
            );

            if let Ok(analysis_result) = result {
                for issues in analysis_result.emitted_issues.values() {
                    for issue in issues {
                        had_error = true;
                        println!("{}", issue.format());
                    }
                }

                if !had_error {
                    println!("\nNo issues reported!\n");
                }

                if let Some(output_file) = output_file {
                    write_output_files(output_file, &cwd, &analysis_result);
                }

                if show_symbol_map {
                    println!("{:#?}", analysis_result.symbol_references);
                }

                if show_mixed_function_counts {
                    let mut mixed_sources = analysis_result
                        .mixed_source_counts
                        .iter()
                        .map(|(k, v)| format!("{}\t{}", k, v.len()))
                        .collect::<Vec<_>>();

                    mixed_sources.sort();

                    println!("{}", mixed_sources.join("\n"));
                }
            }
        }
        Some(("security-check", sub_matches)) => {
            let mut analysis_config = config::Config::new(cwd.clone());
            analysis_config.graph_kind = GraphKind::WholeProgram(WholeProgramKind::Taint);

            let config_path = config_path.unwrap();

            if config_path.exists() {
                analysis_config.update_from_file(&cwd, config_path);
            }

            let output_file = sub_matches.value_of("output").map(|f| f.to_string());

            analysis_config.security_config.max_depth =
                if let Some(val) = sub_matches.value_of("max-depth").map(|f| f.to_string()) {
                    val.parse::<u8>().unwrap()
                } else {
                    20
                };

            analysis_config.hooks = analysis_hooks;

            let result = hakana_workhorse::scan_and_analyze(
                true,
                Vec::new(),
                None,
                None,
                Arc::new(analysis_config),
                None,
                threads,
                debug,
                &header,
                None,
            );
            if let Ok(analysis_result) = result {
                for issues in analysis_result.emitted_issues.values() {
                    for issue in issues {
                        had_error = true;
                        println!("{}", issue.format());
                    }
                }

                if !had_error {
                    println!("\nNo security issues found!\n");
                }

                if let Some(output_file) = output_file {
                    write_output_files(output_file, &cwd, &analysis_result);
                }
            }
        }
        Some(("find-paths", sub_matches)) => {
            let mut analysis_config = config::Config::new(cwd.clone());
            analysis_config.graph_kind = GraphKind::WholeProgram(WholeProgramKind::Query);

            let config_path = config_path.unwrap();

            if config_path.exists() {
                analysis_config.update_from_file(&cwd, config_path);
            }

            analysis_config.security_config.max_depth =
                if let Some(val) = sub_matches.value_of("max-depth").map(|f| f.to_string()) {
                    val.parse::<u8>().unwrap()
                } else {
                    20
                };

            analysis_config.hooks = analysis_hooks;

            let result = hakana_workhorse::scan_and_analyze(
                true,
                Vec::new(),
                None,
                None,
                Arc::new(analysis_config),
                None,
                threads,
                debug,
                &header,
                None,
            );
            if let Ok(analysis_result) = result {
                for issues in analysis_result.emitted_issues.values() {
                    for issue in issues {
                        had_error = true;
                        println!("{}", issue.format());
                    }
                }

                if !had_error {
                    println!("\nNo security issues found!\n");
                }
            }
        }
        Some(("migrate", sub_matches)) => {
            let migration_name = sub_matches.value_of("migration").unwrap().to_string();
            let migration_source = sub_matches.value_of("symbols").unwrap().to_string();

            let mut analysis_config = config::Config::new(root_dir.clone());
            analysis_config.hooks = migration_hooks;

            let config_path = config_path.unwrap();

            if config_path.exists() {
                analysis_config.update_from_file(&cwd, config_path);
            }

            let file_path = format!("{}/{}", cwd, migration_source);

            let buf = fs::read_to_string(file_path.clone());

            if let Ok(contents) = buf {
                analysis_config.migration_symbols = contents
                    .lines()
                    .map(|v| (migration_name.clone(), Arc::new(v.to_string())))
                    .collect();
            } else {
                println!(
                    "\nERROR: File {} does not exist or could not be read\n",
                    file_path
                );
                exit(1);
            }

            let result = hakana_workhorse::scan_and_analyze(
                true,
                Vec::new(),
                None,
                None,
                Arc::new(analysis_config),
                None,
                threads,
                debug,
                &header,
                None,
            );

            if let Ok(analysis_result) = result {
                update_files(analysis_result, &root_dir);
            }
        }
        Some(("fix", sub_matches)) => {
            let issue_name = sub_matches.value_of("issue").unwrap().to_string();
            let issue_kind = IssueKind::from_str(&issue_name).unwrap();

            let filter = sub_matches.value_of("filter").map(|f| f.to_string());

            let mut config = config::Config::new(root_dir.clone());
            config.find_unused_expressions = issue_kind.is_unused_expression();
            config.find_unused_definitions = issue_kind.is_unused_definition();
            config.issues_to_fix.insert(issue_kind);

            let config_path = config_path.unwrap();

            if config_path.exists() {
                config.update_from_file(&cwd, config_path);
            }

            let result = hakana_workhorse::scan_and_analyze(
                true,
                Vec::new(),
                filter,
                None,
                Arc::new(config),
                None,
                threads,
                debug,
                &header,
                None,
            );

            if let Ok(analysis_result) = result {
                update_files(analysis_result, &root_dir);
            }
        }
        Some(("test", sub_matches)) => {
            test_runner.run_test(
                sub_matches.value_of("TEST").expect("required").to_string(),
                &mut had_error,
                header,
            );
        }
        _ => unreachable!(), // If all subcommands are defined above, anything else is unreachabe!()
    }

    if had_error {
        exit(1);
    }
}

fn write_output_files(output_file: String, cwd: &String, analysis_result: &AnalysisResult) {
    if output_file.ends_with("checkpoint_results.json") {
        let output_path = if output_file.starts_with("/") {
            output_file
        } else {
            format!("{}/{}", cwd, output_file)
        };
        let mut output_path = fs::File::create(Path::new(&output_path)).unwrap();
        let mut checkpoint_entries = vec![];

        for issues in analysis_result.emitted_issues.values() {
            for issue in issues {
                checkpoint_entries.push(CheckPointEntry::from_issue(issue));
            }
        }

        let checkpoint_json = serde_json::to_string_pretty(&checkpoint_entries).unwrap();

        write!(output_path, "{}", checkpoint_json).unwrap();
    }
}

fn update_files(analysis_result: AnalysisResult, root_dir: &String) {
    for (filename, replacements) in &analysis_result.replacements {
        println!("updating {}", filename);

        let file_path = format!("{}/{}", root_dir, filename);

        let file_contents = fs::read_to_string(&file_path).unwrap();
        let mut file = File::create(&file_path).unwrap();
        file.write_all(replace_contents(file_contents, replacements).as_bytes())
            .unwrap_or_else(|_| panic!("Could not write file {}", &file_path));
    }
}

fn replace_contents(
    mut file_contents: String,
    replacements: &BTreeMap<(usize, usize), String>,
) -> String {
    for ((start, end), replacement) in replacements.iter().rev() {
        file_contents =
            file_contents[..*start].to_string() + replacement + &*file_contents[*end..].to_string();
    }

    file_contents
}
