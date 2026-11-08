use crate::{file_event_to_status, serve};
use hakana_orchestrator::file::FileStatus;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::time::{Duration, timeout};
use tower_lsp::lsp_types::{FileChangeType, FileEvent, Url};

/// Helper struct to manage the language server process
struct LanguageServer {
    client: BufReader<DuplexStream>,
    request_id: Arc<AtomicI64>,
}

impl LanguageServer {
    /// Spawn a new language server process
    fn new(client: DuplexStream) -> Self {
        Self {
            client: BufReader::new(client),
            request_id: Arc::new(AtomicI64::new(1)),
        }
    }

    /// Send a JSON-RPC request to the language server
    async fn send_request(&mut self, method: &str, params: Value) -> std::io::Result<i64> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&request)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        self.client.write_all(message.as_bytes()).await?;

        Ok(id)
    }

    /// Send a notification (no response expected)
    async fn send_notification(&mut self, method: &str, params: Value) -> std::io::Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&notification)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        self.client.write_all(message.as_bytes()).await?;

        Ok(())
    }

    /// Read a single response from the language server
    async fn read_response(reader: &mut BufReader<DuplexStream>) -> std::io::Result<Value> {
        let mut headers = Vec::new();

        // Read headers
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;

            if line == "\r\n" || line == "\n" {
                break;
            }
            headers.push(line.trim().to_string());
        }

        // Parse Content-Length
        let content_length: usize = headers
            .iter()
            .find(|h| h.starts_with("Content-Length:"))
            .and_then(|h| h.split(':').nth(1))
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing Content-Length")
            })?;

        // Read content
        let mut buffer = vec![0u8; content_length];
        reader.read_exact(&mut buffer).await?;

        let content = String::from_utf8(buffer)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Read responses until we get the one with the matching ID or timeout
    async fn wait_for_response(
        &mut self,
        expected_id: i64,
        timeout_secs: u64,
    ) -> std::io::Result<Value> {
        let start = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(timeout_secs);

        loop {
            if start.elapsed() > timeout_duration {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Timeout waiting for response",
                ));
            }

            let result = timeout(timeout_duration, Self::read_response(&mut self.client)).await?;
            let response = result?;

            // Check if this is the response we're waiting for
            if let Some(id) = response.get("id")
                && id.as_i64() == Some(expected_id)
            {
                return Ok(response);
            }

            // Otherwise continue reading (could be a notification or other response)
        }
    }
}

fn get_project_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to src/language_server
    // We need to go up two levels to get to hakana-core
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // -> src
        .expect("Failed to get parent")
        .parent() // -> hakana-core
        .expect("Failed to get project root")
        .to_path_buf()
}

fn get_test_directory() -> PathBuf {
    get_project_root().join("tests/goto-definition/classDefinition")
}

#[test]
fn file_change_uri_is_converted_to_a_filesystem_path() {
    let path = PathBuf::from("/tmp/hakana project/input.hack");
    let event = FileEvent {
        uri: Url::from_file_path(&path).unwrap(),
        typ: FileChangeType::CHANGED,
    };

    let (changed_path, status) = file_event_to_status(event).unwrap();

    assert_eq!(changed_path, path.to_string_lossy());
    assert!(matches!(status, FileStatus::Modified(0, 0)));
}

#[test]
fn deleted_directory_with_extension_is_invalidated() {
    let path = PathBuf::from("/tmp/hakana/generated.v2");
    let event = FileEvent {
        uri: Url::from_file_path(&path).unwrap(),
        typ: FileChangeType::DELETED,
    };

    let (changed_path, status) = file_event_to_status(event).unwrap();

    assert_eq!(changed_path, path.to_string_lossy());
    assert!(matches!(status, FileStatus::DeletedDir));
}

#[tokio::test]
async fn publishing_diagnostics_twice_does_not_clear_the_first_publication() {
    let (service, socket) = tower_lsp::LspService::new(|client| {
        crate::Backend::new(
            client,
            hakana_analyzer::config::Config::new(String::new(), Default::default()),
            hakana_str::Interner::default(),
        )
    });
    drop(socket);
    let uri = Url::parse("file:///tmp/input.hack").unwrap();
    *service.inner().all_diagnostics.write().await = Some(rustc_hash::FxHashMap::from_iter([(
        uri.clone(),
        vec![tower_lsp::lsp_types::Diagnostic::default()],
    )]));
    service.inner().emit_issues().await;
    assert!(
        service
            .inner()
            .files_with_errors
            .read()
            .await
            .contains(&uri)
    );
    service.inner().emit_issues().await;
    assert!(
        service
            .inner()
            .files_with_errors
            .read()
            .await
            .contains(&uri)
    );
}

#[tokio::test]
async fn watchers_are_registered_before_initial_analysis() {
    let project = tempfile::tempdir().unwrap();
    let config = hakana_analyzer::config::Config::new(
        project.path().to_string_lossy().into_owned(),
        Default::default(),
    );
    let (client, server) = tokio::io::duplex(8192);
    let mut lsp = LanguageServer::new(client);
    let task = tokio::spawn(async move {
        let (reader, writer) = tokio::io::split(server);
        let (service, socket) = tower_lsp::LspService::new(|client| {
            crate::Backend::new(client, config, hakana_str::Interner::default())
        });
        tower_lsp::Server::new(reader, writer, socket)
            .serve(service)
            .await;
    });
    let id = lsp
        .send_request("initialize", json!({"capabilities": {}}))
        .await
        .unwrap();
    lsp.wait_for_response(id, 30).await.unwrap();
    lsp.send_notification("initialized", json!({}))
        .await
        .unwrap();
    let first = timeout(
        Duration::from_secs(30),
        LanguageServer::read_response(&mut lsp.client),
    )
    .await
    .unwrap()
    .unwrap();
    task.abort();
    assert_eq!(
        first["method"], "client/registerCapability",
        "analysis started before watching files: {first}"
    );
}

#[tokio::test]
async fn test_language_server_goto_definition() {
    let (client, server) = tokio::io::duplex(64);
    let mut lsp = LanguageServer::new(client);

    // tower-lsp does not actually exit when receiving an "exit" notification
    // https://github.com/ebkalderon/tower-lsp/issues/399
    // Use a channel to end the server task instead once we're done with the test.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let server_handle = tokio::spawn(async move {
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let lsp_server = serve(
            &mut server_reader,
            &mut server_writer,
            Ok(get_test_directory()),
        );

        tokio::select! {
            _ = lsp_server => {},
            _ = shutdown_rx => {}
        }
    });

    // 1. Send initialize request
    let init_id = lsp
        .send_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": format!("file://{}", get_test_directory().display()),
                "capabilities": {},
            }),
        )
        .await
        .expect("Failed to send initialize request");

    // Wait for initialize response
    let init_response = lsp
        .wait_for_response(init_id, 30)
        .await
        .expect("Failed to get initialize response");

    assert!(
        init_response.get("result").is_some(),
        "Initialize response missing result"
    );

    // 2. Send initialized notification
    lsp.send_notification("initialized", json!({}))
        .await
        .expect("Failed to send initialized notification");

    // 3. Wait for initialized notification
    let registration = lsp
        .wait_for_response(0, 30)
        .await
        .expect("Failed to get watcher registration");
    let response = serde_json::to_string(&json!({
        "jsonrpc": "2.0", "id": registration["id"], "result": null,
    }))
    .unwrap();
    lsp.client
        .write_all(format!("Content-Length: {}\r\n\r\n{}", response.len(), response).as_bytes())
        .await
        .unwrap();
    loop {
        let notification = timeout(
            Duration::from_secs(30),
            LanguageServer::read_response(&mut lsp.client),
        )
        .await
        .unwrap()
        .unwrap();
        if notification["params"]["message"] == "server initialized!" {
            break;
        }
    }

    // 4. Send goto-definition request
    // Position is at line 8, character 15 (the "MyClass" in "new MyClass()")
    let test_file_uri = format!(
        "file://{}",
        get_test_directory().join("input.hack").display()
    );

    let goto_def_id = lsp
        .send_request(
            "textDocument/definition",
            json!({
                "textDocument": {
                    "uri": test_file_uri
                },
                "position": {
                    "line": 9,  // 0-indexed, so line 8 in the file
                    "character": 15
                }
            }),
        )
        .await
        .expect("Failed to send goto-definition request");

    // Wait for goto-definition response
    let goto_def_response = lsp
        .wait_for_response(goto_def_id, 10)
        .await
        .expect("Failed to get goto-definition response");

    // 5. Verify the response
    let result = goto_def_response
        .get("result")
        .expect("Goto-definition response missing result");

    // The response should be a Location pointing to the class definition
    assert!(
        !result.is_null(),
        "Expected goto-definition to return a location, got null"
    );

    if let Some(location) = result.as_object() {
        // Verify we got a URI
        assert!(location.contains_key("uri"), "Location missing uri field");

        // Verify we got a range
        let range = location.get("range").expect("Location missing range field");

        let start = range.get("start").expect("Range missing start field");

        // The class definition is on line 1 (0-indexed = line 0)
        assert_eq!(
            start.get("line").and_then(|v| v.as_u64()),
            Some(2),
            "Expected definition at line 2 (line 3 in file)"
        );
    } else {
        panic!("Expected goto-definition result to be a Location object");
    }

    // 5. Send shutdown request
    let shutdown_id = lsp
        .send_request("shutdown", json!(null))
        .await
        .expect("Failed to send shutdown request");

    lsp.wait_for_response(shutdown_id, 5)
        .await
        .expect("Failed to get shutdown response");

    // 6. Send exit notification
    lsp.send_notification("exit", json!(null))
        .await
        .expect("Failed to send exit notification");

    shutdown_tx.send(true).ok();
    server_handle.await.ok();
}

/// Tests of the server-backed language server against a scripted stand-in for
/// `hakana server`, so that the order of analysis results, edits and failures
/// is fully under the test's control.
mod server_based {
    use super::LanguageServer;
    use crate::server_backend::ServerBasedBackend;
    use crate::server_client::ServerConnection;
    use hakana_protocol::{
        GetIssuesRequest, GetIssuesResponse, Message, ProtocolIssue, ServerSocket, SocketPath,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tokio::sync::{Mutex, mpsc};
    use tokio::time::{Duration, timeout};

    const ROOT: &str = "/hakana-fake-project";

    /// Answers each `GetIssues` request with the next response the test supplies,
    /// holding the request open until then (as the real server's long-poll does).
    struct FakeHakanaServer {
        socket_path: SocketPath,
        responses: mpsc::UnboundedSender<GetIssuesResponse>,
        requests: mpsc::UnboundedReceiver<GetIssuesRequest>,
        task: tokio::task::JoinHandle<()>,
        _dir: tempfile::TempDir,
    }

    impl FakeHakanaServer {
        fn start() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let socket_path = SocketPath::from_path(dir.path().join("hakana.sock"));
            let socket = ServerSocket::bind(socket_path.clone()).unwrap();
            let (responses, response_rx) = mpsc::unbounded_channel::<GetIssuesResponse>();
            let (request_tx, requests) = mpsc::unbounded_channel();
            let response_rx = Arc::new(Mutex::new(response_rx));

            let task = tokio::spawn(async move {
                let mut connections = tokio::task::JoinSet::new();
                loop {
                    let Ok(mut conn) = socket.accept().await else {
                        return;
                    };
                    let response_rx = response_rx.clone();
                    let request_tx = request_tx.clone();
                    connections.spawn(async move {
                        while let Ok(msg) = conn.read_message().await {
                            let Message::GetIssues(request) = msg else {
                                continue;
                            };
                            let _ = request_tx.send(request);
                            let Some(response) = response_rx.lock().await.recv().await else {
                                return;
                            };
                            if conn
                                .write_message(&Message::GetIssuesResult(response))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    });
                }
            });

            Self {
                socket_path,
                responses,
                requests,
                task,
                _dir: dir,
            }
        }

        fn respond(&self, response: GetIssuesResponse) {
            self.responses.send(response).unwrap();
        }

        async fn next_request(&mut self) -> GetIssuesRequest {
            timeout(Duration::from_secs(10), self.requests.recv())
                .await
                .expect("timed out waiting for a GetIssues request")
                .unwrap()
        }

        /// Stop listening and drop every open connection.
        async fn stop(&mut self) {
            self.task.abort();
            let _ = (&mut self.task).await;
        }
    }

    fn issue(file: &str, line: u32) -> ProtocolIssue {
        ProtocolIssue {
            kind: "InvalidArgument".to_string(),
            description: "test issue".to_string(),
            file_path: file.to_string(),
            start_line: line,
            start_column: 1,
            end_line: line,
            end_column: 5,
            suggestion: None,
        }
    }

    fn complete(issues: Vec<ProtocolIssue>) -> GetIssuesResponse {
        GetIssuesResponse {
            analysis_complete: true,
            issues,
            files_scanned: 0,
            total_files_to_scan: 0,
            files_analyzed: 0,
            total_files_to_analyze: 0,
            phase: "Complete".to_string(),
            generation: 0,
            rescanning: false,
        }
    }

    fn rescanning(generation: u64) -> GetIssuesResponse {
        GetIssuesResponse {
            analysis_complete: false,
            generation,
            rescanning: true,
            ..complete(vec![])
        }
    }

    fn incomplete() -> GetIssuesResponse {
        GetIssuesResponse {
            analysis_complete: false,
            ..complete(vec![])
        }
    }

    fn uri(file: &str) -> String {
        format!("file://{ROOT}/{file}")
    }

    /// Start a server-backed language server talking to `server` and complete the
    /// LSP handshake.
    async fn start_language_server(
        server: &FakeHakanaServer,
    ) -> (LanguageServer, tokio::task::JoinHandle<()>) {
        let connection = ServerConnection::for_tests(server.socket_path.clone());
        let config = hakana_analyzer::config::Config::new(ROOT.to_string(), Default::default());
        let (client, server_stream) = tokio::io::duplex(1 << 16);
        let mut lsp = LanguageServer::new(client);
        let task = tokio::spawn(async move {
            let (reader, writer) = tokio::io::split(server_stream);
            let (service, socket) = tower_lsp::LspService::new(|client| {
                ServerBasedBackend::new(client, config, connection)
            });
            tower_lsp::Server::new(reader, writer, socket)
                .serve(service)
                .await;
        });
        let id = lsp
            .send_request("initialize", json!({"capabilities": {}}))
            .await
            .unwrap();
        lsp.wait_for_response(id, 10).await.unwrap();
        lsp.send_notification("initialized", json!({}))
            .await
            .unwrap();
        (lsp, task)
    }

    async fn next_message(lsp: &mut LanguageServer, wait: Duration) -> Option<Value> {
        timeout(wait, LanguageServer::read_response(&mut lsp.client))
            .await
            .ok()
            .map(|message| message.unwrap())
    }

    /// The next published diagnostics, as (uri, start line of each diagnostic).
    async fn next_publish(lsp: &mut LanguageServer) -> (String, Vec<u64>) {
        loop {
            let message = next_message(lsp, Duration::from_secs(10))
                .await
                .expect("timed out waiting for diagnostics");
            if let Some(publish) = as_publish(&message) {
                return publish;
            }
        }
    }

    fn as_publish(message: &Value) -> Option<(String, Vec<u64>)> {
        if message["method"] != "textDocument/publishDiagnostics" {
            return None;
        }
        let lines = message["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|diagnostic| diagnostic["range"]["start"]["line"].as_u64().unwrap())
            .collect();
        Some((
            message["params"]["uri"].as_str().unwrap().to_string(),
            lines,
        ))
    }

    async fn insert_line_at_top(lsp: &mut LanguageServer, file: &str) {
        lsp.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": uri(file), "version": 2},
                "contentChanges": [{
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 0},
                    },
                    "text": "\n",
                }],
            }),
        )
        .await
        .unwrap();
    }

    /// Race 1: the user edits a buffer, and then an analysis of what is on disk
    /// arrives. Its positions have to be moved by the unsaved edits, as the
    /// positions already on screen were.
    #[tokio::test]
    async fn analysis_results_are_adjusted_for_unsaved_edits() {
        let mut server = FakeHakanaServer::start();
        let (mut lsp, task) = start_language_server(&server).await;

        server.next_request().await;
        server.respond(complete(vec![issue("a.hack", 11)]));
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![10]));

        insert_line_at_top(&mut lsp, "a.hack").await;
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![11]));

        // Some other file was saved: the file on disk is unchanged, so the server
        // reports the issue where it was.
        server.next_request().await;
        server.respond(complete(vec![issue("a.hack", 11)]));
        assert_eq!(
            next_publish(&mut lsp).await,
            (uri("a.hack"), vec![11]),
            "the unsaved edit was forgotten when the analysis result arrived"
        );

        // Once the buffer is saved the file on disk matches it again.
        lsp.send_notification(
            "textDocument/didSave",
            json!({"textDocument": {"uri": uri("a.hack")}}),
        )
        .await
        .unwrap();
        server.next_request().await;
        server.respond(complete(vec![issue("a.hack", 12)]));
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![11]));

        task.abort();
    }

    /// Race 2: an analysis can complete between two requests. The server can only
    /// hand it over if it is told which result the language server already has.
    #[tokio::test]
    async fn tells_the_server_which_result_it_has() {
        let mut server = FakeHakanaServer::start();
        let (mut lsp, task) = start_language_server(&server).await;

        // Whatever the server has is wanted first.
        let request = server.next_request().await;
        assert!(!request.block_until_next_analysis);
        assert_eq!(request.last_seen_generation, Some(0));
        server.respond(GetIssuesResponse {
            generation: 7,
            ..complete(vec![issue("a.hack", 11)])
        });
        next_publish(&mut lsp).await;

        let request = server.next_request().await;
        assert!(request.block_until_next_analysis);
        assert_eq!(request.last_seen_generation, Some(7));

        // A failed analysis has been seen too: asking for anything but generation
        // 7 again would be answered with the same failure, for ever.
        server.respond(GetIssuesResponse {
            generation: 8,
            ..incomplete()
        });
        let request = server.next_request().await;
        assert!(request.block_until_next_analysis);
        assert_eq!(request.last_seen_generation, Some(8));

        // A server that predates generations reports none.
        server.respond(complete(vec![]));
        let request = server.next_request().await;
        assert!(request.block_until_next_analysis);
        assert_eq!(request.last_seen_generation, Some(0));

        task.abort();
    }

    /// The next published diagnostics, as (uri, message of each diagnostic).
    async fn next_publish_messages(lsp: &mut LanguageServer) -> (String, Vec<String>) {
        loop {
            let message = next_message(lsp, Duration::from_secs(10))
                .await
                .expect("timed out waiting for diagnostics");
            if message["method"] == "textDocument/publishDiagnostics" {
                let messages = message["params"]["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|diagnostic| diagnostic["message"].as_str().unwrap().to_string())
                    .collect();
                return (
                    message["params"]["uri"].as_str().unwrap().to_string(),
                    messages,
                );
            }
        }
    }

    async fn open(lsp: &mut LanguageServer, file: &str) {
        lsp.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {"uri": uri(file), "languageId": "hack", "version": 1, "text": ""},
            }),
        )
        .await
        .unwrap();
    }

    async fn close(lsp: &mut LanguageServer, file: &str) {
        lsp.send_notification(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri(file)}}),
        )
        .await
        .unwrap();
    }

    /// While the server analyses everything again it has no results, and those
    /// from before may no longer hold: they go, and one notice says why.
    #[tokio::test]
    async fn rescanning_replaces_the_diagnostics_with_one_notice() {
        let mut server = FakeHakanaServer::start();
        let (mut lsp, task) = start_language_server(&server).await;
        open(&mut lsp, "a.hack").await;
        open(&mut lsp, "b.hack").await;

        server.next_request().await;
        server.respond(GetIssuesResponse {
            generation: 1,
            ..complete(vec![issue("a.hack", 11)])
        });
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![10]));

        server.next_request().await;
        server.respond(rescanning(2));
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![]));
        let (notice_uri, notice) = next_publish_messages(&mut lsp).await;
        assert_eq!(
            notice_uri,
            uri("b.hack"),
            "shown on the document opened last"
        );
        assert_eq!(notice.len(), 1);
        assert!(notice[0].contains("rescanning"), "{}", notice[0]);

        // The notice stays in view: it moves on from a document that is closed.
        close(&mut lsp, "b.hack").await;
        assert_eq!(next_publish(&mut lsp).await, (uri("b.hack"), vec![]));
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![0]));

        // The rescan has been seen; what is wanted next is its result.
        let request = server.next_request().await;
        assert_eq!(request.last_seen_generation, Some(2));
        server.respond(GetIssuesResponse {
            generation: 3,
            ..complete(vec![issue("c.hack", 5)])
        });
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![]));
        assert_eq!(next_publish(&mut lsp).await, (uri("c.hack"), vec![4]));

        task.abort();
    }

    /// With no document open there is nowhere to show the notice, until one is.
    /// And if the rescan fails, the notice no longer promises results.
    #[tokio::test]
    async fn notice_waits_for_a_document_and_reports_a_failed_rescan() {
        let mut server = FakeHakanaServer::start();
        let (mut lsp, task) = start_language_server(&server).await;

        server.next_request().await;
        server.respond(rescanning(1));
        server.next_request().await;

        open(&mut lsp, "a.hack").await;
        let (notice_uri, notice) = next_publish_messages(&mut lsp).await;
        assert_eq!(notice_uri, uri("a.hack"));
        assert!(notice[0].contains("rescanning"), "{}", notice[0]);

        server.respond(GetIssuesResponse {
            generation: 2,
            ..incomplete()
        });
        let (notice_uri, notice) = next_publish_messages(&mut lsp).await;
        assert_eq!(notice_uri, uri("a.hack"));
        assert_eq!(notice.len(), 1);
        assert!(notice[0].contains("could not analyse"), "{}", notice[0]);

        task.abort();
    }

    /// Race 3a: an analysis that failed says nothing about the code, so what is on
    /// screen has to stay.
    #[tokio::test]
    async fn failed_analysis_does_not_clear_diagnostics() {
        let mut server = FakeHakanaServer::start();
        let (mut lsp, task) = start_language_server(&server).await;

        server.next_request().await;
        server.respond(complete(vec![issue("a.hack", 11)]));
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![10]));

        server.next_request().await;
        server.respond(incomplete());

        server.next_request().await;
        server.respond(complete(vec![issue("a.hack", 11), issue("b.hack", 3)]));

        loop {
            let (published_uri, lines) = next_publish(&mut lsp).await;
            if published_uri == uri("b.hack") {
                break;
            }
            assert_eq!(
                lines,
                vec![10],
                "diagnostics were cleared by an analysis that did not complete"
            );
        }

        task.abort();
    }

    /// Race 3b: while the server is unreachable the diagnostics stay, and the
    /// language server does not spin trying to reach it.
    #[tokio::test]
    async fn unreachable_server_does_not_clear_diagnostics() {
        let mut server = FakeHakanaServer::start();
        let (mut lsp, task) = start_language_server(&server).await;

        server.next_request().await;
        server.respond(complete(vec![issue("a.hack", 11)]));
        assert_eq!(next_publish(&mut lsp).await, (uri("a.hack"), vec![10]));

        server.next_request().await;
        server.stop().await;

        let mut failures = 0;
        while let Some(message) = next_message(&mut lsp, Duration::from_millis(500)).await {
            assert_eq!(
                as_publish(&message),
                None,
                "diagnostics were changed while the server was unreachable"
            );
            if message["params"]["message"]
                .as_str()
                .is_some_and(|text| text.starts_with("Failed to get issues"))
            {
                failures += 1;
                assert!(failures < 10, "retrying without any delay");
            }
        }
        assert!(failures > 0, "the failure was not reported");

        task.abort();
    }
}

/// Tests of the language server that analyses in its own process: the fallback
/// for when no server can be started.
mod local_backend {
    use super::LanguageServer;
    use crate::Backend;
    use hakana_analyzer::config::Config;
    use hakana_analyzer::custom_hook::{CustomHook, InternalHook};
    use hakana_code_info::codebase_info::CodebaseInfo;
    use hakana_str::Interner;
    use serde_json::{Value, json};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::AsyncWriteExt;
    use tokio::time::{Duration, timeout};
    use tower_lsp::LanguageServer as _;
    use tower_lsp::lsp_types::{
        GotoDefinitionParams, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
    };

    /// Lets a test fail an analysis run, or hold one in flight.
    #[derive(Debug, Default)]
    struct AnalysisControl {
        panic: AtomicBool,
        closed: AtomicBool,
        holding: AtomicBool,
    }

    impl AnalysisControl {
        async fn wait_until_holding(&self) {
            let wait = async {
                while !self.holding.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            };
            timeout(Duration::from_secs(20), wait)
                .await
                .expect("no analysis run started");
        }
    }

    #[derive(Debug)]
    struct ControlHook(Arc<AnalysisControl>);

    impl InternalHook for ControlHook {
        fn after_populate(&self, _: &CodebaseInfo, _: &Interner, _: &Config) {
            while self.0.closed.load(Ordering::SeqCst) {
                self.0.holding.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
            }
            self.0.holding.store(false, Ordering::SeqCst);
            if self.0.panic.load(Ordering::SeqCst) {
                panic!("analysis failure injected by the test");
            }
        }
    }

    impl CustomHook for ControlHook {}

    struct Project {
        dir: tempfile::TempDir,
        control: Arc<AnalysisControl>,
    }

    impl Project {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("a.hack"), "function main(): void {}").unwrap();
            Self {
                dir,
                control: Default::default(),
            }
        }

        fn config(&self) -> Config {
            let mut config = Config::new(
                self.dir.path().to_string_lossy().into_owned(),
                Default::default(),
            );
            config.hooks = vec![Arc::new(ControlHook(self.control.clone()))];
            config
        }

        fn uri(&self) -> Url {
            Url::from_file_path(self.dir.path().join("a.hack")).unwrap()
        }
    }

    /// Race 11: an analysis that panics must come to an end like any other.
    #[tokio::test]
    async fn analysis_panic_does_not_hang_the_language_server() {
        let project = Project::new();
        let (service, socket) = tower_lsp::LspService::new(|client| {
            Backend::new(client, project.config(), Interner::default())
        });
        drop(socket);
        let backend = service.inner();

        project.control.panic.store(true, Ordering::SeqCst);
        for attempt in ["first", "second"] {
            timeout(Duration::from_secs(20), backend.do_analysis())
                .await
                .unwrap_or_else(|_| panic!("the {attempt} failed analysis never ended"));
        }

        // And the next one works.
        project.control.panic.store(false, Ordering::SeqCst);
        timeout(Duration::from_secs(20), backend.do_analysis())
            .await
            .expect("analysis never ended");
        assert!(backend.previous_scan_data.read().await.is_some());
    }

    /// Race 12a: a request made while an analysis is running is answered - with
    /// nothing, there being no result to answer from - rather than held until the
    /// analysis is done.
    #[tokio::test]
    async fn requests_are_answered_during_analysis() {
        let project = Project::new();
        let (service, socket) = tower_lsp::LspService::new(|client| {
            Backend::new(client, project.config(), Interner::default())
        });
        drop(socket);
        let backend = service.inner();

        project.control.closed.store(true, Ordering::SeqCst);
        let analysis = backend.do_analysis();
        tokio::pin!(analysis);

        let definition = async {
            project.control.wait_until_holding().await;
            timeout(
                Duration::from_secs(2),
                backend.goto_definition(GotoDefinitionParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri: project.uri() },
                        position: Position::new(0, 10),
                    },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                }),
            )
            .await
        };

        let definition = tokio::select! {
            _ = &mut analysis => panic!("the analysis was not held"),
            definition = definition => definition,
        };
        project.control.closed.store(false, Ordering::SeqCst);
        analysis.await;

        assert!(
            definition.is_ok(),
            "the request waited for the analysis to finish"
        );
    }

    async fn respond(lsp: &mut LanguageServer, request: &Value) {
        let response = serde_json::to_string(&json!({
            "jsonrpc": "2.0", "id": request["id"], "result": null,
        }))
        .unwrap();
        lsp.client
            .write_all(format!("Content-Length: {}\r\n\r\n{}", response.len(), response).as_bytes())
            .await
            .unwrap();
    }

    /// Race 12b: file changes reported while an analysis is running must not
    /// each tie up one of the few requests the server handles at a time, waiting
    /// for it. With four of them waiting, nothing else gets through.
    #[tokio::test]
    async fn file_changes_during_analysis_do_not_block_requests() {
        let project = Project::new();
        let config = project.config();
        let (client, server) = tokio::io::duplex(1 << 16);
        let mut lsp = LanguageServer::new(client);
        let task = tokio::spawn(async move {
            let (reader, writer) = tokio::io::split(server);
            let (service, socket) = tower_lsp::LspService::new(|client| {
                Backend::new(client, config, Interner::default())
            });
            tower_lsp::Server::new(reader, writer, socket)
                .serve(service)
                .await;
        });

        let id = lsp
            .send_request("initialize", json!({"capabilities": {}}))
            .await
            .unwrap();
        lsp.wait_for_response(id, 30).await.unwrap();
        lsp.send_notification("initialized", json!({}))
            .await
            .unwrap();
        loop {
            let message = timeout(
                Duration::from_secs(30),
                LanguageServer::read_response(&mut lsp.client),
            )
            .await
            .unwrap()
            .unwrap();
            if message["method"] == "client/registerCapability" {
                respond(&mut lsp, &message).await;
            }
            if message["params"]["message"] == "server initialized!" {
                break;
            }
        }

        project.control.closed.store(true, Ordering::SeqCst);
        for _ in 0..6 {
            lsp.send_notification(
                "workspace/didChangeWatchedFiles",
                json!({"changes": [{"uri": project.uri(), "type": 2}]}),
            )
            .await
            .unwrap();
        }
        project.control.wait_until_holding().await;

        let id = lsp
            .send_request(
                "textDocument/hover",
                json!({
                    "textDocument": {"uri": project.uri()},
                    "position": {"line": 0, "character": 10},
                }),
            )
            .await
            .unwrap();
        let response = lsp.wait_for_response(id, 2).await;

        project.control.closed.store(false, Ordering::SeqCst);
        task.abort();

        assert!(
            response.is_ok(),
            "no request is handled while file changes wait for the analysis"
        );
    }
}
