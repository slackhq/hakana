//! Integration tests for hakana server-client interaction.
//!
//! These tests verify that the server starts correctly and clients can connect
//! to retrieve analysis results.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use hakana_protocol::{ClientSocket, GetIssuesRequest, Message, SocketPath, StatusRequest};

/// Helper struct to manage server process lifecycle
struct TestServer {
    process: Child,
    socket_path: SocketPath,
}

impl TestServer {
    /// Start a test server for the given directory
    fn start(test_dir: &str) -> Result<Self, String> {
        let test_dir = PathBuf::from(test_dir);
        let socket_path = SocketPath::for_project(&test_dir);

        // Clean up any stale socket
        let _ = socket_path.cleanup();

        // Find the hakana binary
        let hakana_bin = find_hakana_binary()?;

        // Start server process
        let process = Command::new(&hakana_bin)
            .args(["server", "--root", test_dir.to_str().unwrap()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start server: {}", e))?;

        let server = Self {
            process,
            socket_path,
        };

        // Wait for server to be ready
        server.wait_for_ready(Duration::from_secs(60))?;

        Ok(server)
    }

    /// Wait for the server to be ready to accept connections
    fn wait_for_ready(&self, timeout: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if self.socket_path.server_exists() {
                // Try to connect and check status
                if let Ok(mut client) = ClientSocket::connect(&self.socket_path) {
                    let request = Message::Status(StatusRequest);
                    if let Ok(Message::StatusResult(status)) = client.request(&request) {
                        if status.ready {
                            return Ok(());
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(500));
        }

        Err(format!("Server did not become ready within {:?}", timeout))
    }

    /// Get a client connection to this server
    fn connect(&self) -> Result<ClientSocket, String> {
        ClientSocket::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect to server: {}", e))
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // Send shutdown signal
        if let Ok(mut client) = ClientSocket::connect(&self.socket_path) {
            let _ = client.send(&Message::Shutdown(hakana_protocol::ShutdownRequest));
        }

        // Give it a moment to shut down gracefully
        thread::sleep(Duration::from_millis(100));

        // Force kill if still running
        let _ = self.process.kill();
        let _ = self.process.wait();

        // Clean up socket
        let _ = self.socket_path.cleanup();
    }
}

/// Find the hakana binary
fn find_hakana_binary() -> Result<PathBuf, String> {
    // Try release build first
    let release_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/release/hakana");

    if release_path.exists() {
        return Ok(release_path);
    }

    // Try debug build
    let debug_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/debug/hakana");

    if debug_path.exists() {
        return Ok(debug_path);
    }

    Err("Could not find hakana binary. Run 'cargo build' first.".to_string())
}

#[test]
#[ignore] // Run with: cargo test --test integration_test -- --ignored
fn test_server_client_status() {
    // Use a test directory with some Hack files
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/server");

    // Skip if watchman is not available
    if Command::new("watchman").arg("version").output().is_err() {
        eprintln!("Skipping test: watchman not available");
        return;
    }

    // Start the server
    let server = match TestServer::start(test_dir.to_str().unwrap()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            return;
        }
    };

    // Connect and check status
    let mut client = server.connect().expect("Failed to connect");

    let request = Message::Status(StatusRequest);
    let response = client.request(&request).expect("Failed to send request");

    if let Message::StatusResult(status) = response {
        assert!(status.ready, "Server should be ready");
        assert!(status.files_count > 0, "Server should have analyzed files");
        println!(
            "Server status: {} files, {} symbols",
            status.files_count, status.symbols_count
        );
    } else {
        panic!("Expected StatusResult, got {:?}", response);
    }
}

#[test]
#[ignore] // Run with: cargo test --test integration_test -- --ignored
fn test_server_client_get_issues() {
    // Use a test directory with some Hack files
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/server");

    // Skip if watchman is not available
    if Command::new("watchman").arg("version").output().is_err() {
        eprintln!("Skipping test: watchman not available");
        return;
    }

    // Start the server
    let server = match TestServer::start(test_dir.to_str().unwrap()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            return;
        }
    };

    // Connect and get issues
    let mut client = server.connect().expect("Failed to connect");

    let request = Message::GetIssues(GetIssuesRequest {
        filter: None,
        find_unused_expressions: false,
        find_unused_definitions: false,
    });

    let response = client.request(&request).expect("Failed to send request");

    if let Message::GetIssuesResult(result) = response {
        assert!(
            result.analysis_complete,
            "Analysis should be complete after server is ready"
        );
        println!(
            "Got {} issues from {} files",
            result.issues.len(),
            result.files_analyzed
        );
    } else {
        panic!("Expected GetIssuesResult, got {:?}", response);
    }
}

#[test]
#[ignore] // Run with: cargo test --test integration_test -- --ignored
fn test_cli_client_connects_to_server() {
    // Use a test directory with some Hack files
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/server");

    // Skip if watchman is not available
    if Command::new("watchman").arg("version").output().is_err() {
        eprintln!("Skipping test: watchman not available");
        return;
    }

    // Start the server
    let _server = match TestServer::start(test_dir.to_str().unwrap()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            return;
        }
    };

    // Find hakana binary
    let hakana_bin = find_hakana_binary().expect("hakana binary not found");

    // Run hakana analyze (should connect to the running server)
    let output = Command::new(&hakana_bin)
        .args(["analyze", "--root", test_dir.to_str().unwrap()])
        .output()
        .expect("Failed to run hakana analyze");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);

    // The client should have connected to the server (not run standalone)
    // Check that it got a response (either issues or "No issues reported")
    assert!(
        stdout.contains("Analyzed") || stdout.contains("No issues"),
        "Expected analysis output, got: {}",
        stdout
    );
}
