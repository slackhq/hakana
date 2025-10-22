use clap::{Command, arg};
use hakana_analyzer::config::{self};
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::analysis_result::{
    AnalysisResult, CheckPointEntry, CheckPointEntryLevel, FullEntry, HhClientEntry, Replacement,
};
use hakana_code_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_code_info::issue::IssueKind;
use hakana_logger::{Logger, Verbosity};
use hakana_str::Interner;
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
    codegen_hooks: Vec<Box<dyn CustomHook>>,
    header: &str,
    test_runner: &TestRunner,
    custom_linters: Vec<Box<dyn hakana_lint::Linter>>,
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

    let matches =
        Command::new("hakana")
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
                    .arg(arg!(--"all-issues").required(false).help("Show all issues"))
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
                    )
                    .arg(arg!(--"json-format" <FORMAT>).required(false).help(
                        "Format for JSON output. Options: checkpoint (default), full, hh_client",
                    )),
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
                        arg!(--"filter" <PATH>)
                            .required(false)
                            .help("Filter the files that are analyzed"),
                    )
                    .arg(
                        arg!(--"debug")
                            .required(false)
                            .help("Add output for debugging"),
                    ),
            )
            .subcommand(
                Command::new("codegen")
                    .about("Generates codegen")
                    .arg(arg!(--"root" <PATH>).required(false).help(
                        "The root directory that Hakana runs in. Defaults to the current directory",
                    ))
                    .arg(
                        arg!(--"config" <PATH>)
                            .required(false)
                            .help("Hakana config path — defaults to ./hakana.json"),
                    )
                    .arg(arg!(--"name" <PATH>).required(false).help(
                        "The codegen you want to perform — if omitted, all codegen is generated",
                    ))
                    .arg(
                        arg!(--"check")
                            .required(false)
                            .help("If passed, will just verify that codegen is accurate"),
                    )
                    .arg(
                        arg!(--"overwrite")
                            .required(false)
                            .help("If passed, will overwrite any conflicting files"),
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
                            .help("Length of the longest allowable path — defaults to 20, and overrides config file value"),
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
                            .help("Length of the longest allowable path — defaults to 20, and overrides config file value"),
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
            .subcommand(
                Command::new("find-executable")
                    .about("Finds all executable lines of code")
                    .arg(arg!(--"root" <PATH>).required(false).help(
                        "The root directory that Hakana runs in. Defaults to the current directory",
                    ))
                    .arg(
                        arg!(--"output" <PATH>)
                            .required(true)
                            .help("File to save output to"),
                    ),
            )
            .subcommand(
                Command::new("lint")
                    .about("Runs linters on Hack code (HHAST-compatible)")
                    .arg(arg!(--"root" <PATH>).required(false).help(
                        "The root directory that Hakana runs in. Defaults to the current directory",
                    ))
                    .arg(
                        arg!(--"config" <PATH>)
                            .required(false)
                            .help("Path to hhast-lint.json config file — defaults to ./hhast-lint.json"),
                    )
                    .arg(
                        arg!(--"threads" <COUNT>)
                            .required(false)
                            .help("How many threads to use"),
                    )
                    .arg(
                        arg!(--"fix")
                            .required(false)
                            .help("Apply auto-fixes where available"),
                    )
                    .arg(
                        arg!(--"linter" <NAME>)
                            .required(false)
                            .multiple(true)
                            .help("Run specific linter(s) by HHAST name or kebab-case name"),
                    )
                    .arg(
                        arg!(--"no-codeowners")
                            .required(false)
                            .help("Skip files that have codeowners (listed in CODEOWNERS file)"),
                    )
                    .arg(
                        arg!(--"debug")
                            .required(false)
                            .help("Add output for debugging"),
                    )
                    .arg(
                        arg!([PATH] "Optional file or directory to lint (defaults to config roots)")
                            .required(false)
                            .multiple(true),
                    ),
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
        Some(("codegen", sub_matches)) => {
            do_codegen(
                sub_matches,
                &root_dir,
                all_custom_issues,
                analysis_hooks,
                codegen_hooks,
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
                    Some(rng.r#gen())
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
        Some(("find-executable", sub_matches)) => {
            do_find_executable(sub_matches, &root_dir, &cwd, threads, logger);
        }
        Some(("lint", sub_matches)) => {
            do_lint(sub_matches, &root_dir, &mut had_error, custom_linters);
        }
        _ => unreachable!(), // If all subcommands are defined above, anything else is unreachable!()
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

    config.find_unused_expressions = issue_kind.requires_dataflow_analysis();
    config.find_unused_definitions = issue_kind.is_unused_definition();
    config.issues_to_fix.insert(issue_kind);

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(&cwd, config_path, &mut interner)
            .ok();
    }

    config.allowed_issues = None;

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
    );

    if let Ok((mut analysis_result, successfull_run_data)) = result {
        update_files(
            &mut analysis_result,
            &root_dir,
            &successfull_run_data.interner,
        );
    }
}

fn do_find_executable(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    cwd: &String,
    threads: u8,
    logger: Logger,
) {
    let output_file = sub_matches.value_of("output").unwrap().to_string();
    let config = config::Config::new(root_dir.to_string(), FxHashSet::default());

    match executable_finder::scan_files(
        &vec![root_dir.to_string()],
        None,
        &Arc::new(config),
        threads,
        Arc::new(logger),
    ) {
        Ok(file_infos) => {
            let output_path = if output_file.starts_with('/') {
                output_file
            } else {
                format!("{}/{}", cwd, output_file)
            };
            let mut out = fs::File::create(Path::new(&output_path)).unwrap();
            match write!(
                out,
                "{}",
                serde_json::to_string_pretty(&file_infos).unwrap()
            ) {
                Ok(_) => {
                    println!("Done")
                }
                Err(err) => {
                    println!("error: {}", err)
                }
            }
        }
        Err(_) => {
            println!("error")
        }
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

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }
    config.allowed_issues = None;

    config.find_unused_expressions = true;
    config.find_unused_definitions = true;

    config.remove_fixmes = true;

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
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

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }
    config.allowed_issues = None;

    config.add_fixmes = true;

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
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

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
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
        config.in_migration = true;
    } else {
        println!(
            "\nERROR: File {} does not exist or could not be read\n",
            file_path
        );
        exit(1);
    }

    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
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

    config.in_migration = true;

    if config.hooks.is_empty() {
        println!("Migration {} not recognised", migration_name);
        exit(1);
    }

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }
    config.allowed_issues = None;

    let config = Arc::new(config);

    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        filter,
        None,
        config.clone(),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
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

fn do_codegen(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    all_custom_issues: FxHashSet<String>,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    mut codegen_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    logger: Logger,
    header: &str,
) {
    let codegen_name = sub_matches.value_of("name");
    let check_codegen = sub_matches.is_present("check");
    let overwrite_codegen = sub_matches.is_present("overwrite");

    let output_file = sub_matches.value_of("output").map(|f| f.to_string());

    let mut config = config::Config::new(root_dir.to_string(), all_custom_issues);
    config.hooks = analysis_hooks;
    config.in_codegen = true;

    if let Some(codegen_name) = codegen_name {
        codegen_hooks.retain(|hook| {
            if let Some(candidate_name) = hook.get_codegen_name() {
                candidate_name == codegen_name
            } else {
                false
            }
        });

        if codegen_hooks.is_empty() {
            println!("Codegen {} not recognised", codegen_name);
            exit(1);
        }
    }

    config.hooks.append(&mut codegen_hooks);

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }
    config.allowed_issues = None;

    let config = Arc::new(config);

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        None,
        None,
        config.clone(),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
    );

    if let Ok(result) = result {
        let mut errors = vec![];
        let mut updated_count = 0;
        let mut verified_count = 0;

        let mut already_written_files = FxHashSet::default();

        for (name, info) in result.0.codegen {
            if already_written_files.contains(&name) {
                errors.push((name.clone(), "File already written".to_string()));
                continue;
            }

            already_written_files.insert(name.clone());

            let path = Path::new(&name);
            if !path.exists() {
                if check_codegen {
                    errors.push((name.clone(), "Doesn’t exist".to_string()));
                    continue;
                }
            } else {
                match &info {
                    Ok(info) => {
                        let existing_contents = fs::read_to_string(path).unwrap();
                        if existing_contents.trim() != info.trim() {
                            if check_codegen || !overwrite_codegen {
                                errors.push((name, "differs from codegen".to_string()));
                                continue;
                            }
                        } else {
                            verified_count += 1;
                            continue;
                        }
                    }
                    Err(err) => {
                        errors.push((name, err.clone()));
                        continue;
                    }
                }
            }

            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).unwrap();
            }

            let mut output_path = fs::File::create(path).unwrap();
            match info {
                Ok(info) => {
                    write!(output_path, "{}", &info).unwrap();
                    println!("Saved {}", name);
                    updated_count += 1;
                }
                Err(err) => {
                    errors.push((name, err));
                    continue;
                }
            }
        }

        if let Some(output_file) = output_file {
            write_codegen_output_files(output_file, cwd, &errors);
        }

        if !errors.is_empty() {
            println!(
                "\nCodegen verification failed.\n\nUse hakana codegen --overwrite to regenerate\n\n{}\n\n",
                errors
                    .into_iter()
                    .map(|(k, v)| format!("Error: {}\n - {}", k, v))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            exit(1);
        }

        if check_codegen {
            println!("\n{} codegen files verified!", verified_count);
        } else {
            println!("\n{} files generated", updated_count);
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

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }
    config.allowed_issues = None;

    config.security_config.max_depth =
        if let Some(val) = sub_matches.value_of("max-depth").map(|f| f.to_string()) {
            val.parse::<u8>().unwrap()
        } else {
            config.security_config.max_depth
        };

    config.hooks = analysis_hooks;

    let root_dir = config.root_dir.clone();

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        None,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
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

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }
    config.allowed_issues = None;

    let output_file = sub_matches.value_of("output").map(|f| f.to_string());

    config.security_config.max_depth =
        if let Some(val) = sub_matches.value_of("max-depth").map(|f| f.to_string()) {
            val.parse::<u8>().unwrap()
        } else {
            config.security_config.max_depth
        };

    config.hooks = analysis_hooks;

    let root_dir = config.root_dir.clone();

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        None,
        None,
        Arc::new(config),
        None,
        threads,
        Arc::new(logger),
        header,
        interner,
        None,
        None,
        None,
        || {},
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
            write_analysis_output_files(
                output_file,
                None,
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
    let output_format = sub_matches.value_of("json-format").map(|f| f.to_string());

    let ignored = sub_matches
        .values_of("ignore")
        .map(|values| values.map(|f| f.to_string()).collect::<FxHashSet<_>>());
    let mut find_unused_expressions = sub_matches.is_present("find-unused-expressions");
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

    let show_all_issues = sub_matches.is_present("all-issues");

    if let Some(filter_issue_strings) = filter_issue_strings {
        for filter_issue_string in filter_issue_strings {
            if let Ok(issue_kind) =
                IssueKind::from_str_custom(filter_issue_string, &all_custom_issues)
            {
                if issue_kind.requires_dataflow_analysis() {
                    find_unused_expressions = true;
                }
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

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }

    // do this after we've loaded from file, as they can be overridden
    if !issue_kinds_filter.is_empty() {
        config.allowed_issues = Some(issue_kinds_filter);
    } else if show_all_issues {
        config.allowed_issues = None;
    }

    let root_dir = config.root_dir.clone();

    let result = hakana_orchestrator::scan_and_analyze(
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
        interner,
        None,
        None,
        None,
        || {},
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
            write_analysis_output_files(
                output_file,
                output_format,
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
                .map(|(k, v)| {
                    format!(
                        "{}\t{}",
                        k.to_string(&successful_run_data.interner),
                        v.len()
                    )
                })
                .collect::<Vec<_>>();

            mixed_sources.sort();

            println!("{}", mixed_sources.join("\n"));
        }
    }
}

fn write_analysis_output_files(
    output_file: String,
    output_format: Option<String>,
    cwd: &String,
    analysis_result: &AnalysisResult,
    interner: &Interner,
) {
    let output_path = if output_file.starts_with('/') {
        output_file
    } else {
        format!("{}/{}", cwd, output_file)
    };
    let mut output_path = fs::File::create(Path::new(&output_path)).unwrap();

    let json = match output_format {
        Some(format) if format == "full" => {
            let mut entries = vec![];

            for (file_path, issues) in analysis_result.get_all_issues(interner, cwd, true) {
                for issue in issues {
                    entries.push(FullEntry::from_issue(issue, &file_path));
                }
            }

            serde_json::to_string_pretty(&entries).unwrap()
        }
        Some(format) if format == "hh_client" => {
            let mut entries = vec![];

            for (file_path, issues) in analysis_result.get_all_issues(interner, cwd, true) {
                for issue in issues {
                    entries.push(HhClientEntry::from_issue(issue, &file_path));
                }
            }

            serde_json::to_string_pretty(&entries).unwrap()
        }
        _ => {
            let mut checkpoint_entries = vec![];

            for (file_path, issues) in analysis_result.get_all_issues(interner, cwd, true) {
                for issue in issues {
                    checkpoint_entries.push(CheckPointEntry::from_issue(issue, &file_path));
                }
            }

            serde_json::to_string_pretty(&checkpoint_entries).unwrap()
        }
    };
    write!(output_path, "{}", json).unwrap();
}

fn write_codegen_output_files(output_file: String, cwd: &String, errors: &Vec<(String, String)>) {
    let output_path = if output_file.starts_with('/') {
        output_file
    } else {
        format!("{}/{}", cwd, output_file)
    };
    let mut output_path = fs::File::create(Path::new(&output_path)).unwrap();

    let mut checkpoint_entries = vec![];

    for (file_path, issue) in errors {
        checkpoint_entries.push(CheckPointEntry {
            case: "Codegen error".to_string(),
            level: CheckPointEntryLevel::Failure,
            filename: file_path.clone(),
            line: 1,
            output: issue.to_string(),
        });
    }

    let json = serde_json::to_string_pretty(&checkpoint_entries).unwrap();
    write!(output_path, "{}", json).unwrap();
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

    for (&(mut start, mut end), replacements) in replacements.iter().rev() {
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

                    file_contents = file_contents[..start as usize].to_string()
                        + &*file_contents[end as usize..].to_string();
                }
                Replacement::TrimPrecedingWhitespaceAndTrailingComma(beg_of_line) => {
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

                    if end as usize + 1 < (file_contents.len() + 1)
                        && &file_contents[end as usize..end as usize + 1] == ","
                    {
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

/// Convert byte offset to line and column number
fn offset_to_line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

/// Parse CODEOWNERS file and return patterns and exact file paths
fn parse_codeowners_files(root_dir: &str) -> (Vec<glob::Pattern>, FxHashSet<String>) {
    let mut codeowner_patterns = Vec::new();
    let mut exact_files = FxHashSet::default();

    let codeowners_path = format!("{}/.github/CODEOWNERS", root_dir);
    let codeowners_content = match fs::read_to_string(&codeowners_path) {
        Ok(content) => content,
        Err(_) => {
            // Try alternate location
            let alt_path = format!("{}/CODEOWNERS", root_dir);
            match fs::read_to_string(&alt_path) {
                Ok(content) => content,
                Err(_) => return (codeowner_patterns, exact_files), // No CODEOWNERS file found
            }
        }
    };

    for line in codeowners_content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse line: "pattern @owner1 @owner2..."
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let pattern = parts[0];

        // Check if there are any owners specified
        if parts.len() < 2 {
            continue;
        }

        // Process pattern
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            // Glob pattern - compile it
            // Convert CODEOWNERS pattern to glob pattern
            let glob_pattern = if pattern.starts_with('/') {
                // Absolute pattern from root
                pattern.to_string()
            } else if pattern.starts_with("**/") {
                // Already in glob format
                pattern.to_string()
            } else {
                // Relative pattern - match anywhere in tree
                format!("**/{}", pattern)
            };

            if let Ok(compiled) = glob::Pattern::new(&glob_pattern) {
                codeowner_patterns.push(compiled);
            }
        } else if pattern.ends_with('/') {
            // Directory pattern with trailing slash - match all files under this directory
            let dir_pattern = if pattern.starts_with('/') {
                format!("{}**", pattern)
            } else {
                format!("**/{pattern}**")
            };

            if let Ok(compiled) = glob::Pattern::new(&dir_pattern) {
                codeowner_patterns.push(compiled);
            }
        } else {
            // Could be either an exact file or a directory without trailing slash
            // Check if it looks like a file (has an extension like .hack or .php)
            let is_file = pattern.ends_with(".hack")
                || pattern.ends_with(".php")
                || pattern.ends_with(".hhi")
                || pattern.contains('.');

            if is_file {
                // Treat as exact file path
                if pattern.starts_with('/') {
                    // Pattern has leading slash - match from root
                    exact_files.insert(pattern.to_string());
                } else {
                    // Pattern doesn't have leading slash - add leading slash for matching
                    // since relative_path in matching code always has a leading slash
                    exact_files.insert(format!("/{}", pattern));
                }
            } else {
                // Treat as directory path - match all files under this directory
                if pattern.starts_with('/') {
                    // Absolute directory pattern from root
                    if let Ok(compiled) = glob::Pattern::new(&format!("{}/**", pattern)) {
                        codeowner_patterns.push(compiled);
                    }
                } else {
                    // Relative directory pattern - match anywhere in tree
                    if let Ok(compiled) = glob::Pattern::new(&format!("**/{pattern}/**")) {
                        codeowner_patterns.push(compiled);
                    }
                }
            }
        }
    }

    (codeowner_patterns, exact_files)
}

fn do_lint(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    had_error: &mut bool,
    custom_linters: Vec<Box<dyn hakana_lint::Linter>>,
) {
    use hakana_lint::{HhastLintConfig, examples};
    use rustc_hash::FxHashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use walkdir::WalkDir;

    let config_path = sub_matches
        .value_of("config")
        .unwrap_or(&format!("{}/hhast-lint.json", root_dir))
        .to_string();

    let apply_fixes = sub_matches.is_present("fix");
    let skip_codeowners = sub_matches.is_present("no-codeowners");

    // Get specific linters to run (if provided)
    let specific_linters = Arc::new(
        sub_matches
            .values_of("linter")
            .map(|values| values.map(|s| s.to_string()).collect::<FxHashSet<_>>()),
    );

    // Parse CODEOWNERS file if skip_codeowners is enabled
    let (codeowner_patterns, codeowner_exact_files) = if skip_codeowners {
        parse_codeowners_files(root_dir)
    } else {
        (Vec::new(), FxHashSet::default())
    };

    // Load HHAST lint configuration
    let mut hhast_config = if Path::new(&config_path).exists() {
        match HhastLintConfig::from_file(Path::new(&config_path)) {
            Ok(config) => config,
            Err(e) => {
                println!("Error loading lint config: {}", e);
                *had_error = true;
                return;
            }
        }
    } else {
        println!(
            "No hhast-lint.json found at {}. Using default configuration.",
            config_path
        );
        HhastLintConfig::default()
    };

    // Build linter registry with all available linters
    let mut registry = hakana_lint::linter::LinterRegistry::new();
    registry.register(Box::new(
        examples::no_empty_statements::NoEmptyStatementsLinter,
    ));
    registry.register(Box::new(
        examples::no_whitespace_at_end_of_line::NoWhitespaceAtEndOfLineLinter,
    ));
    registry.register(Box::new(
        examples::use_statement_without_kind::UseStatementWithoutKindLinter,
    ));
    registry.register(Box::new(
        examples::dont_discard_new_expressions::DontDiscardNewExpressionsLinter,
    ));
    registry.register(Box::new(
        examples::must_use_braces_for_control_flow::MustUseBracesForControlFlowLinter,
    ));
    registry.register(Box::new(examples::no_await_in_loop::NoAwaitInLoopLinter));
    registry.register(Box::new(
        examples::unused_use_clause::UnusedUseClauseLinter::new(),
    ));

    // Register custom linters
    for linter in custom_linters {
        registry.register(linter);
    }

    // Map requested linter names to HHAST names and add to config
    if let Some(requested_linters) = specific_linters.as_ref() {
        for linter_name in requested_linters.iter() {
            // Try to find linter by name or HHAST name
            let mut found = false;
            for linter in registry.all() {
                if let Some(hhast_name) = linter.hhast_name() {
                    let short_name = hhast_name.split('\\').last().unwrap_or(hhast_name);
                    if linter_name == hhast_name
                        || linter_name == short_name
                        || linter_name == linter.name()
                    {
                        if !hhast_config.extra_linters.contains(&hhast_name.to_string()) {
                            hhast_config.extra_linters.push(hhast_name.to_string());
                        }
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                println!("Warning: Unknown linter '{}'", linter_name);
            }
        }
    }
    let hhast_config = Arc::new(hhast_config);

    let registry = Arc::new(registry);

    // Determine files to lint
    let paths_to_lint: Vec<String> = if let Some(paths) = sub_matches.values_of("PATH") {
        // Use explicitly provided paths
        paths.map(|s| s.to_string()).collect()
    } else if !hhast_config.roots.is_empty() {
        // Use roots from config
        hhast_config
            .roots
            .iter()
            .map(|r| format!("{}/{}", root_dir, r))
            .collect()
    } else {
        // Default to root_dir
        vec![root_dir.to_string()]
    };

    // Collect all files to lint
    let mut files_to_lint = Vec::new();
    let mut skipped_codeowner_files = 0;
    for base_path in paths_to_lint {
        for entry in WalkDir::new(&base_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let path_str = path.to_string_lossy();
            if !path_str.ends_with(".hack") && !path_str.ends_with(".php") {
                continue;
            }

            // Skip files with codeowners if requested
            if skip_codeowners {
                // Canonicalize both the file path and root_dir to handle relative paths
                let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
                let canonical_root = std::path::Path::new(&root_dir)
                    .canonicalize()
                    .unwrap_or_else(|_| std::path::PathBuf::from(&root_dir));

                let relative_path = if let Ok(rel) = canonical_path.strip_prefix(&canonical_root) {
                    format!("/{}", rel.to_string_lossy())
                } else {
                    // Fallback: try to make path relative to root_dir if it's not already
                    format!("/{}", path_str.trim_start_matches("./"))
                };

                // Check exact file match
                let has_codeowner = codeowner_exact_files.contains(&relative_path)
                    // Check glob patterns
                    || codeowner_patterns.iter().any(|pattern| {
                        // Try matching with and without leading slash
                        pattern.matches(&relative_path)
                            || pattern.matches(relative_path.trim_start_matches('/'))
                    });

                if has_codeowner {
                    skipped_codeowner_files += 1;
                    continue;
                }
            }

            files_to_lint.push(path.to_path_buf());
        }
    }

    if skip_codeowners && skipped_codeowner_files > 0 {
        println!(
            "Skipped {} file(s) with codeowners",
            skipped_codeowner_files
        );
    }

    if files_to_lint.is_empty() {
        println!("\nNo files to lint.");
        return;
    }

    let total_errors = Arc::new(Mutex::new(0usize));
    let total_files = Arc::new(Mutex::new(0usize));
    let total_fixed = Arc::new(Mutex::new(0usize));
    let lint_output = Arc::new(Mutex::new(Vec::new()));

    // Determine number of threads from CLI
    let threads = if let Some(val) = sub_matches.value_of("threads").map(|f| f.to_string()) {
        val.parse::<usize>().unwrap_or(8)
    } else {
        8
    };

    let mut group_size = threads;

    if files_to_lint.len() < 4 * group_size {
        group_size = 1;
    }

    let mut path_groups: FxHashMap<usize, Vec<PathBuf>> = FxHashMap::default();

    for (i, path) in files_to_lint.into_iter().enumerate() {
        let group = i % group_size;
        path_groups.entry(group).or_insert_with(Vec::new).push(path);
    }

    let mut handles = vec![];

    let root_dir = root_dir.to_string();

    for (_, path_group) in path_groups {
        let hhast_config = hhast_config.clone();
        let registry = registry.clone();
        let total_errors = total_errors.clone();
        let total_files = total_files.clone();
        let total_fixed = total_fixed.clone();
        let lint_output = lint_output.clone();
        let root_dir = root_dir.clone();

        let handle = std::thread::spawn(move || {
            let lint_config = hakana_lint::LintConfig {
                allow_auto_fix: apply_fixes,
                apply_auto_fix: apply_fixes,
                enabled_linters: Vec::new(),
                disabled_linters: Vec::new(),
            };

            for path in path_group {
                let path_str = path.to_string_lossy();

                // Make path relative to root_dir for pattern matching
                let relative_path = if let Ok(rel) = path.strip_prefix(&root_dir) {
                    rel.to_string_lossy().to_string()
                } else {
                    path_str.to_string()
                };

                // Determine which linters to run for this file
                let mut file_linters: Vec<&dyn hakana_lint::Linter> = Vec::new();

                for linter in registry.all() {
                    if let Some(hhast_name) = linter.hhast_name() {
                        if hhast_config.is_linter_enabled(hhast_name, &relative_path) {
                            file_linters.push(linter.as_ref());
                        }
                    } else {
                        // No HHAST name, run unconditionally
                        file_linters.push(linter.as_ref());
                    }
                }

                if file_linters.is_empty() {
                    continue;
                }

                // Read file contents
                let contents = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        lint_output
                            .lock()
                            .unwrap()
                            .push(format!("Error reading {}: {}", path_str, e));
                        continue;
                    }
                };

                *total_files.lock().unwrap() += 1;

                // Run linters
                match hakana_lint::run_linters(&path, &contents, &file_linters, &lint_config) {
                    Ok(result) => {
                        // Collect errors
                        if !result.errors.is_empty() {
                            *total_errors.lock().unwrap() += result.errors.len();
                            for error in &result.errors {
                                // Convert offset to line/column
                                let (line, column) =
                                    offset_to_line_column(&contents, error.start_offset);
                                lint_output.lock().unwrap().push(format!(
                                    "{}:{}:{}: {} - {}",
                                    relative_path, line, column, error.severity, error.message
                                ));
                            }
                        }

                        // Apply fixes if requested and available
                        if apply_fixes {
                            let mut file_ops_applied = false;

                            // Apply file operations (create/delete files)
                            if !result.file_operations.is_empty() {
                                for file_op in &result.file_operations {
                                    match &file_op.op_type {
                                        hakana_lint::FileOpType::Create => {
                                            if let Some(ref content) = file_op.content {
                                                // Resolve relative paths relative to the source file's directory
                                                let target_path = if file_op.path.is_absolute() {
                                                    file_op.path.clone()
                                                } else if let Some(parent) = path.parent() {
                                                    parent.join(&file_op.path)
                                                } else {
                                                    file_op.path.clone()
                                                };

                                                // Create parent directories if they don't exist
                                                if let Some(parent_dir) = target_path.parent() {
                                                    if let Err(e) = fs::create_dir_all(parent_dir) {
                                                        lint_output.lock().unwrap().push(format!(
                                                            "Error creating directory {}: {}",
                                                            parent_dir.display(),
                                                            e
                                                        ));
                                                        continue;
                                                    }
                                                }

                                                match fs::write(&target_path, content) {
                                                    Ok(_) => {
                                                        lint_output.lock().unwrap().push(format!(
                                                            "Created: {}",
                                                            target_path.display()
                                                        ));
                                                        file_ops_applied = true;
                                                    }
                                                    Err(e) => {
                                                        lint_output.lock().unwrap().push(format!(
                                                            "Error creating {}: {}",
                                                            target_path.display(),
                                                            e
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                        hakana_lint::FileOpType::Delete => {
                                            let target_path = if file_op.path.is_absolute() {
                                                file_op.path.clone()
                                            } else if let Some(parent) = path.parent() {
                                                parent.join(&file_op.path)
                                            } else {
                                                file_op.path.clone()
                                            };

                                            match fs::remove_file(&target_path) {
                                                Ok(_) => {
                                                    lint_output.lock().unwrap().push(format!(
                                                        "Deleted: {}",
                                                        target_path.display()
                                                    ));
                                                    file_ops_applied = true;
                                                }
                                                Err(e) => {
                                                    lint_output.lock().unwrap().push(format!(
                                                        "Error deleting {}: {}",
                                                        target_path.display(),
                                                        e
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Apply text edits to the source file
                            if let Some(fixed_source) = result.modified_source {
                                match fs::write(&path, fixed_source) {
                                    Ok(_) => {
                                        *total_fixed.lock().unwrap() += 1;
                                        lint_output
                                            .lock()
                                            .unwrap()
                                            .push(format!("Fixed: {}", relative_path));
                                    }
                                    Err(e) => {
                                        lint_output.lock().unwrap().push(format!(
                                            "Error writing fixes to {}: {}",
                                            path_str, e
                                        ));
                                    }
                                }
                            } else if file_ops_applied {
                                // If we only did file operations, count that as a fix
                                *total_fixed.lock().unwrap() += 1;
                            }
                        }
                    }
                    Err(e) => {
                        lint_output
                            .lock()
                            .unwrap()
                            .push(format!("Error linting {}: {}", path_str, e));
                    }
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Print all output
    let output = lint_output.lock().unwrap();
    for line in output.iter() {
        println!("{}", line);
    }

    let final_errors = *total_errors.lock().unwrap();
    let final_files = *total_files.lock().unwrap();
    let final_fixed = *total_fixed.lock().unwrap();

    if final_errors > 0 {
        *had_error = true;
    }

    // Print summary
    if final_errors > 0 {
        println!(
            "\nFound {} lint issue(s) in {} file(s)",
            final_errors, final_files
        );
        if apply_fixes && final_fixed > 0 {
            println!("Fixed {} file(s)", final_fixed);
        }
    } else {
        println!("\nNo lint issues found! Checked {} file(s).", final_files);
    }
}
