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
pub struct AddFixmesArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// The issue or issues to add fixmes for
    #[arg(long, value_name = "PATH", required = true, num_args = 1..)]
    pub issue: Vec<String>,

    /// Filter the files that have added fixmes
    #[arg(long, value_name = "PATH")]
    pub filter: Option<String>,
}

pub fn handle(
    args: &AddFixmesArgs,
    all_custom_issues: FxHashSet<String>,
    root_dir: &String,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
    header: &str,
) {
    let mut issue_kinds_filter = FxHashSet::default();

    for filter_issue_string in &args.issue {
        if let Ok(issue_kind) = IssueKind::from_str_custom(filter_issue_string, &all_custom_issues)
        {
            issue_kinds_filter.insert(issue_kind);
        } else {
            println!("Invalid issue type {}", filter_issue_string);
            exit(1);
        }
    }

    let filter = args.filter.clone();

    let mut config = config::Config::new(root_dir.clone(), all_custom_issues);

    config.issues_to_fix.extend(issue_kinds_filter);
    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();
    config.find_unused_expressions = true;
    config.find_unused_definitions = true;

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists()
        && let Err(error) = config.update_from_file(cwd, config_path, &mut interner)
    {
        println!("Invalid config: {}", error);
        exit(1);
    }

    config.allowed_issues = None;

    config.add_fixmes = true;

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

    if let Ok((mut analysis_result, successful_run_data)) = result {
        update_files(
            &mut analysis_result,
            root_dir,
            &successful_run_data.interner,
        );
    }
}
