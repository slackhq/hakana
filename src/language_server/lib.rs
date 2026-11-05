use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hakana_analyzer::config::{self, Config};
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::code_location::HPos;
use hakana_orchestrator::file::FileStatus;
use hakana_orchestrator::{SuccessfulScanData, scan_and_analyze};
use hakana_str::Interner;
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::lsp::jsonrpc::Result;
use crate::lsp::types::*;
use crate::lsp::{Client, LanguageServer, LspService, Server};
use crate::server_backend::ServerBasedBackend;

pub mod lsp;
pub mod server_backend;
pub mod server_client;
mod server_factory;

#[derive(Debug)]
pub struct Backend {
    client: Client,
    analysis_config: Arc<Config>,
    starter_interner: Arc<Interner>,
    previous_scan_data: Option<SuccessfulScanData>,
    previous_analysis_result: Option<AnalysisResult>,
    all_diagnostics: Option<FxHashMap<Url, Vec<Diagnostic>>>,
    file_changes: Option<FxHashMap<String, FileStatus>>,
    files_with_errors: FxHashSet<Url>,
}

impl Backend {
    pub fn new(client: Client, analysis_config: Config, starter_interner: Interner) -> Self {
        Self {
            client,
            analysis_config: Arc::new(analysis_config),
            starter_interner: Arc::new(starter_interner),
            previous_scan_data: None,
            previous_analysis_result: None,
            all_diagnostics: None,
            file_changes: None,
            files_with_errors: FxHashSet::default(),
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

fn file_event_to_status(file_event: FileEvent) -> Option<(String, FileStatus)> {
    let file_path = file_event.uri.to_file_path().ok()?;
    let is_git_path = file_path
        .components()
        .any(|component| component.as_os_str() == ".git");
    let is_source_file = file_path
        .extension()
        .is_some_and(|extension| extension == "php" || extension == "hack" || extension == "hhi");
    let file_path = file_path.to_string_lossy().into_owned();

    let status = if is_source_file {
        match file_event.typ {
            FileChangeType::CREATED => FileStatus::Added(0, 0),
            FileChangeType::CHANGED => FileStatus::Modified(0, 0),
            FileChangeType::DELETED => FileStatus::Deleted,
            _ => return None,
        }
    } else if !is_git_path && file_event.typ == FileChangeType::DELETED {
        FileStatus::DeletedDir
    } else {
        return None;
    };

    Some((file_path, status))
}

impl LanguageServer for Backend {
    async fn initialize(&mut self) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: TextDocumentSyncOptions {
                    open_close: true,
                    change: TextDocumentSyncKind::NONE,
                    will_save: false,
                    will_save_wait_until: false,
                    save: true,
                },
                definition_provider: true,
            },
        })
    }

    async fn initialized(&mut self) {
        let registration = Registration {
            id: "watch-hack-files".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(
                serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![
                        FileSystemWatcher {
                            glob_pattern: "**/*.{hack,php,hhi}".to_string(),
                            kind: None,
                        },
                        FileSystemWatcher {
                            glob_pattern: "**/.git/index.lock".to_string(),
                            kind: Some(WatchKind::DELETE),
                        },
                        FileSystemWatcher {
                            glob_pattern: "**/[!.]*/**/".to_string(),
                            kind: Some(WatchKind::DELETE),
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

        self.do_analysis().await;
        self.emit_issues();

        self.client
            .log_message(MessageType::INFO, "server initialized!");
    }

    async fn did_change_watched_files(&mut self, params: DidChangeWatchedFilesParams) {
        let mut new_file_statuses = FxHashMap::default();

        self.client
            .log_message(MessageType::INFO, "watched files changed");

        for file_event in params.changes {
            if let Some((file_path, status)) = file_event_to_status(file_event) {
                new_file_statuses.insert(file_path, status);
            }
        }

        if !new_file_statuses.is_empty() {
            self.file_changes
                .get_or_insert_default()
                .extend(new_file_statuses);
        } else if self.file_changes.is_none() {
            self.client
                .log_message(MessageType::INFO, "No files updated");
            return;
        }

        if Path::new(".git/index.lock").exists() {
            self.client
                .log_message(MessageType::INFO, "Waiting a sec while git is doing stuff");
        } else {
            self.do_analysis().await;
            self.emit_issues();
        }
    }

    async fn goto_definition(&mut self, params: GotoDefinitionParams) -> Result<Option<Location>> {
        let position = params.position;
        let uri = params.text_document.uri;
        let Ok(file_path) = uri.to_file_path() else {
            return Ok(None);
        };
        let file_path = file_path.to_string_lossy().into_owned();

        if let Some(scan_data) = self.previous_scan_data.as_ref() {
            // Convert LSP position to offset
            if let Ok(file_contents) = std::fs::read_to_string(&file_path) {
                let offset = self.position_to_offset(&file_contents, position);

                // Create FilePath from the file path string
                if let Some(file_path_id) = scan_data.interner.get(&file_path) {
                    let file_path_obj = hakana_code_info::code_location::FilePath(file_path_id);

                    // Check member definition locations (methods, properties, constants)
                    if let Some(analysis_result) = self.previous_analysis_result.as_ref()
                        && let Some(definition_locations) =
                            analysis_result.definition_locations.get(&file_path_obj)
                    {
                        // Find all matching ranges and pick the most specific (narrowest) one
                        let mut best_match = None;
                        let mut best_range_size = u32::MAX;

                        for ((start_offset, end_offset), (classlike_name, member_name)) in
                            definition_locations
                        {
                            if (offset as u32) >= *start_offset && (offset as u32) <= *end_offset {
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

        Ok(None)
    }
}

fn pos_to_offset(def_pos: HPos, interner: &Interner) -> Option<Location> {
    if let Ok(def_uri) = Url::from_file_path(interner.lookup(&def_pos.file_path.0)) {
        return Some(Location {
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
        });
    }

    None
}

impl Backend {
    async fn do_analysis(&mut self) {
        let successful_scan_data = self.previous_scan_data.take();

        let analysis_result = self.previous_analysis_result.take();

        let file_changes = self.file_changes.take();

        self.client.log_message(
            MessageType::INFO,
            format!("scan & analyze changes — {:?}", file_changes),
        );

        let analysis_config = self.analysis_config.clone();
        let interner = self.starter_interner.clone();

        let (log_tx, mut log_rx) = tokio::sync::mpsc::channel(64);

        let mut task_result = tokio::task::spawn_blocking(move || {
            hakana_logger::init_channel_logger(log_tx, log::LevelFilter::Info);

            scan_and_analyze(
                Vec::new(),
                None,
                None,
                analysis_config,
                None,
                8,
                false,
                "",
                interner,
                successful_scan_data,
                analysis_result,
                file_changes,
                || {},
            )
        });

        loop {
            tokio::select! {
                Some(log_message) = log_rx.recv() => {
                    self.client.log_message(MessageType::INFO, log_message);
                },
                Ok(result) = (&mut task_result) => {
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
                                    diagnostics.push(Diagnostic {
                                        range: Range {
                                            start: Position {
                                                line: emitted_issue.pos.start_line - 1,
                                                character: emitted_issue.pos.start_column as u32 - 1,
                                            },
                                            end: Position {
                                                line: emitted_issue.pos.end_line - 1,
                                                character: emitted_issue.pos.end_column as u32 - 1,
                                            },
                                        },
                                        severity: Some(get_diagnostic_severity(&emitted_issue.kind.to_string())),
                                        code: Some(emitted_issue.kind.to_string()),
                                        source: Some("Hakana".to_string()),
                                        message: emitted_issue.description.clone(),
                                    });
                                }

                                match Url::from_file_path(&file) {
                                    Ok(url) => {
                                        all_diagnostics.insert(url, diagnostics);
                                    }
                                    Err(_) => {
                                        self.client.log_message(
                                            MessageType::ERROR,
                                            format!("Failure to get url from file {}", file),
                                        );
                                    }
                                }
                            }

                            self.all_diagnostics = Some(all_diagnostics);
                            self.previous_scan_data = Some(successful_scan_data);
                            self.previous_analysis_result = Some(analysis_result);
                        }
                        Err(error) => {
                            self.all_diagnostics = None;

                            self.client.log_message(
                                MessageType::ERROR,
                                format!("Analysis failed with error {}", error),
                            );
                        }
                    }
                    break;
                }
            }
        }
    }

    fn emit_issues(&mut self) {
        // Consume each analysis result once, so publishing again without a new
        // analysis doesn't clear the diagnostics already sent.
        if let Some(all_diagnostics) = self.all_diagnostics.take() {
            let mut new_files_with_errors = FxHashSet::default();

            for (uri, diagnostics) in all_diagnostics {
                self.client.publish_diagnostics(uri.clone(), diagnostics);
                new_files_with_errors.insert(uri);
            }

            for old_uri in self.files_with_errors.iter() {
                if !new_files_with_errors.contains(old_uri) {
                    self.client.publish_diagnostics(old_uri.clone(), vec![]);
                }
            }

            self.files_with_errors = new_files_with_errors;

            self.client
                .log_message(MessageType::INFO, "Diagnostics sent");
        }
    }
}

/// Unused code issues are reported as warnings so they are visually distinct from
/// type errors in the editor.
pub(crate) fn get_diagnostic_severity(issue_kind: &str) -> DiagnosticSeverity {
    if issue_kind.starts_with("Unused") {
        DiagnosticSeverity::WARNING
    } else {
        DiagnosticSeverity::ERROR
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

    // Try to connect to an existing hakana server and use it for analysis. or start one.
    // Fall back to local analysis if we can't start the server.
    let server_connection =
        match server_client::ServerConnection::connect_or_spawn(&cwd_path, None).await {
            Ok(conn) => {
                stderr
                    .write_all_buf(&mut Cursor::new(b"Connected to hakana server\n"))
                    .await
                    .ok();
                Some(conn)
            }
            Err(e) => {
                stderr
                    .write_all_buf(&mut Cursor::new(format!(
                        "Error starting hakana server: {} . Using local analysis.\n",
                        e
                    )))
                    .await
                    .ok();
                None
            }
        };

    if let Some(server_conn) = server_connection {
        let (service, socket) =
            LspService::new(|client| ServerBasedBackend::new(client, config, server_conn));
        Server::new(reader, writer, socket).serve(service).await;
        return;
    }

    let (service, socket) = LspService::new(|client| Backend::new(client, config, interner));

    Server::new(reader, writer, socket).serve(service).await;
}

#[cfg(test)]
mod tests;
