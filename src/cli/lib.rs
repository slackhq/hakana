use clap::{arg, Command};
use hakana_analyzer::config::{self};
use hakana_analyzer::custom_hook::CustomHook;
use hakana_logger::{Logger, Verbosity};
use hakana_reflection_info::analysis_result::{AnalysisResult, CheckPointEntry, Replacement};
use hakana_reflection_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::Interner;
use indexmap::IndexMap;
use rand::Rng;
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
    test_runner: &TestRunner,
) {
    println!("{}\n", header);

    let mut all_custom_issues = vec![];

    for analysis_hook in &analysis_hooks {
        all_custom_issues.extend(analysis_hook.get_custom_issue_names());
    }

    for migration_hook in &migration_hooks {
        all_custom_issues.extend(migration_hook.get_custom_issue_names());
    }

    let all_custom_issues = all_custom_issues
        .into_iter()
        .map(|i| i.to_string())
        .collect::<FxHashSet<_>>();

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
                    arg!(--"show-timing")
                        .required(false)
                        .help("If set, timing info will be displayed"),
                )
                .arg(
                    arg!(--"no-cache")
                        .required(false)
                        .help("Whether to ignore the cache"),
                )
                .arg(
                    arg!(--"diff")
                        .required(false)
                        .help("Whether perform AST-based diffing to speed up execution"),
                )
                .arg(
                    arg!(--"show-issue-stats")
                        .required(false)
                        .help("Output a summary of issue counts"),
                )
                .arg(
                    arg!(--"output" <PATH>)
                        .required(false)
                        .help("File to save output to"),
                ),
        )
        .subcommand(
            Command::new("migration-candidates")
                .about("Generates a list of all migration candidates")
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
                    arg!(--"filter" <PATH>)
                        .required(false)
                        .help("Filter the files that are analyzed"),
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
            Command::new("add-fixmes")
                .about("Adds fixmes to suppress Hakana issues")
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
                        .multiple(true)
                        .help("The issue or issues to add fixmes for"),
                )
                .arg(
                    arg!(--"filter" <PATH>)
                        .required(false)
                        .help("Filter the files that have added fixmes"),
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
            Command::new("remove-unused-fixmes")
                .about("Removes all fixmes that are never used")
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
                    arg!(--"filter" <PATH>)
                        .required(false)
                        .help("Filter the files that have added fixmes"),
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
                .arg(
                    arg!(--"no-cache")
                        .required(false)
                        .help("Whether to use cache"),
                )
                .arg(
                    arg!(--"reuse-codebase")
                        .required(false)
                        .help("Whether to reuse codebase between tests"),
                )
                .arg(
                    arg!(--"randomize")
                        .required(false)
                        .help("Whether to randomise test order"),
                )
                .arg(
                    arg!(--"seed" <COUNT>)
                        .required(false)
                        .help("Seed for random test execution"),
                )
                .arg(
                    arg!(--"debug")
                        .required(false)
                        .help("Whether to show debug output"),
                )
                .arg(
                    arg!(--"repeat" <COUNT>)
                        .required(false)
                        .help("How many times to repeat the test (useful for profiling)"),
                )
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

    let logger = match matches.subcommand() {
        Some(("test", sub_matches)) => {
            if sub_matches.is_present("debug") {
                Logger::CommandLine(Verbosity::Debugging)
            } else {
                Logger::DevNull
            }
        }
        Some((_, sub_matches)) => Logger::CommandLine(if sub_matches.is_present("debug") {
            Verbosity::Debugging
        } else if sub_matches.is_present("show-timing") {
            Verbosity::Timing
        } else {
            Verbosity::Simple
        }),
        _ => Logger::CommandLine(Verbosity::Simple),
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

    let config_path = config_path.as_ref().map(Path::new);

    let cache_dir = format!("{}/.hakana_cache", root_dir);

    if !Path::new(&cache_dir).is_dir() && fs::create_dir(&cache_dir).is_err() {
        panic!("could not create aast cache directory");
    }

    let mut had_error = false;

    match matches.subcommand() {
        Some(("analyze", sub_matches)) => {
            do_analysis(
                sub_matches,
                all_custom_issues,
                &root_dir,
                analysis_hooks,
                config_path,
                &cwd,
                cache_dir,
                threads,
                logger,
                header,
                &mut had_error,
            );
        }
        Some(("security-check", sub_matches)) => {
            do_security_check(
                &cwd,
                all_custom_issues,
                config_path,
                sub_matches,
                analysis_hooks,
                threads,
                logger,
                header,
                &mut had_error,
            );
        }
        Some(("find-paths", sub_matches)) => {
            do_find_paths(
                &cwd,
                all_custom_issues,
                config_path,
                sub_matches,
                analysis_hooks,
                threads,
                logger,
                header,
                &mut had_error,
            );
        }
        Some(("migrate", sub_matches)) => {
            do_migrate(
                sub_matches,
                &root_dir,
                all_custom_issues,
                migration_hooks,
                config_path,
                &cwd,
                threads,
                logger,
                header,
            );
        }
        Some(("migration-candidates", sub_matches)) => {
            do_migration_candidates(
                sub_matches,
                &root_dir,
                all_custom_issues,
                migration_hooks,
                config_path,
                &cwd,
                threads,
                logger,
                header,
            );
        }
        Some(("add-fixmes", sub_matches)) => {
            do_add_fixmes(
                sub_matches,
                all_custom_issues,
                &root_dir,
                analysis_hooks,
                config_path,
                &cwd,
                threads,
                logger,
                header,
            );
        }
        Some(("remove-unused-fixmes", sub_matches)) => {
            do_remove_unused_fixmes(
                sub_matches,
                &root_dir,
                all_custom_issues,
                analysis_hooks,
                config_path,
                &cwd,
                threads,
                logger,
                header,
            );
        }
        Some(("fix", sub_matches)) => {
            do_fix(
                sub_matches,
                all_custom_issues,
                analysis_hooks,
                root_dir,
                config_path,
                cwd,
                threads,
                logger,
                header,
            );
        }
        Some(("test", sub_matches)) => {
            let repeat = if let Some(val) = sub_matches.value_of("repeat").map(|f| f.to_string()) {
                val.parse::<u16>().unwrap()
            } else {
                0
            };

            let random_seed = if sub_matches.is_present("randomize") {
                if let Some(val) = sub_matches.value_of("seed").map(|f| f.to_string()) {
                    Some(val.parse::<u64>().unwrap())
                } else {
                    let mut rng = rand::thread_rng();
                    Some(rng.gen())
                }
            } else {
                None
            };

            test_runner.run_test(
                sub_matches.value_of("TEST").expect("required").to_string(),
                Arc::new(logger),
                !sub_matches.is_present("no-cache"),
                sub_matches.is_present("reuse-codebase"),
                &mut had_error,
                header,
                repeat,
                random_seed,
            );
        }
        _ => unreachable!(), // If all subcommands are defined above, anything else is unreachabe!()
    }

    if had_error {
        exit(1);
    }
}

fn do_fix(
    sub_matches: &clap::ArgMatches,
    all_custom_issues: FxHashSet<String>,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    root_dir: String,
    config_path: Option<&Path>,
    cwd: String,
    threads: u8,
    logger: Logger,
    header: &str,
) {
    let issue_name = sub_matches.value_of("issue").unwrap().to_string();
    let issue_kind = IssueKind::from_str_custom(&issue_name, &all_custom_issues).unwrap();

    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);
    config.hooks = analysis_hooks;

    config.find_unused_expressions = issue_kind.is_unused_expression();
    config.find_unused_definitions = issue_kind.is_unused_definition();
    config.issues_to_fix.insert(issue_kind);

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(&cwd, config_path).ok();
    }

    config.allowed_issues = None;

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok((mut analysis_result, successfull_run_data)) = result {
        update_files(
            &mut analysis_result,
            &root_dir,
            &successfull_run_data.interner,
        );
    }
}

fn do_remove_unused_fixmes(
    sub_matches: &clap::ArgMatches,
    root_dir: &String,
    all_custom_issues: FxHashSet<String>,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    logger: Logger,
    header: &str,
) {
    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);

    config.hooks = analysis_hooks;

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(cwd, config_path).ok();
    }
    config.allowed_issues = None;

    config.find_unused_expressions = true;
    config.find_unused_definitions = true;

    config.remove_fixmes = true;

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok((mut analysis_result, successful_run_data)) = result {
        update_files(
            &mut analysis_result,
            root_dir,
            &successful_run_data.interner,
        );
    }
}

fn do_add_fixmes(
    sub_matches: &clap::ArgMatches,
    all_custom_issues: FxHashSet<String>,
    root_dir: &String,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    logger: Logger,
    header: &str,
) {
    let filter_issue_strings = sub_matches
        .values_of("issue")
        .map(|values| values.collect::<FxHashSet<_>>());

    let mut issue_kinds_filter = FxHashSet::default();

    if let Some(filter_issue_strings) = filter_issue_strings {
        for filter_issue_string in filter_issue_strings {
            if let Ok(issue_kind) =
                IssueKind::from_str_custom(filter_issue_string, &all_custom_issues)
            {
                issue_kinds_filter.insert(issue_kind);
            } else {
                println!("Invalid issue type {}", filter_issue_string);
                exit(1);
            }
        }
    }

    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);

    config.issues_to_fix.extend(issue_kinds_filter);
    config.hooks = analysis_hooks;
    config.find_unused_expressions = true;
    config.find_unused_definitions = true;

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(cwd, config_path).ok();
    }
    config.allowed_issues = None;

    config.add_fixmes = true;

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok((mut analysis_result, successful_run_data)) = result {
        update_files(
            &mut analysis_result,
            root_dir,
            &successful_run_data.interner,
        );
    }
}

fn do_migrate(
    sub_matches: &clap::ArgMatches,
    root_dir: &String,
    all_custom_issues: FxHashSet<String>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    logger: Logger,
    header: &str,
) {
    let migration_name = sub_matches.value_of("migration").unwrap().to_string();
    let migration_source = sub_matches.value_of("symbols").unwrap().to_string();

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);
    config.hooks = migration_hooks
        .into_iter()
        .filter(|m| {
            if let Some(name) = m.get_migration_name() {
                migration_name == name
            } else {
                false
            }
        })
        .collect();

    if config.hooks.is_empty() {
        println!("Migration {} not recognised", migration_name);
        exit(1);
    }

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(cwd, config_path).ok();
    }
    config.allowed_issues = None;

    let file_path = format!("{}/{}", cwd, migration_source);

    let buf = fs::read_to_string(file_path.clone());

    if let Ok(contents) = buf {
        config.migration_symbols = contents
            .lines()
            .map(|v| {
                let mut parts = v.split(',').collect::<Vec<_>>();
                let first_part = parts.remove(0);
                (first_part.to_string(), parts.join(","))
            })
            .collect();
    } else {
        println!(
            "\nERROR: File {} does not exist or could not be read\n",
            file_path
        );
        exit(1);
    }

    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok((mut analysis_result, successful_run_data)) = result {
        update_files(
            &mut analysis_result,
            root_dir,
            &successful_run_data.interner,
        );
    }
}

fn do_migration_candidates(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    all_custom_issues: FxHashSet<String>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    logger: Logger,
    header: &str,
) {
    let migration_name = sub_matches.value_of("migration").unwrap().to_string();

    let mut config = config::Config::new(root_dir.to_string(), all_custom_issues);
    config.hooks = migration_hooks
        .into_iter()
        .filter(|m| {
            if let Some(name) = m.get_migration_name() {
                migration_name == name
            } else {
                false
            }
        })
        .collect();

    if config.hooks.is_empty() {
        println!("Migration {} not recognised", migration_name);
        exit(1);
    }

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(cwd, config_path).ok();
    }
    config.allowed_issues = None;

    let config = Arc::new(config);

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        None,
        None,
        config.clone(),
        None,
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok(result) = result {
        println!("\nSymbols to migrate:\n");
        for config_hook in &config.hooks {
            let migration_candidates =
                config_hook.get_candidates(&result.1.codebase, &result.1.interner, &result.0);

            for migration_candidate in migration_candidates {
                println!("{}", migration_candidate);
            }
        }
    }
}

fn do_find_paths(
    cwd: &String,
    all_custom_issues: FxHashSet<String>,
    config_path: Option<&Path>,
    sub_matches: &clap::ArgMatches,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    threads: u8,
    logger: Logger,
    header: &str,
    had_error: &mut bool,
) {
    let mut config = config::Config::new(cwd.clone(), all_custom_issues);
    config.graph_kind = GraphKind::WholeProgram(WholeProgramKind::Query);

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(cwd, config_path).ok();
    }
    config.allowed_issues = None;

    config.security_config.max_depth =
        if let Some(val) = sub_matches.value_of("max-depth").map(|f| f.to_string()) {
            val.parse::<u8>().unwrap()
        } else {
            20
        };

    config.hooks = analysis_hooks;

    let root_dir = config.root_dir.clone();

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        None,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok((analysis_result, successful_run_data)) = result {
        for (file_path, issues) in
            analysis_result.get_all_issues(&successful_run_data.interner, &root_dir, true)
        {
            for issue in issues {
                *had_error = true;
                println!("{}", issue.format(&file_path));
            }
        }

        if !*had_error {
            println!("\nNo security issues found!\n");
        }
    }
}

fn do_security_check(
    cwd: &String,
    all_custom_issues: FxHashSet<String>,
    config_path: Option<&Path>,
    sub_matches: &clap::ArgMatches,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    threads: u8,
    logger: Logger,
    header: &str,
    had_error: &mut bool,
) {
    let mut config = config::Config::new(cwd.clone(), all_custom_issues);
    config.graph_kind = GraphKind::WholeProgram(WholeProgramKind::Taint);

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(cwd, config_path).ok();
    }
    config.allowed_issues = None;

    let output_file = sub_matches.value_of("output").map(|f| f.to_string());

    config.security_config.max_depth =
        if let Some(val) = sub_matches.value_of("max-depth").map(|f| f.to_string()) {
            val.parse::<u8>().unwrap()
        } else {
            20
        };

    config.hooks = analysis_hooks;

    let root_dir = config.root_dir.clone();

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        None,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok((analysis_result, successful_run_data)) = result {
        for (file_path, issues) in
            analysis_result.get_all_issues(&successful_run_data.interner, &root_dir, true)
        {
            for issue in issues {
                *had_error = true;
                println!("{}", issue.format(&file_path));
            }
        }

        if !*had_error {
            println!("\nNo security issues found!\n");
        }

        if let Some(output_file) = output_file {
            write_output_files(
                output_file,
                cwd,
                &analysis_result,
                &successful_run_data.interner,
            );
        }
    }
}

fn do_analysis(
    sub_matches: &clap::ArgMatches,
    all_custom_issues: FxHashSet<String>,
    root_dir: &str,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    cache_dir: String,
    threads: u8,
    logger: Logger,
    header: &str,
    had_error: &mut bool,
) {
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
    let show_issue_stats = sub_matches.is_present("show-issue-stats");
    let do_ast_diff = sub_matches.is_present("diff");

    let mut issue_kinds_filter = FxHashSet::default();

    let filter_issue_strings = sub_matches
        .values_of("show-issue")
        .map(|values| values.collect::<FxHashSet<_>>());

    if let Some(filter_issue_strings) = filter_issue_strings {
        for filter_issue_string in filter_issue_strings {
            if let Ok(issue_kind) =
                IssueKind::from_str_custom(filter_issue_string, &all_custom_issues)
            {
                issue_kinds_filter.insert(issue_kind);
            } else {
                println!("Invalid issue type {}", filter_issue_string);
                exit(1);
            }
        }
    }

    let mut config = config::Config::new(root_dir.to_string(), all_custom_issues);
    config.find_unused_expressions = find_unused_expressions;
    config.find_unused_definitions = find_unused_definitions;
    config.ignore_mixed_issues = ignore_mixed_issues;
    config.ast_diff = do_ast_diff;

    config.hooks = analysis_hooks;

    let config_path = config_path.unwrap();

    if config_path.exists() {
        config.update_from_file(cwd, config_path).ok();
    }

    // do this after we've loaded from file, as they can be overridden
    if !issue_kinds_filter.is_empty() {
        config.allowed_issues = Some(issue_kinds_filter);
    }

    let root_dir = config.root_dir.clone();

    let result = hakana_workhorse::scan_and_analyze(
        Vec::new(),
        filter,
        ignored,
        Arc::new(config),
        if sub_matches.is_present("no-cache") {
            None
        } else {
            Some(&cache_dir)
        },
        threads,
        Arc::new(logger),
        header,
        None,
        None,
        None,
    );

    if let Ok((analysis_result, successful_run_data)) = result {
        for (file_path, issues) in
            analysis_result.get_all_issues(&successful_run_data.interner, &root_dir, true)
        {
            for issue in issues {
                *had_error = true;
                println!("{}", issue.format(&file_path));
            }
        }

        if !*had_error {
            println!("\nNo issues reported!\n");
        }

        if let Some(output_file) = output_file {
            write_output_files(
                output_file,
                cwd,
                &analysis_result,
                &successful_run_data.interner,
            );
        }

        if show_issue_stats {
            let mut issues_by_kind = analysis_result
                .issue_counts
                .into_iter()
                .collect::<IndexMap<_, _>>();
            issues_by_kind.sort_by(|_, a, _, b| b.cmp(a));

            for (issue, count) in issues_by_kind {
                println!("{}\t{}", issue.to_string(), count);
            }
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

fn write_output_files(
    output_file: String,
    cwd: &String,
    analysis_result: &AnalysisResult,
    interner: &Interner,
) {
    if output_file.ends_with("checkpoint_results.json") {
        let output_path = if output_file.starts_with('/') {
            output_file
        } else {
            format!("{}/{}", cwd, output_file)
        };
        let mut output_path = fs::File::create(Path::new(&output_path)).unwrap();
        let mut checkpoint_entries = vec![];

        for (file_path, issues) in analysis_result.get_all_issues(interner, cwd, true) {
            for issue in issues {
                checkpoint_entries.push(CheckPointEntry::from_issue(issue, &file_path));
            }
        }

        let checkpoint_json = serde_json::to_string_pretty(&checkpoint_entries).unwrap();

        write!(output_path, "{}", checkpoint_json).unwrap();
    }
}

fn update_files(analysis_result: &mut AnalysisResult, root_dir: &String, interner: &Interner) {
    let mut replacement_and_insertion_keys = analysis_result
        .replacements
        .keys()
        .copied()
        .collect::<FxHashSet<_>>();
    replacement_and_insertion_keys.extend(analysis_result.insertions.keys().copied());

    for (relative_path, original_path) in replacement_and_insertion_keys
        .into_iter()
        .map(|v| (v.get_relative_path(interner, root_dir), v))
        .collect::<BTreeMap<_, _>>()
    {
        println!("updating {}", relative_path);
        let file_path = format!("{}/{}", root_dir, relative_path);
        let file_contents = fs::read_to_string(&file_path).unwrap();
        let mut file = File::create(&file_path).unwrap();
        let replacements = analysis_result
            .replacements
            .remove(&original_path)
            .unwrap_or_default();

        let insertions = analysis_result
            .insertions
            .remove(&original_path)
            .unwrap_or_default();

        file.write_all(replace_contents(file_contents, replacements, insertions).as_bytes())
            .unwrap_or_else(|_| panic!("Could not write file {}", &file_path));
    }
}

fn replace_contents(
    mut file_contents: String,
    replacements: BTreeMap<(u32, u32), Replacement>,
    insertions: BTreeMap<u32, Vec<String>>,
) -> String {
    let mut replacements = replacements
        .into_iter()
        .map(|(k, v)| (k, vec![v]))
        .collect::<BTreeMap<_, _>>();

    for (offset, insertion) in insertions {
        replacements
            .entry((offset, offset))
            .or_insert_with(Vec::new)
            .extend(insertion.into_iter().rev().map(Replacement::Substitute));
    }

    for ((mut start, mut end), replacements) in replacements.iter().rev() {
        for replacement in replacements {
            match replacement {
                Replacement::Remove => {
                    file_contents = file_contents[..start as usize].to_string()
                        + &*file_contents[end as usize..].to_string();
                }
                Replacement::TrimPrecedingWhitespace(beg_of_line) => {
                    let potential_whitespace =
                        file_contents[(*beg_of_line as usize)..start as usize].to_string();
                    if potential_whitespace.trim() == "" {
                        start = *beg_of_line;

                        if beg_of_line > &0
                            && &file_contents[((*beg_of_line as usize) - 1)..start as usize] == "\n"
                        {
                            start -= 1;
                        }
                    }

                    if &file_contents[end as usize..end as usize + 1] == "," {
                        end += 1;
                    }

                    file_contents = file_contents[..start as usize].to_string()
                        + &*file_contents[end as usize..].to_string();
                }
                Replacement::TrimTrailingWhitespace(end_of_line) => {
                    let potential_whitespace =
                        file_contents[end as usize..(*end_of_line as usize)].to_string();

                    let trimmed = potential_whitespace.trim();

                    file_contents = file_contents[..start as usize].to_string()
                        + trimmed
                        + &*file_contents[*end_of_line as usize..].to_string();
                }
                Replacement::Substitute(string) => {
                    file_contents = file_contents[..start as usize].to_string()
                        + string
                        + &*file_contents[end as usize..].to_string();
                }
            }
        }
    }

    file_contents
}
