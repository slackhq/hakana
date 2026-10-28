use super::CommonArgs;
use clap::Args;
use hakana_analyzer::config;
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

#[derive(Args)]
pub struct CyclomaticComplexityArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Filter the files that are analyzed (glob patterns)
    #[arg(long, value_name = "PATH", num_args = 1..)]
    pub filter: Option<Vec<String>>,

    /// Complexity threshold to report (default: 10)
    #[arg(long, value_name = "NUM")]
    pub threshold: Option<u32>,

    /// File to save JSON output to
    #[arg(long, value_name = "PATH")]
    pub output: Option<String>,
}

pub fn handle(
    args: &CyclomaticComplexityArgs,
    all_custom_issues: FxHashSet<String>,
    root_dir: &str,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
    header: &str,
    had_error: &mut bool,
) {
    let mut config = config::Config::new(root_dir.to_string(), all_custom_issues);

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists()
        && let Err(error) = config.update_from_file(cwd, config_path, &mut interner)
    {
        println!("Invalid config: {}", error);
        std::process::exit(1);
    }
    config.allowed_issues = None;

    config.analyze_cyclomatic_complexity = true;

    config.cyclomatic_complexity_file_patterns = args
        .filter
        .as_ref()
        .map(|values| {
            values
                .iter()
                .map(|f| {
                    glob::Pattern::new(&format!("{}/{}", cwd, f)).expect("invalid filter pattern")
                })
                .collect()
        })
        .unwrap_or_default();

    let threshold = args.threshold.unwrap_or(10);
    config.cyclomatic_complexity_threshold = threshold;

    let output_file = args.output.clone();

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
        let mut results = analysis_result.cyclomatic_complexity;
        results.sort_by(|a, b| b.cmp(&successful_run_data.interner, a));

        let total_functions = successful_run_data.codebase.functionlike_infos.len();

        if results.is_empty() {
            tty_println!("No functions exceed complexity threshold of {}", threshold);
        } else {
            tty_println!(
                "{}/{} functions exceed the complexity threshold:\n",
                results.len(),
                total_functions
            );

            for function_complexity in &results {
                println!(
                    "{}",
                    function_complexity.to_string(&successful_run_data.interner)
                );
            }
        }

        if let Some(output_path_str) = output_file {
            let report = json!({
                "threshold": threshold,
                "total_functions": total_functions,
                "functions_over_threshold": results.len(),
                "results": results.iter().map(|c| c.to_json(&successful_run_data.interner)).collect::<Vec<_>>(),
            });

            let json = serde_json::to_string_pretty(&report).unwrap();
            let mut out = fs::File::create(Path::new(&output_path_str)).unwrap();
            write!(out, "{}", json).unwrap();
            tty_println!("\nResults written to: {}", output_path_str);
        }

        if !results.is_empty() {
            *had_error = true;
        }
    }
}
