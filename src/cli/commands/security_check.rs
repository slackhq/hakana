use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;

use crate::helpers::write_analysis_output_files;

pub fn get_subcommand() -> Command<'static> {
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
        )
}

pub fn handle(
    cwd: &String,
    all_custom_issues: FxHashSet<String>,
    config_path: Option<&Path>,
    sub_matches: &clap::ArgMatches,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    threads: u8,
    show_progress: bool,
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

    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();

    let root_dir = config.root_dir.clone();

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        None,
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
            tty_println!("\nNo security issues found!\n");
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
