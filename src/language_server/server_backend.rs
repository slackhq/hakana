use std::ffi::OsStr;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{get_diagnostic_severity, server_client};
use hakana_analyzer::config::Config;
use hakana_code_info::issue::IssueKind;

use rustc_hash::FxHashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

enum DiagnosticsEvent {
    Analysis(FxHashMap<Url, Vec<Diagnostic>>),
    Edit(Url, Vec<TextDocumentContentChangeEvent>),
    Opened(Url),
    Saved(Url),
    Closed(Url),
    /// The server has let go of its results and is analysing everything again.
    Rescanning,
    /// An analysis failed.
    AnalysisFailed,
}

/// What asking the server for its issues came to. The generations are those of
/// the server's responses, 0 from a server that predates them.
enum FetchedIssues {
    /// The diagnostics of a completed analysis, by file.
    Analysis {
        generation: u64,
        diagnostics: FxHashMap<Url, Vec<Diagnostic>>,
    },
    /// The server has no results for now: it is analysing the whole codebase
    /// again.
    Rescanning { generation: u64 },
    /// The server answered, but has no completed analysis to report: the
    /// analysis failed. This says nothing about the code.
    NoResult { generation: u64 },
    /// The server could not be reached.
    Unreachable,
}

/// Where an edit happened, without the text: all that is needed to move the
/// diagnostics around it.
#[derive(Clone, Copy)]
struct BufferEdit {
    /// The range that was replaced. `None` when the whole document was.
    replaced: Option<Range>,
    /// Where the replacement text ends.
    replacement_end: Position,
}

impl BufferEdit {
    const WHOLE_DOCUMENT: Self = Self {
        replaced: None,
        replacement_end: Position {
            line: 0,
            character: 0,
        },
    };

    fn new(content_change: &TextDocumentContentChangeEvent) -> Self {
        match content_change.range {
            Some(range) => Self {
                replaced: Some(range),
                replacement_end: ServerBasedBackend::position_after_text(
                    range.start,
                    &content_change.text,
                ),
            },
            None => Self::WHOLE_DOCUMENT,
        }
    }
}

/// The diagnostics the editor is showing, and how its buffers differ from the
/// files the server analyses.
///
/// The server reports positions in the files on disk. A buffer with unsaved
/// edits has moved on from that, so its diagnostics are moved the same way -
/// those on screen when an edit is made, and those of every analysis that
/// arrives until the buffer is saved.
///
/// While the server has no results - it is analysing the whole codebase again -
/// there are no diagnostics to show, and the ones from before may no longer
/// hold. A single notice says so instead, on one of the open documents.
#[derive(Default)]
struct PublishedDiagnostics {
    by_file: FxHashMap<Url, Vec<Diagnostic>>,
    unsaved_edits: FxHashMap<Url, Vec<BufferEdit>>,
    /// The documents open in the editor, the one opened last at the end.
    open_documents: Vec<Url>,
    notice: Option<Notice>,
}

/// What is shown in place of diagnostics while there are none to show.
struct Notice {
    message: &'static str,
    /// The document it is shown on: none while no document is open.
    shown_on: Option<Url>,
}

impl Notice {
    const RESCANNING: &'static str =
        "Hakana is rescanning the codebase. Its diagnostics will be back when it is done.";
    const ANALYSIS_FAILED: &'static str =
        "Hakana could not analyse the codebase. It will try again when a file changes.";

    fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            Range {
                start: Position::new(0, 0),
                end: Position::new(0, 1),
            },
            Some(DiagnosticSeverity::ERROR),
            None,
            Some("Hakana".to_string()),
            self.message.to_string(),
            None,
            None,
        )
    }
}

impl PublishedDiagnostics {
    /// Past this many unsaved edits to one buffer they are no longer tracked one
    /// by one: its diagnostics stay hidden until it is saved.
    const MAX_UNSAVED_EDITS: usize = 10_000;

    /// Take a new analysis result. Returns what to publish, in order.
    fn apply_analysis(
        &mut self,
        mut all_diagnostics: FxHashMap<Url, Vec<Diagnostic>>,
    ) -> Vec<(Url, Vec<Diagnostic>)> {
        for (uri, edits) in &self.unsaved_edits {
            if let Some(diagnostics) = all_diagnostics.get_mut(uri) {
                ServerBasedBackend::apply_edits(diagnostics, edits);
                if diagnostics.is_empty() {
                    all_diagnostics.remove(uri);
                }
            }
        }

        let mut to_publish = vec![];

        // There are diagnostics again, so the notice goes.
        if let Some(Notice {
            shown_on: Some(uri),
            ..
        }) = self.notice.take()
            && !all_diagnostics.contains_key(&uri)
        {
            to_publish.push((uri, vec![]));
        }

        for old_uri in self.by_file.keys() {
            if !all_diagnostics.contains_key(old_uri) {
                to_publish.push((old_uri.clone(), vec![]));
            }
        }

        for (uri, diagnostics) in &all_diagnostics {
            to_publish.push((uri.clone(), diagnostics.clone()));
        }

        self.by_file = all_diagnostics;

        to_publish
    }

    /// Take an edit to a buffer. Returns the file's diagnostics if they changed.
    fn apply_edit(
        &mut self,
        uri: &Url,
        content_changes: &[TextDocumentContentChangeEvent],
    ) -> Option<Vec<Diagnostic>> {
        let edits: Vec<BufferEdit> = content_changes.iter().map(BufferEdit::new).collect();

        let unsaved_edits = self.unsaved_edits.entry(uri.clone()).or_default();
        if edits.iter().any(|edit| edit.replaced.is_none())
            || unsaved_edits.len() + edits.len() > Self::MAX_UNSAVED_EDITS
        {
            // Nothing before a whole-document replacement matters any more.
            *unsaved_edits = vec![BufferEdit::WHOLE_DOCUMENT];
        } else {
            unsaved_edits.extend(&edits);
        }

        let diagnostics = self.by_file.get_mut(uri)?;

        if !ServerBasedBackend::apply_edits(diagnostics, &edits) {
            return None;
        }

        let diagnostics = diagnostics.clone();
        if diagnostics.is_empty() {
            self.by_file.remove(uri);
        }

        Some(diagnostics)
    }

    /// The buffer matches the file on disk again: it was saved, opened or closed.
    fn matches_disk(&mut self, uri: &Url) {
        self.unsaved_edits.remove(uri);
    }

    /// Replace the diagnostics with a notice. Returns what to publish, in order.
    fn show_notice(&mut self, message: &'static str) -> Vec<(Url, Vec<Diagnostic>)> {
        let mut to_publish: Vec<_> = self.by_file.drain().map(|(uri, _)| (uri, vec![])).collect();

        let shown_on = match self.notice.take() {
            Some(notice) if notice.message == message => {
                // It is there already.
                self.notice = Some(notice);
                return to_publish;
            }
            Some(notice) => notice.shown_on,
            None => self.open_documents.last().cloned(),
        };
        let notice = Notice { message, shown_on };
        if let Some(uri) = &notice.shown_on {
            to_publish.push((uri.clone(), vec![notice.diagnostic()]));
        }
        self.notice = Some(notice);

        to_publish
    }

    /// An analysis failed. The diagnostics on screen stay; but if what is on
    /// screen is the notice that results are on their way, they no longer are.
    fn analysis_failed(&mut self) -> Vec<(Url, Vec<Diagnostic>)> {
        if self.notice.is_some() {
            self.show_notice(Notice::ANALYSIS_FAILED)
        } else {
            vec![]
        }
    }

    /// A document was opened. Returns what to publish.
    fn opened(&mut self, uri: Url) -> Vec<(Url, Vec<Diagnostic>)> {
        self.matches_disk(&uri);
        self.open_documents.retain(|open| *open != uri);
        self.open_documents.push(uri.clone());

        // A notice that had no document to be shown on has one now.
        match &mut self.notice {
            Some(notice) if notice.shown_on.is_none() => {
                notice.shown_on = Some(uri.clone());
                vec![(uri, vec![notice.diagnostic()])]
            }
            _ => vec![],
        }
    }

    /// A document was closed. Returns what to publish, in order.
    fn closed(&mut self, uri: Url) -> Vec<(Url, Vec<Diagnostic>)> {
        self.matches_disk(&uri);
        self.open_documents.retain(|open| *open != uri);

        // The notice moves on from a document that is no longer in view.
        match &mut self.notice {
            Some(notice) if notice.shown_on.as_ref() == Some(&uri) => {
                notice.shown_on = self.open_documents.last().cloned();
                let mut to_publish = vec![(uri, vec![])];
                if let Some(next) = &notice.shown_on {
                    to_publish.push((next.clone(), vec![notice.diagnostic()]));
                }
                to_publish
            }
            _ => vec![],
        }
    }
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
    /// How long to wait before asking the server again after it could not be
    /// reached, doubling up to the maximum while it stays unreachable.
    const MIN_RETRY_DELAY: Duration = Duration::from_millis(250);
    const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

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
    #[cfg(test)]
    fn apply_content_changes(
        diagnostics: &mut Vec<Diagnostic>,
        content_changes: &[TextDocumentContentChangeEvent],
    ) -> bool {
        let edits: Vec<BufferEdit> = content_changes.iter().map(BufferEdit::new).collect();
        Self::apply_edits(diagnostics, &edits)
    }

    /// Move diagnostics to where the edits left the code they are about, dropping
    /// those whose code was edited. Returns whether anything changed.
    fn apply_edits(diagnostics: &mut Vec<Diagnostic>, edits: &[BufferEdit]) -> bool {
        let mut diagnostics_changed = false;

        for edit in edits {
            // For full document replacements, clear out all diagnostics.
            let Some(edit_range) = edit.replaced else {
                diagnostics_changed |= !diagnostics.is_empty();
                diagnostics.clear();
                continue;
            };

            let replacement_end = edit.replacement_end;

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

    fn send_diagnostics_event(&self, event: DiagnosticsEvent) {
        if let Some(diagnostics_tx) = self
            .diagnostics_tx
            .lock()
            .ok()
            .and_then(|tx| tx.as_ref().cloned())
        {
            let _ = diagnostics_tx.send(event);
        }
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
        last_seen_generation: u64,
    ) -> FetchedIssues {
        client
            .log_message(MessageType::INFO, "Fetching issues from server")
            .await;

        // Get issues from the server
        let result = server_conn
            .get_issues(
                None,
                true,
                true,
                block_until_next_analysis,
                Some(last_seen_generation),
            )
            .await;

        match result {
            Ok(response) => {
                let generation = response.generation;

                if response.rescanning {
                    client
                        .log_message(MessageType::INFO, "Server is rescanning the codebase")
                        .await;
                    return FetchedIssues::Rescanning { generation };
                }

                if !response.analysis_complete {
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Server analysis in progress: {}", response.phase),
                        )
                        .await;
                    // Don't update diagnostics while analysis is in progress
                    return FetchedIssues::NoResult { generation };
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
                        Some(get_diagnostic_severity(&issue.kind)),
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

                FetchedIssues::Analysis {
                    generation,
                    diagnostics: all_diagnostics,
                }
            }
            Err(e) => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to get issues from server: {}", e),
                    )
                    .await;

                FetchedIssues::Unreachable
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
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.send_diagnostics_event(DiagnosticsEvent::Opened(params.text_document.uri));
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.send_diagnostics_event(DiagnosticsEvent::Saved(params.text_document.uri));
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.send_diagnostics_event(DiagnosticsEvent::Closed(params.text_document.uri));
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
        {
            self.send_diagnostics_event(DiagnosticsEvent::Edit(file_uri, params.content_changes));
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
                // What the server last told us, 0 for nothing yet. Given this, the server
                // answers at once if it has had something else to say since - without
                // it, an analysis that completes between two requests would be missed.
                let mut last_seen_generation = 0;
                let mut retry_delay = Self::MIN_RETRY_DELAY;

                loop {
                    let fetched = Self::do_analysis_via_server(
                        &analysis_client,
                        &analysis_config,
                        &analysis_conn,
                        block_until_next_analysis,
                        last_seen_generation,
                    )
                    .await;

                    // Only a completed analysis says what the diagnostics are. Anything
                    // else leaves those on screen as they are.
                    let all_diagnostics = match fetched {
                        FetchedIssues::Analysis {
                            generation,
                            diagnostics,
                        } => {
                            block_until_next_analysis = true;
                            last_seen_generation = generation;
                            retry_delay = Self::MIN_RETRY_DELAY;
                            diagnostics
                        }
                        FetchedIssues::Rescanning { generation } => {
                            block_until_next_analysis = true;
                            last_seen_generation = generation;
                            retry_delay = Self::MIN_RETRY_DELAY;
                            if diagnostics_tx.send(DiagnosticsEvent::Rescanning).is_err() {
                                break;
                            }
                            continue;
                        }
                        FetchedIssues::NoResult { generation } => {
                            block_until_next_analysis = true;
                            last_seen_generation = generation;
                            retry_delay = Self::MIN_RETRY_DELAY;
                            if diagnostics_tx
                                .send(DiagnosticsEvent::AnalysisFailed)
                                .is_err()
                            {
                                break;
                            }
                            tokio::time::sleep(retry_delay).await;
                            continue;
                        }
                        FetchedIssues::Unreachable => {
                            // The server that answers next may be a new one: take
                            // whatever it has, rather than wait for it to change.
                            block_until_next_analysis = false;
                            tokio::time::sleep(retry_delay).await;
                            retry_delay = (retry_delay * 2).min(Self::MAX_RETRY_DELAY);
                            continue;
                        }
                    };

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

            let mut published = PublishedDiagnostics::default();

            loop {
                tokio::select! {
                    Some(event) = diagnostics_rx.recv() => {
                        match event {
                            DiagnosticsEvent::Analysis(all_diagnostics) => {
                                for (uri, diagnostics) in published.apply_analysis(all_diagnostics) {
                                    client.publish_diagnostics(uri, diagnostics, None).await;
                                }

                                client
                                    .log_message(MessageType::INFO, "Diagnostics sent")
                                    .await;
                            }
                            DiagnosticsEvent::Edit(file_uri, content_changes) => {
                                if let Some(diagnostics) =
                                    published.apply_edit(&file_uri, &content_changes)
                                {
                                    client
                                        .publish_diagnostics(file_uri, diagnostics, None)
                                        .await;
                                }
                            }
                            DiagnosticsEvent::Saved(file_uri) => {
                                published.matches_disk(&file_uri);
                            }
                            DiagnosticsEvent::Opened(file_uri) => {
                                for (uri, diagnostics) in published.opened(file_uri) {
                                    client.publish_diagnostics(uri, diagnostics, None).await;
                                }
                            }
                            DiagnosticsEvent::Closed(file_uri) => {
                                for (uri, diagnostics) in published.closed(file_uri) {
                                    client.publish_diagnostics(uri, diagnostics, None).await;
                                }
                            }
                            DiagnosticsEvent::Rescanning => {
                                for (uri, diagnostics) in
                                    published.show_notice(Notice::RESCANNING)
                                {
                                    client.publish_diagnostics(uri, diagnostics, None).await;
                                }
                            }
                            DiagnosticsEvent::AnalysisFailed => {
                                for (uri, diagnostics) in published.analysis_failed() {
                                    client.publish_diagnostics(uri, diagnostics, None).await;
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
