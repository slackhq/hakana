use std::{path::Path, process::exit};

use hakana_reflection_info::{
    data_flow::graph::GraphKind,
    issue::{Issue, IssueKind},
    taint::SinkType,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::custom_hook::CustomHook;

pub mod json_config;

#[derive(Copy, Clone)]
pub enum Verbosity {
    Quiet,
    Simple,
    Timing,
    Debugging,
    DebuggingByLine,
}

pub struct Config {
    pub migration_symbols: FxHashSet<(String, String)>,
    pub find_unused_expressions: bool,
    pub find_unused_definitions: bool,
    pub allowed_issues: Option<FxHashSet<IssueKind>>,
    pub allowable_issues: Option<FxHashSet<IssueKind>>,
    pub issues_to_fix: FxHashSet<IssueKind>,
    pub graph_kind: GraphKind,
    pub ignore_files: Vec<String>,
    pub test_files: Vec<String>,
    pub ignore_issue_files: FxHashMap<IssueKind, Vec<String>>,
    pub ignore_all_issues_in_files: Vec<String>,
    pub security_config: SecurityConfig,
    pub root_dir: String,
    pub hooks: Vec<Box<dyn CustomHook>>,
    pub ignore_mixed_issues: bool,
    pub add_fixmes: bool,
    pub remove_fixmes: bool,
    pub all_custom_issues: FxHashSet<String>,
    pub ast_diff: bool,
}

#[derive(Clone, Debug)]
pub struct SecurityConfig {
    ignore_files: Vec<String>,
    ignore_sink_files: FxHashMap<String, Vec<String>>,
    pub max_depth: u8,
}

impl SecurityConfig {
    pub fn new() -> Self {
        Self {
            ignore_files: Vec::new(),
            ignore_sink_files: FxHashMap::default(),
            max_depth: 40,
        }
    }
}

impl Config {
    pub fn new(root_dir: String, all_custom_issues: FxHashSet<String>) -> Self {
        Self {
            root_dir,
            find_unused_expressions: false,
            find_unused_definitions: false,
            ignore_mixed_issues: false,
            allowed_issues: None,
            migration_symbols: FxHashSet::default(),
            graph_kind: GraphKind::FunctionBody,
            ignore_files: Vec::new(),
            test_files: Vec::new(),
            ignore_issue_files: FxHashMap::default(),
            ignore_all_issues_in_files: vec![],
            security_config: SecurityConfig::new(),
            issues_to_fix: FxHashSet::default(),
            hooks: vec![],
            add_fixmes: false,
            remove_fixmes: false,
            all_custom_issues,
            allowable_issues: None,
            ast_diff: false,
        }
    }

    pub fn update_from_file(&mut self, cwd: &String, config_path: &Path) {
        println!("Loading config from {:?}", config_path);
        let json_config = json_config::read_from_file(config_path).unwrap_or_else(|e| {
            println!("{}", e.to_string());
            exit(1)
        });

        self.ignore_files = json_config
            .ignore_files
            .into_iter()
            .map(|v| format!("{}/{}", cwd, v))
            .collect();

        self.test_files = json_config
            .test_files
            .into_iter()
            .map(|v| format!("{}/{}", cwd, v))
            .collect();

        self.ignore_issue_files = json_config
            .ignore_issue_files
            .iter()
            .filter(|(k, _)| *k != "*")
            .map(|(k, v)| {
                (
                    IssueKind::from_str_custom(k.as_str(), &self.all_custom_issues).unwrap(),
                    v.into_iter().map(|v| format!("{}/{}", cwd, v)).collect(),
                )
            })
            .collect();

        if let Some(v) = json_config.ignore_issue_files.get("*") {
            self.ignore_all_issues_in_files =
                v.into_iter().map(|v| format!("{}/{}", cwd, v)).collect();
        }

        self.allowed_issues = if json_config.allowed_issues.is_empty() {
            None
        } else {
            Some(
                json_config
                    .allowed_issues
                    .into_iter()
                    .map(|s| {
                        IssueKind::from_str_custom(s.as_str(), &self.all_custom_issues).unwrap()
                    })
                    .collect::<FxHashSet<_>>(),
            )
        };

        self.security_config.ignore_files = json_config
            .security_analysis
            .ignore_files
            .into_iter()
            .map(|v| format!("{}/{}", cwd, v))
            .collect();
        self.security_config.ignore_sink_files = json_config
            .security_analysis
            .ignore_sink_files
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|v| format!("{}/{}", cwd, v)).collect()))
            .collect();
    }

    pub fn can_add_issue(&self, issue: &Issue) -> bool {
        if let Some(issue_filter) = &self.allowed_issues {
            if !issue_filter.contains(&issue.kind) {
                return false;
            }
        }

        true
    }

    pub fn allow_issues_in_file(&self, file: &str) -> bool {
        for ignore_file_path in &self.ignore_all_issues_in_files {
            if glob::Pattern::new(ignore_file_path).unwrap().matches(&file) {
                return false;
            }
        }

        true
    }

    pub fn allow_issue_kind_in_file(&self, issue_kind: &IssueKind, file: &str) -> bool {
        if let Some(issue_entries) = self.ignore_issue_files.get(&issue_kind) {
            for ignore_file_path in issue_entries {
                if glob::Pattern::new(ignore_file_path).unwrap().matches(&file) {
                    return false;
                }
            }
        }

        true
    }

    pub fn allow_taints_in_file(&self, file: &String) -> bool {
        for ignore_file_path in &self.security_config.ignore_files {
            if glob::Pattern::new(ignore_file_path).unwrap().matches(file) {
                return false;
            }
        }

        true
    }

    pub fn allow_sink_in_file(&self, taint_type: &SinkType, file: &str) -> bool {
        let str_type = taint_type.to_string();

        if let Some(issue_entries) = self.security_config.ignore_sink_files.get(&str_type) {
            for ignore_file_path in issue_entries {
                if glob::Pattern::new(ignore_file_path).unwrap().matches(&file) {
                    return false;
                }
            }
        }

        true
    }
}
