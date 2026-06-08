use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;

use crate::helpers::update_files;

pub fn get_subcommand() -> Command<'static> {
    Command::new("remove-unused-fixmes")
        .about("Removes all fixmes that are never used")
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
        )
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    root_dir: &String,
    all_custom_issues: FxHashSet<String>,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
    header: &str,
) {
    let filter = sub_matches.value_of("filter").map(|f| f.to_string());

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);

    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists() {
        if let Err(error) = config.update_from_file(cwd, config_path, &mut interner) {
            println!("Invalid config: {}", error);
            std::process::exit(1);
        }
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
