use std::collections::HashMap;
use std::error::Error;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hakana_analyzer::config::Config;
use hakana_orchestrator::file::FileStatus;
use rustc_hash::FxHashMap;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tokio::time::sleep;
use uuid::Uuid;

use crate::analysis_manager::AnalysisManager;
use crate::protocol::Notification;
use crate::ClientInfo;

#[derive(Debug)]
pub struct FileWatcher {
    config: Arc<Config>,
    analysis_manager: Arc<AnalysisManager>,
    clients: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
    watchman_available: bool,
    file_changes: Arc<RwLock<FxHashMap<String, FileStatus>>>,
    last_analysis: Arc<RwLock<Option<Instant>>>,
}

impl FileWatcher {
    pub async fn new(
        config: Arc<Config>,
        analysis_manager: Arc<AnalysisManager>,
        clients: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
    ) -> Result<Self, Box<dyn Error>> {
        let watchman_available = Self::check_watchman_available().await;

        if watchman_available {
            println!("Watchman available - using for file watching");
        } else {
            println!("Watchman not available - using polling fallback");
        }

        Ok(Self {
            config,
            analysis_manager,
            clients,
            watchman_available,
            file_changes: Arc::new(RwLock::new(FxHashMap::default())),
            last_analysis: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn start(&self) -> Result<(), Box<dyn Error>> {
        if self.watchman_available {
            self.start_watchman().await
        } else {
            self.start_polling().await
        }
    }

    async fn check_watchman_available() -> bool {
        match Command::new("watchman").arg("version").output() {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    async fn start_watchman(&self) -> Result<(), Box<dyn Error>> {
        let config = Arc::clone(&self.config);
        let analysis_manager = Arc::clone(&self.analysis_manager);
        let clients = Arc::clone(&self.clients);
        let file_changes = Arc::clone(&self.file_changes);
        let last_analysis = Arc::clone(&self.last_analysis);

        tokio::spawn(async move {
            loop {
                match Self::watch_with_watchman(&config.root_dir).await {
                    Ok(changes) => {
                        if !changes.is_empty() {
                            Self::handle_file_changes(
                                changes,
                                Arc::clone(&config),
                                Arc::clone(&analysis_manager),
                                Arc::clone(&clients),
                                Arc::clone(&file_changes),
                                Arc::clone(&last_analysis),
                            ).await;
                        }
                    }
                    Err(error_msg) => {
                        eprintln!("Watchman error: {}", error_msg);
                        sleep(Duration::from_secs(5)).await;
                    }
                }
                sleep(Duration::from_millis(100)).await;
            }
        });

        Ok(())
    }

    async fn start_polling(&self) -> Result<(), Box<dyn Error>> {
        let config = Arc::clone(&self.config);
        let analysis_manager = Arc::clone(&self.analysis_manager);
        let clients = Arc::clone(&self.clients);
        let file_changes = Arc::clone(&self.file_changes);
        let last_analysis = Arc::clone(&self.last_analysis);

        tokio::spawn(async move {
            let mut last_check = Instant::now();
            loop {
                sleep(Duration::from_secs(2)).await;

                let changes = Self::poll_for_changes(&config.root_dir, last_check).await;
                last_check = Instant::now();

                if !changes.is_empty() {
                    Self::handle_file_changes(
                        changes,
                        Arc::clone(&config),
                        Arc::clone(&analysis_manager),
                        Arc::clone(&clients),
                        Arc::clone(&file_changes),
                        Arc::clone(&last_analysis),
                    ).await;
                }
            }
        });

        Ok(())
    }

    async fn watch_with_watchman(root_dir: &str) -> Result<FxHashMap<String, FileStatus>, String> {
        let mut changes = FxHashMap::default();

        // For now, we'll use a simple watchman query
        // In a real implementation, you'd want to set up a subscription
        let output = Command::new("watchman")
            .args(&[
                "query",
                root_dir,
                "{'expression': ['anyof', ['suffix', 'hack'], ['suffix', 'php'], ['suffix', 'hhi']], 'fields': ['name', 'mtime_ms'], 'since': 'n:state'}"
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            let response: Value = serde_json::from_slice(&output.stdout)
                .map_err(|e| e.to_string())?;
            if let Some(files) = response.get("files").and_then(|f| f.as_array()) {
                for file in files {
                    if let Some(name) = file.get("name").and_then(|n| n.as_str()) {
                        let full_path = format!("{}/{}", root_dir, name);
                        changes.insert(full_path, FileStatus::Modified(0, 0));
                    }
                }
            }
        }

        Ok(changes)
    }

    async fn poll_for_changes(root_dir: &str, since: Instant) -> FxHashMap<String, FileStatus> {
        let mut changes = FxHashMap::default();

        if let Ok(entries) = std::fs::read_dir(root_dir) {
            for entry in entries.flatten() {
                if let Ok(path) = entry.path().canonicalize() {
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if matches!(ext, "hack" | "php" | "hhi") {
                                if let Ok(metadata) = entry.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        let since_time = std::time::UNIX_EPOCH + since.elapsed();
                                        if modified > since_time {
                                            if let Some(path_str) = path.to_str() {
                                                changes.insert(path_str.to_string(), FileStatus::Modified(0, 0));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if path.is_dir() {
                        // Recursively check subdirectories
                        let subdir_changes = Box::pin(Self::poll_for_changes(
                            path.to_str().unwrap_or(""),
                            since
                        )).await;
                        changes.extend(subdir_changes);
                    }
                }
            }
        }

        changes
    }

    async fn handle_file_changes(
        changes: FxHashMap<String, FileStatus>,
        _config: Arc<Config>,
        analysis_manager: Arc<AnalysisManager>,
        clients: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
        file_changes: Arc<RwLock<FxHashMap<String, FileStatus>>>,
        last_analysis: Arc<RwLock<Option<Instant>>>,
    ) {
        println!("File changes detected: {} files", changes.len());

        // Accumulate changes
        {
            let mut file_changes_guard = file_changes.write().await;
            file_changes_guard.extend(changes.clone());
        }

        // Debounce analysis - wait for a short period to batch changes
        let should_analyze = {
            let last_analysis_guard = last_analysis.read().await;
            match last_analysis_guard.as_ref() {
                Some(last) => last.elapsed() > Duration::from_millis(500),
                None => true,
            }
        };

        if should_analyze {
            // Update last analysis time
            {
                let mut last_analysis_guard = last_analysis.write().await;
                *last_analysis_guard = Some(Instant::now());
            }

            // Get accumulated changes
            let changes_to_analyze = {
                let mut file_changes_guard = file_changes.write().await;
                let changes = file_changes_guard.clone();
                file_changes_guard.clear();
                changes
            };

            // Perform analysis
            let interner = Arc::new(hakana_str::Interner::default());
            if let Err(e) = analysis_manager.perform_analysis(Some(changes_to_analyze), interner).await {
                eprintln!("Analysis failed: {}", e);
                return;
            }

            // Notify clients about analysis completion
            Self::notify_clients_of_analysis_completion(
                Arc::clone(&analysis_manager),
                Arc::clone(&clients),
            ).await;
        }
    }

    async fn notify_clients_of_analysis_completion(
        analysis_manager: Arc<AnalysisManager>,
        clients: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
    ) {
        // Get all diagnostics
        let all_diagnostics = match analysis_manager.get_all_diagnostics().await {
            Ok(diagnostics) => diagnostics,
            Err(e) => {
                let error_msg = e.to_string();
                eprintln!("Failed to get diagnostics: {}", error_msg);
                return;
            }
        };

        // Send diagnostics to appropriate clients
        let clients_guard = clients.read().await;
        for (client_id, client_info) in clients_guard.iter() {
            if client_info.client_type.should_receive_diagnostics() {
                for (file_path, diagnostics) in &all_diagnostics {
                    let uri = format!("file://{}", file_path);
                    let notification = Notification::diagnostics_published(uri, json!(diagnostics));

                    if let Err(e) = client_info.tx.send(notification).await {
                        let error_msg = e.to_string();
                        eprintln!("Failed to send diagnostics to client {}: {}", client_id, error_msg);
                    }
                }
            }
        }

        println!("Analysis completed and diagnostics sent to clients");
    }
}