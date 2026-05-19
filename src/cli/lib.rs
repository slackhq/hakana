use clap::Command;
use hakana_analyzer::custom_hook::CustomHook;
use log::LevelFilter;
use rustc_hash::FxHashSet;
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use std::process::exit;
use test_runners::test_runner::TestRunner;

/// Print a message only when stdout is a terminal (not piped).
/// Use this for informational/summary output that should not appear in piped output.
macro_rules! tty_println {
    ($($arg:tt)*) => {
        if <std::io::Stdout as std::io::IsTerminal>::is_terminal(&std::io::stdout()) {
            println!($($arg)*);
        }
    };
}

pub mod commands;
pub mod helpers;
pub mod mcp;
pub mod test_runners;

pub async fn init(
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    codegen_hooks: Vec<Box<dyn CustomHook>>,
    header: &str,
    test_runner: &TestRunner,
    custom_linters: Vec<Box<dyn hakana_lint::Linter>>,
) {
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
        .subcommand(commands::analyze::get_subcommand())
        .subcommand(commands::migration_candidates::get_subcommand())
        .subcommand(commands::codegen::get_subcommand())
        .subcommand(commands::migrate::get_subcommand())
        .subcommand(commands::add_fixmes::get_subcommand())
        .subcommand(commands::remove_unused_fixmes::get_subcommand())
        .subcommand(commands::fix::get_subcommand())
        .subcommand(commands::security_check::get_subcommand())
        .subcommand(commands::find_paths::get_subcommand())
        .subcommand(commands::test::get_subcommand())
        .subcommand(commands::find_executable::get_subcommand())
        .subcommand(commands::lint::get_subcommand())
        .subcommand(commands::cyclomatic_complexity::get_subcommand())
        .subcommand(commands::server::get_subcommand())
        .get_matches();

    let cwd = (env::current_dir()).unwrap().to_str().unwrap().to_string();

    let stdout_is_tty = std::io::stdout().is_terminal();

    if stdout_is_tty {
        println!("{}\n", header);
    }

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

    let show_progress = match matches.subcommand() {
        Some(("test", sub_matches)) => {
            if sub_matches.is_present("debug") {
                hakana_logger::init_stdout_logger(LevelFilter::Debug);
            } else {
                hakana_logger::init_stdout_logger(LevelFilter::Off);
            }
            false
        }
        Some(("server", sub_matches)) => {
            let level = if sub_matches.is_present("debug") {
                LevelFilter::Debug
            } else {
                LevelFilter::Info
            };
            hakana_logger::init_file_logger("/tmp/hakana-server.log", level);
            false
        }
        Some((_, sub_matches)) => {
            if sub_matches.is_present("debug") {
                hakana_logger::init_stdout_logger(LevelFilter::Debug);
                false
            } else if sub_matches.is_present("show-timing") {
                hakana_logger::init_stdout_logger(LevelFilter::Debug);
                false
            } else if stdout_is_tty {
                hakana_logger::init_stdout_logger(LevelFilter::Info);
                true
            } else {
                hakana_logger::init_stdout_logger(LevelFilter::Off);
                false
            }
        }
        _ => {
            if stdout_is_tty {
                hakana_logger::init_stdout_logger(LevelFilter::Info);
                true
            } else {
                hakana_logger::init_stdout_logger(LevelFilter::Off);
                false
            }
        }
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
            commands::analyze::handle(
                sub_matches,
                all_custom_issues,
                &root_dir,
                analysis_hooks,
                config_path,
                &cwd,
                cache_dir,
                threads,
                show_progress,
                header,
                &mut had_error,
            )
            .await;
        }
        Some(("security-check", sub_matches)) => {
            commands::security_check::handle(
                &cwd,
                all_custom_issues,
                config_path,
                sub_matches,
                analysis_hooks,
                threads,
                show_progress,
                header,
                &mut had_error,
            );
        }
        Some(("find-paths", sub_matches)) => {
            commands::find_paths::handle(
                &cwd,
                all_custom_issues,
                config_path,
                sub_matches,
                analysis_hooks,
                threads,
                show_progress,
                header,
                &mut had_error,
            );
        }
        Some(("migrate", sub_matches)) => {
            commands::migrate::handle(
                sub_matches,
                &root_dir,
                all_custom_issues,
                migration_hooks,
                config_path,
                &cwd,
                threads,
                show_progress,
                header,
            );
        }
        Some(("migration-candidates", sub_matches)) => {
            commands::migration_candidates::handle(
                sub_matches,
                &root_dir,
                all_custom_issues,
                migration_hooks,
                config_path,
                &cwd,
                threads,
                show_progress,
                header,
            );
        }
        Some(("codegen", sub_matches)) => {
            commands::codegen::handle(
                sub_matches,
                &root_dir,
                all_custom_issues,
                analysis_hooks,
                codegen_hooks,
                config_path,
                &cwd,
                threads,
                show_progress,
                header,
            );
        }
        Some(("add-fixmes", sub_matches)) => {
            commands::add_fixmes::handle(
                sub_matches,
                all_custom_issues,
                &root_dir,
                analysis_hooks,
                config_path,
                &cwd,
                threads,
                show_progress,
                header,
            );
        }
        Some(("remove-unused-fixmes", sub_matches)) => {
            commands::remove_unused_fixmes::handle(
                sub_matches,
                &root_dir,
                all_custom_issues,
                analysis_hooks,
                config_path,
                &cwd,
                threads,
                show_progress,
                header,
            );
        }
        Some(("fix", sub_matches)) => {
            commands::fix::handle(
                sub_matches,
                all_custom_issues,
                analysis_hooks,
                root_dir,
                config_path,
                cwd,
                threads,
                show_progress,
                header,
            );
        }
        Some(("test", sub_matches)) => {
            commands::test::handle(
                sub_matches,
                test_runner,
                show_progress,
                &mut had_error,
                header,
            );
        }
        Some(("find-executable", sub_matches)) => {
            commands::find_executable::handle(sub_matches, &root_dir, &cwd, threads, show_progress);
        }
        Some(("lint", sub_matches)) => {
            commands::lint::handle(sub_matches, &root_dir, &mut had_error, custom_linters);
        }
        Some(("server", sub_matches)) => {
            commands::server::handle(sub_matches, &root_dir, threads, header, analysis_hooks).await;
        }
        Some(("cyclomatic-complexity", sub_matches)) => {
            commands::cyclomatic_complexity::handle(
                sub_matches,
                all_custom_issues,
                &root_dir,
                config_path,
                &cwd,
                threads,
                show_progress,
                header,
                &mut had_error,
            );
        }
        _ => unreachable!(),
    }

    if had_error {
        exit(1);
    }
}
