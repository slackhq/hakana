use std::ffi::OsStr;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::server_client;
use hakana_analyzer::config::Config;
use hakana_code_info::issue::IssueKind;

use rustc_hash::FxHashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

enum DiagnosticsEvent {
    Analysis(FxHashMap<Url, Vec<Diagnostic>>),
    Edit(Url, Vec<TextDocumentContentChangeEvent>),
}

#[derive(Debug)]
pub struct ServerBasedBackend {
    client: Arc<Client>,
    analysis_config: Arc<Config>,
    server_conn: Arc<server_client::ServerConnection>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<bool>>>>,
    diagnostics_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<DiagnosticsEvent>>>>,
}

/// Issue kinds that duplicate equivalent Hack typechecker issues
/// and should therefore not be reported as an LSP diagnostic.
static EXCLUDED_ISSUE_KINDS: [IssueKind; 2] =
    [IssueKind::UndefinedVariable, IssueKind::TooFewArguments];

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
            diagnostics_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Update existing diagnostics after a file is changed in the editor.
    fn apply_content_changes(
        diagnostics: &mut Vec<Diagnostic>,
        content_changes: &[TextDocumentContentChangeEvent],
    ) -> bool {
        let mut diagnostics_changed = false;

        for content_change in content_changes {
            // For full document replacements, clear out all diagnostics.
            let Some(edit_range) = content_change.range else {
                diagnostics_changed |= !diagnostics.is_empty();
                diagnostics.clear();
                continue;
            };

            let replacement_end = Self::position_after_text(edit_range.start, &content_change.text);

            // Keep diagnostics outside the edit range, shifting their positions as needed
            // to account for the changed content.
            diagnostics.retain_mut(|diagnostic| {
                if Self::ranges_overlap(diagnostic.range, edit_range) {
                    diagnostics_changed = true;
                    return false;
                }

                if diagnostic.range.start >= edit_range.end {
                    let translated_range = Range {
                        start: Self::translate_position(
                            diagnostic.range.start,
                            edit_range.end,
                            replacement_end,
                        ),
                        end: Self::translate_position(
                            diagnostic.range.end,
                            edit_range.end,
                            replacement_end,
                        ),
                    };

                    diagnostics_changed |= translated_range != diagnostic.range;
                    diagnostic.range = translated_range;
                }

                true
            });
        }

        diagnostics_changed
    }

    /// Determine whether two ranges overlap.
    fn ranges_overlap(diagnostic_range: Range, edit_range: Range) -> bool {
        if edit_range.start == edit_range.end {
            diagnostic_range.start <= edit_range.start && edit_range.start < diagnostic_range.end
        } else {
            diagnostic_range.start < edit_range.end && edit_range.start < diagnostic_range.end
        }
    }

    /// Compute the end position of an edit.
    fn position_after_text(start: Position, text: &str) -> Position {
        let line_count = text.bytes().filter(|byte| *byte == b'\n').count() as u32;

        if line_count == 0 {
            return Position {
                line: start.line,
                character: start
                    .character
                    .saturating_add(text.encode_utf16().count() as u32),
            };
        }

        Position {
            line: start.line.saturating_add(line_count),
            character: text
                .rsplit_once('\n')
                .map_or(0, |(_, last_line)| last_line.encode_utf16().count() as u32),
        }
    }

    /// Shift an existing in-editor position based on the end position of the text range replaced by an edit
    /// and the end position of its replacement.
    fn translate_position(
        position: Position,
        replaced_range_end: Position,
        replacement_end: Position,
    ) -> Position {
        if position.line == replaced_range_end.line {
            Position {
                line: replacement_end.line,
                character: replacement_end.character.saturating_add(
                    position
                        .character
                        .saturating_sub(replaced_range_end.character),
                ),
            }
        } else {
            Position {
                line: replacement_end
                    .line
                    .saturating_add(position.line.saturating_sub(replaced_range_end.line)),
                character: position.character,
            }
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
                            format!("Server analysis in progress: {}", response.phase),
                        )
                        .await;
                    // Don't update diagnostics while analysis is in progress
                    return FxHashMap::default();
                }

                let mut all_diagnostics = FxHashMap::default();

                for issue in response.issues {
                    // Don't report issues that have close typechecker equivalents in LSP diagnostics
                    // to reduce clutter.
                    if let Ok(issue_kind) = IssueKind::from_str(&issue.kind)
                        && EXCLUDED_ISSUE_KINDS.contains(&issue_kind)
                    {
                        continue;
                    }

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

                all_diagnostics
            }
            Err(e) => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to get issues from server: {}", e),
                    )
                    .await;

                FxHashMap::default()
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
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: Some(false),
                        will_save_wait_until: Some(false),
                        save: Some(TextDocumentSyncSaveOptions::Supported(false)),
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let file_uri = params.text_document.uri;

        static SUPPORTED_EXTENSIONS: [&str; 2] = ["php", "hack"];

        // Clear diagnostics on changed lines in PHP/Hack files
        // so that we don't show stale diagnostics until they're saved
        // and analysis results come back.
        if let Some(file_ext) = Path::new(file_uri.path())
            .extension()
            .and_then(&OsStr::to_str)
            && SUPPORTED_EXTENSIONS.contains(&file_ext)
            && let Some(diagnostics_tx) = self
                .diagnostics_tx
                .lock()
                .ok()
                .and_then(|tx| tx.as_ref().cloned())
        {
            let _ = diagnostics_tx.send(DiagnosticsEvent::Edit(file_uri, params.content_changes));
        }
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
        let (diagnostics_tx, mut diagnostics_rx) = tokio::sync::mpsc::unbounded_channel();
        let _ = self
            .diagnostics_tx
            .lock()
            .unwrap()
            .insert(diagnostics_tx.clone());

        tokio::spawn(async move {
            client
                .log_message(MessageType::INFO, "started watching for diagnostics")
                .await;

            let analysis_client = client.clone();
            let analysis_config = config.clone();
            let analysis_conn = conn.clone();
            let analysis_handle = tokio::spawn(async move {
                // On startup, allow populating initial diagnostics from warm server state if
                // it exists, then block until subsequent analysis runs.
                let mut block_until_next_analysis = false;

                loop {
                    let all_diagnostics = Self::do_analysis_via_server(
                        &analysis_client,
                        &analysis_config,
                        &analysis_conn,
                        block_until_next_analysis,
                    )
                    .await;
                    block_until_next_analysis = true;

                    if diagnostics_tx
                        .send(DiagnosticsEvent::Analysis(all_diagnostics))
                        .is_err()
                    {
                        analysis_client
                            .log_message(
                                MessageType::ERROR,
                                "error reporting diagnostics from server",
                            )
                            .await;
                        break;
                    }
                }
            });

            let mut diagnostics_by_file: FxHashMap<Url, Vec<Diagnostic>> = FxHashMap::default();

            loop {
                tokio::select! {
                    Some(event) = diagnostics_rx.recv() => {
                        match event {
                            DiagnosticsEvent::Analysis(all_diagnostics) => {
                                for old_uri in diagnostics_by_file.keys() {
                                    if !all_diagnostics.contains_key(old_uri) {
                                        client
                                            .publish_diagnostics(old_uri.clone(), vec![], None)
                                            .await;
                                    }
                                }

                                for (uri, diagnostics) in &all_diagnostics {
                                    client
                                        .publish_diagnostics(
                                            uri.clone(),
                                            diagnostics.clone(),
                                            None,
                                        )
                                        .await;
                                }

                                diagnostics_by_file = all_diagnostics;

                                client
                                    .log_message(MessageType::INFO, "Diagnostics sent")
                                    .await;
                            }
                            DiagnosticsEvent::Edit(file_uri, content_changes) => {
                                let Some(diagnostics) = diagnostics_by_file.get_mut(&file_uri)
                                else {
                                    continue;
                                };

                                if Self::apply_content_changes(diagnostics, &content_changes) {
                                    let diagnostics = diagnostics.clone();
                                    if diagnostics.is_empty() {
                                        diagnostics_by_file.remove(&file_uri);
                                    }

                                    client
                                        .publish_diagnostics(file_uri, diagnostics, None)
                                        .await;
                                }
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        analysis_handle.abort();
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
                if response.found
                    && let (
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
                    )
                {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn position(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn range(start_line: u32, start_character: u32, end_line: u32, end_character: u32) -> Range {
        Range {
            start: position(start_line, start_character),
            end: position(end_line, end_character),
        }
    }

    fn diagnostic(range: Range) -> Diagnostic {
        Diagnostic::new(
            range,
            None,
            None,
            Some("Hakana".to_string()),
            "test diagnostic".to_string(),
            None,
            None,
        )
    }

    fn content_change(range: Option<Range>, text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range,
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn removes_only_overlapping_diagnostics() {
        let retained_range = range(4, 0, 4, 4);
        let mut diagnostics = vec![diagnostic(range(1, 2, 1, 5)), diagnostic(retained_range)];

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[content_change(Some(range(1, 3, 1, 4)), "x")],
        );

        assert!(changed);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].range, retained_range);
    }

    #[test]
    fn preserves_diagnostics_at_edit_boundaries() {
        let before_range = range(1, 0, 1, 2);
        let mut diagnostics = vec![diagnostic(before_range), diagnostic(range(1, 4, 1, 6))];

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[content_change(Some(range(1, 2, 1, 4)), "x")],
        );

        assert!(changed);
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].range, before_range);
        assert_eq!(diagnostics[1].range, range(1, 3, 1, 5));
    }

    #[test]
    fn insertion_overlaps_inside_but_not_at_end() {
        let diagnostic_range = range(2, 2, 2, 6);
        let mut diagnostics = vec![diagnostic(diagnostic_range)];

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[content_change(Some(range(2, 6, 2, 6)), "x")],
        );

        assert!(!changed);
        assert_eq!(diagnostics[0].range, diagnostic_range);

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[content_change(Some(range(2, 4, 2, 4)), "x")],
        );

        assert!(changed);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn translates_surviving_ranges_with_utf16_offsets() {
        let mut diagnostics = vec![diagnostic(range(1, 5, 1, 9))];

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[content_change(Some(range(1, 2, 1, 2)), "😀")],
        );

        assert!(changed);
        assert_eq!(diagnostics[0].range, range(1, 7, 1, 11));
    }

    #[test]
    fn translates_surviving_ranges_across_lines() {
        let mut diagnostics = vec![
            diagnostic(range(2, 8, 2, 12)),
            diagnostic(range(4, 3, 4, 7)),
        ];

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[content_change(Some(range(1, 3, 2, 4)), "x\nyz")],
        );

        assert!(changed);
        assert_eq!(diagnostics[0].range, range(2, 6, 2, 10));
        assert_eq!(diagnostics[1].range, range(4, 3, 4, 7));
    }

    #[test]
    fn applies_multiple_changes_in_order() {
        let mut diagnostics = vec![diagnostic(range(2, 5, 2, 8))];

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[
                content_change(Some(range(0, 0, 0, 0)), "\n"),
                content_change(Some(range(3, 6, 3, 7)), ""),
            ],
        );

        assert!(changed);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn full_document_change_clears_all_diagnostics() {
        let mut diagnostics = vec![diagnostic(range(1, 0, 1, 4)), diagnostic(range(3, 0, 3, 4))];

        let changed = ServerBasedBackend::apply_content_changes(
            &mut diagnostics,
            &[content_change(None, "replacement contents")],
        );

        assert!(changed);
        assert!(diagnostics.is_empty());
    }
}
