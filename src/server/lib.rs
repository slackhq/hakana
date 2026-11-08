mod handler;
mod state;
mod watchman;

pub use handler::RequestHandler;
use state::AnalysisScope;
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

use crate::watchman::WatchmanHandle;

/// How an analysis run ended.
pub struct AnalysisOutcome {
    /// Which run this was: each run has a higher generation than the one before.
    pub generation: u64,
    pub result: Result<Arc<(AnalysisResult, SuccessfulScanData)>, String>,
    /// How far the run got. The live counters belong to the next run as soon as
    /// that starts.
    pub files_scanned: u32,
    pub total_files_to_scan: u32,
    pub files_analyzed: u32,
    pub total_files_to_analyze: u32,
}

/// Something the server has to report about its analysis of the codebase.
pub enum AnalysisUpdate {
    /// The server has let go of the result it had and is analysing the whole
    /// codebase again. Until that is done it has no result to answer from.
    Rescanning { generation: u64 },
    /// An analysis run has ended.
    Completed(AnalysisOutcome),
}

impl AnalysisUpdate {
    /// Each update has a higher generation than the one before.
    pub fn generation(&self) -> u64 {
        match self {
            Self::Rescanning { generation } => *generation,
            Self::Completed(outcome) => outcome.generation,
        }
    }
}

/// The latest thing the server has to report, if there has been any.
///
/// This is published through a `watch` channel: whoever looks sees the latest
/// update, whether it was published before or after they started looking. (A
/// channel of *events* loses the ones sent before the receiver subscribed, and
/// replays old ones to a receiver that was not reading.)
pub type LatestUpdate = Option<Arc<AnalysisUpdate>>;

#[derive(Clone)]
pub struct ServerConfig {
    pub root_dir: String,
    pub threads: u8,
    pub config_path: Option<String>,
    pub plugins: Vec<Arc<dyn CustomHook>>,
    /// Migration hooks, consulted only when computing migration candidates.
    /// These are intentionally kept out of the analysis hooks so the warm
    /// analysis stays in normal (non-migration) mode.
    pub migration_hooks: Vec<Arc<dyn CustomHook>>,
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
            migration_hooks: Vec::new(),
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
    config_changed: bool,
    update_rx: tokio::sync::watch::Receiver<LatestUpdate>,
    update_tx: tokio::sync::watch::Sender<LatestUpdate>,
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

        let (update_tx, update_rx) = tokio::sync::watch::channel(None);
        let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel(1);

        Ok(Self {
            config: Arc::new(config),
            socket,
            state: Arc::new(Mutex::new(ServerState::new())),
            start_time: Instant::now(),
            config_changed: false,
            update_rx,
            update_tx,
            shutdown_tx,
            shutdown_rx,
        })
    }

    pub fn socket_path(&self) -> &SocketPath {
        self.socket.socket_path()
    }

    pub async fn run(&mut self) -> io::Result<()> {
        let ignore_files = self.load_ignore_files();

        info!(
            "Starting watchman subscription for: {}",
            self.config.root_dir
        );
        let config_path = self.config.config_path.as_ref().map(PathBuf::from);
        let handle = watchman::start_subscription(
            PathBuf::from(&self.config.root_dir),
            ignore_files,
            config_path,
        )
        .await;

        self.main_loop(handle).await
    }

    async fn main_loop(&mut self, mut watchman: WatchmanHandle) -> io::Result<()> {
        info!("Performing initial analysis...");

        {
            let mut state = self.state.lock().unwrap();
            self.spawn_analysis(&mut state, "Scanning", AnalysisScope::Everything);
        }

        loop {
            {
                let mut state = self.state.lock().unwrap();

                if !state.is_analysis_in_progress() {
                    // Kick off re-analysis if needed and not already running
                    if self.config_changed {
                        self.config_changed = false;
                        state.pending_changes.clear();
                        state.unanalyzed = None;
                        info!("Config file changed, performing full re-analysis...");
                        self.spawn_analysis(
                            &mut state,
                            "Reloading config",
                            AnalysisScope::Everything,
                        );
                    } else if !state.pending_changes.is_empty() {
                        let changes = std::mem::take(&mut state.pending_changes);
                        // Whatever a failed run left unanalysed comes along.
                        let scope = match state.unanalyzed.take() {
                            Some(AnalysisScope::Everything) => AnalysisScope::Everything,
                            Some(AnalysisScope::Changes(mut unanalyzed)) => {
                                unanalyzed.extend(changes);
                                AnalysisScope::Changes(unanalyzed)
                            }
                            None => AnalysisScope::Changes(changes),
                        };
                        match &scope {
                            AnalysisScope::Everything => {
                                info!(
                                    "Re-analyzing everything, as the last full analysis failed..."
                                )
                            }
                            AnalysisScope::Changes(changes) => {
                                info!("Re-analyzing {} changed files...", changes.len())
                            }
                        }
                        self.spawn_analysis(&mut state, "Analyzing changes", scope);
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
                watchman_event = watchman.recv() => {
                    if let Some(event) = watchman_event {
                        self.handle_watchman_event(event);
                    } else {
                        log::error!("watchman subscriber is not active");
                    }
                }
                Ok(()) = self.update_rx.changed() => {
                    let update = self.update_rx.borrow_and_update().clone();
                    if let Some(AnalysisUpdate::Completed(outcome)) = update.as_deref() {
                        self.handle_analysis_outcome(outcome);
                    }
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
        phase: &str,
        scope: AnalysisScope,
    ) {
        let config = self.config.clone();
        state.in_flight = Some(scope.clone());

        let (previous_analysis_data, changes) = match scope {
            AnalysisScope::Everything => {
                // The last result is let go of before the codebase is analysed
                // again from nothing, rather than kept next to the one being
                // built: it is as large again, and what it says may no longer
                // hold. Clients are told that the server is rescanning.
                state.analysis_data = None;
                let generation = state.next_generation();
                self.update_tx
                    .send_replace(Some(Arc::new(AnalysisUpdate::Rescanning { generation })));
                (None, None)
            }
            // The last result stays in the state while changes are analysed (the
            // run works on a copy of it), so that requests about the codebase -
            // where is this defined? - are answered from what was known a moment
            // ago rather than not at all.
            AnalysisScope::Changes(changes) => (state.analysis_data.clone(), Some(changes)),
        };
        let generation = state.begin_analysis(phase);

        let files_scanned = state.files_scanned.clone();
        let total_files_to_scan = state.total_files_to_scan.clone();

        let files_analyzed = state.files_analyzed.clone();
        let total_files_to_analyze = state.total_files_to_analyze.clone();

        let update_tx = self.update_tx.clone();

        // The run gets a thread of its own, which nothing waits for. (The async
        // runtime, when it shuts down, waits for the blocking tasks it was given
        // - a server told to stop would stay until its analysis was done, with
        // its socket gone already.)
        let run = move || {
            // There has to be an outcome however the run ends: the server starts
            // no other run, and answers no request for results, until there is one.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_analysis(
                    &config,
                    previous_analysis_data,
                    changes,
                    files_scanned.clone(),
                    total_files_to_scan.clone(),
                    files_analyzed.clone(),
                    total_files_to_analyze.clone(),
                )
            }))
            .unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");
                Err(format!("analysis panicked: {message}"))
            })
            .map(Arc::new);

            let count = |counter: &AtomicU32| counter.load(std::sync::atomic::Ordering::Relaxed);
            update_tx.send_replace(Some(Arc::new(AnalysisUpdate::Completed(AnalysisOutcome {
                generation,
                result,
                files_scanned: count(&files_scanned),
                total_files_to_scan: count(&total_files_to_scan),
                files_analyzed: count(&files_analyzed),
                total_files_to_analyze: count(&total_files_to_analyze),
            }))));
        };

        std::thread::Builder::new()
            .name("hakana-analysis".to_string())
            .spawn(run)
            .expect("failed to start the analysis thread");
    }

    fn handle_analysis_outcome(&mut self, outcome: &AnalysisOutcome) {
        let mut state = self.state.lock().unwrap();
        match &outcome.result {
            Ok(result) => {
                let (analysis_result, _) = result.as_ref();
                let issue_count: usize = analysis_result
                    .emitted_issues
                    .values()
                    .map(|v| v.len())
                    .sum();
                info!("Analysis complete: {} issues", issue_count);
                state.update_state(result.clone());
                state.in_flight = None;
            }
            Err(e) => {
                // The last result stays as it was. What this run was to cover is
                // left for the next one, which the next change to a file starts:
                // trying again right away could fail again, in a loop.
                log::error!("Analysis failed: {}", e);
                state.unanalyzed = state.in_flight.take();
            }
        }
        state.set_analysis_in_progress(false);
        state.set_phase("Ready".to_string());
    }

    fn handle_watchman_event(&mut self, event: watchman::WatchmanEvent) {
        match event {
            watchman::WatchmanEvent::ConfigChanged | watchman::WatchmanEvent::FreshInstance => {
                if matches!(event, watchman::WatchmanEvent::ConfigChanged) {
                    info!("Config file changed, scheduling full re-analysis");
                } else {
                    info!(
                        "Scheduling full re-analysis due to non-incremental watchman notification"
                    );
                }
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
            if path.exists()
                && let Ok(json_config) = json_config::read_from_file(path)
            {
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
        Vec::new()
    }

    fn handle_connection(&self, mut conn: ClientConnection) {
        let handler = RequestHandler::new(
            self.config.clone(),
            self.state.clone(),
            self.shutdown_tx.clone(),
            self.start_time,
            self.update_tx.subscribe(),
        );

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
                    Message::GetIssues(req) => handler.handle_get_issues(req).await,
                    Message::GetMigrationCandidates(req) => {
                        handler.handle_get_migration_candidates(req).await
                    }
                    Message::Status(_) => handler.handle_status(),
                    Message::Shutdown(_) => handler.handle_shutdown().await,
                    Message::GotoDefinition(req) => handler.handle_goto_definition(req).await,
                    Message::GotoDefinitionByName(req) => {
                        handler.handle_goto_definition_by_name(req)
                    }
                    Message::FindReferences(req) => handler.handle_find_references(req),
                    Message::FindSymbolReferences(req) => {
                        handler.handle_find_symbol_references(req).await
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
            if let Err(error) =
                analysis_config.update_from_file(&config.root_dir, path, &mut interner)
            {
                log::error!("Invalid config {}: {}", config_path, error);
            }
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
    use hakana_analyzer::custom_hook::{CustomHook, InternalHook};
    use hakana_code_info::analysis_result::AnalysisResult;
    use hakana_code_info::codebase_info::CodebaseInfo;
    use hakana_protocol::{ClientSocket, GetIssuesRequest, GetMigrationCandidatesRequest, Message};
    use hakana_str::Interner;
    use std::time::Instant;
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    use tokio::fs;

    use crate::{
        Server, ServerConfig,
        watchman::{self, WatchmanEvent},
    };

    /// A migration hook whose `get_candidates` returns a fixed list, used to exercise
    /// the server-side migration-candidate path.
    #[derive(Debug)]
    struct StubMigrationHook;

    impl InternalHook for StubMigrationHook {
        fn get_migration_name(&self) -> Option<&str> {
            Some("stub-migration")
        }

        fn get_candidates(
            &self,
            _codebase: &CodebaseInfo,
            _interner: &Interner,
            _analysis_result: &AnalysisResult,
        ) -> Vec<String> {
            vec!["main".to_string()]
        }
    }

    impl CustomHook for StubMigrationHook {}

    #[tokio::test]
    async fn handles_file_changes_during_analysis() -> std::io::Result<()> {
        let tmp = tempfile::Builder::new()
            .prefix("hakana-test")
            .tempdir()
            .expect("failed to create temp dir");
        // Watchman reports canonical paths, and the temp dir can be behind a symlink.
        let root = tmp.path().canonicalize()?;
        let hack_file = root.join("index.hack");
        let config_path = root.join("hakana.json");
        fs::write(hack_file.clone(), "function main(): void {}")
            .await
            .expect("failed to create test file");
        fs::write(config_path.clone(), "{}")
            .await
            .expect("failed to create test file");

        eprintln!("{:?}", hack_file);

        let did_mutate = AtomicBool::new(false);

        let server_config = ServerConfig {
            root_dir: root.to_str().unwrap().to_string(),
            threads: 2,
            config_path: Some(config_path.to_str().unwrap().to_string()),
            plugins: vec![],
            migration_hooks: vec![],
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

        let mut watchman_handle = watchman::start_subscription(
            PathBuf::from(&server_config.root_dir),
            vec![],
            server_config.config_path.as_ref().map(&PathBuf::from),
        )
        .await;

        let mut server = Server::new(server_config).expect("failed to create server");

        let socket_path = server.socket_path().clone();
        let shutdown_tx = server.shutdown_tx.clone();

        let server_task =
            tokio::spawn(async move { server.run().await.expect("failed to run server") });

        // Wait for Watchman to send an incremental notification (which should also have notified the server)
        while let Some(event) = watchman_handle.recv().await {
            if matches!(event, WatchmanEvent::FileChanges(..)) {
                break;
            }
        }

        let mut client = ClientSocket::connect(&socket_path)
            .await
            .expect("failed to connect");

        let request = Message::GetIssues(GetIssuesRequest {
            filter: None,
            find_unused_expressions: false,
            find_unused_definitions: false,
            block_until_next_analysis: false,
            send_progress_report: false,
            last_seen_generation: None,
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

    // ── races between clients and analysis runs ──

    const CLEAN: &str = "function main(): void {}";
    const BROKEN: &str = "function main(): int { return 'not an int'; }";

    /// A project in a temp dir with a server running on it.
    struct TestServer {
        root: PathBuf,
        socket_path: hakana_protocol::SocketPath,
        state: Arc<std::sync::Mutex<crate::ServerState>>,
        shutdown_tx: tokio::sync::mpsc::Sender<bool>,
        task: tokio::task::JoinHandle<()>,
        _tmp: tempfile::TempDir,
    }

    impl TestServer {
        async fn start(chaos_monkey: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
            let tmp = tempfile::Builder::new()
                .prefix("hakana-test")
                .tempdir()
                .expect("failed to create temp dir");
            // Watchman reports canonical paths, and the temp dir can be behind a symlink.
            let root = tmp.path().canonicalize().unwrap();
            std::fs::write(root.join("index.hack"), CLEAN).unwrap();
            std::fs::write(root.join("hakana.json"), "{}").unwrap();

            let mut server = Server::new(ServerConfig {
                root_dir: root.to_str().unwrap().to_string(),
                threads: 2,
                config_path: Some(root.join("hakana.json").to_str().unwrap().to_string()),
                plugins: vec![],
                migration_hooks: vec![],
                header: "".to_string(),
                chaos_monkey,
            })
            .expect("failed to create server");

            let socket_path = server.socket_path().clone();
            let state = server.state.clone();
            let shutdown_tx = server.shutdown_tx.clone();
            let task =
                tokio::spawn(async move { server.run().await.expect("failed to run server") });

            let server = Self {
                root,
                socket_path,
                state,
                shutdown_tx,
                task,
                _tmp: tmp,
            };
            server.settle().await;
            server
        }

        fn write(&self, contents: &str) {
            std::fs::write(self.root.join("index.hack"), contents).unwrap();
        }

        async fn connect(&self) -> ClientSocket {
            ClientSocket::connect(&self.socket_path)
                .await
                .expect("failed to connect")
        }

        async fn status(&self) -> hakana_protocol::StatusResponse {
            match self
                .connect()
                .await
                .request(&Message::Status(hakana_protocol::StatusRequest))
                .await
            {
                Ok(Message::StatusResult(status)) => status,
                other => panic!("expected a status, got {:?}", other),
            }
        }

        /// Wait until the server has been idle for a while, so that the startup
        /// analyses are over.
        async fn settle(&self) {
            let mut idle_checks = 0;
            while idle_checks < 6 {
                if self.status().await.ready {
                    idle_checks += 1;
                } else {
                    idle_checks = 0;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }

        /// Wait, as a client of its own, until the server reports `count` issues.
        async fn wait_for_issue_count(&self, count: usize) {
            let wait = async {
                loop {
                    let response = get_issues(&mut self.connect().await, request(false)).await;
                    if response.analysis_complete && response.issues.len() == count {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            };
            tokio::time::timeout(std::time::Duration::from_secs(20), wait)
                .await
                .unwrap_or_else(|_| panic!("the server never reported {count} issue(s)"));
        }

        async fn stop(self) {
            self.shutdown_tx
                .send(true)
                .await
                .expect("failed to shutdown server");
            self.task.await.expect("failed to join server task");
        }
    }

    fn request(block_until_next_analysis: bool) -> GetIssuesRequest {
        GetIssuesRequest {
            filter: None,
            find_unused_expressions: false,
            find_unused_definitions: false,
            block_until_next_analysis,
            send_progress_report: false,
            last_seen_generation: None,
        }
    }

    async fn get_issues(
        client: &mut ClientSocket,
        request: GetIssuesRequest,
    ) -> hakana_protocol::GetIssuesResponse {
        match client.request(&Message::GetIssues(request)).await {
            Ok(Message::GetIssuesResult(response)) => response,
            other => panic!("expected issues, got {:?}", other),
        }
    }

    const PROMPTLY: std::time::Duration = std::time::Duration::from_secs(2);

    /// Lets a test hold an analysis run in flight: while closed, a run stops
    /// between scanning and analysing.
    #[derive(Default)]
    struct Gate {
        closed: AtomicBool,
        holding: AtomicBool,
        fail_next_run: AtomicBool,
        runs: std::sync::atomic::AtomicUsize,
    }

    impl Gate {
        fn chaos_monkey(self: &Arc<Self>) -> Option<Arc<dyn Fn() + Send + Sync>> {
            let gate = self.clone();
            Some(Arc::new(move || {
                gate.runs.fetch_add(1, Ordering::SeqCst);
                while gate.closed.load(Ordering::SeqCst) {
                    gate.holding.store(true, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                gate.holding.store(false, Ordering::SeqCst);
                if gate.fail_next_run.swap(false, Ordering::SeqCst) {
                    panic!("analysis failure injected by the test");
                }
            }))
        }

        fn fail_next_run(&self) {
            self.fail_next_run.store(true, Ordering::SeqCst);
        }

        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }

        fn open(&self) {
            self.closed.store(false, Ordering::SeqCst);
        }

        async fn wait_until_holding(&self) {
            let wait = async {
                while !self.holding.load(Ordering::SeqCst) {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            };
            tokio::time::timeout(std::time::Duration::from_secs(20), wait)
                .await
                .expect("no analysis run started");
        }
    }

    async fn definition_of(
        server: &TestServer,
        symbol_name: &str,
    ) -> hakana_protocol::GotoDefinitionResponse {
        let request = Message::GotoDefinitionByName(hakana_protocol::GotoDefinitionByNameRequest {
            symbol_name: symbol_name.to_string(),
        });
        match server.connect().await.request(&request).await {
            Ok(Message::GotoDefinitionResult(response)) => response,
            other => panic!("expected a definition, got {:?}", other),
        }
    }

    /// Race 7: an analysis run that fails - here, by panicking - must not leave
    /// the server waiting for it for ever, and the changes it was looking at are
    /// still to be analysed.
    #[tokio::test]
    async fn server_recovers_from_a_failed_analysis() {
        let gate = Arc::new(Gate::default());
        let server = TestServer::start(gate.chaos_monkey()).await;
        let before = get_issues(&mut server.connect().await, request(false)).await;
        assert_eq!(0, before.issues.len());

        // A client is waiting for the next analysis, which is the one that fails.
        let mut waiting_client = server.connect().await;
        let waiting = tokio::spawn(async move {
            get_issues(
                &mut waiting_client,
                GetIssuesRequest {
                    last_seen_generation: Some(before.generation),
                    ..request(true)
                },
            )
            .await
        });

        gate.fail_next_run();
        server.write(BROKEN);

        let failed = tokio::time::timeout(PROMPTLY, waiting)
            .await
            .expect("a client is left waiting for an analysis that failed")
            .unwrap();
        assert!(!failed.analysis_complete);
        assert_ne!(failed.generation, before.generation);

        tokio::time::timeout(PROMPTLY, server.settle())
            .await
            .expect("the server still thinks the failed analysis is running");

        // What was known before the failure is still known.
        assert!(definition_of(&server, "main").await.found);

        // The next change is analysed - together with the one whose analysis
        // failed, which nothing else would ever look at again.
        std::fs::write(server.root.join("other.hack"), "function other(): void {}").unwrap();
        server.wait_for_issue_count(1).await;

        server.stop().await;
    }

    /// A client that does not depend on the async runtime the server under test
    /// runs on, for tests in which that runtime may be stuck.
    struct BlockingClient(std::os::unix::net::UnixStream);

    impl BlockingClient {
        fn connect(server: &TestServer) -> Self {
            let stream = std::os::unix::net::UnixStream::connect(server.socket_path.path())
                .expect("failed to connect");
            stream.set_read_timeout(Some(PROMPTLY)).unwrap();
            Self(stream)
        }

        fn send(&mut self, request: &Message) {
            use std::io::Write;
            self.0
                .write_all(&hakana_protocol::encode_message(request))
                .unwrap();
        }

        /// The response, unless none arrives in time.
        fn receive(&mut self) -> Option<Message> {
            use std::io::Read;
            let mut frame = vec![0u8; 4];
            self.0.read_exact(&mut frame).ok()?;
            let len = u32::from_le_bytes(frame[..].try_into().unwrap()) as usize;
            frame.resize(4 + len, 0);
            self.0.read_exact(&mut frame[4..]).ok()?;
            Some(hakana_protocol::decode_message(&frame).unwrap().0)
        }

        fn request(server: &TestServer, request: &Message) -> Option<Message> {
            let mut client = Self::connect(server);
            client.send(request);
            client.receive()
        }
    }

    /// Race 8: a request that reads files must not hold up everything else while
    /// it does. Here the file is a pipe nobody writes to, so the read never ends.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn slow_file_read_does_not_block_other_requests() {
        let server = TestServer::start(None).await;

        let pipe = server.root.join("pipe.hack");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&pipe)
                .status()
                .unwrap()
                .success()
        );
        // While this end is open and nothing is written, reads of the pipe wait.
        let pipe_writer = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&pipe)
            .unwrap();

        let mut reading_client = BlockingClient::connect(&server);
        reading_client.send(&Message::GotoDefinition(
            hakana_protocol::GotoDefinitionRequest {
                file_path: "pipe.hack".to_string(),
                line: 1,
                column: 1,
            },
        ));
        std::thread::sleep(std::time::Duration::from_millis(200));

        let status =
            BlockingClient::request(&server, &Message::Status(hakana_protocol::StatusRequest));
        let definition = BlockingClient::request(
            &server,
            &Message::GotoDefinitionByName(hakana_protocol::GotoDefinitionByNameRequest {
                symbol_name: "main".to_string(),
            }),
        );

        // Let the read end, whatever happened: with no writer left, the reader
        // is at the end of the file.
        drop(pipe_writer);
        assert!(matches!(
            reading_client.receive(),
            Some(Message::GotoDefinitionResult(response)) if !response.found
        ));

        assert!(
            status.is_some(),
            "a status request waited for another request's file read"
        );
        assert!(
            matches!(definition, Some(Message::GotoDefinitionResult(response)) if response.found),
            "a definition request waited for another request's file read"
        );

        server.stop().await;
    }

    /// Race 9: a server told to stop while an analysis is running must stop
    /// there and then. If stopping waits for the analysis, the process stays
    /// around for as long as that takes - with its socket gone, so that the next
    /// client starts a second server next to it.
    #[test]
    fn shutdown_does_not_wait_for_a_running_analysis() {
        let gate = Arc::new(Gate::default());
        let (stopping_tx, stopping_rx) = std::sync::mpsc::channel();
        let (stopped_tx, stopped_rx) = std::sync::mpsc::channel();

        // The server gets a runtime of its own, as it has in its own process.
        // Dropping the runtime is the last thing that process does.
        let server_gate = gate.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let server = TestServer::start(server_gate.chaos_monkey()).await;
                server_gate.close();
                server.write(BROKEN);
                server_gate.wait_until_holding().await;
                let _ = stopping_tx.send(());
                server.stop().await;
            });
            drop(runtime);
            let _ = stopped_tx.send(());
        });

        // Starting up takes as long as it takes; stopping is what is timed.
        let stopping = stopping_rx.recv_timeout(std::time::Duration::from_secs(120));
        let stopped = stopped_rx.recv_timeout(std::time::Duration::from_secs(15));
        gate.open();
        assert!(stopping.is_ok(), "the server never got to analysing");
        assert!(
            stopped.is_ok(),
            "the server did not stop while an analysis was running"
        );
    }

    /// The socket is bound before the first analysis starts (the file watcher is
    /// set up in between, which takes a while on a large project). A client that
    /// asks in that time must not be told the server is ready.
    #[test]
    fn server_is_not_ready_before_its_first_analysis() {
        use crate::{RequestHandler, ServerState};
        use std::sync::Mutex;

        let (_update_tx, update_rx) = tokio::sync::watch::channel(None);
        let (shutdown_tx, _shutdown_rx) = tokio::sync::mpsc::channel(1);
        let handler = RequestHandler::new(
            Arc::new(ServerConfig::new("/project".to_string())),
            Arc::new(Mutex::new(ServerState::new())),
            shutdown_tx,
            Instant::now(),
            update_rx,
        );

        let Message::StatusResult(status) = handler.handle_status() else {
            panic!("expected a status");
        };
        assert!(!status.ready, "ready with nothing analysed");
        assert!(status.analysis_in_progress);
    }

    /// When everything is analysed again from nothing (here, because the config
    /// changed), the server lets go of the result it had - which is as large as
    /// the one it is about to build, and may no longer hold - and tells clients
    /// that it is rescanning rather than answer from it.
    #[tokio::test]
    async fn full_rescan_lets_go_of_the_last_result_and_says_so() {
        let gate = Arc::new(Gate::default());
        let server = TestServer::start(gate.chaos_monkey()).await;
        let before = get_issues(&mut server.connect().await, request(false)).await;
        assert!(before.analysis_complete);
        assert!(!before.rescanning);
        let last_result = {
            let state = server.state.lock().unwrap();
            Arc::downgrade(state.analysis_data.as_ref().unwrap())
        };

        gate.close();
        std::fs::write(server.root.join("hakana.json"), "{ }").unwrap();
        gate.wait_until_holding().await;

        // A client that says what it has is told at once.
        let aware = |last_seen_generation| GetIssuesRequest {
            last_seen_generation: Some(last_seen_generation),
            ..request(true)
        };
        let during = tokio::time::timeout(
            PROMPTLY,
            get_issues(&mut server.connect().await, aware(before.generation)),
        )
        .await
        .expect("the client was not told that the server is rescanning");
        assert!(during.rescanning);
        assert!(!during.analysis_complete);
        assert!(during.issues.is_empty());
        assert_ne!(during.generation, before.generation);

        // The old result is gone: nothing is answered from it, and it is freed.
        assert!(!definition_of(&server, "main").await.found);
        assert!(
            last_result.upgrade().is_none(),
            "the last result is kept during a full rescan"
        );

        // A client that does not say what it has only ever gets a result.
        let mut plain_client = server.connect().await;
        let mut plain = Box::pin(get_issues(&mut plain_client, request(false)));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(300), &mut plain)
                .await
                .is_err()
        );

        gate.open();

        let after = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            get_issues(&mut server.connect().await, aware(during.generation)),
        )
        .await
        .expect("no result after the rescan");
        assert!(after.analysis_complete);
        assert!(!after.rescanning);
        assert!(plain.await.analysis_complete);
        assert!(definition_of(&server, "main").await.found);

        server.stop().await;
    }

    /// A server that starts analyses the codebase once. (Watchman begins a new
    /// subscription by listing every file; taking that for a reason to analyse
    /// everything again doubled the time the server took to start.)
    #[tokio::test]
    async fn startup_analyses_the_codebase_once() {
        let gate = Arc::new(Gate::default());
        let server = TestServer::start(gate.chaos_monkey()).await;

        // Long enough for a second analysis to have started, were there one.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        server.settle().await;
        assert_eq!(1, gate.runs.load(Ordering::SeqCst));

        // What changes from here on is still heard of.
        server.write(BROKEN);
        server.wait_for_issue_count(1).await;
        assert_eq!(2, gate.runs.load(Ordering::SeqCst));

        server.stop().await;
    }

    /// Race 5: go-to-definition asked for while a file that was just saved is
    /// being analysed. The server knows the codebase as it was a moment ago, and
    /// answers from that.
    #[tokio::test]
    async fn definitions_are_served_during_analysis() {
        let gate = Arc::new(Gate::default());
        let server = TestServer::start(gate.chaos_monkey()).await;
        assert!(definition_of(&server, "main").await.found);

        gate.close();
        server.write(BROKEN);
        gate.wait_until_holding().await;

        let during_analysis = definition_of(&server, "main").await;
        gate.open();
        assert!(
            during_analysis.found,
            "no definitions while a change is being analysed"
        );

        server.stop().await;
    }

    /// Race 2: the language server fetches a result, and by the time it is back
    /// with its next (blocking) request another analysis has completed. It has
    /// to be given that one, not made to wait for an analysis after it.
    #[tokio::test]
    async fn result_completed_between_two_fetches_is_not_missed() {
        let server = TestServer::start(None).await;

        let first = get_issues(&mut server.connect().await, request(false)).await;
        assert!(first.analysis_complete);
        assert_eq!(0, first.issues.len());

        server.write(BROKEN);
        server.wait_for_issue_count(1).await;

        let second = tokio::time::timeout(
            PROMPTLY,
            get_issues(
                &mut server.connect().await,
                GetIssuesRequest {
                    last_seen_generation: Some(first.generation),
                    ..request(true)
                },
            ),
        )
        .await
        .expect("the analysis that completed between the two fetches was missed");
        assert_eq!(1, second.issues.len());

        server.stop().await;
    }

    /// Race 10: a client that keeps its connection open must not be handed the
    /// results of analyses that completed before it asked.
    #[tokio::test]
    async fn open_connection_is_not_given_stale_results() {
        let server = TestServer::start(None).await;
        let mut client = server.connect().await;

        server.write(BROKEN);
        server.wait_for_issue_count(1).await;
        server.write(CLEAN);
        server.wait_for_issue_count(0).await;
        server.settle().await;

        // Nothing is running, so "the next analysis" is still to come.
        let mut next = Box::pin(get_issues(&mut client, request(true)));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(500), &mut next)
                .await
                .is_err(),
            "answered with an analysis that completed before the request"
        );

        server.write(BROKEN);
        let response = tokio::time::timeout(PROMPTLY, next)
            .await
            .expect("no answer once the next analysis completed");
        assert_eq!(1, response.issues.len());

        server.stop().await;
    }

    /// Race 6: the result is published by the analysis thread before the main
    /// loop notes that the analysis is over. A client arriving in between sees
    /// "in progress", and must still get that result.
    #[tokio::test]
    async fn result_published_before_the_main_loop_notes_it_is_not_missed() {
        use crate::{AnalysisOutcome, AnalysisUpdate, RequestHandler, ServerState};
        use hakana_code_info::data_flow::graph::GraphKind;
        use hakana_code_info::symbol_references::SymbolReferences;
        use std::sync::Mutex;

        let state = Arc::new(Mutex::new(ServerState::new()));
        let generation = state.lock().unwrap().begin_analysis("Scanning");
        let (update_tx, update_rx) = tokio::sync::watch::channel(None);
        let (shutdown_tx, _shutdown_rx) = tokio::sync::mpsc::channel(1);

        // The analysis thread publishes its result...
        let result = Arc::new((
            AnalysisResult::new(GraphKind::FunctionBody, SymbolReferences::new()),
            hakana_orchestrator::SuccessfulScanData::default(),
        ));
        update_tx.send_replace(Some(Arc::new(AnalysisUpdate::Completed(AnalysisOutcome {
            generation,
            result: Ok(result.clone()),
            files_scanned: 0,
            total_files_to_scan: 0,
            files_analyzed: 0,
            total_files_to_analyze: 0,
        }))));

        // ...then a connection is accepted, and its request handled...
        let handler = RequestHandler::new(
            Arc::new(ServerConfig::new("/project".to_string())),
            state.clone(),
            shutdown_tx,
            Instant::now(),
            update_rx,
        );
        let response = tokio::spawn(async move { handler.handle_get_issues(request(false)).await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // ...and only then does the main loop get to the result.
        {
            let mut state = state.lock().unwrap();
            state.update_state(result);
            state.set_analysis_in_progress(false);
        }

        let response = tokio::time::timeout(PROMPTLY, response)
            .await
            .expect("the request waits for an analysis after the one that just completed")
            .unwrap();
        assert!(
            matches!(response, Message::GetIssuesResult(response) if response.analysis_complete)
        );
    }

    #[tokio::test]
    async fn serves_migration_candidates() -> std::io::Result<()> {
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

        let server_config = ServerConfig {
            root_dir: tmp.path().to_str().unwrap().to_string(),
            threads: 2,
            config_path: Some(config_path.to_str().unwrap().to_string()),
            plugins: vec![],
            migration_hooks: vec![Arc::new(StubMigrationHook)],
            header: "".to_string(),
            chaos_monkey: None,
        };

        let mut server = Server::new(server_config).expect("failed to create server");

        let socket_path = server.socket_path().clone();
        let shutdown_tx = server.shutdown_tx.clone();

        let server_task =
            tokio::spawn(async move { server.run().await.expect("failed to run server") });

        let mut client = ClientSocket::connect(&socket_path)
            .await
            .expect("failed to connect");

        // A recognised migration returns the hook's candidates, waiting for analysis.
        let request = Message::GetMigrationCandidates(GetMigrationCandidatesRequest {
            migration: "stub-migration".to_string(),
            filter: None,
            block_until_next_analysis: false,
        });

        let response = loop {
            let mut client = ClientSocket::connect(&socket_path)
                .await
                .expect("failed to connect");
            let response = client
                .request(&request)
                .await
                .expect("failed to send request");
            if let Message::GetMigrationCandidatesResult(ref result) = response
                && !result.analysis_complete
            {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
            break response;
        };

        if let Message::GetMigrationCandidatesResult(result) = response {
            assert!(result.analysis_complete);
            assert!(result.migration_recognized);
            assert_eq!(vec!["main".to_string()], result.candidates);
        } else {
            panic!("Expected GetMigrationCandidatesResult, got {:?}", response);
        }

        // An unrecognised migration reports migration_recognized = false.
        let unknown = client
            .request(&Message::GetMigrationCandidates(
                GetMigrationCandidatesRequest {
                    migration: "does-not-exist".to_string(),
                    filter: None,
                    block_until_next_analysis: false,
                },
            ))
            .await
            .expect("failed to send request");

        if let Message::GetMigrationCandidatesResult(result) = unknown {
            assert!(!result.migration_recognized);
            assert!(result.candidates.is_empty());
        } else {
            panic!("Expected GetMigrationCandidatesResult, got {:?}", unknown);
        }

        shutdown_tx
            .send(true)
            .await
            .expect("failed to shutdown server");

        server_task.await.expect("failed to join server task");

        Ok(())
    }
}
