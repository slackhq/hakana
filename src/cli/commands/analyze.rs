use clap::Args;
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

use super::CommonArgs;
use crate::helpers::write_analysis_output_files;

#[derive(Args)]
pub struct AnalyzeArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Filter the files that are analyzed
    #[arg(long, value_name = "PATH")]
    pub filter: Option<String>,

    /// Ignore certain files during analysis
    #[arg(long, value_name = "PATH", num_args = 1..)]
    pub ignore: Option<Vec<String>>,

    /// No-op: unused expressions are always reported
    #[arg(long, hide = true)]
    pub find_unused_expressions: bool,

    /// No-op: unused definitions are always reported
    #[arg(long, hide = true)]
    pub find_unused_definitions: bool,

    /// Only output issues of this/these type(s)
    #[arg(long, value_name = "PATH", num_args = 1..)]
    pub show_issue: Option<Vec<String>>,

    /// Show all issues
    #[arg(long)]
    pub all_issues: bool,

    /// Ignore mixed/any issues
    #[arg(long)]
    pub ignore_mixed_issues: bool,

    /// Show which functions we lead to mixed types
    #[arg(long)]
    pub show_mixed_function_counts: bool,

    /// Output a JSON map of symbol definitions and usages
    #[arg(long)]
    pub print_symbol_usages: bool,

    /// If set, timing info will be displayed
    #[arg(long)]
    pub show_timing: bool,

    /// Whether to ignore the cache
    #[arg(long)]
    pub no_cache: bool,

    /// Whether perform AST-based diffing to speed up execution
    #[arg(long)]
    pub diff: bool,

    /// Output a summary of issue counts
    #[arg(long)]
    pub show_issue_stats: bool,

    /// File to save output to
    #[arg(long, value_name = "PATH")]
    pub output: Option<String>,

    /// Format for JSON output. Options: checkpoint (default), full, hh_client
    #[arg(long, value_name = "FORMAT")]
    pub json_format: Option<String>,

    /// Run analysis directly without connecting to server (default for CI)
    #[arg(long)]
    pub standalone: bool,

    /// Use server mode: connect to existing server or spawn one if needed
    #[arg(long)]
    pub with_server: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn handle(
    args: &AnalyzeArgs,
    all_custom_issues: FxHashSet<String>,
    root_dir: &String,
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

    let filter = args.filter.clone();
    let standalone = args.standalone;
    let with_server = args.with_server;

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
        let request = Message::GetIssues(GetIssuesRequest {
            filter: filter.clone(),
            find_unused_expressions: true,
            find_unused_definitions: true,
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

    let output_file = args.output.clone();
    let output_format = args.json_format.clone();

    let ignored = args
        .ignore
        .as_ref()
        .map(|values| values.iter().cloned().collect::<FxHashSet<_>>());
    let show_mixed_function_counts = args.show_mixed_function_counts;
    let print_symbol_usages = args.print_symbol_usages;
    let ignore_mixed_issues = args.ignore_mixed_issues;
    let show_issue_stats = args.show_issue_stats;
    let do_ast_diff = args.diff;

    let mut issue_kinds_filter = FxHashSet::default();

    let show_all_issues = args.all_issues;

    if let Some(filter_issue_strings) = &args.show_issue {
        for filter_issue_string in filter_issue_strings {
            if let Ok(issue_kind) =
                IssueKind::from_str_custom(filter_issue_string, &all_custom_issues)
            {
                issue_kinds_filter.insert(issue_kind);
            } else {
                println!("Invalid issue type {}", filter_issue_string);
                exit(1);
            }
        }
    }

    let mut config = config::Config::new(root_dir.to_string(), all_custom_issues);
    config.find_unused_expressions = true;
    config.find_unused_definitions = true;
    config.ignore_mixed_issues = ignore_mixed_issues;
    config.ast_diff = do_ast_diff;

    analysis_hooks.retain(|h| h.run_in_ide());

    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists()
        && let Err(error) = config.update_from_file(root_dir, config_path, &mut interner)
    {
        println!("Invalid config: {}", error);
        exit(1);
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
        if args.no_cache {
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
