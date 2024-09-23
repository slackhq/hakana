use std::{error::Error, path::Path};

use hakana_code_info::{
    data_flow::{graph::GraphKind, tainted_node::TaintedNode},
    issue::{Issue, IssueKind},
    taint::{SinkType, SourceType},
};
use hakana_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::custom_hook::CustomHook;

pub mod json_config;

#[derive(Debug)]
pub struct Config {
    pub migration_symbols: FxHashMap<String, String>,
    pub in_migration: bool,
    pub in_codegen: bool,
    pub find_unused_expressions: bool,
    pub find_unused_definitions: bool,
    pub allowed_issues: Option<FxHashSet<IssueKind>>,
    pub issues_to_fix: FxHashSet<IssueKind>,
    pub graph_kind: GraphKind,
    pub ignore_files: Vec<String>,
    pub test_files: Vec<glob::Pattern>,
    pub ignore_issue_patterns: FxHashMap<IssueKind, Vec<glob::Pattern>>,
    pub ignore_all_issues_in_patterns: Vec<glob::Pattern>,
    pub banned_builtin_functions: FxHashMap<StrId, StrId>,
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
    ignore_patterns: Vec<glob::Pattern>,
    ignore_sink_files: FxHashMap<String, Vec<glob::Pattern>>,
    pub max_depth: u8,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityConfig {
    pub fn new() -> Self {
        Self {
            ignore_patterns: Vec::new(),
            ignore_sink_files: FxHashMap::default(),
            max_depth: 20,
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
            migration_symbols: FxHashMap::default(),
            graph_kind: GraphKind::FunctionBody,
            ignore_files: Vec::new(),
            test_files: Vec::new(),
            ignore_issue_patterns: FxHashMap::default(),
            ignore_all_issues_in_patterns: vec![],
            security_config: SecurityConfig::new(),
            issues_to_fix: FxHashSet::default(),
            hooks: vec![],
            add_fixmes: false,
            remove_fixmes: false,
            all_custom_issues,
            ast_diff: false,
            in_migration: false,
            in_codegen: false,
            banned_builtin_functions: FxHashMap::default(),
        }
    }

    pub fn update_from_file(
        &mut self,
        cwd: &String,
        config_path: &Path,
        interner: &mut Interner,
    ) -> Result<(), Box<dyn Error>> {
        let json_config = json_config::read_from_file(config_path)?;

        self.ignore_files = json_config
            .ignore_files
            .into_iter()
            .map(|v| format!("{}/{}", cwd, v))
            .collect();

        self.test_files = json_config
            .test_files
            .into_iter()
            .map(|v| glob::Pattern::new(&format!("{}/{}", cwd, v)).unwrap())
            .collect();

        self.ignore_issue_patterns = json_config
            .ignore_issue_files
            .iter()
            .filter(|(k, _)| *k != "*")
            .map(|(k, v)| {
                (
                    IssueKind::from_str_custom(k.as_str(), &self.all_custom_issues).unwrap(),
                    v.iter()
                        .map(|v| glob::Pattern::new(&format!("{}/{}", cwd, v)).unwrap())
                        .collect(),
                )
            })
            .collect();

        if let Some(v) = json_config.ignore_issue_files.get("*") {
            self.ignore_all_issues_in_patterns = v
                .iter()
                .map(|v| glob::Pattern::new(&format!("{}/{}", cwd, v)).unwrap())
                .collect();
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

        self.banned_builtin_functions = json_config
            .banned_builtin_functions
            .into_iter()
            .map(|(k, v)| (interner.intern(k), interner.intern(v)))
            .collect();

        self.security_config.ignore_patterns = json_config
            .security_analysis
            .ignore_files
            .into_iter()
            .map(|v| glob::Pattern::new(&format!("{}/{}", cwd, v)).unwrap())
            .collect();
        self.security_config.ignore_sink_files = json_config
            .security_analysis
            .ignore_sink_files
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    v.into_iter()
                        .map(|v| glob::Pattern::new(&format!("{}/{}", cwd, v)).unwrap())
                        .collect(),
                )
            })
            .collect();
        self.security_config.max_depth = json_config.security_analysis.max_depth;

        Ok(())
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
        for ignore_pattern in &self.ignore_all_issues_in_patterns {
            if ignore_pattern.matches(file) {
                return false;
            }
        }

        true
    }

    pub fn allow_issue_kind_in_file(&self, issue_kind: &IssueKind, file: &str) -> bool {
        if let Some(issue_entries) = self.ignore_issue_patterns.get(issue_kind) {
            for ignore_file_pattern in issue_entries {
                if ignore_file_pattern.matches(file) {
                    return false;
                }
            }
        }

        true
    }

    pub fn allow_taints_in_file(&self, file: &str) -> bool {
        for ignore_file_pattern in &self.security_config.ignore_patterns {
            if ignore_file_pattern.matches(file) {
                return false;
            }
        }

        true
    }

    pub fn allow_data_from_source_in_file(
        &self,
        source_type: &SourceType,
        sink_type: &SinkType,
        node: &TaintedNode,
        interner: &Interner,
    ) -> bool {
        let str_type = source_type.to_string() + " -> " + &sink_type.to_string();

        if let Some(ignore_patterns) = self.security_config.ignore_sink_files.get(&str_type) {
            let mut previous = node;

            loop {
                if let Some(pos) = &previous.pos {
                    for ignore_pattern in ignore_patterns {
                        if ignore_pattern.matches(interner.lookup(&pos.file_path.0)) {
                            return false;
                        }
                    }
                }

                if let Some(more_previous) = &previous.previous {
                    previous = more_previous;
                } else {
                    return true;
                }
            }
        }

        true
    }
}
