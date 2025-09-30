use std::error::Error;
use std::sync::Arc;
use std::collections::HashMap;
use std::net::SocketAddr;

use hakana_analyzer::config::Config;
use hakana_str::Interner;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, mpsc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::config::DaemonConfig;
use crate::lifecycle::LifecycleManager;

pub mod protocol;
mod file_watcher;
mod client;
mod analysis_manager;
pub mod daemon_client;
pub mod mcp_client;
pub mod config;
pub mod lifecycle;

pub use client::ClientType;
use protocol::{Request, Response, Notification};
use file_watcher::FileWatcher;
use analysis_manager::AnalysisManager;

#[derive(Debug)]
pub struct ClientInfo {
    pub id: Uuid,
    pub client_type: ClientType,
    pub tx: mpsc::Sender<Notification>,
}

#[derive(Debug)]
pub struct DaemonServer {
    config: Arc<Config>,
    daemon_config: Arc<DaemonConfig>,
    analysis_manager: Arc<AnalysisManager>,
    clients: Arc<RwLock<HashMap<Uuid, ClientInfo>>>,
    file_watcher: Arc<FileWatcher>,
    lifecycle_manager: Arc<LifecycleManager>,
}

impl DaemonServer {
    pub async fn new(
        config: Config,
        daemon_config: DaemonConfig,
        interner: Interner,
    ) -> Result<Self, Box<dyn Error>> {
        daemon_config.validate()?;

        let config = Arc::new(config);
        let daemon_config = Arc::new(daemon_config);
        let analysis_manager = Arc::new(AnalysisManager::new(config.clone(), interner).await?);
        let clients = Arc::new(RwLock::new(HashMap::new()));

        let file_watcher = Arc::new(
            FileWatcher::new(
                config.clone(),
                analysis_manager.clone(),
                clients.clone(),
            ).await?
        );

        let lifecycle_manager = Arc::new(LifecycleManager::new(daemon_config.clone()));

        Ok(Self {
            config,
            daemon_config,
            analysis_manager,
            clients,
            file_watcher,
            lifecycle_manager,
        })
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        // Start lifecycle management
        self.lifecycle_manager.start().await?;

        let addr = format!("{}:{}", self.daemon_config.host, self.daemon_config.port)
            .parse::<SocketAddr>()?;
        let listener = TcpListener::bind(&addr).await?;

        println!("Hakana daemon listening on {}", addr);

        // Start file watching
        self.file_watcher.start().await?;

        // Main server loop
        loop {
            // Check for shutdown signal
            if self.lifecycle_manager.is_shutdown_requested() {
                println!("Shutdown requested, stopping daemon...");
                break;
            }

            tokio::select! {
                // Handle new connections
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer_addr)) => {
                            // Check client limit
                            let client_count = {
                                let clients = self.clients.read().await;
                                clients.len()
                            };

                            if client_count >= self.daemon_config.max_clients {
                                eprintln!("Maximum client limit reached, rejecting connection from {}", peer_addr);
                                continue;
                            }

                            println!("New client connection from {} ({}/{})",
                                peer_addr, client_count + 1, self.daemon_config.max_clients);

                            let server = self.clone();
                            tokio::spawn(async move {
                                if let Err(e) = server.handle_client(stream).await {
                                    eprintln!("Error handling client {}: {}", peer_addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("Error accepting connection: {}", e);
                        }
                    }
                }

                // Check for shutdown every 100ms
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Continue loop to check shutdown signal
                }
            }
        }

        // Graceful shutdown
        self.lifecycle_manager.shutdown().await?;
        Ok(())
    }

    async fn handle_client(&self, stream: TcpStream) -> Result<(), Box<dyn Error>> {
        let client_id = Uuid::new_v4();
        let (notification_tx, mut notification_rx) = mpsc::channel::<Notification>(100);
        let (response_tx, mut response_rx) = mpsc::channel::<String>(100);

        // Default to LSP client type - will be updated during handshake
        let client_info = ClientInfo {
            id: client_id,
            client_type: ClientType::Lsp,
            tx: notification_tx,
        };

        // Register client
        {
            let mut clients = self.clients.write().await;
            clients.insert(client_id, client_info);
        }

        let clients_clone = Arc::clone(&self.clients);
        let analysis_manager_clone = Arc::clone(&self.analysis_manager);

        // Split the stream for reading and writing
        let (stream_read, stream_write) = stream.into_split();

        // Spawn task to send notifications and responses to client
        tokio::spawn(async move {
            let mut stream_write = stream_write;
            loop {
                tokio::select! {
                    notification = notification_rx.recv() => {
                        match notification {
                            Some(notification) => {
                                if let Ok(json) = serde_json::to_string(&notification) {
                                    let message = format!("Content-Length: {}\r\n\r\n{}", json.len(), json);
                                    if stream_write.write_all(message.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    response = response_rx.recv() => {
                        match response {
                            Some(response) => {
                                let message = format!("Content-Length: {}\r\n\r\n{}", response.len(), response);
                                if stream_write.write_all(message.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        // Handle incoming requests
        let mut buffer = Vec::new();
        let mut stream_read = stream_read;
        loop {
            let mut temp_buffer = [0; 4096];
            match stream_read.read(&mut temp_buffer).await {
                Ok(0) => break, // Connection closed
                Ok(n) => {
                    buffer.extend_from_slice(&temp_buffer[..n]);

                    // Process complete messages
                    if let Err(e) = self.process_messages(&mut buffer, client_id, &analysis_manager_clone, &response_tx).await {
                        eprintln!("Error processing messages: {}", e);
                        return Err(e);
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from client {}: {}", client_id, e);
                    break;
                }
            }
        }

        // Cleanup client
        {
            let mut clients = clients_clone.write().await;
            clients.remove(&client_id);
        }

        println!("Client {} disconnected", client_id);
        Ok(())
    }

    async fn process_messages(
        &self,
        buffer: &mut Vec<u8>,
        client_id: Uuid,
        analysis_manager: &AnalysisManager,
        response_tx: &mpsc::Sender<String>,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            match self.extract_message(buffer) {
                Ok(Some(message)) => {
                    if let Ok(request) = serde_json::from_str::<Request>(&message) {
                        let response = self.handle_request(
                            client_id,
                            request,
                            analysis_manager,
                        ).await;

                        if let Ok(json) = serde_json::to_string(&response) {
                            let _ = response_tx.send(json).await;
                        }
                    }
                }
                Ok(None) => {
                    // No more complete messages
                    break;
                }
                Err(e) => {
                    return Err(format!("Message extraction error: {}", e).into());
                }
            }
        }
        Ok(())
    }

    fn extract_message(&self, buffer: &mut Vec<u8>) -> Result<Option<String>, String> {
        // Look for Content-Length header
        let buffer_str = String::from_utf8_lossy(buffer);
        if let Some(header_end) = buffer_str.find("\r\n\r\n") {
            let header = &buffer_str[..header_end];
            if let Some(content_length_line) = header.lines().find(|line| line.starts_with("Content-Length:")) {
                if let Ok(length) = content_length_line[15..].trim().parse::<usize>() {
                    let message_start = header_end + 4;
                    if buffer.len() >= message_start + length {
                        let message = String::from_utf8_lossy(&buffer[message_start..message_start + length]).to_string();
                        buffer.drain(..message_start + length);
                        return Ok(Some(message));
                    }
                }
            }
        }
        Ok(None)
    }

    async fn handle_request(
        &self,
        client_id: Uuid,
        request: Request,
        analysis_manager: &AnalysisManager,
    ) -> Response {
        match request.method.as_str() {
            "initialize" => {
                // Update client type based on initialization params
                if let Some(params) = &request.params {
                    if let Some(client_info) = params.get("clientInfo") {
                        if let Some(name) = client_info.get("name").and_then(|v| v.as_str()) {
                            let client_type = match name {
                                "hakana-lsp" => ClientType::Lsp,
                                "hakana-mcp" => ClientType::Mcp,
                                "hakana-cli" => ClientType::Cli,
                                _ => ClientType::Unknown,
                            };

                            let mut clients = self.clients.write().await;
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.client_type = client_type;
                            }
                        }
                    }
                }

                Response {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(json!({
                        "capabilities": {
                            "definitionProvider": true,
                            "hoverProvider": true,
                            "referencesProvider": true,
                            "symbolProvider": true,
                            "diagnosticsProvider": true
                        }
                    })),
                    error: None,
                }
            }
            "textDocument/definition" => {
                match analysis_manager.get_definition(&request.params).await {
                    Ok(result) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(result),
                        error: None,
                    },
                    Err(e) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(json!({
                            "code": -1,
                            "message": e.to_string()
                        })),
                    }
                }
            }
            "textDocument/references" => {
                match analysis_manager.get_references(&request.params).await {
                    Ok(result) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(result),
                        error: None,
                    },
                    Err(e) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(json!({
                            "code": -1,
                            "message": e.to_string()
                        })),
                    }
                }
            }
            "textDocument/hover" => {
                match analysis_manager.get_hover(&request.params).await {
                    Ok(result) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(result),
                        error: None,
                    },
                    Err(e) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(json!({
                            "code": -1,
                            "message": e.to_string()
                        })),
                    }
                }
            }
            "workspace/symbol" => {
                match analysis_manager.search_symbols(&request.params).await {
                    Ok(result) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(result),
                        error: None,
                    },
                    Err(e) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(json!({
                            "code": -1,
                            "message": e.to_string()
                        })),
                    }
                }
            }
            "textDocument/diagnostics" => {
                match analysis_manager.get_diagnostics(&request.params).await {
                    Ok(result) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(result),
                        error: None,
                    },
                    Err(e) => Response {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(json!({
                            "code": -1,
                            "message": e.to_string()
                        })),
                    }
                }
            }
            _ => Response {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(json!({
                    "code": -32601,
                    "message": "Method not found"
                })),
            }
        }
    }
}

impl Clone for DaemonServer {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            daemon_config: Arc::clone(&self.daemon_config),
            analysis_manager: Arc::clone(&self.analysis_manager),
            clients: Arc::clone(&self.clients),
            file_watcher: Arc::clone(&self.file_watcher),
            lifecycle_manager: Arc::clone(&self.lifecycle_manager),
        }
    }
}

// CLI functions for daemon management
pub async fn run_daemon_cli(
    plugins: Vec<Box<dyn hakana_analyzer::custom_hook::CustomHook>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Handle command line arguments
    if args.len() > 1 {
        match args[1].as_str() {
            "start" => {
                start_daemon_with_plugins(plugins).await?;
            }
            "stop" => {
                stop_daemon().await?;
            }
            "restart" => {
                stop_daemon().await?;
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                start_daemon_with_plugins(plugins).await?;
            }
            "status" => {
                check_daemon_status().await?;
            }
            "config" => {
                if args.len() > 2 && args[2] == "init" {
                    init_daemon_config().await?;
                } else {
                    show_daemon_config().await?;
                }
            }
            _ => {
                print_daemon_usage();
            }
        }
    } else {
        // Default to start if no arguments
        start_daemon_with_plugins(plugins).await?;
    }

    Ok(())
}

pub async fn start_daemon_with_plugins(
    plugins: Vec<Box<dyn hakana_analyzer::custom_hook::CustomHook>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?
        .to_str()
        .ok_or("Invalid current directory")?
        .to_string();

    // Load daemon configuration
    let daemon_config = crate::config::DaemonConfig::load_from_project_dir(&cwd)?;

    // Check if daemon is already running
    if let Some(pid_file) = &daemon_config.pid_file {
        if let Some(pid) = crate::lifecycle::LifecycleManager::check_if_daemon_running(pid_file)? {
            eprintln!("Daemon is already running with PID {}", pid);
            return Ok(());
        }
    }

    // Load analysis configuration with provided plugins
    let mut interner = Interner::default();
    let config = get_config(plugins, &cwd, &mut interner)?;

    println!("Starting Hakana daemon...");
    let daemon = DaemonServer::new(config, daemon_config, interner).await?;
    daemon.run().await?;

    Ok(())
}

pub async fn stop_daemon() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?
        .to_str()
        .ok_or("Invalid current directory")?
        .to_string();

    let daemon_config = crate::config::DaemonConfig::load_from_project_dir(&cwd)?;

    if let Some(pid_file) = &daemon_config.pid_file {
        crate::lifecycle::LifecycleManager::stop_daemon(pid_file).await?;
    } else {
        eprintln!("No PID file configured, cannot stop daemon");
    }

    Ok(())
}

pub async fn check_daemon_status() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?
        .to_str()
        .ok_or("Invalid current directory")?
        .to_string();

    let daemon_config = crate::config::DaemonConfig::load_from_project_dir(&cwd)?;

    if let Some(pid_file) = &daemon_config.pid_file {
        let status = crate::lifecycle::LifecycleManager::get_daemon_status(pid_file)?;
        println!("Daemon status: {}", status);
    } else {
        println!("No PID file configured");
    }

    Ok(())
}

pub async fn init_daemon_config() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("hakana-daemon.toml");

    if config_path.exists() {
        eprintln!("Configuration file already exists: {}", config_path.display());
        return Ok(());
    }

    crate::config::DaemonConfig::create_default_config_file(&config_path)?;
    println!("Created default configuration file: {}", config_path.display());

    Ok(())
}

pub async fn show_daemon_config() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?
        .to_str()
        .ok_or("Invalid current directory")?
        .to_string();

    let daemon_config = crate::config::DaemonConfig::load_from_project_dir(&cwd)?;
    let config_toml = toml::to_string_pretty(&daemon_config)?;
    println!("Current daemon configuration:\n{}", config_toml);

    Ok(())
}

pub fn print_daemon_usage() {
    println!("Hakana Daemon");
    println!("Usage: hakana-daemon [COMMAND]");
    println!();
    println!("Commands:");
    println!("  start           Start the daemon (default)");
    println!("  stop            Stop the daemon");
    println!("  restart         Restart the daemon");
    println!("  status          Check daemon status");
    println!("  config          Show current configuration");
    println!("  config init     Create default configuration file");
}

pub fn get_config(
    plugins: Vec<Box<dyn hakana_analyzer::custom_hook::CustomHook>>,
    cwd: &String,
    interner: &mut Interner,
) -> std::result::Result<Config, Box<dyn Error>> {
    use hakana_analyzer::config;
    use std::path::Path;

    let mut all_custom_issues = vec![];

    for analysis_hook in &plugins {
        all_custom_issues.extend(analysis_hook.get_custom_issue_names());
    }

    let mut config = config::Config::new(
        cwd.clone(),
        all_custom_issues
            .into_iter()
            .map(|i| i.to_string())
            .collect(),
    );

    config.find_unused_expressions = true;
    config.find_unused_definitions = true;
    config.ignore_mixed_issues = true;
    config.ast_diff = true;
    config.collect_goto_definition_locations = !cfg!(target_os = "linux");

    config.hooks = plugins;

    let config_path_str = format!("{}/hakana.json", cwd);
    let config_path = Path::new(&config_path_str);

    if config_path.exists() {
        config.update_from_file(cwd, config_path, interner)?;
    }

    Ok(config)
}