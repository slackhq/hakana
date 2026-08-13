use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_language_server::server_client::ServerConnection;
use hakana_protocol::{ClientSocket, GetMigrationCandidatesRequest, Message, SocketPath};
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
            arg!(--"standalone")
                .required(false)
                .help("Run analysis directly without connecting to server (default for CI)"),
        )
        .arg(
            arg!(--"with-server")
                .required(false)
                .help("Use server mode: connect to existing server or spawn one if needed"),
        )
        .arg(
            arg!(--"debug")
                .required(false)
                .help("Add output for debugging"),
        )
}

pub async fn handle(
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

    // Validate the filter glob up front so an invalid pattern fails identically
    // regardless of whether we run against a server or standalone.
    let filter = sub_matches
        .value_of("filter")
        .map(|f| glob::Pattern::new(f).unwrap_or_else(|_| panic!("Invalid filter pattern {}", f)));

    // Prefer a running server unless --standalone was passed. This mirrors the
    // behavior of the `analyze` command.
    let standalone = sub_matches.is_present("standalone");
    let with_server = sub_matches.is_present("with-server");
    let project_root = Path::new(root_dir);
    let socket_path = SocketPath::for_project(project_root);
    let use_server = if standalone {
        false
    } else if with_server {
        if !socket_path.server_exists() {
            ServerConnection::connect_or_spawn(project_root, None)
                .await
                .inspect_err(|e| {
                    println!(
                        "Failed to spawn server: {}. Falling back to standalone analysis.",
                        e
                    )
                })
                .is_ok()
        } else {
            true
        }
    } else {
        socket_path.server_exists()
    };

    if use_server
        && handle_via_server(
            &socket_path,
            &migration_name,
            sub_matches.value_of("filter"),
        )
        .await
    {
        return;
    }

    handle_standalone(
        migration_name,
        filter,
        all_custom_issues,
        migration_hooks,
        config_path,
        cwd,
        threads,
        show_progress,
        header,
        root_dir,
    );
}

/// Request migration candidates from a running server. Returns `true` if the request
/// completed (whether or not candidates were found); `false` if we should fall back to
/// a standalone analysis (e.g. the connection dropped).
async fn handle_via_server(
    socket_path: &SocketPath,
    migration_name: &str,
    filter: Option<&str>,
) -> bool {
    use std::time::Duration;

    let request = Message::GetMigrationCandidates(GetMigrationCandidatesRequest {
        migration: migration_name.to_string(),
        filter: filter.map(|f| f.to_string()),
        block_until_next_analysis: false,
    });

    loop {
        let mut client = match ClientSocket::connect(socket_path).await {
            Ok(c) => c,
            Err(e) => {
                println!("Error connecting to server: {}", e);
                return false;
            }
        };

        match client.request(&request).await {
            Ok(Message::GetMigrationCandidatesResult(result)) => {
                if !result.analysis_complete {
                    // Analysis is still running; wait and retry.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }

                if !result.migration_recognized {
                    println!("Migration {} not recognised", migration_name);
                    exit(1);
                }

                tty_println!("\nSymbols to migrate:\n");
                for candidate in result.candidates {
                    println!("{}", candidate);
                }
                return true;
            }
            Ok(Message::Error(err)) => {
                println!("Server error: {} - {}", err.code as u32, err.message);
                exit(1);
            }
            Ok(_) => {
                println!("Unexpected response from server");
                exit(1);
            }
            Err(e) => {
                println!("Error communicating with server: {}", e);
                return false;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_standalone(
    migration_name: String,
    filter: Option<glob::Pattern>,
    all_custom_issues: FxHashSet<String>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
    header: &str,
    root_dir: &str,
) {
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

    if config_path.exists()
        && let Err(error) = config.update_from_file(cwd, config_path, &mut interner) {
            println!("Invalid config: {}", error);
            exit(1);
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
