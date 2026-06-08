use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::fs;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;

use crate::helpers::update_files;

pub fn get_subcommand() -> Command<'static> {
    Command::new("migrate")
        .about("Migrates code in the current directory")
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
        )
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    root_dir: &String,
    all_custom_issues: FxHashSet<String>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
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
        .map(Arc::from)
        .collect();

    if config.hooks.is_empty() {
        println!("Migration {} not recognised", migration_name);
        exit(1);
    }

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists() {
        if let Err(error) = config.update_from_file(cwd, config_path, &mut interner) {
            println!("Invalid config: {}", error);
            exit(1);
        }
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
