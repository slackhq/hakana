use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::issue::IssueKind;
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;

use crate::helpers::update_files;

pub fn get_subcommand() -> Command<'static> {
    Command::new("add-fixmes")
        .about("Adds fixmes to suppress Hakana issues")
        .arg(
            arg!(--"root" <PATH>)
                .required(false)
                .help("The root directory that Hakana runs in. Defaults to the current directory"),
        )
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
        )
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    all_custom_issues: FxHashSet<String>,
    root_dir: &String,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
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
    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();
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
        show_progress,
        header,
        Arc::new(interner),
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
