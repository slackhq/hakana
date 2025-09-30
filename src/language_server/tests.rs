use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Helper struct to manage the language server process
struct LanguageServerProcess {
    process: Child,
    request_id: Arc<AtomicI64>,
}

impl LanguageServerProcess {
    /// Spawn a new language server process
    fn new() -> std::io::Result<Self> {
        // Build the language server first to ensure it's up to date
        let build_status = Command::new("cargo")
            .args(&["build", "--release", "--bin", "hakana-language-server"])
            .current_dir(get_project_root())
            .status()?;

        if !build_status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to build language server",
            ));
        }

        let process = Command::new(get_project_root().join("target/release/hakana-language-server"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(get_test_directory())
            .spawn()?;

        Ok(Self {
            process,
            request_id: Arc::new(AtomicI64::new(1)),
        })
    }

    /// Send a JSON-RPC request to the language server
    fn send_request(&mut self, method: &str, params: Value) -> std::io::Result<i64> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&request)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        if let Some(stdin) = self.process.stdin.as_mut() {
            stdin.write_all(message.as_bytes())?;
            stdin.flush()?;
        }

        Ok(id)
    }

    /// Send a notification (no response expected)
    fn send_notification(&mut self, method: &str, params: Value) -> std::io::Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let content = serde_json::to_string(&notification)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        if let Some(stdin) = self.process.stdin.as_mut() {
            stdin.write_all(message.as_bytes())?;
            stdin.flush()?;
        }

        Ok(())
    }

    /// Read a single response from the language server
    fn read_response(&mut self) -> std::io::Result<Value> {
        if let Some(stdout) = self.process.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            let mut headers = Vec::new();

            // Read headers
            loop {
                let mut line = String::new();
                reader.read_line(&mut line)?;

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
            std::io::Read::read_exact(&mut reader, &mut buffer)?;

            let content = String::from_utf8(buffer)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            serde_json::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "No stdout available",
            ))
        }
    }

    /// Read responses until we get the one with the matching ID or timeout
    fn wait_for_response(&mut self, expected_id: i64, timeout_secs: u64) -> std::io::Result<Value> {
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > Duration::from_secs(timeout_secs) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Timeout waiting for response",
                ));
            }

            let response = self.read_response()?;

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

impl Drop for LanguageServerProcess {
    fn drop(&mut self) {
        // Kill the process if it's still running
        let _ = self.process.kill();
        let _ = self.process.wait();
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
fn test_language_server_goto_definition() {
    let mut lsp = LanguageServerProcess::new().expect("Failed to spawn language server");

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
        .expect("Failed to send initialize request");

    // Wait for initialize response
    let init_response = lsp
        .wait_for_response(init_id, 30)
        .expect("Failed to get initialize response");

    assert!(
        init_response.get("result").is_some(),
        "Initialize response missing result"
    );

    // 2. Send initialized notification
    lsp.send_notification("initialized", json!({}))
        .expect("Failed to send initialized notification");

    // Give the server time to perform initial analysis
    std::thread::sleep(Duration::from_secs(5));

    // 3. Send goto-definition request
    // Position is at line 8, character 15 (the "MyClass" in "new MyClass()")
    let test_file_uri = format!("file://{}", get_test_directory().join("input.hack").display());

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
        .expect("Failed to send goto-definition request");

    // Wait for goto-definition response
    let goto_def_response = lsp
        .wait_for_response(goto_def_id, 10)
        .expect("Failed to get goto-definition response");

    // 4. Verify the response
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
        assert!(
            location.contains_key("uri"),
            "Location missing uri field"
        );

        // Verify we got a range
        let range = location
            .get("range")
            .expect("Location missing range field");

        let start = range
            .get("start")
            .expect("Range missing start field");

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
        .expect("Failed to send shutdown request");

    lsp.wait_for_response(shutdown_id, 5)
        .expect("Failed to get shutdown response");

    // 6. Send exit notification
    lsp.send_notification("exit", json!(null))
        .expect("Failed to send exit notification");

    // Wait for the process to exit with a timeout
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        match lsp.process.try_wait() {
            Ok(Some(_status)) => {
                // Process exited successfully
                return;
            }
            Ok(None) => {
                // Process still running, wait a bit
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break,
        }
    }

    // If we get here, the process didn't exit cleanly, so kill it
    let _ = lsp.process.kill();
}
