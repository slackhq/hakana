use crate::serve;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::time::{Duration, timeout};

/// Helper struct to manage the language server process
struct LanguageServer {
    client: DuplexStream,
    request_id: Arc<AtomicI64>,
}

impl LanguageServer {
    /// Spawn a new language server process
    fn new(client: DuplexStream) -> Self {
        Self {
            client,
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
    async fn read_response(&mut self) -> std::io::Result<Value> {
        let mut reader = BufReader::new(&mut self.client);
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

            let result = timeout(timeout_duration, self.read_response()).await?;
            let response = result?;

            // Check if this is the response we're waiting for
            if let Some(id) = response.get("id") {
                if id.as_i64() == Some(expected_id) {
                    return Ok(response);
                }
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
    lsp.wait_for_response(0, 30)
        .await
        .expect("Failed to get initialize notification");

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
