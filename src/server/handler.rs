//! Request handlers for the hakana server.

use crate::{ServerConfig, ServerState};
use hakana_analyzer::config::Config;
use hakana_code_info::analysis_result::{self, AnalysisResult};
use hakana_code_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_logger::Logger;
use hakana_orchestrator::SuccessfulScanData;
use hakana_orchestrator::file::FileStatus;
use hakana_protocol::{
    AckResponse, AnalyzeRequest, AnalyzeResponse, ErrorCode, ErrorResponse, FileChange,
    FindReferencesRequest, FindReferencesResponse, FindSymbolReferencesRequest,
    FindSymbolReferencesResponse, GotoDefinitionRequest, GotoDefinitionResponse, Message,
    ProtocolIssue, ReferenceLocation, SecurityCheckRequest, SecurityCheckResponse, StatusResponse,
};
use hakana_protocol::{FileChangeStatus, GetIssuesResponse};
use hakana_str::Interner;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc::Sender;

/// Handles incoming requests.
pub struct RequestHandler {
    config: Arc<ServerConfig>,
    state: Arc<Mutex<ServerState>>,
    logger: Arc<Logger>,
    shutdown_tx: Sender<bool>,
    start_time: Instant,
}

impl RequestHandler {
    pub fn new(
        config: Arc<ServerConfig>,
        state: Arc<Mutex<ServerState>>,
        logger: Arc<Logger>,
        shutdown_tx: Sender<bool>,
        start_time: Instant,
    ) -> Self {
        Self {
            config,
            state,
            logger,
            shutdown_tx,
            start_time,
        }
    }

    /// Handle a goto-definition request.
    pub fn handle_goto_definition(&self, req: GotoDefinitionRequest) -> Message {
        let state = self.state.lock().unwrap();
        let (scan_data, analysis_result) = match (state.scan_data(), state.analysis_result()) {
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
        if let Some(definition_locations) = analysis_result.definition_locations.get(&file_path_obj)
        {
            // Find the most specific (narrowest) matching range
            let mut best_match = None;
            let mut best_range_size = u32::MAX;

            for ((start_offset, end_offset), (classlike_name, member_name)) in definition_locations
            {
                if (offset as u32) >= *start_offset && (offset as u32) <= *end_offset {
                    let range_size = end_offset - start_offset;
                    if range_size < best_range_size {
                        best_range_size = range_size;
                        best_match = Some((classlike_name, member_name));
                    }
                }
            }

            if let Some((classlike_name, member_name)) = best_match {
                if let Some(pos) = scan_data
                    .codebase
                    .get_symbol_pos(classlike_name, member_name)
                {
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
    pub fn handle_find_references(&self, req: FindReferencesRequest) -> Message {
        // TODO: Implement using cached scan data
        // For now, return empty list
        let _ = req;

        Message::FindReferencesResult(FindReferencesResponse {
            references: Vec::new(),
        })
    }

    /// Handle a find-symbol-references request (by symbol name).
    pub fn handle_find_symbol_references(&self, req: FindSymbolReferencesRequest) -> Message {
        use hakana_code_info::symbol_references_utils::get_references_for_symbol;
        let state = self.state.lock().unwrap();

        // Check if analysis is ready
        let (scan_data, analysis_result) = match (state.scan_data(), state.analysis_result()) {
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
            None => Message::FindSymbolReferencesResult(FindSymbolReferencesResponse {
                symbol_found: false,
                references: Vec::new(),
            }),
        }
    }

    /// Handle status request.
    pub fn handle_status(&self) -> Message {
        let uptime = self.start_time.elapsed();
        let state = self.state.lock().unwrap();

        Message::StatusResult(StatusResponse {
            ready: !state.is_analysis_in_progress(),
            files_count: state.files_count(),
            symbols_count: state.symbols_count(),
            uptime_secs: uptime.as_secs(),
            analysis_in_progress: state.is_analysis_in_progress(),
            pending_requests: state.pending_requests(),
            project_root: self.config.root_dir.clone(),
        })
    }

    /// Handle shutdown request.
    pub async fn handle_shutdown(&self) -> Message {
        self.logger.log_sync("Shutdown requested");
        if let Err(e) = self.shutdown_tx.send(true).await {
            let msg = format!("error requesting shutdown: {}", e);
            self.logger.log_sync(msg.as_str());
        }
        Message::Ack(AckResponse)
    }

    pub async fn handle_get_issues(
        &self,
        analysis_rx: &mut tokio::sync::broadcast::Receiver<
            Result<Arc<(AnalysisResult, SuccessfulScanData)>, String>,
        >,
        req: hakana_protocol::GetIssuesRequest,
    ) -> Message {
        if !req.block_until_next_analysis {
            let state = self.state.lock().unwrap();
            let analysis_complete = !state.is_analysis_in_progress();

            if analysis_complete && let Some(analysis_result) = &state.analysis_data {
                return self.create_get_issues_response(req, analysis_result);
            }
        }

        if let Ok(result) = analysis_rx.recv().await
            && let Ok(result) = result.as_ref()
        {
            return self.create_get_issues_response(req, result);
        }
        Message::GetIssuesResult(GetIssuesResponse {
            analysis_complete: false,
            issues: vec![],
            files_analyzed: 0,
            total_files: 0,
            phase: "Complete".to_string(),
            progress_percent: 100,
        })
    }

    fn create_get_issues_response(
        &self,
        req: hakana_protocol::GetIssuesRequest,
        result: &Arc<(AnalysisResult, SuccessfulScanData)>,
    ) -> Message {
        let (analysis_result, scan_data) = result.as_ref();
        let mut issues = Vec::new();
        for (file_path, file_issues) in &analysis_result.emitted_issues {
            let file_path_str =
                file_path.get_relative_path(&scan_data.interner, &self.config.root_dir);

            if let Some(ref filter) = req.filter {
                if !file_path_str.starts_with(filter) {
                    continue;
                }
            }

            for issue in file_issues {
                issues.push(ProtocolIssue::from_issue(issue, &file_path_str));
            }
        }

        self.logger
            .log_sync(&format!("Returning {} issues", issues.len()));

        return Message::GetIssuesResult(GetIssuesResponse {
            analysis_complete: true,
            issues,
            files_analyzed: 0,
            total_files: 0,
            phase: "Complete".to_string(),
            progress_percent: 100,
        });
    }

    pub fn handle_file_changed(&self, changes: Vec<hakana_protocol::FileChange>) -> Message {
        use hakana_protocol::FileChangeStatus;

        let change_count = changes.len();
        self.logger.log_sync(&format!(
            "Received {} file change notification(s) from client",
            change_count
        ));

        let mut state = self.state.lock().unwrap();

        for change in changes {
            let status = match change.status {
                FileChangeStatus::Added => FileStatus::Added(0, 0),
                FileChangeStatus::Modified => FileStatus::Modified(0, 0),
                FileChangeStatus::Deleted => FileStatus::Deleted,
            };
            state.pending_changes.insert(change.path, status);
        }

        self.logger.log_sync(&format!(
            "Total pending changes: {}",
            state.pending_changes.len()
        ));

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
