use clap::Args;
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::issue::IssueKind;
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;

use super::CommonArgs;
use crate::helpers::update_files;

#[derive(Args)]
pub struct FixArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// The issue to fix
    #[arg(long, value_name = "PATH")]
    pub issue: String,

    /// Filter the files that are fixed
    #[arg(long, value_name = "PATH")]
    pub filter: Option<String>,

    /// JSON array of Function::$parameter or Class::method::$parameter names to fix
    #[arg(long, value_name = "PATH")]
    pub parameter_allowlist: Option<String>,
}

pub fn handle(
    args: &FixArgs,
    all_custom_issues: FxHashSet<String>,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    root_dir: String,
    config_path: Option<&Path>,
    cwd: String,
    threads: u8,
    show_progress: bool,
    header: &str,
) {
    let issue_name = args.issue.clone();
    let issue_kind = IssueKind::from_str_custom(&issue_name, &all_custom_issues).unwrap();

    if !matches!(issue_kind, IssueKind::CustomIssue(..)) && !issue_kind.has_autofix() {
        println!("Issue type {} does not support autofixing", issue_kind);
        exit(1);
    }

    let filter = args.filter.clone();

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);
    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();

    config.find_unused_expressions = issue_kind.requires_dataflow_analysis();
    config.find_unused_definitions = issue_kind.is_unused_definition();
    if let Some(path) = args.parameter_allowlist.as_deref() {
        if issue_kind != IssueKind::UnusedOptionalParameter {
            eprintln!("--parameter-allowlist requires --issue UnusedOptionalParameter");
            exit(1);
        }
        config.parameter_fix_allowlist = Some(
            std::fs::read_to_string(path)
                .map_err(|error| error.to_string())
                .and_then(|json| serde_json::from_str(&json).map_err(|error| error.to_string()))
                .unwrap_or_else(|error| {
                    eprintln!("Invalid parameter allowlist: {error}");
                    exit(1);
                }),
        );
    }
    config.issues_to_fix.insert(issue_kind);

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists()
        && let Err(error) = config.update_from_file(&cwd, config_path, &mut interner)
    {
        println!("Invalid config: {}", error);
        exit(1);
    }

    config.allowed_issues = None;

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

    if let Ok((mut analysis_result, successfull_run_data)) = result {
        update_files(
            &mut analysis_result,
            &root_dir,
            &successfull_run_data.interner,
        );
    }
}
