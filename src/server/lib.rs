//! Hakana server for handling analysis requests.
//!
//! The server maintains warm codebase state and handles requests from CLI and LSP clients.
//! It performs initial analysis on startup and uses watchman to watch for file changes.

mod handler;
mod state;
mod watchman;

pub use handler::RequestHandler;
pub use state::ServerState;

use hakana_analyzer::config::Config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_logger::Logger;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_orchestrator::file::FileStatus;
use hakana_orchestrator::SuccessfulScanData;
use hakana_protocol::{
    AckResponse, ClientConnection, ErrorCode, ErrorResponse, GetIssuesResponse, Message,
    ProtocolIssue, ServerSocket, SocketPath,
};
use hakana_str::Interner;
use rustc_hash::{FxHashMap, FxHashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Server configuration.
#[derive(Clone)]
pub struct ServerConfig {
    /// Project root directory.
    pub root_dir: String,
    /// Number of threads for analysis.
    pub threads: u8,
    /// Path to hakana.json config file.
    pub config_path: Option<String>,
    /// Analysis plugins.
    pub plugins: Vec<Arc<dyn CustomHook>>,
    /// Build header for cache validation.
    pub header: String,
    /// Find unused expressions.
    pub find_unused_expressions: bool,
    /// Find unused definitions.
    pub find_unused_definitions: bool,
}

impl ServerConfig {
    pub fn new(root_dir: String) -> Self {
        Self {
            root_dir,
            threads: 8,
            config_path: None,
            plugins: Vec::new(),
            header: String::new(),
            find_unused_expressions: false,
            find_unused_definitions: false,
        }
    }
}

/// Check if watchman is available.
pub fn check_watchman_available() -> Result<(), String> {
    watchman::check_available()
}

/// The hakana server.
pub struct Server {
    config: ServerConfig,
    socket: ServerSocket,
    state: ServerState,
    logger: Arc<Logger>,
    start_time: Instant,
    /// Pending file changes from watchman
    pending_changes: FxHashMap<String, FileStatus>,
    /// Handle for receiving file changes from watchman
    watchman_handle: Option<watchman::WatchmanHandle>,
}

impl Server {
    /// Create a new server. Requires watchman to be available.
    pub fn new(config: ServerConfig, logger: Arc<Logger>) -> io::Result<Self> {
        // Check watchman is available
        if let Err(e) = watchman::check_available() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Watchman is required for server mode: {}", e),
            ));
        }

        let socket_path = SocketPath::for_project(Path::new(&config.root_dir));

        // Check if a server is already running
        if socket_path.server_exists() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!(
                    "Server already running on socket: {}",
                    socket_path.path().display()
                ),
            ));
        }

        let socket = ServerSocket::bind(socket_path)?;

        logger.log_sync(&format!(
            "Server listening on: {}",
            socket.socket_path().path().display()
        ));

        Ok(Self {
            config,
            socket,
            state: ServerState::new(),
            logger,
            start_time: Instant::now(),
            pending_changes: FxHashMap::default(),
            watchman_handle: None,
        })
    }

    /// Get the socket path.
    pub fn socket_path(&self) -> &SocketPath {
        self.socket.socket_path()
    }

    /// Run the server main loop.
    /// This performs initial analysis while accepting connections, then watches for file changes.
    pub fn run(&mut self) -> io::Result<()> {
        // Load config to get ignore_files before starting watchman
        let ignore_files = self.load_ignore_files();

        // Get watchman clock BEFORE initial analysis to avoid race conditions
        // Any file changes during analysis will be captured
        self.logger.log_sync("Getting watchman clock...");
        let watchman_clock = watchman::get_clock(Path::new(&self.config.root_dir))?;
        self.logger.log_sync(&format!("Watchman clock: {:?}", watchman_clock));

        // Set socket to non-blocking so we can accept connections during analysis
        self.socket.set_accept_timeout(Some(Duration::from_millis(10)))?;

        self.logger.log_sync("Performing initial analysis...");

        // Set state to analyzing
        self.state.set_analysis_in_progress(true);
        self.state.set_phase("Scanning".to_string());

        // Perform initial analysis while accepting connections
        if let Err(e) = self.do_initial_analysis_with_connections() {
            self.logger.log_sync(&format!("Initial analysis failed: {}", e));
            return Err(io::Error::new(io::ErrorKind::Other, e));
        }

        self.state.set_analysis_in_progress(false);
        self.state.set_phase("Ready".to_string());
        self.logger.log_sync("Initial analysis complete. Waiting for connections...");

        // Start watchman subscription with the clock we got before analysis
        self.logger.log_sync(&format!(
            "Starting watchman subscription for: {}",
            self.config.root_dir
        ));
        let handle = watchman::start_subscription(
            PathBuf::from(&self.config.root_dir),
            ignore_files,
            watchman_clock,
        );
        self.watchman_handle = Some(handle);

        // Set socket to non-blocking for the main loop
        self.socket.set_accept_timeout(Some(Duration::from_millis(100)))?;

        // Main loop
        loop {
            // Poll for file changes from watchman
            self.poll_watchman_changes();

            // Check for pending file changes and re-analyze if needed
            if !self.pending_changes.is_empty() && !self.state.is_analysis_in_progress() {
                self.do_incremental_analysis();
            }

            match self.socket.accept() {
                Ok(mut conn) => {
                    if let Err(e) = self.handle_connection(&mut conn) {
                        self.logger.log_sync(&format!("Connection error: {}", e));
                    }
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        // Non-blocking accept, no connection available
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    self.logger.log_sync(&format!("Accept error: {}", e));
                }
            }

            // Check for shutdown
            if self.state.is_shutting_down() {
                self.logger.log_sync("Server shutting down...");
                break;
            }
        }

        Ok(())
    }

    /// Perform initial analysis while accepting connections.
    /// This runs analysis in a background thread while the main thread handles client connections.
    fn do_initial_analysis_with_connections(&mut self) -> Result<(), String> {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        use std::sync::Arc as StdArc;

        // Shared state for communicating between threads
        let analysis_done = StdArc::new(AtomicBool::new(false));
        let analysis_error: StdArc<std::sync::Mutex<Option<String>>> =
            StdArc::new(std::sync::Mutex::new(None));

        // Shared progress counters for real-time progress updates
        let progress_phase: StdArc<std::sync::Mutex<String>> =
            StdArc::new(std::sync::Mutex::new("Starting".to_string()));
        let progress_files_analyzed = StdArc::new(AtomicU32::new(0));
        let progress_total_files = StdArc::new(AtomicU32::new(0));

        // Clone what we need for the analysis thread
        let config = self.config.clone();
        let logger = self.logger.clone();
        let analysis_done_clone = StdArc::clone(&analysis_done);
        let analysis_error_clone = StdArc::clone(&analysis_error);
        let progress_phase_clone = StdArc::clone(&progress_phase);
        let progress_files_analyzed_clone = StdArc::clone(&progress_files_analyzed);
        let progress_total_files_clone = StdArc::clone(&progress_total_files);

        // Channel to send back the results
        let (tx, rx) = std::sync::mpsc::channel();

        // Spawn analysis thread
        let analysis_thread = thread::spawn(move || {
            let result = Self::run_initial_analysis_with_progress(
                &config,
                &logger,
                &progress_phase_clone,
                &progress_files_analyzed_clone,
                &progress_total_files_clone,
            );
            match result {
                Ok((analysis_result, scan_data)) => {
                    let _ = tx.send(Some((analysis_result, scan_data)));
                }
                Err(e) => {
                    *analysis_error_clone.lock().unwrap() = Some(e);
                    let _ = tx.send(None);
                }
            }
            analysis_done_clone.store(true, Ordering::SeqCst);
        });

        // While analysis is running, accept and handle connections
        while !analysis_done.load(Ordering::SeqCst) {
            // Update state from shared progress counters
            if let Ok(phase) = progress_phase.lock() {
                self.state.set_phase(phase.clone());
            }
            self.state.set_progress(
                progress_files_analyzed.load(Ordering::Relaxed),
                progress_total_files.load(Ordering::Relaxed),
            );

            // Try to accept a connection (non-blocking due to timeout set earlier)
            match self.socket.accept() {
                Ok(mut conn) => {
                    if let Err(e) = self.handle_connection(&mut conn) {
                        self.logger.log_sync(&format!("Connection error during init: {}", e));
                    }
                }
                Err(e) => {
                    if e.kind() != io::ErrorKind::WouldBlock {
                        self.logger.log_sync(&format!("Accept error during init: {}", e));
                    }
                }
            }
            // Small sleep to avoid busy-waiting
            thread::sleep(Duration::from_millis(10));
        }

        // Wait for analysis thread to finish
        analysis_thread.join().map_err(|_| "Analysis thread panicked".to_string())?;

        // Check for errors
        if let Some(err) = analysis_error.lock().unwrap().take() {
            return Err(err);
        }

        // Get the results
        let received: Result<Option<(AnalysisResult, SuccessfulScanData)>, _> = rx.recv();
        match received {
            Ok(Some((analysis_result, scan_data))) => {
                let issue_count: usize = analysis_result
                    .emitted_issues
                    .values()
                    .map(|v: &Vec<_>| v.len())
                    .sum();
                self.logger.log_sync(&format!(
                    "Analysis complete: {} files, {} issues",
                    scan_data.codebase.files.len(),
                    issue_count
                ));
                self.state.update_state(scan_data, analysis_result);
                Ok(())
            }
            Ok(None) => Err("Analysis failed".to_string()),
            Err(_) => Err("Failed to receive analysis results".to_string()),
        }
    }

    /// Run initial analysis with progress updates (called from background thread).
    fn run_initial_analysis_with_progress(
        config: &ServerConfig,
        logger: &Arc<Logger>,
        progress_phase: &std::sync::Arc<std::sync::Mutex<String>>,
        progress_files_analyzed: &std::sync::Arc<std::sync::atomic::AtomicU32>,
        progress_total_files: &std::sync::Arc<std::sync::atomic::AtomicU32>,
    ) -> Result<(AnalysisResult, SuccessfulScanData), String> {
        use std::sync::atomic::Ordering;

        // Collect custom issue names from plugins
        let all_custom_issues: FxHashSet<String> = config.plugins
            .iter()
            .flat_map(|h| h.get_custom_issue_names())
            .map(|s| s.to_string())
            .collect();

        let mut analysis_config =
            Config::new(config.root_dir.clone(), all_custom_issues);
        analysis_config.find_unused_expressions = config.find_unused_expressions;
        analysis_config.find_unused_definitions = config.find_unused_definitions;
        analysis_config.ast_diff = true;
        analysis_config.hooks = config.plugins.clone();

        let mut interner = Interner::default();

        // Load config from file if specified
        if let Some(config_path) = &config.config_path {
            let path = Path::new(config_path);
            if path.exists() {
                logger.log_sync(&format!("Loading config from: {}", config_path));
                let _ = analysis_config.update_from_file(&config.root_dir, path, &mut interner);
                if let Some(ref allowed) = analysis_config.allowed_issues {
                    logger.log_sync(&format!("Allowed issues: {} types", allowed.len()));
                } else {
                    logger.log_sync("No allowed_issues filter (all issues enabled)");
                }
            } else {
                logger.log_sync(&format!("Config file not found: {}", config_path));
            }
        } else {
            logger.log_sync("No config path specified");
        }

        // Clone the Arc references for the progress callback closure
        let phase_ref = std::sync::Arc::clone(progress_phase);
        let total_files_ref = std::sync::Arc::clone(progress_total_files);
        // Clone for total_files_to_scan parameter
        let total_files_to_scan_ref = std::sync::Arc::clone(progress_total_files);

        // Create separate counters for scanning and analysis phases
        let files_scanned_counter = std::sync::Arc::clone(progress_files_analyzed);
        let files_analyzed_counter = std::sync::Arc::clone(progress_files_analyzed);
        // Clone for the callback to reset the counter when phase changes
        let files_counter_for_callback = std::sync::Arc::clone(progress_files_analyzed);

        // Create a tokio runtime to run the async analysis
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to build tokio runtime: {}", e))?;

        rt.block_on(hakana_orchestrator::scan_and_analyze_async(
            Vec::new(),
            None,
            None,
            Arc::new(analysis_config),
            config.threads,
            None,
            &config.header,
            Arc::new(interner),
            None,
            None,
            None,
            Some(move |progress: hakana_orchestrator::AnalysisProgress| {
                // Reset counter when phase changes to Analyzing
                if progress.phase == "Analyzing" {
                    files_counter_for_callback.store(0, Ordering::Relaxed);
                }
                // Update shared progress state
                if let Ok(mut phase) = phase_ref.lock() {
                    *phase = progress.phase;
                }
                total_files_ref.store(progress.total_files, Ordering::Relaxed);
            }),
            // Pass files_scanned counter for real-time polling during scanning
            Some(files_scanned_counter),
            // Pass total_files_to_scan counter - set by scan_files when it knows how many files to scan
            Some(total_files_to_scan_ref),
            // Pass files_analyzed counter for real-time polling during analysis
            Some(files_analyzed_counter),
        )).map_err(|e| e.to_string())
    }

    /// Perform incremental analysis with pending changes.
    fn do_incremental_analysis(&mut self) {
        if self.pending_changes.is_empty() {
            return;
        }

        self.state.set_analysis_in_progress(true);
        self.state.set_phase("Analyzing changes".to_string());

        let changes = std::mem::take(&mut self.pending_changes);
        let change_count = changes.len();
        self.logger.log_sync(&format!("Re-analyzing {} changed files...", change_count));

        // Collect custom issue names from plugins
        let all_custom_issues: FxHashSet<String> = self.config.plugins
            .iter()
            .flat_map(|h| h.get_custom_issue_names())
            .map(|s| s.to_string())
            .collect();

        let mut analysis_config =
            Config::new(self.config.root_dir.clone(), all_custom_issues);
        analysis_config.find_unused_expressions = self.config.find_unused_expressions;
        analysis_config.find_unused_definitions = self.config.find_unused_definitions;
        // Enable AST diffing for incremental analysis
        analysis_config.ast_diff = true;
        // Clone the Arc references for the hooks
        analysis_config.hooks = self.config.plugins.clone();

        let mut interner = Interner::default();

        if let Some(config_path) = &self.config.config_path {
            let path = Path::new(config_path);
            if path.exists() {
                let _ = analysis_config.update_from_file(&self.config.root_dir, path, &mut interner);
            }
        }

        let (previous_scan_data, previous_analysis_result) = (
            self.state.scan_data.take(),
            self.state.analysis_result.take(),
        );

        let result = hakana_orchestrator::scan_and_analyze(
            Vec::new(),
            None,
            None,
            Arc::new(analysis_config),
            None,
            self.config.threads,
            self.logger.clone(),
            &self.config.header,
            interner,
            previous_scan_data,
            previous_analysis_result,
            Some(changes),
            || {},
        );

        match result {
            Ok((analysis_result, scan_data)) => {
                let issue_count: usize = analysis_result
                    .emitted_issues
                    .values()
                    .map(|v| v.len())
                    .sum();
                self.logger.log_sync(&format!(
                    "Incremental analysis complete: {} issues",
                    issue_count
                ));
                self.state.update_state(scan_data, analysis_result);
            }
            Err(e) => {
                self.logger.log_sync(&format!("Incremental analysis failed: {}", e));
            }
        }

        self.state.set_analysis_in_progress(false);
        self.state.set_phase("Ready".to_string());
    }

    /// Load ignore_files from config file.
    fn load_ignore_files(&self) -> Vec<String> {
        use hakana_analyzer::config::json_config;

        if let Some(config_path) = &self.config.config_path {
            let path = Path::new(config_path);
            if path.exists() {
                if let Ok(json_config) = json_config::read_from_file(path) {
                    let ignore_files: Vec<String> = json_config
                        .ignore_files
                        .into_iter()
                        .map(|v| format!("{}/{}", self.config.root_dir, v))
                        .collect();
                    if !ignore_files.is_empty() {
                        self.logger.log_sync(&format!(
                            "Watchman will ignore {} path pattern(s)",
                            ignore_files.len()
                        ));
                    }
                    return ignore_files;
                }
            }
        }
        Vec::new()
    }

    /// Poll for file changes from the watchman thread.
    fn poll_watchman_changes(&mut self) {
        if let Some(handle) = &self.watchman_handle {
            let changes = handle.poll_changes();
            if !changes.is_empty() {
                let change_count = changes.len();
                self.pending_changes.extend(changes);
                self.logger.log_sync(&format!(
                    "Received {} file change(s) from watchman ({} pending)",
                    change_count,
                    self.pending_changes.len()
                ));
            }
        }
    }

    /// Handle a single client connection.
    fn handle_connection(&mut self, conn: &mut ClientConnection) -> io::Result<()> {
        // Set read timeout to detect dead connections
        conn.set_read_timeout(Some(Duration::from_secs(300)))?;

        // Handle one request per connection (simpler, avoids issues with client disconnects)
        let msg = match conn.read_message() {
            Ok(msg) => msg,
            Err(e) => {
                // Connection closed or error
                self.logger.log_sync(&format!("Read error: {}", e));
                return Ok(());
            }
        };

        self.logger
            .log_sync(&format!("Received: {:?}", msg.message_type()));

        let response = self.handle_message(msg);

        self.logger.log_sync(&format!("Sending response: {:?}", response.message_type()));

        if let Err(e) = conn.write_message(&response) {
            self.logger.log_sync(&format!("Write error: {}", e));
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()));
        }

        self.logger.log_sync("Response sent successfully");

        Ok(())
    }

    /// Handle a single message and return a response.
    fn handle_message(&mut self, msg: Message) -> Message {
        match msg {
            Message::GetIssues(req) => self.handle_get_issues(req),
            Message::Status(_) => {
                let handler = RequestHandler::new(
                    &self.config,
                    &mut self.state,
                    &self.logger,
                    self.start_time,
                );
                handler.handle_status()
            }
            Message::Shutdown(_) => {
                let handler = RequestHandler::new(
                    &self.config,
                    &mut self.state,
                    &self.logger,
                    self.start_time,
                );
                handler.handle_shutdown()
            }
            Message::GotoDefinition(req) => {
                let handler = RequestHandler::new(
                    &self.config,
                    &mut self.state,
                    &self.logger,
                    self.start_time,
                );
                handler.handle_goto_definition(req)
            }
            Message::FindReferences(req) => {
                let handler = RequestHandler::new(
                    &self.config,
                    &mut self.state,
                    &self.logger,
                    self.start_time,
                );
                handler.handle_find_references(req)
            }
            Message::FileChanged(changes) => {
                self.handle_file_changed(changes)
            }
            _ => Message::Error(ErrorResponse {
                code: ErrorCode::UnsupportedMessage,
                message: "Use GetIssues to retrieve analysis results".to_string(),
            }),
        }
    }

    /// Handle GetIssues request - returns current issues or progress info.
    fn handle_get_issues(&self, req: hakana_protocol::GetIssuesRequest) -> Message {
        let analysis_complete = !self.state.is_analysis_in_progress();

        if !analysis_complete {
            // Return progress information
            return Message::GetIssuesResult(GetIssuesResponse {
                analysis_complete: false,
                issues: Vec::new(),
                files_analyzed: self.state.files_analyzed(),
                total_files: self.state.total_files(),
                phase: self.state.phase().to_string(),
                progress_percent: self.state.progress_percent(),
            });
        }

        // Return all issues
        let mut issues = Vec::new();

        if let Some(ref analysis_result) = self.state.analysis_result {
            if let Some(ref scan_data) = self.state.scan_data {
                for (file_path, file_issues) in &analysis_result.emitted_issues {
                    let file_path_str =
                        file_path.get_relative_path(&scan_data.interner, &self.config.root_dir);

                    // Apply filter if specified
                    if let Some(ref filter) = req.filter {
                        if !file_path_str.starts_with(filter) {
                            continue;
                        }
                    }

                    for issue in file_issues {
                        issues.push(ProtocolIssue::from_issue(issue, &file_path_str));
                    }
                }
            }
        }

        self.logger.log_sync(&format!("Returning {} issues", issues.len()));

        Message::GetIssuesResult(GetIssuesResponse {
            analysis_complete: true,
            issues,
            files_analyzed: self.state.files_count(),
            total_files: self.state.files_count(),
            phase: "Complete".to_string(),
            progress_percent: 100,
        })
    }

    /// Handle file changed notifications from clients (e.g., LSP forwarding VS Code events).
    fn handle_file_changed(&mut self, changes: Vec<hakana_protocol::FileChange>) -> Message {
        use hakana_protocol::FileChangeStatus;

        let change_count = changes.len();
        self.logger.log_sync(&format!(
            "Received {} file change notification(s) from client",
            change_count
        ));

        // Convert protocol changes to FileStatus and add to pending changes
        for change in changes {
            let status = match change.status {
                FileChangeStatus::Added => FileStatus::Added(0, 0),
                FileChangeStatus::Modified => FileStatus::Modified(0, 0),
                FileChangeStatus::Deleted => FileStatus::Deleted,
            };
            self.pending_changes.insert(change.path, status);
        }

        self.logger.log_sync(&format!(
            "Total pending changes: {}",
            self.pending_changes.len()
        ));

        Message::Ack(AckResponse)
    }
}
