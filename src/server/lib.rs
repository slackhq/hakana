mod handler;
mod state;
mod watchman;

pub use handler::RequestHandler;
pub use state::ServerState;

use hakana_analyzer::config::Config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_orchestrator::file::FileStatus;
use hakana_orchestrator::{AnalysisProgress, SuccessfulScanData};
use hakana_protocol::{
    ClientConnection, ErrorCode, ErrorResponse, Message, ServerSocket, SocketPath,
};
use hakana_str::Interner;
use log::info;
use rustc_hash::{FxHashMap, FxHashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

#[derive(Clone)]
pub struct ServerConfig {
    pub root_dir: String,
    pub threads: u8,
    pub config_path: Option<String>,
    pub plugins: Vec<Arc<dyn CustomHook>>,
    pub header: String,
    pub chaos_monkey: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ServerConfig {
    pub fn new(root_dir: String) -> Self {
        Self {
            root_dir,
            threads: 8,
            config_path: None,
            plugins: Vec::new(),
            header: String::new(),
            chaos_monkey: None,
        }
    }
}

pub fn check_watchman_available() -> Result<(), String> {
    watchman::check_available()
}

pub struct Server {
    config: Arc<ServerConfig>,
    socket: ServerSocket,
    state: Arc<Mutex<ServerState>>,
    start_time: Instant,
    watchman_handle: Option<watchman::WatchmanHandle>,
    config_changed: bool,
    analysis_rx:
        tokio::sync::broadcast::Receiver<Result<Arc<(AnalysisResult, SuccessfulScanData)>, String>>,
    analysis_tx:
        tokio::sync::broadcast::Sender<Result<Arc<(AnalysisResult, SuccessfulScanData)>, String>>,
    shutdown_tx: tokio::sync::mpsc::Sender<bool>,
    shutdown_rx: tokio::sync::mpsc::Receiver<bool>,
}

impl Server {
    pub fn new(config: ServerConfig) -> io::Result<Self> {
        if let Err(e) = watchman::check_available() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Watchman is required for server mode: {}", e),
            ));
        }

        let socket_path = SocketPath::for_project(Path::new(&config.root_dir));

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

        info!(
            "Server listening on: {}",
            socket.socket_path().path().display()
        );

        let (analysis_tx, analysis_rx) = tokio::sync::broadcast::channel(8);
        let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel(1);

        Ok(Self {
            config: Arc::new(config),
            socket,
            state: Arc::new(Mutex::new(ServerState::new())),
            start_time: Instant::now(),
            watchman_handle: None,
            config_changed: false,
            analysis_rx,
            analysis_tx,
            shutdown_tx,
            shutdown_rx,
        })
    }

    pub fn socket_path(&self) -> &SocketPath {
        self.socket.socket_path()
    }

    pub async fn run(&mut self) -> io::Result<()> {
        let ignore_files = self.load_ignore_files();

        info!("Getting watchman clock...");
        let watchman_clock = watchman::get_clock(Path::new(&self.config.root_dir)).await?;
        info!("Watchman clock: {:?}", watchman_clock);

        info!(
            "Starting watchman subscription for: {}",
            self.config.root_dir
        );
        let config_path = self.config.config_path.as_ref().map(PathBuf::from);
        let handle = watchman::start_subscription(
            PathBuf::from(&self.config.root_dir),
            ignore_files,
            watchman_clock,
            config_path,
        );
        self.watchman_handle = Some(handle);

        self.main_loop().await
    }

    async fn main_loop(&mut self) -> io::Result<()> {
        info!("Performing initial analysis...");

        {
            let mut state = self.state.lock().unwrap();
            state.set_analysis_in_progress(true);
            state.set_phase("Scanning".to_string());
            self.spawn_analysis(&mut state, None);
        }

        loop {
            {
                let mut state = self.state.lock().unwrap();

                if !state.is_analysis_in_progress() {
                    // Kick off re-analysis if needed and not already running
                    if self.config_changed {
                        self.config_changed = false;
                        state.pending_changes.clear();
                        state.set_analysis_in_progress(true);
                        state.set_phase("Reloading config".to_string());
                        info!("Config file changed, performing full re-analysis...");
                        state.analysis_data = None;
                        self.spawn_analysis(&mut state, None);
                    } else if !state.pending_changes.is_empty() {
                        let changes = std::mem::take(&mut state.pending_changes);
                        let change_count = changes.len();
                        state.set_analysis_in_progress(true);
                        state.set_phase("Analyzing changes".to_string());
                        info!("Re-analyzing {} changed files...", change_count);
                        self.spawn_analysis(&mut state, Some(changes));
                    }
                }
            }

            tokio::select! {
                accept_result = self.socket.accept() => {
                    match accept_result {
                        Ok(conn) => {
                            self.handle_connection(conn);
                        }
                        Err(e) => {
                            info!("Accept error: {}", e);
                        }
                    }
                }
                Some(event) = async {
                    match self.watchman_handle.as_mut() {
                        Some(handle) => handle.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.handle_watchman_event(event);
                }
                result = self.analysis_rx.recv() => {
                    self.handle_analysis_result(result);
                }
                _ = self.shutdown_rx.recv() => {
                    info!("Server shutting down...");
                    return Ok(());
                }
            }
        }
    }

    fn spawn_analysis(
        &self,
        state: &mut MutexGuard<ServerState>,
        changes: Option<FxHashMap<String, FileStatus>>,
    ) {
        let config = self.config.clone();
        let previous_analysis_data = state.analysis_data.take();

        let files_scanned = state.files_scanned.clone();
        let total_files_to_scan = state.total_files_to_scan.clone();

        let files_analyzed = state.files_analyzed.clone();
        let total_files_to_analyze = state.total_files_to_analyze.clone();

        let tx = self.analysis_tx.clone();

        tokio::task::spawn_blocking(move || {
            let result = run_analysis(
                &config,
                previous_analysis_data,
                changes,
                files_scanned,
                total_files_to_scan,
                files_analyzed,
                total_files_to_analyze,
            )
            .map(&Arc::new);
            let _ = tx.send(result);
        });
    }

    fn handle_analysis_result(
        &mut self,
        result: Result<
            Result<Arc<(AnalysisResult, SuccessfulScanData)>, String>,
            tokio::sync::broadcast::error::RecvError,
        >,
    ) {
        let mut state = self.state.lock().unwrap();
        match result {
            Ok(Ok(result)) => {
                let (analysis_result, _) = result.as_ref();
                let issue_count: usize = analysis_result
                    .emitted_issues
                    .values()
                    .map(|v| v.len())
                    .sum();
                info!("Analysis complete: {} issues", issue_count);
                state.update_state(result.clone());
            }
            Ok(Err(e)) => {
                info!("Analysis failed: {}", e);
            }
            Err(_) => {
                info!("Analysis task was cancelled");
            }
        }
        state.set_analysis_in_progress(false);
        state.set_phase("Ready".to_string());
    }

    fn handle_watchman_event(&mut self, event: watchman::WatchmanEvent) {
        match event {
            watchman::WatchmanEvent::ConfigChanged => {
                info!("Config file changed, scheduling full re-analysis");
                self.config_changed = true;
                let mut state = self.state.lock().unwrap();
                state.pending_changes.clear();
            }
            watchman::WatchmanEvent::FileChanges(changes) => {
                if !self.config_changed {
                    let mut state = self.state.lock().unwrap();
                    let change_count = changes.len();
                    state.pending_changes.extend(changes);
                    info!(
                        "Received {} file change(s) from watchman ({} pending)",
                        change_count,
                        state.pending_changes.len()
                    );
                }
            }
        }
    }

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
                        info!(
                            "Watchman will ignore {} path pattern(s)",
                            ignore_files.len()
                        );
                    }
                    return ignore_files;
                }
            }
        }
        Vec::new()
    }

    fn handle_connection(&self, mut conn: ClientConnection) {
        let handler = RequestHandler::new(
            self.config.clone(),
            self.state.clone(),
            self.shutdown_tx.clone(),
            self.start_time,
        );
        let mut analysis_rx = self.analysis_tx.subscribe();

        tokio::spawn(async move {
            loop {
                let msg = match conn.read_message().await {
                    Ok(msg) => msg,
                    Err(e) => {
                        info!("Read error: {}", e);
                        return;
                    }
                };

                info!("Received: {:?}", msg.message_type());

                let response = match msg {
                    Message::GetIssues(req) => {
                        handler.handle_get_issues(&mut analysis_rx, req).await
                    }
                    Message::Status(_) => handler.handle_status(),
                    Message::Shutdown(_) => handler.handle_shutdown().await,
                    Message::GotoDefinition(req) => handler.handle_goto_definition(req),
                    Message::FindReferences(req) => handler.handle_find_references(req),
                    Message::FindSymbolReferences(req) => {
                        handler.handle_find_symbol_references(req)
                    }
                    Message::FileChanged(changes) => handler.handle_file_changed(changes),
                    _ => Message::Error(ErrorResponse {
                        code: ErrorCode::UnsupportedMessage,
                        message: "Use GetIssues to retrieve analysis results".to_string(),
                    }),
                };

                info!("Sending response: {:?}", response.message_type());

                if let Err(e) = conn.write_message(&response).await {
                    info!("Write error: {}", e);
                    return;
                }

                info!("Response sent successfully");
            }
        });
    }
}

fn run_analysis(
    config: &ServerConfig,
    previous_analysis_data: Option<Arc<(AnalysisResult, SuccessfulScanData)>>,
    changes: Option<FxHashMap<String, FileStatus>>,
    files_scanned: Arc<AtomicU32>,
    total_files_to_scan: Arc<AtomicU32>,
    files_analyzed: Arc<AtomicU32>,
    total_files_to_analyze: Arc<AtomicU32>,
) -> Result<(AnalysisResult, SuccessfulScanData), String> {
    let all_custom_issues: FxHashSet<String> = config
        .plugins
        .iter()
        .flat_map(|h| h.get_custom_issue_names())
        .map(|s| s.to_string())
        .collect();

    let mut analysis_config = Config::new(config.root_dir.clone(), all_custom_issues);
    analysis_config.find_unused_expressions = true;
    analysis_config.find_unused_definitions = true;
    analysis_config.ast_diff = true;
    analysis_config.collect_goto_definition_locations = true;
    analysis_config.hooks = config.plugins.clone();

    let mut interner = Interner::default();

    if let Some(config_path) = &config.config_path {
        let path = Path::new(config_path);
        if path.exists() {
            info!("Loading config from: {}", config_path);
            let _ = analysis_config.update_from_file(&config.root_dir, path, &mut interner);
            if let Some(ref allowed) = analysis_config.allowed_issues {
                info!("Allowed issues: {} types", allowed.len());
            } else {
                info!("No allowed_issues filter (all issues enabled)");
            }
        } else {
            info!("Config file not found: {}", config_path);
        }
    } else {
        info!("No config path specified");
    }

    let (previous_scan_data, previous_analysis_result) = previous_analysis_data
        .map(|d| (Some(d.1.clone()), Some(d.0.clone())))
        .unwrap_or((None, None));

    files_scanned.store(0, std::sync::atomic::Ordering::Relaxed);
    total_files_to_scan.store(0, std::sync::atomic::Ordering::Relaxed);
    files_analyzed.store(0, std::sync::atomic::Ordering::Relaxed);
    total_files_to_analyze.store(0, std::sync::atomic::Ordering::Relaxed);

    let progress = AnalysisProgress {
        files_scanned,
        total_files_to_scan,
        files_analyzed,
        total_files_to_analyze,
    };

    hakana_orchestrator::scan_and_analyze_with_progress(
        Vec::new(),
        None,
        None,
        Arc::new(analysis_config),
        None,
        config.threads,
        false,
        &config.header,
        Arc::new(interner),
        previous_scan_data,
        previous_analysis_result,
        changes,
        || {
            if let Some(f) = &config.chaos_monkey {
                f();
            }
        },
        Some(progress),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use hakana_protocol::{ClientSocket, GetIssuesRequest, Message};
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, atomic::AtomicBool},
    };
    use tokio::fs;

    use crate::{Server, ServerConfig, watchman};

    #[tokio::test]
    async fn handles_file_changes_during_analysis() -> std::io::Result<()> {
        let tmp = tempfile::Builder::new()
            .prefix("hakana-test")
            .tempdir()
            .expect("failed to create temp dir");
        let hack_file = tmp.path().join("index.hack");
        let config_path = tmp.path().join("hakana.json");
        fs::write(hack_file.clone(), "function main(): void {}")
            .await
            .expect("failed to create test file");
        fs::write(config_path.clone(), "{}")
            .await
            .expect("failed to create test file");

        eprintln!("{:?}", hack_file);

        let did_mutate = AtomicBool::new(false);

        let server_config = ServerConfig {
            root_dir: tmp.path().to_str().unwrap().to_string(),
            threads: 2,
            config_path: Some(config_path.to_str().unwrap().to_string()),
            plugins: vec![],
            header: "".to_string(),
            chaos_monkey: Some(Arc::new(move || {
                // Simulate a file change between the first scan and first analysis
                if !did_mutate.load(std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("editing file before analysis");
                    did_mutate.store(true, std::sync::atomic::Ordering::Relaxed);
                    std::fs::write(hack_file.clone(), "function foo(): void {}")
                        .expect("failed to mutate file before analysis");
                }
            })),
        };

        let watchman_clock = watchman::get_clock(Path::new(&server_config.root_dir)).await?;
        let mut watchman_handle = watchman::start_subscription(
            PathBuf::from(&server_config.root_dir),
            vec![],
            watchman_clock,
            server_config.config_path.as_ref().map(&PathBuf::from),
        );

        let mut server = Server::new(server_config).expect("failed to create server");

        let socket_path = server.socket_path().clone();
        let shutdown_tx = server.shutdown_tx.clone();

        let server_task =
            tokio::spawn(async move { server.run().await.expect("failed to run server") });

        // Wait for Watchman to send a notification (which should also have notified the server)
        watchman_handle.recv().await;

        let mut client = ClientSocket::connect(&socket_path)
            .await
            .expect("failed to connect");

        let request = Message::GetIssues(GetIssuesRequest {
            filter: None,
            find_unused_expressions: false,
            find_unused_definitions: false,
            block_until_next_analysis: false,
            send_progress_report: false,
        });

        let response = client
            .request(&request)
            .await
            .expect("Failed to send request");

        if let Message::GetIssuesResult(result) = response {
            eprintln!("{:?}", result.issues);
            assert!(
                result.analysis_complete,
                "Analysis should be complete after server is ready"
            );
            assert_eq!(0, result.issues.len(), "there should be no issues reported");
            assert_eq!(
                1, result.files_analyzed,
                "one file should have been analyzed"
            )
        } else {
            panic!("Expected GetIssuesResult, got {:?}", response);
        }

        shutdown_tx
            .send(true)
            .await
            .expect("failed to shutdown server");

        server_task.await.expect("failed to join server task");

        Ok(())
    }
}
