use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_str::{Interner, StrId};
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
                .help("Only return migration candidates matching this glob expression"),
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

    let filter = sub_matches
        .value_of("filter")
        .map(|f| glob::Pattern::new(f).expect(&format!("Invalid filter pattern {}", f)));

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        None,
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

    if let Ok((result, scan_data)) = result {
        tty_println!("\nSymbols to migrate:\n");
        for config_hook in &config.hooks {
            let migration_candidates =
                config_hook.get_candidates(&scan_data.codebase, &scan_data.interner, &result);

            for migration_candidate in migration_candidates {
                let (classlike_id, member_id) = if let Some((classlike_name, member_name)) =
                    migration_candidate.split_once("::")
                {
                    (
                        scan_data.interner.get(classlike_name),
                        scan_data.interner.get(member_name),
                    )
                } else {
                    (
                        scan_data.interner.get(&migration_candidate),
                        Some(StrId::EMPTY),
                    )
                };

                // If a filter expression is given, only yield migration candidates that match it.
                if let Some(classlike_id) = classlike_id
                    && let Some(member_id) = member_id
                    && let Some(location) =
                        scan_data.codebase.get_symbol_pos(&classlike_id, &member_id)
                {
                    let relative_definition_path = location
                        .file_path
                        .get_relative_path(&scan_data.interner, &config.root_dir);

                    if filter
                        .as_ref()
                        .is_none_or(|f| f.matches(&relative_definition_path))
                    {
                        println!("{}", migration_candidate);
                    }
                }
            }
        }
    }
}
