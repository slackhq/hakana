use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;

pub fn get_subcommand() -> Command<'static> {
    Command::new("migration-candidates")
        .about("Generates a list of all migration candidates")
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
        )
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    all_custom_issues: FxHashSet<String>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
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
        .map(Arc::from)
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
        show_progress,
        header,
        Arc::new(interner),
        None,
        None,
        None,
        || {},
    );

    if let Ok(result) = result {
        tty_println!("\nSymbols to migrate:\n");
        for config_hook in &config.hooks {
            let migration_candidates =
                config_hook.get_candidates(&result.1.codebase, &result.1.interner, &result.0);

            for migration_candidate in migration_candidates {
                println!("{}", migration_candidate);
            }
        }
    }
}
