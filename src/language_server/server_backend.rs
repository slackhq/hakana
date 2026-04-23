use std::sync::{Arc, Mutex};

use crate::server_client;
use hakana_analyzer::config::Config;
use rustc_hash::{FxHashMap, FxHashSet};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

#[derive(Debug)]
pub struct ServerBasedBackend {
    client: Arc<Client>,
    analysis_config: Arc<Config>,
    server_conn: Arc<server_client::ServerConnection>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<bool>>>>,
}

impl ServerBasedBackend {
    pub fn new(
        client: Client,
        analysis_config: Config,
        server_conn: server_client::ServerConnection,
    ) -> Self {
        Self {
            client: Arc::new(client),
            analysis_config: Arc::new(analysis_config),
            server_conn: Arc::new(server_conn),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }
    /// Perform analysis by querying the hakana server.
    async fn do_analysis_via_server(
        client: &Arc<Client>,
        analysis_config: &Arc<Config>,
        server_conn: &Arc<server_client::ServerConnection>,
        block_until_next_analysis: bool,
    ) -> FxHashMap<Url, Vec<Diagnostic>> {
        client
            .log_message(MessageType::INFO, "Fetching issues from server")
            .await;

        // Get issues from the server
        let result = server_conn
            .get_issues(None, true, true, block_until_next_analysis)
            .await;

        match result {
            Ok(response) => {
                if !response.analysis_complete {
                    client
                        .log_message(
                            MessageType::INFO,
                            format!(
                                "Server analysis in progress: {} ({}%)",
                                response.phase, response.progress_percent
                            ),
                        )
                        .await;
                    // Don't update diagnostics while analysis is in progress
                    return FxHashMap::default();
                }

                let mut all_diagnostics = FxHashMap::default();

                for issue in response.issues {
                    let file_path = format!("{}/{}", analysis_config.root_dir, issue.file_path);

                    let diagnostic = Diagnostic::new(
                        Range {
                            start: Position {
                                line: issue.start_line - 1,
                                character: issue.start_column as u32 - 1,
                            },
                            end: Position {
                                line: issue.end_line - 1,
                                character: issue.end_column as u32 - 1,
                            },
                        },
                        Some(DiagnosticSeverity::ERROR),
                        Some(NumberOrString::String(issue.kind)),
                        Some("Hakana".to_string()),
                        issue.description,
                        None,
                        None,
                    );

                    match Url::from_file_path(&file_path) {
                        Ok(url) => {
                            all_diagnostics
                                .entry(url)
                                .or_insert_with(Vec::new)
                                .push(diagnostic);
                        }
                        Err(_) => {
                            client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Failure to get url from file {}", file_path),
                                )
                                .await;
                        }
                    }
                }

                client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "Received {} file(s) with issues from server",
                            all_diagnostics.len()
                        ),
                    )
                    .await;

                return all_diagnostics;
            }
            Err(e) => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to get issues from server: {}", e),
                    )
                    .await;

                return FxHashMap::default();
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for ServerBasedBackend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::NONE),
                        will_save: Some(false),
                        will_save_wait_until: Some(false),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn did_save(&self, _params: DidSaveTextDocumentParams) {
        // handled by the server
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(None)
    }

    async fn initialized(&self, _: InitializedParams) {
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        let _ = self.shutdown_tx.lock().unwrap().insert(shutdown_tx);

        let client = self.client.clone();
        let config = self.analysis_config.clone();
        let conn = self.server_conn.clone();

        tokio::spawn(async move {
            client
                .log_message(MessageType::INFO, "started watching for diagnostics")
                .await;

            let mut files_with_errors: FxHashSet<Url> = FxHashSet::default();

            // On startup, allow populating initial diagnostics from warm server state if exists,
            // then block until subsequent analysis runs.
            let mut block_until_next_analysis = false;

            loop {
                tokio::select! {
                    all_diagnostics = Self::do_analysis_via_server(&client, &config, &conn, block_until_next_analysis) => {
                        block_until_next_analysis = true;
                        let mut new_files_with_errors = FxHashSet::default();

                        for (uri, diagnostics) in all_diagnostics {
                            client
                                .publish_diagnostics(uri.clone(), diagnostics, None)
                                .await;
                            new_files_with_errors.insert(uri);
                        }

                        for old_uri in files_with_errors.iter() {
                            if !new_files_with_errors.contains(old_uri) {
                                client
                                    .publish_diagnostics(old_uri.clone(), vec![], None)
                                    .await;
                            }
                        }

                        files_with_errors = new_files_with_errors;

                        client
                            .log_message(MessageType::INFO, "Diagnostics sent")
                            .await;
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let file_path = uri.path().to_string();

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Forwarding goto-definition to server: {}:{}:{}",
                    file_path,
                    position.line + 1,
                    position.character + 1
                ),
            )
            .await;

        // Convert absolute path to relative path for the server
        let relative_path = if file_path.starts_with(&self.analysis_config.root_dir) {
            file_path
                .strip_prefix(&self.analysis_config.root_dir)
                .and_then(|p| p.strip_prefix('/'))
                .unwrap_or(&file_path)
                .to_string()
        } else {
            file_path.to_string()
        };

        let result = {
            self.server_conn
                .goto_definition(
                    relative_path,
                    position.line + 1, // LSP is 0-indexed, server expects 1-indexed
                    position.character + 1,
                )
                .await
        };

        match result {
            Ok(response) => {
                if response.found {
                    if let (
                        Some(def_file_path),
                        Some(start_line),
                        Some(start_column),
                        Some(end_line),
                        Some(end_column),
                    ) = (
                        response.file_path,
                        response.start_line,
                        response.start_column,
                        response.end_line,
                        response.end_column,
                    ) {
                        self.client
                            .log_message(
                                MessageType::INFO,
                                format!(
                                    "Definition found: {}:{}:{}",
                                    def_file_path, start_line, start_column
                                ),
                            )
                            .await;

                        if let Ok(def_uri) = Url::from_file_path(&def_file_path) {
                            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                                uri: def_uri,
                                range: Range {
                                    start: Position {
                                        line: start_line - 1, // Convert back to 0-indexed for LSP
                                        character: (start_column - 1) as u32,
                                    },
                                    end: Position {
                                        line: end_line - 1,
                                        character: (end_column - 1) as u32,
                                    },
                                },
                            })));
                        }
                    }
                }
                self.client
                    .log_message(MessageType::INFO, "Definition not found")
                    .await;
                Ok(None)
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to get definition from server: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }

    async fn shutdown(&self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.lock().ok().and_then(|mut o| o.take()) {
            let _ = shutdown_tx.send(true);
        }

        Ok(())
    }
}
