use clap::{Parser, Subcommand};
use commands::*;
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

#[derive(Parser)]
#[command(name = "hakana", about = "Another static analysis tool for Hack")]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyzes code in the current directory
    #[command(alias = "analyse")]
    Analyze(analyze::AnalyzeArgs),
    /// Generates a list of all migration candidates
    MigrationCandidates(migration_candidates::MigrationCandidatesArgs),
    /// Generates codegen
    Codegen(codegen::CodegenArgs),
    /// Migrates code in the current directory
    Migrate(migrate::MigrateArgs),
    /// Adds fixmes to suppress Hakana issues
    AddFixmes(add_fixmes::AddFixmesArgs),
    /// Removes all fixmes that are never used
    RemoveUnusedFixmes(remove_unused_fixmes::RemoveUnusedFixmesArgs),
    /// Fixes issues in the codebase
    Fix(fix::FixArgs),
    /// Looks for vulnerabilities in the codebase
    SecurityCheck(security_check::SecurityCheckArgs),
    /// Does whole-program analysis querying
    FindPaths(find_paths::FindPathsArgs),
    /// Runs one or more Hakana tests
    Test(test::TestArgs),
    /// Finds all executable lines of code
    FindExecutable(find_executable::FindExecutableArgs),
    /// Runs linters on Hack code (HHAST-compatible)
    Lint(lint::LintArgs),
    /// Format Hack source using hackfmt-compatible rules
    #[command(after_help = hakana_format::cli::AFTER_HELP, args_override_self = true)]
    Format(hakana_format::cli::FormatArgs),
    /// Analyzes cyclomatic complexity of functions in the codebase
    CyclomaticComplexity(cyclomatic_complexity::CyclomaticComplexityArgs),
    /// Start or manage the hakana server
    Server(server::ServerArgs),
}

impl Commands {
    fn common(&self) -> Option<&CommonArgs> {
        match self {
            Commands::Analyze(args) => Some(&args.common),
            Commands::MigrationCandidates(args) => Some(&args.common),
            Commands::Codegen(args) => Some(&args.common),
            Commands::Migrate(args) => Some(&args.common),
            Commands::AddFixmes(args) => Some(&args.common),
            Commands::RemoveUnusedFixmes(args) => Some(&args.common),
            Commands::Fix(args) => Some(&args.common),
            Commands::SecurityCheck(args) => Some(&args.common),
            Commands::FindPaths(args) => Some(&args.common),
            Commands::CyclomaticComplexity(args) => Some(&args.common),
            Commands::Server(args) => Some(&args.common),
            Commands::Test(_)
            | Commands::FindExecutable(_)
            | Commands::Lint(_)
            | Commands::Format(_) => None,
        }
    }

    fn root(&self) -> Option<&str> {
        match self {
            Commands::FindExecutable(args) => args.root.as_deref(),
            Commands::Lint(args) => args.root.as_deref(),
            _ => self.common().and_then(|c| c.root.as_deref()),
        }
    }

    fn config(&self) -> Option<&str> {
        match self {
            Commands::Lint(args) => args.config.as_deref(),
            _ => self.common().and_then(|c| c.config.as_deref()),
        }
    }

    fn threads(&self) -> Option<u8> {
        self.common().and_then(|c| c.threads)
    }

    fn debug(&self) -> bool {
        match self {
            Commands::Test(args) => args.debug,
            Commands::Lint(args) => args.debug,
            Commands::Analyze(args) => args.common.debug || args.show_timing,
            _ => self.common().is_some_and(|c| c.debug),
        }
    }
}

pub async fn init(
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    codegen_hooks: Vec<Box<dyn CustomHook>>,
    header: &str,
    test_runner: &TestRunner,
    custom_linters: Vec<Box<dyn hakana_lint::Linter>>,
) {
    // Formatting is independent of analysis configuration, logging, and caches.
    if env::args().nth(1).as_deref() == Some("format") {
        exit(hakana_format::cli::run(env::args().skip(2)));
    }

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

    let command = Cli::parse().command;

    let cwd = (env::current_dir()).unwrap().to_str().unwrap().to_string();

    let stdout_is_tty = std::io::stdout().is_terminal();

    if stdout_is_tty {
        println!("{}\n", header);
    }

    let threads = match &command {
        Commands::Test(_) => 1,
        _ => command.threads().unwrap_or(8),
    };

    let show_progress = match &command {
        Commands::Test(args) => {
            if args.debug {
                hakana_logger::init_stdout_logger(LevelFilter::Debug);
            } else {
                hakana_logger::init_stdout_logger(LevelFilter::Off);
            }
            false
        }
        Commands::Server(args) => {
            let level = if args.common.debug {
                LevelFilter::Debug
            } else {
                LevelFilter::Info
            };
            hakana_logger::init_file_logger("/tmp/hakana-server.log", level);
            false
        }
        _ => {
            if command.debug() {
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
    };

    let root_dir = match &command {
        Commands::Test(_) => cwd.clone(),
        _ => command.root().unwrap_or(cwd.as_str()).to_string(),
    };

    let config_path = match &command {
        Commands::Test(_) => None,
        _ => Some(
            command
                .config()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}/hakana.json", root_dir)),
        ),
    };

    let config_path = config_path.as_ref().map(Path::new);

    let cache_dir = format!("{}/.hakana_cache", root_dir);

    if !Path::new(&cache_dir).is_dir() && fs::create_dir(&cache_dir).is_err() {
        panic!("could not create aast cache directory");
    }

    let mut had_error = false;

    match &command {
        Commands::Analyze(args) => {
            commands::analyze::handle(
                args,
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
        Commands::SecurityCheck(args) => {
            commands::security_check::handle(
                &cwd,
                all_custom_issues,
                config_path,
                args,
                analysis_hooks,
                threads,
                show_progress,
                header,
                &mut had_error,
            );
        }
        Commands::FindPaths(args) => {
            commands::find_paths::handle(
                &cwd,
                all_custom_issues,
                config_path,
                args,
                analysis_hooks,
                threads,
                show_progress,
                header,
                &mut had_error,
            );
        }
        Commands::Migrate(args) => {
            commands::migrate::handle(
                args,
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
        Commands::MigrationCandidates(args) => {
            commands::migration_candidates::handle(
                args,
                &root_dir,
                all_custom_issues,
                migration_hooks,
                config_path,
                &cwd,
                threads,
                show_progress,
                header,
            )
            .await;
        }
        Commands::Codegen(args) => {
            commands::codegen::handle(
                args,
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
        Commands::AddFixmes(args) => {
            commands::add_fixmes::handle(
                args,
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
        Commands::RemoveUnusedFixmes(args) => {
            commands::remove_unused_fixmes::handle(
                args,
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
        Commands::Fix(args) => {
            commands::fix::handle(
                args,
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
        Commands::Test(args) => {
            commands::test::handle(args, test_runner, show_progress, &mut had_error, header);
        }
        Commands::FindExecutable(args) => {
            commands::find_executable::handle(args, &root_dir, &cwd, threads, show_progress);
        }
        Commands::Lint(args) => {
            commands::lint::handle(args, &root_dir, &mut had_error, custom_linters);
        }
        Commands::Server(args) => {
            commands::server::handle(
                args,
                &root_dir,
                threads,
                header,
                analysis_hooks,
                migration_hooks,
            )
            .await;
        }
        Commands::CyclomaticComplexity(args) => {
            commands::cyclomatic_complexity::handle(
                args,
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
        Commands::Format(_) => unreachable!("handled before parsing"),
    }

    if had_error {
        exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use clap::{CommandFactory, Parser};

    fn parse(args: &[&str]) -> Commands {
        Cli::try_parse_from(std::iter::once("hakana").chain(args.iter().copied()))
            .unwrap()
            .command
    }

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }

    #[test]
    fn multi_value_args_accept_repeated_and_space_separated_values() {
        for args in [
            &["add-fixmes", "--issue", "A", "B"][..],
            &["add-fixmes", "--issue", "A", "--issue", "B"][..],
        ] {
            let Commands::AddFixmes(add_fixmes) = parse(args) else {
                panic!("expected add-fixmes");
            };
            assert_eq!(add_fixmes.issue, ["A", "B"]);
        }

        let Commands::Lint(lint) = parse(&["lint", "src", "--linter", "foo", "--no-codeowners"])
        else {
            panic!("expected lint");
        };
        assert_eq!(lint.paths, ["src"]);
        assert_eq!(lint.linter.as_deref(), Some(&["foo".to_string()][..]));
        assert!(lint.no_codeowners);
    }

    #[test]
    fn shared_args_are_exposed_through_commands() {
        let command = parse(&["analyse", "--root", "/r", "--threads", "4", "--show-timing"]);
        assert!(matches!(command, Commands::Analyze(_)));
        assert_eq!(command.root(), Some("/r"));
        assert_eq!(command.threads(), Some(4));
        assert!(command.debug());

        let command = parse(&["lint", "--config", "hhast.json"]);
        assert_eq!(command.config(), Some("hhast.json"));
        assert_eq!(command.threads(), None);
    }
}
