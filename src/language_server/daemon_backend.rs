use std::sync::Arc;
use std::process::Stdio;
use std::time::Duration;

use hakana_daemon_server::daemon_client::DaemonClient;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, timeout};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

#[derive(Debug)]
pub struct DaemonBackend {
    client: Client,
    daemon_client: Arc<Mutex<Option<DaemonClient>>>,
    notification_rx: Arc<Mutex<Option<mpsc::Receiver<hakana_daemon_server::protocol::Notification>>>>,
}

impl DaemonBackend {
    pub fn new(lsp_client: Client) -> Self {
        Self {
            client: lsp_client,
            daemon_client: Arc::new(Mutex::new(None)),
            notification_rx: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_daemon_connection(&self) -> std::result::Result<(), String> {
        let mut daemon_client_guard = self.daemon_client.lock().await;
        if daemon_client_guard.is_none() {
            // First, try to connect to existing daemon
            let connect_result = DaemonClient::connect("127.0.0.1:9999", "hakana-lsp").await
                .map_err(|e| format!("Connection failed: {}", e));

            match connect_result {
                Ok((client, notification_rx)) => {
                    self.client
                        .log_message(MessageType::INFO, "Connected to existing Hakana daemon")
                        .await;
                    self.setup_daemon_connection(client, notification_rx, &mut daemon_client_guard).await;
                }
                Err(_) => {
                    // No daemon running, try to start one
                    self.client
                        .log_message(MessageType::INFO, "Daemon not running, attempting to start...")
                        .await;

                    if let Err(e) = self.start_daemon_and_connect(&mut daemon_client_guard).await {
                        let error_msg = format!("Failed to start daemon: {}", e);
                        self.client
                            .log_message(MessageType::ERROR, &error_msg)
                            .await;
                        return Err(error_msg);
                    }
                }
            }
        }
        Ok(())
    }

    async fn setup_daemon_connection(
        &self,
        client: DaemonClient,
        notification_rx: mpsc::Receiver<hakana_daemon_server::protocol::Notification>,
        daemon_client_guard: &mut tokio::sync::MutexGuard<'_, Option<DaemonClient>>,
    ) {
        **daemon_client_guard = Some(client);

        // Store notification receiver
        {
            let mut notification_rx_guard = self.notification_rx.lock().await;
            *notification_rx_guard = Some(notification_rx);
        }

        // Start notification handling
        let lsp_client = self.client.clone();
        let notification_rx_arc = Arc::clone(&self.notification_rx);

        tokio::spawn(async move {
            Self::handle_daemon_notifications(lsp_client, notification_rx_arc).await;
        });
    }

    async fn start_daemon_and_connect(&self, daemon_client_guard: &mut tokio::sync::MutexGuard<'_, Option<DaemonClient>>) -> std::result::Result<(), String> {
        // Try to find the daemon binary
        let daemon_binary = self.find_daemon_binary().await?;

        self.client
            .log_message(MessageType::INFO, &format!("Starting daemon: {}", daemon_binary))
            .await;

        // Start the daemon process as a detached background process
        // Note: Do NOT use kill_on_drop - we want the daemon to persist independently
        let mut daemon_process = Command::new(&daemon_binary)
            .args(&["start"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn daemon process: {}", e))?;

        // Wait for daemon to be ready (with timeout - 2 minutes for initial scan)
        let ready_result = timeout(Duration::from_secs(120), self.wait_for_daemon_ready()).await;

        match ready_result {
            Ok(Ok(())) => {
                self.client
                    .log_message(MessageType::INFO, "Daemon started successfully, connecting...")
                    .await;

                // Now try to connect
                let connect_result = DaemonClient::connect("127.0.0.1:9999", "hakana-lsp").await
                    .map_err(|e| format!("Daemon started but connection failed: {}", e));

                match connect_result {
                    Ok((client, notification_rx)) => {
                        self.client
                            .log_message(MessageType::INFO, "Connected to newly started Hakana daemon")
                            .await;
                        self.setup_daemon_connection(client, notification_rx, daemon_client_guard).await;

                        // Detach the process so it continues running independently
                        // The daemon process handle will be dropped but the process keeps running
                        std::mem::forget(daemon_process);

                        Ok(())
                    }
                    Err(e) => {
                        let _ = daemon_process.kill().await;
                        Err(e)
                    }
                }
            }
            Ok(Err(e)) => {
                let _ = daemon_process.kill().await;
                Err(format!("Daemon failed to become ready: {}", e))
            }
            Err(_) => {
                let _ = daemon_process.kill().await;
                Err("Daemon startup timed out after 2 minutes".to_string())
            }
        }
    }

    async fn find_daemon_binary(&self) -> std::result::Result<String, String> {
        // Get the directory of the current language server binary
        let current_exe = std::env::current_exe()
            .map_err(|e| format!("Could not get current executable path: {}", e))?;

        let exe_dir = current_exe.parent()
            .ok_or("Could not get parent directory of executable")?;

        // Look for hakana-daemon in the same directory as the language server
        let daemon_path = exe_dir.join("hakana-daemon");

        // Test if the daemon binary exists and works
        if let Ok(output) = Command::new(&daemon_path)
            .args(&["--help"])
            .output()
            .await
        {
            if output.status.success() ||
               String::from_utf8_lossy(&output.stderr).contains("Hakana") ||
               String::from_utf8_lossy(&output.stdout).contains("Hakana") {
                return Ok(daemon_path.to_string_lossy().to_string());
            }
        }

        Err(format!(
            "Could not find hakana-daemon binary in the same directory as language server: {}",
            exe_dir.display()
        ))
    }

    async fn wait_for_daemon_ready(&self) -> std::result::Result<(), String> {
        let max_attempts = 120; // 2 minutes max wait time (1 second per attempt)
        let mut attempts = 0;

        while attempts < max_attempts {
            match timeout(Duration::from_secs(2), TcpStream::connect("127.0.0.1:9999")).await {
                Ok(Ok(_)) => {
                    return Ok(());
                }
                Ok(Err(_)) | Err(_) => {
                    attempts += 1;

                    // Log progress every 10 seconds
                    if attempts % 10 == 0 && attempts > 0 {
                        self.client
                            .log_message(
                                MessageType::INFO,
                                &format!("Still waiting for daemon to start... ({} seconds)", attempts)
                            )
                            .await;
                    }

                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        Err("Daemon failed to start within 2 minutes".to_string())
    }

    async fn handle_daemon_notifications(
        lsp_client: Client,
        notification_rx: Arc<Mutex<Option<mpsc::Receiver<hakana_daemon_server::protocol::Notification>>>>,
    ) {
        let rx = {
            let mut guard = notification_rx.lock().await;
            guard.take()
        };

        if let Some(mut rx) = rx {
            while let Some(notification) = rx.recv().await {
                match notification.method.as_str() {
                    "textDocument/publishDiagnostics" => {
                        if let Some(params) = notification.params {
                            if let (Some(uri_val), Some(diagnostics_val)) =
                                (params.get("uri"), params.get("diagnostics")) {
                                if let (Some(uri_str), Some(diagnostics_array)) =
                                    (uri_val.as_str(), diagnostics_val.as_array()) {
                                    if let Ok(url) = Url::parse(uri_str) {
                                        let mut diagnostics = Vec::new();
                                        for diag_val in diagnostics_array {
                                            if let Ok(diagnostic) = serde_json::from_value::<Diagnostic>(diag_val.clone()) {
                                                diagnostics.push(diagnostic);
                                            }
                                        }
                                        lsp_client.publish_diagnostics(url, diagnostics, None).await;
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        // Handle other notifications as needed
                    }
                }
            }
        }
    }

    fn json_to_location(value: &Value) -> Option<Location> {
        let uri_str = value.get("uri")?.as_str()?;
        let range = value.get("range")?;
        let start = range.get("start")?;
        let end = range.get("end")?;

        let uri = Url::parse(uri_str).ok()?;
        let location = Location {
            uri,
            range: Range {
                start: Position {
                    line: start.get("line")?.as_u64()? as u32,
                    character: start.get("character")?.as_u64()? as u32,
                },
                end: Position {
                    line: end.get("line")?.as_u64()? as u32,
                    character: end.get("character")?.as_u64()? as u32,
                },
            },
        };

        Some(location)
    }

    fn json_to_hover(value: &Value) -> Option<Hover> {
        if value.is_null() {
            return None;
        }

        // For now, return a simple hover response
        // This would need to be implemented based on the daemon's response format
        Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String("Hover information".to_string())),
            range: None,
        })
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for DaemonBackend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        if let Err(e) = self.ensure_daemon_connection().await {
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!("Failed to initialize daemon connection: {}", e),
                )
                .await;
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::NONE,
                )),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Hakana language server initialized!")
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        if let Err(_) = self.ensure_daemon_connection().await {
            return Ok(None);
        }

        let daemon_client_guard = self.daemon_client.lock().await;
        if let Some(daemon_client) = daemon_client_guard.as_ref() {
            let uri = params.text_document_position_params.text_document.uri.to_string();
            let position = params.text_document_position_params.position;

            match daemon_client.get_hover(&uri, position.line, position.character).await {
                Ok(result) => Ok(Self::json_to_hover(&result)),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        if let Err(_) = self.ensure_daemon_connection().await {
            return Ok(None);
        }

        let daemon_client_guard = self.daemon_client.lock().await;
        if let Some(daemon_client) = daemon_client_guard.as_ref() {
            let uri = params.text_document_position_params.text_document.uri.to_string();
            let position = params.text_document_position_params.position;

            match daemon_client.get_definition(&uri, position.line, position.character).await {
                Ok(result) => {
                    if let Some(location) = Self::json_to_location(&result) {
                        Ok(Some(GotoDefinitionResponse::Scalar(location)))
                    } else {
                        Ok(None)
                    }
                }
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        if let Err(_) = self.ensure_daemon_connection().await {
            return Ok(None);
        }

        let daemon_client_guard = self.daemon_client.lock().await;
        if let Some(daemon_client) = daemon_client_guard.as_ref() {
            let uri = params.text_document_position.text_document.uri.to_string();
            let position = params.text_document_position.position;
            let include_declaration = params.context.include_declaration;

            match daemon_client.get_references(&uri, position.line, position.character, include_declaration).await {
                Ok(result) => {
                    if let Some(locations_array) = result.as_array() {
                        let mut locations = Vec::new();
                        for location_val in locations_array {
                            if let Some(location) = Self::json_to_location(location_val) {
                                locations.push(location);
                            }
                        }
                        Ok(Some(locations))
                    } else {
                        Ok(Some(Vec::new()))
                    }
                }
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        if let Err(_) = self.ensure_daemon_connection().await {
            return Ok(None);
        }

        let daemon_client_guard = self.daemon_client.lock().await;
        if let Some(daemon_client) = daemon_client_guard.as_ref() {
            match daemon_client.search_symbols(&params.query).await {
                Ok(_result) => {
                    // TODO: Convert daemon response to SymbolInformation
                    Ok(Some(Vec::new()))
                }
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}