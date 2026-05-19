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
    Command::new("fix")
        .about("Fixes issues in the codebase")
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
        )
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    all_custom_issues: FxHashSet<String>,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    root_dir: String,
    config_path: Option<&Path>,
    cwd: String,
    threads: u8,
    show_progress: bool,
    header: &str,
) {
    let issue_name = sub_matches.value_of("issue").unwrap().to_string();
    let issue_kind = IssueKind::from_str_custom(&issue_name, &all_custom_issues).unwrap();

    if !matches!(issue_kind, IssueKind::CustomIssue(..)) && !issue_kind.has_autofix() {
        println!("Issue type {} does not support autofixing", issue_kind);
        exit(1);
    }

    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);
    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();

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
        show_progress,
        header,
        Arc::new(interner),
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
