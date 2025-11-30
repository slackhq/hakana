//! Request handlers for the hakana server.

use hakana_protocol::{
    AckResponse, AnalyzeRequest, AnalyzeResponse, ErrorCode, ErrorResponse, FileChange,
    FindReferencesRequest, FindReferencesResponse, GotoDefinitionRequest, GotoDefinitionResponse,
    Message, ProtocolIssue, SecurityCheckRequest, SecurityCheckResponse,
    StatusResponse,
};
use crate::{ServerConfig, ServerState};
use hakana_analyzer::config::Config;
use hakana_code_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_logger::Logger;
use hakana_orchestrator::file::FileStatus;
use hakana_protocol::FileChangeStatus;
use hakana_str::Interner;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Handles incoming requests.
pub struct RequestHandler<'a> {
    config: &'a ServerConfig,
    state: &'a mut ServerState,
    logger: &'a Arc<Logger>,
    start_time: Instant,
}

impl<'a> RequestHandler<'a> {
    pub fn new(
        config: &'a ServerConfig,
        state: &'a mut ServerState,
        logger: &'a Arc<Logger>,
        start_time: Instant,
    ) -> Self {
        Self {
            config,
            state,
            logger,
            start_time,
        }
    }

    /// Handle an analyze request.
    pub fn handle_analyze(mut self, req: AnalyzeRequest) -> Message {
        if self.state.is_analysis_in_progress() {
            return Message::Error(ErrorResponse {
                code: ErrorCode::ServerBusy,
                message: "Analysis already in progress".to_string(),
            });
        }

        self.state.set_analysis_in_progress(true);

        let result = self.do_analyze(req);

        self.state.set_analysis_in_progress(false);

        result
    }

    fn do_analyze(&mut self, req: AnalyzeRequest) -> Message {
        let scan_start = Instant::now();

        // Build config
        let mut analysis_config = Config::new(self.config.root_dir.clone(), FxHashSet::default());
        analysis_config.find_unused_expressions = req.find_unused_expressions;
        analysis_config.find_unused_definitions = req.find_unused_definitions;

        // Apply allowed issues filter
        if let Some(allowed) = req.allowed_issues {
            let mut issue_filter = FxHashSet::default();
            for issue_name in allowed {
                if let Ok(issue_kind) =
                    hakana_code_info::issue::IssueKind::from_str_custom(&issue_name, &FxHashSet::default())
                {
                    issue_filter.insert(issue_kind);
                }
            }
            if !issue_filter.is_empty() {
                analysis_config.allowed_issues = Some(issue_filter);
            }
        }

        // Load config from file if specified
        let mut interner = Interner::default();
        if let Some(config_path) = &self.config.config_path {
            let path = Path::new(config_path);
            if path.exists() {
                let _ = analysis_config.update_from_file(&self.config.root_dir, path, &mut interner);
            }
        }

        // Note: Plugins are currently not supported in server mode
        // due to the Config using Box<dyn CustomHook> rather than Arc
        // TODO: Refactor Config to use Arc<dyn CustomHook> for server support

        // Convert file changes to the expected format
        let language_server_changes = if !req.file_changes.is_empty() {
            let mut changes = FxHashMap::default();
            for change in req.file_changes {
                let status = match change.status {
                    FileChangeStatus::Added => {
                        FileStatus::Added(0, 0)
                    }
                    FileChangeStatus::Modified => {
                        FileStatus::Modified(0, 0)
                    }
                    FileChangeStatus::Deleted => FileStatus::Deleted,
                };
                changes.insert(change.path, status);
            }
            Some(changes)
        } else {
            None
        };

        // Get previous state if available and not forcing full rescan
        let (previous_scan_data, previous_analysis_result) = if !req.full_rescan {
            (
                self.state.scan_data.take(),
                self.state.analysis_result.take(),
            )
        } else {
            (None, None)
        };

        let scan_elapsed = scan_start.elapsed();

        let analysis_start = Instant::now();

        // Run analysis
        let result = hakana_orchestrator::scan_and_analyze(
            Vec::new(),
            req.filter,
            None,
            Arc::new(analysis_config),
            None, // No cache dir for server mode
            self.config.threads,
            self.logger.clone(),
            &self.config.header,
            interner,
            previous_scan_data,
            previous_analysis_result,
            language_server_changes,
            || {},
        );

        let analysis_elapsed = analysis_start.elapsed();

        match result {
            Ok((analysis_result, scan_data)) => {
                // Collect issues
                let mut issues = Vec::new();
                let mut files_with_issues = FxHashSet::default();

                for (file_path, file_issues) in &analysis_result.emitted_issues {
                    let file_path_str = file_path.get_relative_path(
                        &scan_data.interner,
                        &self.config.root_dir,
                    );
                    files_with_issues.insert(file_path_str.clone());

                    for issue in file_issues {
                        issues.push(ProtocolIssue::from_issue(issue, &file_path_str));
                    }
                }

                let files_analyzed = scan_data.codebase.files.len() as u32;

                // Update server state
                self.state.update_state(scan_data, analysis_result);

                Message::AnalyzeResult(AnalyzeResponse {
                    success: true,
                    issues,
                    scan_time_ms: scan_elapsed.as_millis() as u64,
                    analysis_time_ms: analysis_elapsed.as_millis() as u64,
                    files_analyzed,
                    files_with_issues: files_with_issues.len() as u32,
                })
            }
            Err(e) => Message::Error(ErrorResponse {
                code: ErrorCode::AnalysisFailed,
                message: format!("Analysis failed: {}", e),
            }),
        }
    }

    /// Handle a security check request.
    pub fn handle_security_check(mut self, req: SecurityCheckRequest) -> Message {
        if self.state.is_analysis_in_progress() {
            return Message::Error(ErrorResponse {
                code: ErrorCode::ServerBusy,
                message: "Analysis already in progress".to_string(),
            });
        }

        self.state.set_analysis_in_progress(true);

        let result = self.do_security_check(req);

        self.state.set_analysis_in_progress(false);

        result
    }

    fn do_security_check(&mut self, req: SecurityCheckRequest) -> Message {
        let analysis_start = Instant::now();

        // Build config for taint analysis
        let mut analysis_config = Config::new(self.config.root_dir.clone(), FxHashSet::default());
        analysis_config.graph_kind = GraphKind::WholeProgram(WholeProgramKind::Taint);

        if let Some(max_depth) = req.max_depth {
            analysis_config.security_config.max_depth = max_depth as u8;
        }

        // Load config from file if specified
        let mut interner = Interner::default();
        if let Some(config_path) = &self.config.config_path {
            let path = Path::new(config_path);
            if path.exists() {
                let _ = analysis_config.update_from_file(&self.config.root_dir, path, &mut interner);
            }
        }

        // Run security analysis
        let result = hakana_orchestrator::scan_and_analyze(
            Vec::new(),
            req.filter,
            None,
            Arc::new(analysis_config),
            None,
            self.config.threads,
            self.logger.clone(),
            &self.config.header,
            interner,
            None,
            None,
            None,
            || {},
        );

        let analysis_elapsed = analysis_start.elapsed();

        match result {
            Ok((analysis_result, scan_data)) => {
                let mut issues = Vec::new();
                let mut taint_flows = 0u32;

                for (file_path, file_issues) in &analysis_result.emitted_issues {
                    let file_path_str = file_path.get_relative_path(
                        &scan_data.interner,
                        &self.config.root_dir,
                    );

                    for issue in file_issues {
                        // In taint analysis mode, all issues are security-related
                        taint_flows += 1;
                        issues.push(ProtocolIssue::from_issue(issue, &file_path_str));
                    }
                }

                Message::SecurityCheckResult(SecurityCheckResponse {
                    success: true,
                    issues,
                    taint_flows_found: taint_flows,
                    analysis_time_ms: analysis_elapsed.as_millis() as u64,
                })
            }
            Err(e) => Message::Error(ErrorResponse {
                code: ErrorCode::AnalysisFailed,
                message: format!("Security check failed: {}", e),
            }),
        }
    }

    /// Handle a goto-definition request.
    pub fn handle_goto_definition(self, req: GotoDefinitionRequest) -> Message {
        // TODO: Implement using cached scan data
        // For now, return not found
        let _ = req;

        Message::GotoDefinitionResult(GotoDefinitionResponse {
            found: false,
            file_path: None,
            start_line: None,
            start_column: None,
            end_line: None,
            end_column: None,
        })
    }

    /// Handle a find-references request.
    pub fn handle_find_references(self, req: FindReferencesRequest) -> Message {
        // TODO: Implement using cached scan data
        // For now, return empty list
        let _ = req;

        Message::FindReferencesResult(FindReferencesResponse {
            references: Vec::new(),
        })
    }

    /// Handle file changed notifications.
    pub fn handle_file_changed(self, changes: Vec<FileChange>) -> Message {
        // TODO: Track changes for incremental analysis
        self.logger.log_sync(&format!(
            "Received {} file change notification(s)",
            changes.len()
        ));

        Message::Ack(AckResponse)
    }

    /// Handle status request.
    pub fn handle_status(self) -> Message {
        let uptime = self.start_time.elapsed();

        Message::StatusResult(StatusResponse {
            ready: !self.state.is_analysis_in_progress(),
            files_count: self.state.files_count(),
            symbols_count: self.state.symbols_count(),
            uptime_secs: uptime.as_secs(),
            analysis_in_progress: self.state.is_analysis_in_progress(),
            pending_requests: self.state.pending_requests(),
            project_root: self.config.root_dir.clone(),
        })
    }

    /// Handle shutdown request.
    pub fn handle_shutdown(self) -> Message {
        self.logger.log_sync("Shutdown requested");
        self.state.set_shutting_down();
        Message::Ack(AckResponse)
    }
}
