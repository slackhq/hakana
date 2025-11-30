use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hakana_analyzer::config::{self, Config};
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::code_location::HPos;
#[cfg(target_arch = "wasm32")]
use hakana_orchestrator::SuccessfulScanData;
use hakana_orchestrator::file::FileStatus;
#[cfg(not(target_arch = "wasm32"))]
use hakana_orchestrator::{SuccessfulScanData, scan_and_analyze_async};
use hakana_str::Interner;
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[cfg(not(target_arch = "wasm32"))]
pub mod server_client;

#[derive(Debug)]
pub struct Backend {
    client: Client,
    analysis_config: Arc<Config>,
    starter_interner: Arc<Interner>,
    previous_scan_data: RwLock<Option<SuccessfulScanData>>,
    previous_analysis_result: RwLock<Option<AnalysisResult>>,
    all_diagnostics: RwLock<Option<FxHashMap<Url, Vec<Diagnostic>>>>,
    file_changes: RwLock<Option<FxHashMap<String, FileStatus>>>,
    files_with_errors: RwLock<FxHashSet<Url>>,
    /// Connection to the hakana server (used when server mode is enabled)
    #[cfg(not(target_arch = "wasm32"))]
    server_connection: Option<Mutex<server_client::ServerConnection>>,
}

impl Backend {
    pub fn new(client: Client, analysis_config: Config, starter_interner: Interner) -> Self {
        Self {
            client,
            analysis_config: Arc::new(analysis_config),
            starter_interner: Arc::new(starter_interner),
            previous_scan_data: RwLock::new(None),
            previous_analysis_result: RwLock::new(None),
            all_diagnostics: RwLock::new(None),
            file_changes: RwLock::new(None),
            files_with_errors: RwLock::new(FxHashSet::default()),
            #[cfg(not(target_arch = "wasm32"))]
            server_connection: None,
        }
    }

    /// Create a Backend with a server connection.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_server_connection(
        client: Client,
        analysis_config: Config,
        starter_interner: Interner,
        server_connection: Option<Mutex<server_client::ServerConnection>>,
    ) -> Self {
        Self {
            client,
            analysis_config: Arc::new(analysis_config),
            starter_interner: Arc::new(starter_interner),
            previous_scan_data: RwLock::new(None),
            previous_analysis_result: RwLock::new(None),
            all_diagnostics: RwLock::new(None),
            file_changes: RwLock::new(None),
            files_with_errors: RwLock::new(FxHashSet::default()),
            server_connection,
        }
    }

    fn position_to_offset(&self, file_contents: &str, position: Position) -> usize {
        let lines: Vec<&str> = file_contents.lines().collect();
        let mut offset = 0;

        // Add offset for complete lines before the target line
        for (_line_idx, line) in lines.iter().enumerate().take(position.line as usize) {
            offset += line.len() + 1; // +1 for newline character
        }

        // Add offset for characters in the target line
        if let Some(target_line) = lines.get(position.line as usize) {
            offset += std::cmp::min(position.character as usize, target_line.len());
        }

        offset
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        self.do_analysis().await;

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

    async fn initialized(&self, _: InitializedParams) {
        let registration = Registration {
            id: "watch-hack-files".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(
                serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![
                        FileSystemWatcher {
                            glob_pattern: GlobPattern::String("**/*.{hack,php,hhi}".to_string()),
                            kind: None,
                        },
                        FileSystemWatcher {
                            glob_pattern: GlobPattern::String("**/.git/index.lock".to_string()),
                            kind: Some(WatchKind::Delete),
                        },
                        FileSystemWatcher {
                            glob_pattern: GlobPattern::String("**/[!.]*/**/".to_string()),
                            kind: Some(WatchKind::Delete),
                        },
                    ],
                })
                .unwrap(),
            ),
        };

        let registrations = vec![registration];
        self.client
            .register_capability(registrations)
            .await
            .unwrap();

        self.emit_issues().await;

        let mut all_diagnostics = self.all_diagnostics.write().await;
        *all_diagnostics = None;

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut new_file_statuses = FxHashMap::default();

        self.client
            .log_message(MessageType::INFO, "watched files changed")
            .await;

        for file_event in params.changes {
            let change_type = file_event.typ;
            let file_path = file_event.uri.path().to_string();

            if file_path.ends_with(".php")
                || file_path.ends_with(".hack")
                || file_path.ends_with(".hhi")
            {
                match change_type {
                    FileChangeType::CREATED => {
                        new_file_statuses.insert(file_path, FileStatus::Added(0, 0));
                    }
                    FileChangeType::CHANGED => {
                        new_file_statuses.insert(file_path, FileStatus::Modified(0, 0));
                    }
                    FileChangeType::DELETED => {
                        new_file_statuses.insert(file_path, FileStatus::Deleted);
                    }
                    _ => {}
                }
            } else if Path::new(&file_path).extension().is_none() && !file_path.contains("/.git/") {
                if let FileChangeType::DELETED = change_type {
                    new_file_statuses.insert(file_path, FileStatus::DeletedDir);
                }
            }
        }

        if !new_file_statuses.is_empty() {
            let mut existing_file_changes = self.file_changes.write().await;

            if let Some(existing_file_changes) = existing_file_changes.as_mut() {
                existing_file_changes.extend(new_file_statuses);
            } else {
                *existing_file_changes = Some(new_file_statuses);
            }
        } else {
            let file_changes_guard = self.file_changes.read().await;

            if file_changes_guard.is_none() {
                self.client
                    .log_message(MessageType::INFO, "No files updated")
                    .await;
                return;
            }
        }

        if Path::new(".git/index.lock").exists() {
            self.client
                .log_message(MessageType::INFO, "Waiting a sec while git is doing stuff")
                .await;
        } else {
            self.do_analysis().await;
            self.emit_issues().await;
        }
    }

    async fn did_save(&self, _params: DidSaveTextDocumentParams) {
        // File saves are handled by the file watcher
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let file_path = uri.path().to_string();

        let scan_data_guard = self.previous_scan_data.read().await;
        let analysis_result_guard = self.previous_analysis_result.read().await;
        if let Some(scan_data) = scan_data_guard.as_ref() {
            // Convert LSP position to offset
            if let Ok(file_contents) = std::fs::read_to_string(&file_path) {
                let offset = self.position_to_offset(&file_contents, position);

                // Create FilePath from the file path string
                if let Some(file_path_id) = scan_data.interner.get(&file_path) {
                    let file_path_obj = hakana_code_info::code_location::FilePath(file_path_id);

                    // Check member definition locations (methods, properties, constants)
                    if let Some(analysis_result) = analysis_result_guard.as_ref() {
                        if let Some(definition_locations) =
                            analysis_result.definition_locations.get(&file_path_obj)
                        {
                            // Find all matching ranges and pick the most specific (narrowest) one
                            let mut best_match = None;
                            let mut best_range_size = u32::MAX;

                            for ((start_offset, end_offset), (classlike_name, member_name)) in
                                definition_locations
                            {
                                if (offset as u32) >= *start_offset
                                    && (offset as u32) <= *end_offset
                                {
                                    let range_size = end_offset - start_offset;
                                    if range_size < best_range_size {
                                        best_range_size = range_size;
                                        best_match = Some((classlike_name, member_name));
                                    }
                                }
                            }

                            if let Some((classlike_name, member_name)) = best_match {
                                if let Some(pos) = scan_data
                                    .codebase
                                    .get_symbol_pos(classlike_name, member_name)
                                {
                                    return Ok(pos_to_offset(pos, &scan_data.interner));
                                }

                                return Ok(None);
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

fn pos_to_offset(def_pos: HPos, interner: &Interner) -> Option<GotoDefinitionResponse> {
    if let Ok(def_uri) = Url::from_file_path(interner.lookup(&def_pos.file_path.0)) {
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: def_uri,
            range: Range {
                start: Position {
                    line: def_pos.start_line - 1,
                    character: (def_pos.start_column - 1) as u32,
                },
                end: Position {
                    line: def_pos.end_line - 1,
                    character: (def_pos.end_column - 1) as u32,
                },
            },
        }));
    }

    return None;
}

impl Backend {
    #[cfg(not(target_arch = "wasm32"))]
    async fn do_analysis(&self) {
        // If we have a server connection, use it instead of local analysis
        if let Some(ref server_conn) = self.server_connection {
            self.do_analysis_via_server(server_conn).await;
            return;
        }

        // Fall back to local analysis (original behavior)
        self.do_local_analysis().await;
    }

    /// Perform analysis by querying the hakana server.
    #[cfg(not(target_arch = "wasm32"))]
    async fn do_analysis_via_server(&self, server_conn: &Mutex<server_client::ServerConnection>) {
        let mut all_diagnostics_guard = self.all_diagnostics.write().await;

        // First, forward any pending file changes to the server
        let file_changes = {
            let mut file_changes_guard = self.file_changes.write().await;
            file_changes_guard.take()
        };

        if let Some(changes) = file_changes {
            if !changes.is_empty() {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("Forwarding {} file change(s) to server", changes.len()),
                    )
                    .await;

                let mut conn = server_conn.lock().await;
                if let Err(e) = conn.notify_file_changes(changes) {
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to notify server of file changes: {}", e),
                        )
                        .await;
                }
            }
        }

        self.client
            .log_message(MessageType::INFO, "Fetching issues from server")
            .await;

        // Get issues from the server
        let result = {
            let mut conn = server_conn.lock().await;
            conn.get_issues(None, true, true)
        };

        match result {
            Ok(response) => {
                if !response.analysis_complete {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("Server analysis in progress: {} ({}%)", response.phase, response.progress_percent),
                        )
                        .await;
                    // Don't update diagnostics while analysis is in progress
                    return;
                }

                let mut all_diagnostics = FxHashMap::default();

                for issue in response.issues {
                    let file_path = format!("{}/{}", self.analysis_config.root_dir, issue.file_path);

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
                            self.client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Failure to get url from file {}", file_path),
                                )
                                .await;
                        }
                    }
                }

                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("Received {} file(s) with issues from server", all_diagnostics.len()),
                    )
                    .await;

                *all_diagnostics_guard = Some(all_diagnostics);
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to get issues from server: {}", e),
                    )
                    .await;
            }
        }
    }

    /// Perform local analysis (original behavior when no server is connected).
    #[cfg(not(target_arch = "wasm32"))]
    async fn do_local_analysis(&self) {
        let mut previous_scan_data_guard = self.previous_scan_data.write().await;
        let mut previous_analysis_result_guard = self.previous_analysis_result.write().await;
        let mut all_diagnostics_guard = self.all_diagnostics.write().await;

        let successful_scan_data = previous_scan_data_guard.take();

        let analysis_result = previous_analysis_result_guard.take();

        let mut file_changes_guard = self.file_changes.write().await;

        let file_changes = file_changes_guard.take();

        self.client
            .log_message(
                MessageType::INFO,
                format!("scan & analyze changes — {:?}", file_changes),
            )
            .await;

        sleep(Duration::from_millis(10)).await;

        let result = scan_and_analyze_async(
            Vec::new(),
            None,
            None,
            self.analysis_config.clone(),
            8,
            Some(&self.client),
            "",
            self.starter_interner.clone(),
            successful_scan_data,
            analysis_result,
            file_changes,
            None::<fn(hakana_orchestrator::AnalysisProgress)>,
            None,
            None,
            None,
        )
        .await;

        *file_changes_guard = None;

        match result {
            Ok((analysis_result, successful_scan_data)) => {
                let mut all_diagnostics = FxHashMap::default();

                for (file, emitted_issues) in analysis_result.get_all_issues(
                    &successful_scan_data.interner,
                    &self.analysis_config.root_dir,
                    false,
                ) {
                    let mut diagnostics = vec![];
                    for emitted_issue in emitted_issues {
                        diagnostics.push(Diagnostic::new(
                            Range {
                                start: Position {
                                    line: emitted_issue.pos.start_line - 1,
                                    character: emitted_issue.pos.start_column as u32 - 1,
                                },
                                end: Position {
                                    line: emitted_issue.pos.end_line - 1,
                                    character: emitted_issue.pos.end_column as u32 - 1,
                                },
                            },
                            Some(DiagnosticSeverity::ERROR),
                            Some(NumberOrString::String(emitted_issue.kind.to_string())),
                            Some("Hakana".to_string()),
                            emitted_issue.description.clone(),
                            None,
                            None,
                        ));
                    }

                    match Url::from_file_path(&file) {
                        Ok(url) => {
                            all_diagnostics.insert(url, diagnostics);
                        }
                        Err(_) => {
                            self.client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Failure to get url from file {}", file),
                                )
                                .await;
                        }
                    }
                }

                *all_diagnostics_guard = Some(all_diagnostics);
                *previous_scan_data_guard = Some(successful_scan_data);
                *previous_analysis_result_guard = Some(analysis_result);
            }
            Err(error) => {
                *previous_scan_data_guard = None;
                *previous_analysis_result_guard = None;
                *all_diagnostics_guard = None;

                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Analysis failed with error {}", error),
                    )
                    .await;
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn do_analysis(&self) {
        // WASM version doesn't support async analysis
    }

    async fn emit_issues(&self) {
        if let Some(all_diagnostics) = self.all_diagnostics.write().await.as_mut() {
            let mut new_files_with_errors = FxHashSet::default();

            for (uri, diagnostics) in all_diagnostics.drain() {
                self.client
                    .publish_diagnostics(uri.clone(), diagnostics, None)
                    .await;
                new_files_with_errors.insert(uri);
            }

            let mut files_with_errors = self.files_with_errors.write().await;

            for old_uri in files_with_errors.iter() {
                if !new_files_with_errors.contains(old_uri) {
                    self.client
                        .publish_diagnostics(old_uri.clone(), vec![], None)
                        .await;
                }
            }

            *files_with_errors = new_files_with_errors;

            self.client
                .log_message(MessageType::INFO, "Diagnostics sent")
                .await;
        }
    }
}

pub fn get_config(
    plugins: Vec<Box<dyn CustomHook>>,
    cwd: &String,
    interner: &mut Interner,
) -> std::result::Result<Config, String> {
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
    config.collect_goto_definition_locations = true;

    config.hooks = plugins.into_iter().map(std::sync::Arc::from).collect();

    let config_path_str = format!("{}/hakana.json", cwd);

    let config_path = Path::new(&config_path_str);

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, interner)
            .map_err(|err| format!("Config error: {err}"))?;
    }

    Ok(config)
}

/// Serve the language server.
pub async fn serve<Reader, Writer>(
    reader: Reader,
    writer: Writer,
    current_dir: std::io::Result<PathBuf>,
) where
    Reader: AsyncRead + Unpin,
    Writer: AsyncWrite,
{
    serve_with_plugins(reader, writer, current_dir, vec![]).await;
}

/// Serve the language server with custom plugins.
#[cfg(not(target_arch = "wasm32"))]
pub async fn serve_with_plugins<Reader, Writer>(
    reader: Reader,
    writer: Writer,
    current_dir: std::io::Result<PathBuf>,
    plugins: Vec<Box<dyn CustomHook>>,
) where
    Reader: AsyncRead + Unpin,
    Writer: AsyncWrite,
{
    let mut stderr = tokio::io::stderr();

    let cwd = if let Ok(current_dir) = current_dir {
        if let Some(str) = current_dir.to_str() {
            str.to_string()
        } else {
            stderr
                .write_all_buf(&mut Cursor::new(b"Passed current directory is malformed"))
                .await
                .ok();
            return;
        }
    } else {
        stderr
            .write_all_buf(&mut Cursor::new(
                b"Current working directory could not be determined",
            ))
            .await
            .ok();
        return;
    };

    let cwd_path = PathBuf::from(&cwd);

    let mut interner = Interner::default();

    let config = match get_config(plugins, &cwd, &mut interner) {
        Ok(config) => config,
        Err(msg) => {
            stderr.write_all_buf(&mut Cursor::new(msg)).await.ok();
            return;
        }
    };

    // Try to connect to an existing hakana server (don't spawn one)
    // If a server is running, use it for analysis. Otherwise fall back to local analysis.
    let server_connection = match server_client::ServerConnection::try_connect(&cwd_path) {
        Some(conn) => {
            stderr
                .write_all_buf(&mut Cursor::new(b"Connected to hakana server\n"))
                .await
                .ok();
            Some(Mutex::new(conn))
        }
        None => {
            stderr
                .write_all_buf(&mut Cursor::new(b"No hakana server running. Using local analysis.\n"))
                .await
                .ok();
            None
        }
    };

    let (service, socket) = LspService::new(|client| {
        Backend::with_server_connection(client, config, interner, server_connection)
    });

    Server::new(reader, writer, socket).serve(service).await;
}

/// WASM version
#[cfg(target_arch = "wasm32")]
pub async fn serve_with_plugins<Reader, Writer>(
    reader: Reader,
    writer: Writer,
    current_dir: std::io::Result<PathBuf>,
    _plugins: Vec<Box<dyn CustomHook>>,
) where
    Reader: AsyncRead + Unpin,
    Writer: AsyncWrite,
{
    let mut stderr = tokio::io::stderr();

    let cwd = if let Ok(current_dir) = current_dir {
        if let Some(str) = current_dir.to_str() {
            str.to_string()
        } else {
            stderr
                .write_all_buf(&mut Cursor::new(b"Passed current directory is malformed"))
                .await
                .ok();
            return;
        }
    } else {
        stderr
            .write_all_buf(&mut Cursor::new(
                b"Current working directory could not be determined",
            ))
            .await
            .ok();
        return;
    };

    let mut interner = Interner::default();

    let config = match get_config(vec![], &cwd, &mut interner) {
        Ok(config) => config,
        Err(msg) => {
            stderr.write_all_buf(&mut Cursor::new(msg)).await.ok();
            return;
        }
    };

    let (service, socket) = LspService::new(|client| Backend::new(client, config, interner));

    Server::new(reader, writer, socket).serve(service).await;
}

#[cfg(test)]
mod tests;
