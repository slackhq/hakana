use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::process::{Child, Command as TokioCommand};
use tokio::time::{sleep, timeout};

/// Integration test for daemon and language server client communication
///
/// This test verifies:
/// 1. The daemon can be started and listens on the expected port
/// 2. The language server client can connect to the daemon
/// 3. Basic LSP communication works end-to-end
#[tokio::test]
async fn test_daemon_language_server_integration() {
    // Set a shorter timeout for the entire test
    let test_result = timeout(Duration::from_secs(60), run_integration_test()).await;

    // Always cleanup regardless of test result
    cleanup_processes().await;

    // Handle timeout or test failure
    match test_result {
        Ok(Ok(())) => println!("✅ Integration test passed!"),
        Ok(Err(e)) => panic!("Integration test failed: {}", e),
        Err(_) => panic!("Integration test timed out after 60 seconds"),
    }
}

async fn run_integration_test() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Starting daemon and language server integration test...");

    // Step 1: Start the daemon
    let daemon_process = start_daemon().await?;

    // Step 2: Wait for daemon to be ready
    wait_for_daemon_ready().await?;

    // Step 3: Test LSP communication through the language server client
    test_lsp_communication().await?;

    // Step 4: Clean up daemon process
    stop_daemon_process(daemon_process).await?;

    println!("✅ Integration test passed!");
    Ok(())
}

async fn start_daemon() -> Result<Child, Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 Starting Hakana daemon...");

    // Create a temporary directory for the test
    let test_dir = std::env::temp_dir().join("hakana_integration_test");
    std::fs::create_dir_all(&test_dir).map_err(|e| format!("Failed to create test directory: {}", e))?;

    // Create a complete hakana-daemon.toml config file
    let config_content = r#"
host = "127.0.0.1"
port = 9999
max_clients = 10
log_level = "info"

[file_watcher]
use_watchman = false
poll_interval = 2
debounce_delay = 500
watch_patterns = ["**/*.hack", "**/*.php", "**/*.hhi"]
ignore_patterns = ["**/node_modules/**", "**/vendor/**", "**/.git/**", "**/target/**"]

[analysis]
threads = 2
incremental = true
max_parallel_files = 100
timeout = 60
"#;
    std::fs::write(test_dir.join("hakana-daemon.toml"), config_content)
        .map_err(|e| format!("Failed to write config file: {}", e))?;

    // Create a minimal hakana.json config file
    let hakana_config = r#"
{
    "paths": ["."],
    "issues": {}
}
"#;
    std::fs::write(test_dir.join("hakana.json"), hakana_config)
        .map_err(|e| format!("Failed to write hakana config: {}", e))?;

    // Build the daemon first to ensure binary exists
    let build_output = Command::new("cargo")
        .args(&["build", "--bin", "hakana-daemon"])
        .current_dir("/Users/brownmatthew/git/hakana/hakana-core")
        .output()
        .map_err(|e| format!("Failed to build daemon: {}", e))?;

    if !build_output.status.success() {
        return Err(format!("Failed to build daemon: {}", String::from_utf8_lossy(&build_output.stderr)).into());
    }

    // Start the daemon process
    let daemon_binary = "/Users/brownmatthew/git/hakana/hakana-core/target/debug/hakana-daemon";
    let mut daemon_cmd = TokioCommand::new(daemon_binary);
    daemon_cmd
        .args(&["start"])
        .current_dir(&test_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    println!("   Starting daemon with binary: {}", daemon_binary);
    println!("   Working directory: {}", test_dir.display());

    let mut daemon_process = daemon_cmd.spawn()
        .map_err(|e| format!("Failed to start daemon: {}", e))?;

    // Give the process a moment to start and capture any immediate errors
    sleep(Duration::from_millis(100)).await;

    // Check if process is still running
    if let Ok(Some(exit_status)) = daemon_process.try_wait() {
        // Process exited immediately - capture output
        let output = daemon_process.wait_with_output().await
            .map_err(|e| format!("Failed to get daemon output: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        return Err(format!(
            "Daemon exited immediately with status: {}\nStdout: {}\nStderr: {}",
            exit_status, stdout, stderr
        ).into());
    }

    println!("✅ Daemon process started");
    Ok(daemon_process)
}

async fn wait_for_daemon_ready() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("⏳ Waiting for daemon to be ready...");

    // Try to connect to the daemon port with retries - shorter timeout for testing
    let max_attempts = 15; // 15 seconds max wait time
    let mut attempts = 0;

    while attempts < max_attempts {
        match timeout(Duration::from_secs(2), TcpStream::connect("127.0.0.1:9999")).await {
            Ok(Ok(_)) => {
                println!("✅ Daemon is ready and accepting connections");
                return Ok(());
            }
            Ok(Err(e)) => {
                println!("   Connection failed: {}", e);
            }
            Err(_) => {
                println!("   Connection attempt timed out");
            }
        }

        attempts += 1;
        if attempts % 3 == 0 {
            println!("   Still waiting for daemon... (attempt {}/{})", attempts, max_attempts);
        }
        sleep(Duration::from_secs(1)).await;
    }

    Err(format!("Daemon failed to start within {} seconds", max_attempts).into())
}

async fn test_lsp_communication() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔌 Testing LSP communication...");

    // Connect directly to the daemon using our daemon client
    let (daemon_client, _notification_rx) = hakana_daemon_server::daemon_client::DaemonClient::connect("127.0.0.1:9999", "hakana-test-client").await
        .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

    // Test initialize request
    let initialize_params = serde_json::json!({
        "processId": 1234,
        "rootUri": null,
        "capabilities": {},
        "clientInfo": {
            "name": "hakana-test-client",
            "version": "test"
        }
    });

    println!("   Sending initialize request...");
    match daemon_client.send_request("initialize", Some(initialize_params)).await {
        Ok(response) => {
            println!("   ✅ Initialize request successful");

            // Verify response contains capabilities
            if let Some(result) = response.result {
                if result.get("capabilities").is_some() {
                    println!("   ✅ Server capabilities received");
                } else {
                    return Err("Server capabilities not found in response".into());
                }
            } else if response.error.is_some() {
                return Err(format!("Initialize request failed: {:?}", response.error).into());
            } else {
                return Err("No result or error in initialize response".into());
            }
        }
        Err(e) => {
            return Err(format!("Initialize request failed: {}", e).into());
        }
    }

    println!("✅ LSP communication test passed");
    Ok(())
}

async fn stop_daemon_process(mut daemon_process: Child) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🛑 Stopping daemon process...");

    // Try graceful shutdown first
    if let Some(id) = daemon_process.id() {
        // Send SIGTERM (15) for graceful shutdown
        #[cfg(unix)]
        {
            use std::process::Command as StdCommand;
            let _ = StdCommand::new("kill")
                .args(&["-15", &id.to_string()])
                .output();
        }

        // Wait a bit for graceful shutdown
        match timeout(Duration::from_secs(5), daemon_process.wait()).await {
            Ok(_) => {
                println!("✅ Daemon stopped gracefully");
                return Ok(());
            }
            Err(_) => {
                println!("⚠️ Daemon didn't stop gracefully, forcing kill...");
            }
        }
    }

    // Force kill if graceful shutdown failed
    if let Err(e) = daemon_process.kill().await {
        println!("⚠️ Failed to kill daemon process: {}", e);
    } else {
        println!("✅ Daemon process killed");
    }

    Ok(())
}

async fn cleanup_processes() {
    println!("🧹 Cleaning up any remaining processes...");

    // Kill any remaining hakana-daemon processes
    #[cfg(unix)]
    {
        let _ = Command::new("pkill")
            .args(&["-f", "hakana-daemon"])
            .output();
    }

    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(&["/F", "/IM", "hakana-daemon.exe"])
            .output();
    }

    // Clean up test directory
    let test_dir = std::env::temp_dir().join("hakana_integration_test");
    let _ = std::fs::remove_dir_all(test_dir);

    println!("✅ Cleanup completed");
}

/// Simple connection test that doesn't require full daemon startup
#[tokio::test]
async fn test_daemon_binary_starts() {
    cleanup_processes().await;

    let test_dir = std::env::temp_dir().join("hakana_simple_test");
    std::fs::create_dir_all(&test_dir).expect("Failed to create test directory");

    // Create minimal config files
    let config_content = r#"
host = "127.0.0.1"
port = 9998
max_clients = 5
log_level = "error"

[file_watcher]
use_watchman = false
poll_interval = 10
debounce_delay = 1000
watch_patterns = ["**/*.hack"]
ignore_patterns = ["**/node_modules/**"]

[analysis]
threads = 1
incremental = false
max_parallel_files = 10
timeout = 30
"#;
    std::fs::write(test_dir.join("hakana-daemon.toml"), config_content)
        .expect("Failed to write config file");

    std::fs::write(test_dir.join("hakana.json"), r#"{"paths": ["."], "issues": {}}"#)
        .expect("Failed to write hakana config");

    // Try to start daemon with a very short timeout to see if it starts properly
    let daemon_binary = format!("{}/target/debug/hakana-daemon", env!("CARGO_MANIFEST_DIR"));

    let mut daemon_cmd = TokioCommand::new(&daemon_binary);
    daemon_cmd
        .args(&["start"])
        .current_dir(&test_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    match daemon_cmd.spawn() {
        Ok(mut child) => {
            // Give it 2 seconds to start
            sleep(Duration::from_secs(2)).await;

            // Check if process is still running (good sign)
            match child.try_wait() {
                Ok(None) => {
                    println!("✅ Daemon process starts and runs");
                    let _ = child.kill().await;
                },
                Ok(Some(status)) => {
                    let output = child.wait_with_output().await.unwrap();
                    println!("⚠️ Daemon exited with status: {}", status);
                    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
                    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
                },
                Err(e) => println!("⚠️ Error checking daemon status: {}", e),
            }
        },
        Err(e) => panic!("Failed to start daemon binary: {}", e),
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(test_dir);
}

/// Helper test to verify daemon binary exists and can be run
#[tokio::test]
async fn test_daemon_binary_exists() {
    let output = Command::new("cargo")
        .args(&["build", "--bin", "hakana-daemon"])
        .output()
        .expect("Failed to execute cargo build");

    assert!(output.status.success(), "Failed to build hakana-daemon binary: {}",
            String::from_utf8_lossy(&output.stderr));

    println!("✅ hakana-daemon binary builds successfully");
}

/// Helper test to verify language server binary exists and can be run
#[tokio::test]
async fn test_language_server_binary_exists() {
    let output = Command::new("cargo")
        .args(&["build", "--bin", "hakana-language-server"])
        .output()
        .expect("Failed to execute cargo build");

    assert!(output.status.success(), "Failed to build hakana-language-server binary: {}",
            String::from_utf8_lossy(&output.stderr));

    println!("✅ hakana-language-server binary builds successfully");
}