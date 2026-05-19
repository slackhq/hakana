use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::issue::IssueKind;
use hakana_language_server::server_client::ServerConnection;
use hakana_protocol::ClientSocket;
use hakana_str::Interner;
use indexmap::IndexMap;
use indicatif::{ProgressBar, ProgressStyle};
use rustc_hash::FxHashSet;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;

use crate::helpers::write_analysis_output_files;

pub fn get_subcommand() -> Command<'static> {
    Command::new("analyze")
        .alias("analyse")
        .about("Analyzes code in the current directory")
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
            arg!(--"filter" <PATH>)
                .required(false)
                .help("Filter the files that are analyzed"),
        )
        .arg(
            arg!(--"ignore" <PATH>)
                .required(false)
                .multiple(true)
                .help("Ignore certain files during analysis"),
        )
        .arg(
            arg!(--"threads" <PATH>)
                .required(false)
                .help("How many threads to use"),
        )
        .arg(
            arg!(--"find-unused-expressions")
                .required(false)
                .help("Find unused expressions"),
        )
        .arg(
            arg!(--"find-unused-definitions")
                .required(false)
                .help("Find unused definitions — classes, functions, methods etc."),
        )
        .arg(
            arg!(--"show-issue" <PATH>)
                .required(false)
                .multiple(true)
                .help("Only output issues of this/these type(s)"),
        )
        .arg(arg!(--"all-issues").required(false).help("Show all issues"))
        .arg(
            arg!(--"ignore-mixed-issues")
                .required(false)
                .help("Ignore mixed/any issues"),
        )
        .arg(
            arg!(--"show-mixed-function-counts")
                .required(false)
                .help("Show which functions we lead to mixed types"),
        )
        .arg(
            arg!(--"print-symbol-usages")
                .required(false)
                .help("Output a JSON map of symbol definitions and usages"),
        )
        .arg(
            arg!(--"debug")
                .required(false)
                .help("Add output for debugging"),
        )
        .arg(
            arg!(--"show-timing")
                .required(false)
                .help("If set, timing info will be displayed"),
        )
        .arg(
            arg!(--"no-cache")
                .required(false)
                .help("Whether to ignore the cache"),
        )
        .arg(
            arg!(--"diff")
                .required(false)
                .help("Whether perform AST-based diffing to speed up execution"),
        )
        .arg(
            arg!(--"show-issue-stats")
                .required(false)
                .help("Output a summary of issue counts"),
        )
        .arg(
            arg!(--"output" <PATH>)
                .required(false)
                .help("File to save output to"),
        )
        .arg(
            arg!(--"json-format" <FORMAT>)
                .required(false)
                .help("Format for JSON output. Options: checkpoint (default), full, hh_client"),
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
}

#[allow(clippy::too_many_arguments)]
pub async fn handle(
    sub_matches: &clap::ArgMatches,
    all_custom_issues: FxHashSet<String>,
    root_dir: &str,
    mut analysis_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    cache_dir: String,
    threads: u8,
    show_progress: bool,
    header: &str,
    had_error: &mut bool,
) {
    use hakana_protocol::{GetIssuesRequest, Message, SocketPath};
    use std::io::{self, Write};
    use std::time::Duration;

    let filter = sub_matches.value_of("filter").map(|f| f.to_string());
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

    if use_server {
        let stdout_is_tty = io::stdout().is_terminal();

        if stdout_is_tty {
            print!("\r\x1b[K");
            io::stdout().flush().ok();
        }
        let find_unused_expressions = sub_matches.is_present("find-unused-expressions");
        let find_unused_definitions = sub_matches.is_present("find-unused-definitions");

        let request = Message::GetIssues(GetIssuesRequest {
            filter: filter.clone(),
            find_unused_expressions,
            find_unused_definitions,
            block_until_next_analysis: false,
            send_progress_report: true,
        });

        let pb = if stdout_is_tty {
            let pb = ProgressBar::new(100);
            pb.set_style(
                ProgressStyle::with_template("{bar:40.green/yellow} {percent:>3}% - {msg}")
                    .unwrap(),
            );
            pb
        } else {
            ProgressBar::hidden()
        };

        loop {
            let mut client = match ClientSocket::connect(&socket_path).await {
                Ok(c) => c,
                Err(e) => {
                    pb.finish_and_clear();
                    println!("Error connecting to server: {}", e);
                    break;
                }
            };

            match client.request(&request).await {
                Ok(Message::GetIssuesResult(result)) => {
                    if result.analysis_complete {
                        pb.finish_and_clear();

                        for issue in result.issues {
                            *had_error = true;
                            println!(
                                "{}:{}:{} - {} - {}",
                                issue.file_path,
                                issue.start_line,
                                issue.start_column,
                                issue.kind,
                                issue.description
                            );
                        }

                        if !*had_error {
                            tty_println!("\nNo issues reported!\n");
                        }

                        tty_println!("\nAnalyzed {} files", result.files_analyzed);
                        return;
                    } else {
                        let is_analyzing = result.files_analyzed > 0;
                        if is_analyzing {
                            pb.set_length(result.total_files_to_analyze.max(1) as u64);
                            pb.set_position(result.files_analyzed as u64);
                            pb.set_message(format!(
                                "{} ({}/{} files)",
                                result.phase, result.files_analyzed, result.total_files_to_analyze
                            ));
                        } else {
                            pb.set_length(result.total_files_to_scan.max(1) as u64);
                            pb.set_position(result.files_scanned as u64);
                            pb.set_message(format!(
                                "{} ({}/{} files)",
                                result.phase, result.files_scanned, result.total_files_to_scan
                            ));
                        }

                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
                Ok(Message::Error(err)) => {
                    pb.finish_and_clear();
                    println!("Server error: {} - {}", err.code as u32, err.message);
                    exit(1);
                }
                Ok(_) => {
                    pb.finish_and_clear();
                    println!("Unexpected response from server");
                    exit(1);
                }
                Err(e) => {
                    pb.finish_and_clear();
                    println!("Error communicating with server: {}", e);
                    break;
                }
            }
        }
    }

    let output_file = sub_matches.value_of("output").map(|f| f.to_string());
    let output_format = sub_matches.value_of("json-format").map(|f| f.to_string());

    let ignored = sub_matches
        .values_of("ignore")
        .map(|values| values.map(|f| f.to_string()).collect::<FxHashSet<_>>());
    let mut find_unused_expressions = sub_matches.is_present("find-unused-expressions");
    let find_unused_definitions = sub_matches.is_present("find-unused-definitions");
    let show_mixed_function_counts = sub_matches.is_present("show-mixed-function-counts");
    let print_symbol_usages = sub_matches.is_present("print-symbol-usages");
    let ignore_mixed_issues = sub_matches.is_present("ignore-mixed-issues");
    let show_issue_stats = sub_matches.is_present("show-issue-stats");
    let do_ast_diff = sub_matches.is_present("diff");

    let mut issue_kinds_filter = FxHashSet::default();

    let filter_issue_strings = sub_matches
        .values_of("show-issue")
        .map(|values| values.collect::<FxHashSet<_>>());

    let show_all_issues = sub_matches.is_present("all-issues");

    if let Some(filter_issue_strings) = filter_issue_strings {
        for filter_issue_string in filter_issue_strings {
            if let Ok(issue_kind) =
                IssueKind::from_str_custom(filter_issue_string, &all_custom_issues)
            {
                if issue_kind.requires_dataflow_analysis() {
                    find_unused_expressions = true;
                }
                issue_kinds_filter.insert(issue_kind);
            } else {
                println!("Invalid issue type {}", filter_issue_string);
                exit(1);
            }
        }
    }

    let mut config = config::Config::new(root_dir.to_string(), all_custom_issues);
    config.find_unused_expressions = find_unused_expressions;
    config.find_unused_definitions = find_unused_definitions;
    config.ignore_mixed_issues = ignore_mixed_issues;
    config.ast_diff = do_ast_diff;

    analysis_hooks.retain(|h| h.run_in_ide());

    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }

    if !issue_kinds_filter.is_empty() {
        config.allowed_issues = Some(issue_kinds_filter);
    } else if show_all_issues {
        config.allowed_issues = None;
    }

    let root_dir = config.root_dir.clone();

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        filter,
        ignored,
        Arc::new(config),
        if sub_matches.is_present("no-cache") {
            None
        } else {
            Some(&cache_dir)
        },
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
        if print_symbol_usages {
            let interner = &successful_run_data.interner;
            let codebase = &successful_run_data.codebase;

            let mut symbol_definitions: BTreeMap<String, String> = BTreeMap::new();
            let mut file_references: BTreeMap<String, Vec<String>> = BTreeMap::new();

            for (symbol_id, file_paths) in &codebase.classlike_infos_defs {
                let symbol_name = interner.lookup(symbol_id).to_string();
                if let Some(file_path) = file_paths.first() {
                    symbol_definitions.insert(
                        symbol_name,
                        file_path.get_relative_path(interner, &root_dir),
                    );
                }
            }

            for ((class_or_function_id, member_id), file_paths) in &codebase.functionlike_infos_defs
            {
                let symbol_name = if *member_id == hakana_str::StrId::EMPTY {
                    interner.lookup(class_or_function_id).to_string()
                } else {
                    format!(
                        "{}::{}",
                        interner.lookup(class_or_function_id),
                        interner.lookup(member_id)
                    )
                };
                if let Some(file_path) = file_paths.first() {
                    symbol_definitions.insert(
                        symbol_name,
                        file_path.get_relative_path(interner, &root_dir),
                    );
                }
            }

            for (symbol_id, file_paths) in &codebase.type_definitions_defs {
                let symbol_name = interner.lookup(symbol_id).to_string();
                if let Some(file_path) = file_paths.first() {
                    symbol_definitions.insert(
                        symbol_name,
                        file_path.get_relative_path(interner, &root_dir),
                    );
                }
            }

            for (symbol_id, file_paths) in &codebase.constant_infos_defs {
                let symbol_name = interner.lookup(symbol_id).to_string();
                if let Some(file_path) = file_paths.first() {
                    symbol_definitions.insert(
                        symbol_name,
                        file_path.get_relative_path(interner, &root_dir),
                    );
                }
            }

            for ((referencing_symbol, referencing_member), referenced_set) in analysis_result
                .symbol_references
                .symbol_references_to_symbols
                .iter()
                .chain(
                    analysis_result
                        .symbol_references
                        .symbol_references_to_symbols_in_signature
                        .iter(),
                )
            {
                let referencing_name = if *referencing_member == hakana_str::StrId::EMPTY {
                    interner.lookup(referencing_symbol).to_string()
                } else {
                    format!(
                        "{}::{}",
                        interner.lookup(referencing_symbol),
                        interner.lookup(referencing_member)
                    )
                };

                let file_path = symbol_definitions.get(&referencing_name).cloned();
                if let Some(file_path) = file_path {
                    let refs = file_references.entry(file_path).or_default();
                    for (ref_symbol, ref_member) in referenced_set {
                        let ref_name = if *ref_member == hakana_str::StrId::EMPTY {
                            interner.lookup(ref_symbol).to_string()
                        } else {
                            format!(
                                "{}::{}",
                                interner.lookup(ref_symbol),
                                interner.lookup(ref_member)
                            )
                        };
                        if !refs.contains(&ref_name) {
                            refs.push(ref_name);
                        }
                    }
                }
            }

            for refs in file_references.values_mut() {
                refs.sort();
            }

            let output = json!({
                "symbol_definitions": symbol_definitions,
                "file_references": file_references,
            });

            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            return;
        }

        for (file_path, issues) in
            analysis_result.get_all_issues(&successful_run_data.interner, &root_dir, true)
        {
            for issue in issues {
                *had_error = true;
                println!("{}", issue.format(&file_path));
            }
        }

        if !*had_error {
            tty_println!("\nNo issues reported!\n");
        }

        if let Some(output_file) = output_file {
            write_analysis_output_files(
                output_file,
                output_format,
                cwd,
                &analysis_result,
                &successful_run_data.interner,
            );
        }

        if show_issue_stats {
            let mut issues_by_kind = analysis_result
                .issue_counts
                .into_iter()
                .collect::<IndexMap<_, _>>();
            issues_by_kind.sort_by(|_, a, _, b| b.cmp(a));

            for (issue, count) in issues_by_kind {
                println!("{}\t{}", issue.to_string(), count);
            }
        }

        if show_mixed_function_counts {
            let mut mixed_sources = analysis_result
                .mixed_source_counts
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}\t{}",
                        k.to_string(&successful_run_data.interner),
                        v.len()
                    )
                })
                .collect::<Vec<_>>();

            mixed_sources.sort();

            println!("{}", mixed_sources.join("\n"));
        }
    }
}
