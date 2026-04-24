//! Server state management.

use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use hakana_code_info::analysis_result::AnalysisResult;
use hakana_orchestrator::SuccessfulScanData;
use hakana_orchestrator::file::FileStatus;
use rustc_hash::FxHashMap;

/// Warm state maintained by the server.
pub struct ServerState {
    pub analysis_data: Option<Arc<(AnalysisResult, SuccessfulScanData)>>,
    /// Whether server is shutting down.
    shutting_down: bool,
    /// Whether an analysis is currently running.
    analysis_in_progress: bool,
    /// Number of pending requests.
    pending_requests: u32,
    /// Current analysis phase description.
    phase: String,
    /// Files scanned so far (during analysis).
    pub files_scanned: Arc<AtomicU32>,
    /// Total files to scan.
    pub total_files_to_scan: Arc<AtomicU32>,
    /// Files analyzed so far (during analysis).
    pub files_analyzed: Arc<AtomicU32>,
    /// Total files to analyze.
    pub total_files_to_analyze: Arc<AtomicU32>,
    /// Pending file changes.
    pub pending_changes: FxHashMap<String, FileStatus>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            analysis_data: None,
            shutting_down: false,
            analysis_in_progress: false,
            pending_requests: 0,
            phase: "Initializing".to_string(),
            files_scanned: Arc::new(0.into()),
            total_files_to_scan: Arc::new(0.into()),
            files_analyzed: Arc::new(0.into()),
            total_files_to_analyze: Arc::new(0.into()),
            pending_changes: FxHashMap::default(),
        }
    }

    /// Check if server is shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down
    }

    /// Mark server as shutting down.
    pub fn set_shutting_down(&mut self) {
        self.shutting_down = true;
    }

    /// Check if analysis is in progress.
    pub fn is_analysis_in_progress(&self) -> bool {
        self.analysis_in_progress
    }

    /// Set analysis in progress state.
    pub fn set_analysis_in_progress(&mut self, in_progress: bool) {
        self.analysis_in_progress = in_progress;
    }

    /// Get number of pending requests.
    pub fn pending_requests(&self) -> u32 {
        self.pending_requests
    }

    /// Get current phase.
    pub fn phase(&self) -> &str {
        &self.phase
    }

    /// Set current phase.
    pub fn set_phase(&mut self, phase: String) {
        self.phase = phase;
    }

    pub fn files_scanned(&self) -> u32 {
        self.files_scanned
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total_files_to_scan(&self) -> u32 {
        self.total_files_to_scan
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn files_analyzed(&self) -> u32 {
        self.files_analyzed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total_files_to_analyze(&self) -> u32 {
        self.total_files_to_analyze
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Update scan data and analysis result.
    pub fn update_state(&mut self, result: Arc<(AnalysisResult, SuccessfulScanData)>) {
        self.analysis_data = Some(result);
    }

    /// Get files count if available.
    pub fn files_count(&self) -> u32 {
        self.analysis_data
            .as_ref()
            .map(&Arc::as_ref)
            .map(|(_, d)| d.codebase.files.len() as u32)
            .unwrap_or(0)
    }

    /// Get symbols count if available.
    pub fn symbols_count(&self) -> u32 {
        self.analysis_data
            .as_ref()
            .map(&Arc::as_ref)
            .map(|(_, d)| {
                d.codebase.classlike_infos.len() as u32 + d.codebase.functionlike_infos.len() as u32
            })
            .unwrap_or(0)
    }

    /// Get scan data reference.
    pub fn scan_data(&self) -> Option<&SuccessfulScanData> {
        self.analysis_data
            .as_ref()
            .map(&Arc::as_ref)
            .map(|(_, d)| d)
    }

    /// Get analysis result reference.
    pub fn analysis_result(&self) -> Option<&AnalysisResult> {
        self.analysis_data
            .as_ref()
            .map(&Arc::as_ref)
            .map(|(r, _)| r)
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}
