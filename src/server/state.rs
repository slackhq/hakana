//! Server state management.

use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use hakana_code_info::analysis_result::AnalysisResult;
use hakana_orchestrator::SuccessfulScanData;
use hakana_orchestrator::file::FileStatus;
use rustc_hash::FxHashMap;

/// What an analysis run covers.
#[derive(Clone)]
pub(crate) enum AnalysisScope {
    /// The whole codebase, from nothing.
    Everything,
    /// These changes to the files, on top of the last result.
    Changes(FxHashMap<String, FileStatus>),
}

/// Warm state maintained by the server.
pub struct ServerState {
    pub analysis_data: Option<Arc<(AnalysisResult, SuccessfulScanData)>>,
    /// Whether server is shutting down.
    shutting_down: bool,
    /// Whether an analysis is currently running.
    analysis_in_progress: bool,
    /// The generation given out last (see `AnalysisUpdate`).
    last_generation: u64,
    /// The generation of the analysis run started last.
    last_started_generation: u64,
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
    /// What the analysis run under way covers.
    pub(crate) in_flight: Option<AnalysisScope>,
    /// What a run that failed was to cover. The next run covers it as well:
    /// nothing else would bring those changes up again.
    pub(crate) unanalyzed: Option<AnalysisScope>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            analysis_data: None,
            shutting_down: false,
            // The first analysis is on its way from the moment the server exists:
            // until it has completed there is nothing to be ready with.
            analysis_in_progress: true,
            // Generations count up from the time the server started, so that a
            // client that outlives a server does not mistake a run of the server
            // that replaced it for one it has already seen.
            last_generation: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since_epoch| since_epoch.as_millis() as u64),
            last_started_generation: 0,
            pending_requests: 0,
            phase: "Initializing".to_string(),
            files_scanned: Arc::new(0.into()),
            total_files_to_scan: Arc::new(0.into()),
            files_analyzed: Arc::new(0.into()),
            total_files_to_analyze: Arc::new(0.into()),
            pending_changes: FxHashMap::default(),
            in_flight: None,
            unanalyzed: None,
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

    /// Note that an analysis run is starting, and get its generation.
    pub fn begin_analysis(&mut self, phase: &str) -> u64 {
        self.analysis_in_progress = true;
        self.phase = phase.to_string();
        self.last_started_generation = self.next_generation();
        self.last_started_generation
    }

    /// A generation for something the server has to report (see
    /// `AnalysisUpdate`): higher than any before it.
    pub fn next_generation(&mut self) -> u64 {
        self.last_generation += 1;
        self.last_generation
    }

    /// The generation of the analysis run started last: the one that is running,
    /// if one is.
    pub fn last_started_generation(&self) -> u64 {
        self.last_started_generation
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
