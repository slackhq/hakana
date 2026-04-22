use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use hakana_protocol::{ClientSocket, GetIssuesRequest, Message, SocketPath, StatusRequest};

struct TestServer {
    process: Child,
    socket_path: SocketPath,
}

impl TestServer {
    async fn start(test_dir: &str) -> Result<Self, String> {
        let test_dir = PathBuf::from(test_dir);
        let socket_path = SocketPath::for_project(&test_dir);

        socket_path.cleanup().ok();

        let hakana_bin = find_hakana_binary()?;

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

        server.wait_for_ready(Duration::from_secs(15)).await?;

        Ok(server)
    }

    async fn wait_for_ready(&self, timeout: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if self.socket_path.server_exists() {
                match ClientSocket::connect(&self.socket_path).await {
                    Ok(mut client) => {
                        let request = Message::Status(StatusRequest);
                        let x = client.request(&request).await;
                        if let Ok(Message::StatusResult(status)) = x {
                            if status.ready {
                                return Ok(());
                            }
                        }
                    }
                    Err(e) => println!("{:?}", e),
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Err(format!("Server did not become ready within {:?}", timeout))
    }

    async fn connect(&self) -> Result<ClientSocket, String> {
        ClientSocket::connect(&self.socket_path)
            .await
            .map_err(|e| format!("Failed to connect to server: {}", e))
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = self.socket_path.cleanup();
    }
}

fn find_hakana_binary() -> Result<PathBuf, String> {
    let release_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/release/hakana");

    if release_path.exists() {
        return Ok(release_path);
    }

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

#[tokio::test]
async fn test_server_client_status() {
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/server/1");

    if Command::new("watchman").arg("version").output().is_err() {
        eprintln!("Skipping test: watchman not available");
        return;
    }

    let server = TestServer::start(test_dir.to_str().unwrap())
        .await
        .expect("Failed to start server");

    let mut client = server.connect().await.expect("Failed to connect");

    let request = Message::Status(StatusRequest);
    let response = client
        .request(&request)
        .await
        .expect("Failed to send request");

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

#[tokio::test]
async fn test_server_client_get_issues() {
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/server/2");

    if Command::new("watchman").arg("version").output().is_err() {
        eprintln!("Skipping test: watchman not available");
        return;
    }

    let server = TestServer::start(test_dir.to_str().unwrap())
        .await
        .expect("Failed to start server");

    let mut client = server.connect().await.expect("Failed to connect");

    let request = Message::GetIssues(GetIssuesRequest {
        filter: None,
        find_unused_expressions: false,
        find_unused_definitions: false,
        block_until_next_analysis: false,
    });

    let response = client
        .request(&request)
        .await
        .expect("Failed to send request");

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

#[tokio::test]
async fn test_cli_client_connects_to_server() {
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/server/3");

    if Command::new("watchman").arg("version").output().is_err() {
        eprintln!("Skipping test: watchman not available");
        return;
    }

    TestServer::start(test_dir.to_str().unwrap())
        .await
        .expect("Failed to start server");

    let hakana_bin = find_hakana_binary().expect("hakana binary not found");

    let output = Command::new(&hakana_bin)
        .args(["analyze"])
        .current_dir(test_dir.to_str().unwrap())
        .output()
        .expect("Failed to run hakana analyze");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);

    assert!(
        stdout.contains("ERROR:"),
        "Expected analysis output, got: {}",
        stdout
    );
}
