use clap::Args;
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;

use super::CommonArgs;
use crate::helpers::write_analysis_output_files;

#[derive(Args)]
pub struct SecurityCheckArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Length of the longest allowable path — defaults to 40, and overrides config file value
    #[arg(long, value_name = "PATH", value_parser = clap::value_parser!(u8).range(1..))]
    pub max_depth: Option<u8>,

    /// File to save output to
    #[arg(long, value_name = "PATH")]
    pub output: Option<String>,
}

pub fn handle(
    cwd: &String,
    all_custom_issues: FxHashSet<String>,
    config_path: Option<&Path>,
    args: &SecurityCheckArgs,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    threads: u8,
    show_progress: bool,
    header: &str,
    had_error: &mut bool,
) {
    let analysis_root = args.common.root.as_deref().unwrap_or(cwd).to_string();
    let mut config = config::Config::new(analysis_root.clone(), all_custom_issues);
    config.graph_kind = GraphKind::WholeProgram(WholeProgramKind::Taint);

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists()
        && let Err(error) = config.update_from_file(&analysis_root, config_path, &mut interner)
    {
        println!("Invalid config: {}", error);
        std::process::exit(1);
    }
    config.allowed_issues = None;

    let output_file = args.output.clone();

    config.security_config.max_depth = args.max_depth.unwrap_or(config.security_config.max_depth);

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

    let (analysis_result, successful_run_data) = match result {
        Ok(result) => result,
        Err(error) => {
            *had_error = true;
            eprintln!("Security analysis failed: {error}");
            return;
        }
    };
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
