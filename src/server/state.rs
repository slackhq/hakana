//! Server state management.

use hakana_code_info::analysis_result::AnalysisResult;
use hakana_orchestrator::SuccessfulScanData;

/// Warm state maintained by the server.
pub struct ServerState {
    /// Cached scan data from last successful scan.
    pub scan_data: Option<SuccessfulScanData>,
    /// Cached analysis result.
    pub analysis_result: Option<AnalysisResult>,
    /// Whether server is shutting down.
    shutting_down: bool,
    /// Whether an analysis is currently running.
    analysis_in_progress: bool,
    /// Number of pending requests.
    pending_requests: u32,
    /// Current analysis phase description.
    phase: String,
    /// Files analyzed so far (during analysis).
    files_analyzed: u32,
    /// Total files to analyze.
    total_files: u32,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            scan_data: None,
            analysis_result: None,
            shutting_down: false,
            analysis_in_progress: false,
            pending_requests: 0,
            phase: "Initializing".to_string(),
            files_analyzed: 0,
            total_files: 0,
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

    /// Get files analyzed count.
    pub fn files_analyzed(&self) -> u32 {
        self.files_analyzed
    }

    /// Get total files count.
    pub fn total_files(&self) -> u32 {
        self.total_files
    }

    /// Get progress percentage.
    pub fn progress_percent(&self) -> u8 {
        if self.total_files == 0 {
            0
        } else {
            ((self.files_analyzed as f64 / self.total_files as f64) * 100.0) as u8
        }
    }

    /// Set progress counters during analysis.
    pub fn set_progress(&mut self, files_analyzed: u32, total_files: u32) {
        self.files_analyzed = files_analyzed;
        self.total_files = total_files;
    }

    /// Update scan data and analysis result.
    pub fn update_state(
        &mut self,
        scan_data: SuccessfulScanData,
        analysis_result: AnalysisResult,
    ) {
        self.total_files = scan_data.codebase.files.len() as u32;
        self.files_analyzed = self.total_files;
        self.scan_data = Some(scan_data);
        self.analysis_result = Some(analysis_result);
    }

    /// Get files count if available.
    pub fn files_count(&self) -> u32 {
        self.scan_data
            .as_ref()
            .map(|d| d.codebase.files.len() as u32)
            .unwrap_or(0)
    }

    /// Get symbols count if available.
    pub fn symbols_count(&self) -> u32 {
        self.scan_data
            .as_ref()
            .map(|d| {
                d.codebase.classlike_infos.len() as u32
                    + d.codebase.functionlike_infos.len() as u32
            })
            .unwrap_or(0)
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}
