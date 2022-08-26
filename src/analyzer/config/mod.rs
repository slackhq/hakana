use std::{path::Path, process::exit};

use hakana_reflection_info::{
    data_flow::graph::GraphKind,
    issue::{Issue, IssueKind},
    taint::SinkType,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::custom_hook::CustomHook;

pub mod json_config;

pub struct Config {
    pub migration_symbols: FxHashSet<(String, String)>,
    pub find_unused_expressions: bool,
    pub find_unused_definitions: bool,
    pub issue_filter: Option<FxHashSet<IssueKind>>,
    pub issues_to_fix: FxHashSet<IssueKind>,
    pub graph_kind: GraphKind,
    pub ignore_files: Vec<String>,
    pub ignore_issue_files: FxHashMap<String, Vec<String>>,
    pub security_config: SecurityConfig,
    pub root_dir: String,
    pub hooks: Vec<Box<dyn CustomHook>>,
    pub ignore_mixed_issues: bool,
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
            max_depth: 20,
        }
    }
}

impl Config {
    pub fn new(root_dir: String) -> Self {
        Self {
            root_dir,
            find_unused_expressions: false,
            find_unused_definitions: false,
            ignore_mixed_issues: false,
            issue_filter: None,
            migration_symbols: FxHashSet::default(),
            graph_kind: GraphKind::FunctionBody,
            ignore_files: Vec::new(),
            ignore_issue_files: FxHashMap::default(),
            security_config: SecurityConfig::new(),
            issues_to_fix: FxHashSet::default(),
            hooks: vec![],
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
        self.ignore_issue_files = json_config
            .ignore_issue_files
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|v| format!("{}/{}", cwd, v)).collect()))
            .collect();

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
        if let Some(issue_filter) = &self.issue_filter {
            if !issue_filter.contains(&issue.kind) {
                return false;
            }
        }

        true
    }

    pub fn allow_issue_kind_in_file(&self, issue_kind: &IssueKind, file: &String) -> bool {
        let str_issue = issue_kind.to_string();
        let file = format!("{}/{}", self.root_dir, file);

        if let Some(issue_entries) = self.ignore_issue_files.get(&str_issue) {
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

    pub fn allow_sink_in_file(&self, taint_type: &SinkType, file: &String) -> bool {
        let str_type = taint_type.to_string();
        let file = format!("{}/{}", self.root_dir, file);

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
