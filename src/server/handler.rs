//! Request handlers for the hakana server.

use hakana_protocol::{
    AckResponse, AnalyzeRequest, AnalyzeResponse, ErrorCode, ErrorResponse, FileChange,
    FindReferencesRequest, FindReferencesResponse, FindSymbolReferencesRequest,
    FindSymbolReferencesResponse, GotoDefinitionRequest, GotoDefinitionResponse,
    Message, ProtocolIssue, ReferenceLocation, SecurityCheckRequest, SecurityCheckResponse,
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
        let (scan_data, analysis_result) = match (
            self.state.scan_data(),
            self.state.analysis_result(),
        ) {
            (Some(sd), Some(ar)) => (sd, ar),
            _ => {
                return Message::GotoDefinitionResult(GotoDefinitionResponse {
                    found: false,
                    file_path: None,
                    start_line: None,
                    start_column: None,
                    end_line: None,
                    end_column: None,
                });
            }
        };

        // Build the full file path
        let full_path = format!("{}/{}", self.config.root_dir, req.file_path);

        // Read file contents to convert line/column to offset
        let file_contents = match std::fs::read_to_string(&full_path) {
            Ok(contents) => contents,
            Err(_) => {
                return Message::GotoDefinitionResult(GotoDefinitionResponse {
                    found: false,
                    file_path: None,
                    start_line: None,
                    start_column: None,
                    end_line: None,
                    end_column: None,
                });
            }
        };

        // Convert line/column to byte offset (1-indexed input)
        let offset = line_column_to_offset(&file_contents, req.line, req.column);

        // Get the FilePath ID
        let file_path_id = match scan_data.interner.get(&full_path) {
            Some(id) => id,
            None => {
                return Message::GotoDefinitionResult(GotoDefinitionResponse {
                    found: false,
                    file_path: None,
                    start_line: None,
                    start_column: None,
                    end_line: None,
                    end_column: None,
                });
            }
        };

        let file_path_obj = hakana_code_info::code_location::FilePath(file_path_id);

        // Look up in definition_locations
        if let Some(definition_locations) = analysis_result.definition_locations.get(&file_path_obj) {
            // Find the most specific (narrowest) matching range
            let mut best_match = None;
            let mut best_range_size = u32::MAX;

            for ((start_offset, end_offset), (classlike_name, member_name)) in definition_locations {
                if (offset as u32) >= *start_offset && (offset as u32) <= *end_offset {
                    let range_size = end_offset - start_offset;
                    if range_size < best_range_size {
                        best_range_size = range_size;
                        best_match = Some((classlike_name, member_name));
                    }
                }
            }

            if let Some((classlike_name, member_name)) = best_match {
                if let Some(pos) = scan_data.codebase.get_symbol_pos(classlike_name, member_name) {
                    let def_file_path = scan_data.interner.lookup(&pos.file_path.0);
                    return Message::GotoDefinitionResult(GotoDefinitionResponse {
                        found: true,
                        file_path: Some(def_file_path.to_string()),
                        start_line: Some(pos.start_line),
                        start_column: Some(pos.start_column),
                        end_line: Some(pos.end_line),
                        end_column: Some(pos.end_column),
                    });
                }
            }
        }

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

    /// Handle a find-symbol-references request (by symbol name).
    pub fn handle_find_symbol_references(self, req: FindSymbolReferencesRequest) -> Message {
        use hakana_code_info::symbol_references_utils::get_references_for_symbol;

        // Check if analysis is ready
        let (scan_data, analysis_result) = match (
            self.state.scan_data(),
            self.state.analysis_result(),
        ) {
            (Some(sd), Some(ar)) => (sd, ar),
            _ => {
                return Message::FindSymbolReferencesResult(FindSymbolReferencesResponse {
                    symbol_found: false,
                    references: Vec::new(),
                });
            }
        };

        let interner = &scan_data.interner;
        let symbol_name = &req.symbol_name;

        // Normalize symbol name (strip $ from property names)
        let normalized_name = if let Some(idx) = symbol_name.rfind("::$") {
            let class_name = &symbol_name[..idx];
            let prop_name = &symbol_name[idx + 3..];
            format!("{}::{}", class_name, prop_name)
        } else {
            symbol_name.clone()
        };

        // Look up references using shared utility
        let references = get_references_for_symbol(&normalized_name, analysis_result, interner);

        match references {
            Some(refs) => {
                // Memoize file contents to avoid reading the same file multiple times
                let mut file_cache: FxHashMap<String, Option<String>> = FxHashMap::default();

                let locations: Vec<ReferenceLocation> = refs
                    .into_iter()
                    .map(|r| {
                        // Handle both absolute and relative paths
                        let full_path = if r.file.starts_with('/') {
                            r.file.clone()
                        } else {
                            format!("{}/{}", self.config.root_dir, r.file)
                        };

                        // Get or load file contents
                        let contents = file_cache
                            .entry(full_path.clone())
                            .or_insert_with(|| std::fs::read_to_string(&full_path).ok());

                        let (line, column) = contents
                            .as_ref()
                            .map(|c| offset_to_line_column(c, r.start_offset as usize))
                            .unwrap_or((0, 0));

                        // For display, use just the file path from r.file (relative or abs)
                        ReferenceLocation {
                            file_path: r.file,
                            line,
                            column,
                        }
                    })
                    .collect();

                Message::FindSymbolReferencesResult(FindSymbolReferencesResponse {
                    symbol_found: true,
                    references: locations,
                })
            }
            None => {
                Message::FindSymbolReferencesResult(FindSymbolReferencesResponse {
                    symbol_found: false,
                    references: Vec::new(),
                })
            }
        }
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

/// Convert a byte offset to line and column numbers (1-indexed).
fn offset_to_line_column(contents: &str, offset: usize) -> (u32, u16) {
    let bytes = contents.as_bytes();
    let offset = offset.min(bytes.len());

    let mut line: u32 = 1;
    let mut line_start: usize = 0;

    for (i, &byte) in bytes.iter().enumerate().take(offset) {
        if byte == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }

    let column = (offset - line_start + 1) as u16;
    (line, column)
}

/// Convert line and column numbers (1-indexed) to byte offset.
fn line_column_to_offset(contents: &str, line: u32, column: u32) -> usize {
    let lines: Vec<&str> = contents.lines().collect();
    let mut offset = 0;

    // Add offset for complete lines before the target line (line is 1-indexed)
    let target_line_index = (line.saturating_sub(1)) as usize;
    for line_content in lines.iter().take(target_line_index) {
        offset += line_content.len() + 1; // +1 for newline character
    }

    // Add offset for characters in the target line (column is 1-indexed)
    if let Some(target_line) = lines.get(target_line_index) {
        let col_offset = (column.saturating_sub(1)) as usize;
        offset += col_offset.min(target_line.len());
    }

    offset
}
