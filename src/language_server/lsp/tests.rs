use serde_json::{Value, json};
use tokio::io::{BufReader, DuplexStream, ReadHalf, WriteHalf};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

use super::jsonrpc::{self, Error, Id, Message, Request, read_message, write_message};
use super::types::*;
use super::{Client, LanguageServer, LspService, Server};

/// Records the order in which handlers ran and echoes it back via goto-definition.
struct TestServer {
    client: Client,
    events: Vec<String>,
}

impl LanguageServer for TestServer {
    async fn initialize(&mut self) -> jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: TextDocumentSyncOptions {
                    open_close: true,
                    change: TextDocumentSyncKind::INCREMENTAL,
                    will_save: false,
                    will_save_wait_until: false,
                    save: false,
                },
                definition_provider: true,
            },
        })
    }

    async fn initialized(&mut self) {
        let registered = self.client.register_capability(vec![]).await;
        self.events
            .push(format!("registered: {}", registered.is_ok()));
    }

    async fn did_change(&mut self, params: DidChangeTextDocumentParams) {
        self.events
            .push(format!("changed: {}", params.content_changes[0].text));
    }

    async fn goto_definition(
        &mut self,
        params: GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<Location>> {
        self.client
            .log_message(MessageType::INFO, self.events.join(", "));
        Ok(Some(Location {
            uri: params.text_document.uri,
            range: Range {
                start: params.position,
                end: params.position,
            },
        }))
    }
}

struct TestClient {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    server: JoinHandle<()>,
}

impl TestClient {
    fn start() -> Self {
        let (client_stream, server_stream) = tokio::io::duplex(1024);
        let server = tokio::spawn(async move {
            let (reader, writer) = tokio::io::split(server_stream);
            let (service, socket) = LspService::new(|client| TestServer {
                client,
                events: vec![],
            });
            Server::new(reader, writer, socket).serve(service).await;
        });
        let (reader, writer) = tokio::io::split(client_stream);

        Self {
            reader: BufReader::new(reader),
            writer,
            server,
        }
    }

    async fn send(&mut self, message: Value) {
        write_message(&mut self.writer, &message).await.unwrap();
    }

    async fn receive(&mut self) -> Value {
        let body = timeout(Duration::from_secs(5), read_message(&mut self.reader))
            .await
            .expect("timed out waiting for a message")
            .unwrap()
            .expect("server closed the stream");
        serde_json::from_slice(&body).unwrap()
    }

    async fn initialize(&mut self) {
        self.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}))
            .await;
        let response = self.receive().await;
        assert_eq!(
            response["result"]["capabilities"]["definitionProvider"],
            true
        );

        self.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}))
            .await;
        let registration = self.receive().await;
        assert_eq!(registration["method"], "client/registerCapability");
        self.send(json!({"jsonrpc": "2.0", "id": registration["id"], "result": null}))
            .await;
    }

    async fn goto_definition(&mut self, id: i64) -> (Value, Value) {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": {"uri": "file:///tmp/input.hack"},
                "position": {"line": 1, "character": 2},
            },
        }))
        .await;
        (self.receive().await, self.receive().await)
    }
}

#[tokio::test]
async fn requests_before_initialize_are_rejected() {
    let mut client = TestClient::start();

    client
        .send(json!({"jsonrpc": "2.0", "id": "a", "method": "shutdown"}))
        .await;

    let response = client.receive().await;
    assert_eq!(response["id"], "a");
    assert_eq!(response["error"]["code"], -32002);
}

#[tokio::test]
async fn handlers_run_in_order_and_can_await_client_responses() {
    let mut client = TestClient::start();
    client.initialize().await;

    client
        .send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": "file:///tmp/input.hack", "version": 2},
                "contentChanges": [{"text": "new contents"}],
            },
        }))
        .await;

    let (log, response) = client.goto_definition(2).await;
    assert_eq!(log["method"], "window/logMessage");
    assert_eq!(
        log["params"],
        json!({"type": 3, "message": "registered: true, changed: new contents"})
    );
    assert_eq!(
        response,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "uri": "file:///tmp/input.hack",
                "range": {
                    "start": {"line": 1, "character": 2},
                    "end": {"line": 1, "character": 2},
                },
            },
        })
    );
}

#[tokio::test]
async fn unknown_methods_and_invalid_params() {
    let mut client = TestClient::start();
    client.initialize().await;

    client
        .send(json!({"jsonrpc": "2.0", "method": "$/cancelRequest", "params": {"id": 1}}))
        .await;
    client
        .send(json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/hover", "params": {}}))
        .await;
    let response = client.receive().await;
    assert_eq!(response["id"], 2);
    assert_eq!(response["error"]["code"], -32601);

    client
        .send(json!({"jsonrpc": "2.0", "id": 3, "method": "textDocument/definition", "params": {}}))
        .await;
    let response = client.receive().await;
    assert_eq!(response["id"], 3);
    assert_eq!(response["error"]["code"], -32602);

    client
        .send(json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {}}))
        .await;
    let log = client.receive().await;
    assert_eq!(log["method"], "window/logMessage");
    assert_eq!(log["params"]["type"], 1);
}

#[tokio::test]
async fn malformed_json_gets_a_parse_error() {
    let mut client = TestClient::start();

    tokio::io::AsyncWriteExt::write_all(&mut client.writer, b"Content-Length: 3\r\n\r\n{]}")
        .await
        .unwrap();

    let response = client.receive().await;
    assert_eq!(response["id"], Value::Null);
    assert_eq!(response["error"]["code"], -32700);
}

#[tokio::test]
async fn requests_after_shutdown_are_rejected_and_exit_stops_the_server() {
    let mut client = TestClient::start();
    client.initialize().await;

    client
        .send(json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}))
        .await;
    assert_eq!(
        client.receive().await,
        json!({"jsonrpc": "2.0", "id": 2, "result": null})
    );

    client
        .send(json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"}))
        .await;
    let response = client.receive().await;
    assert_eq!(response["id"], 3);
    assert_eq!(response["error"]["code"], -32600);

    client
        .send(json!({"jsonrpc": "2.0", "method": "exit"}))
        .await;
    timeout(Duration::from_secs(5), client.server)
        .await
        .expect("server did not exit")
        .unwrap();
}

#[tokio::test]
async fn server_stops_when_the_client_closes_the_stream() {
    let client = TestClient::start();
    drop(client.reader);
    drop(client.writer);

    timeout(Duration::from_secs(5), client.server)
        .await
        .expect("server did not exit")
        .unwrap();
}

#[tokio::test]
async fn framing_skips_unknown_headers() {
    let input =
        b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}";
    let mut reader = BufReader::new(&input[..]);

    assert_eq!(
        read_message(&mut reader).await.unwrap(),
        Some(b"{}".to_vec())
    );
    assert_eq!(read_message(&mut reader).await.unwrap(), None);
}

#[test]
fn parses_requests_notifications_and_responses() {
    assert_eq!(
        Message::parse(br#"{"jsonrpc":"2.0","id":1,"method":"shutdown"}"#).unwrap(),
        Message::Request(Request {
            id: Some(Id::Number(1)),
            method: "shutdown".to_string(),
            params: Value::Null,
        })
    );
    assert_eq!(
        Message::parse(br#"{"jsonrpc":"2.0","method":"exit"}"#).unwrap(),
        Message::Request(Request {
            id: None,
            method: "exit".to_string(),
            params: Value::Null,
        })
    );
    assert_eq!(
        Message::parse(br#"{"jsonrpc":"2.0","id":"x","result":null}"#).unwrap(),
        Message::Response(Id::String("x".to_string()), Ok(Value::Null))
    );
    assert_eq!(
        Message::parse(br#"{"jsonrpc":"2.0","id":4,"error":{"code":-32601,"message":"no"}}"#)
            .unwrap(),
        Message::Response(
            Id::Number(4),
            Err(Error {
                code: -32601,
                message: "no".to_string(),
                data: None,
            })
        )
    );
    assert_eq!(
        Message::parse(br#"{"jsonrpc":"2.0"}"#).unwrap_err().code,
        -32600
    );
}

#[test]
fn positions_are_ordered_by_line_then_character() {
    let position = |line, character| Position { line, character };

    assert!(position(1, 9) < position(2, 0));
    assert!(position(2, 0) < position(2, 1));
}

#[test]
fn types_use_the_lsp_wire_format() {
    let diagnostic = Diagnostic {
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some("UnusedVariable".to_string()),
        message: "unused".to_string(),
        ..Diagnostic::default()
    };
    assert_eq!(
        serde_json::to_value(diagnostic).unwrap(),
        json!({
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}},
            "severity": 2,
            "code": "UnusedVariable",
            "message": "unused",
        })
    );

    let event: FileEvent =
        serde_json::from_value(json!({"uri": "file:///tmp/a.hack", "type": 3})).unwrap();
    assert_eq!(event.typ, FileChangeType::DELETED);

    assert_eq!(
        serde_json::to_value(FileSystemWatcher {
            glob_pattern: "**/*.hack".to_string(),
            kind: Some(WatchKind::DELETE),
        })
        .unwrap(),
        json!({"globPattern": "**/*.hack", "kind": 4})
    );
}
