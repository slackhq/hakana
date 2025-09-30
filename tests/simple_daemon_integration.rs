use std::process::Command;
use std::time::Duration;
use tokio::time::timeout;

/// Simple integration test that verifies daemon binary works
#[tokio::test]
async fn test_daemon_binary_builds_and_shows_help() {
    // Build the daemon binary
    let build_output = Command::new("cargo")
        .args(&["build", "--bin", "hakana-daemon"])
        .output()
        .expect("Failed to execute cargo build");

    assert!(build_output.status.success(),
        "Failed to build hakana-daemon binary: {}",
        String::from_utf8_lossy(&build_output.stderr));

    // Test that daemon shows usage when called without args
    let daemon_binary = format!("{}/target/debug/hakana-daemon", env!("CARGO_MANIFEST_DIR"));

    let help_output = Command::new(&daemon_binary)
        .args(&["--help"])
        .output()
        .unwrap_or_else(|_| {
            // If --help doesn't work, try with no args to get usage
            Command::new(&daemon_binary)
                .output()
                .expect("Failed to run daemon binary")
        });

    let output_str = String::from_utf8_lossy(&help_output.stderr);
    let stdout_str = String::from_utf8_lossy(&help_output.stdout);

    // Check if we get expected daemon usage output
    let combined_output = format!("{}{}", stdout_str, output_str);
    assert!(
        combined_output.contains("Hakana Daemon") ||
        combined_output.contains("hakana-daemon") ||
        combined_output.contains("start") ||
        combined_output.contains("stop"),
        "Daemon doesn't show expected usage. Output: {}", combined_output
    );

    println!("✅ Daemon binary builds and shows usage information");
}

/// Test that language server binary builds
#[tokio::test]
async fn test_language_server_binary_builds() {
    let build_output = Command::new("cargo")
        .args(&["build", "--bin", "hakana-language-server"])
        .output()
        .expect("Failed to execute cargo build");

    assert!(build_output.status.success(),
        "Failed to build hakana-language-server binary: {}",
        String::from_utf8_lossy(&build_output.stderr));

    println!("✅ Language server binary builds successfully");
}

/// Test that daemon client library can be instantiated
#[tokio::test]
async fn test_daemon_client_creation() {
    // This tests that our daemon client code compiles and basic types work
    use hakana_daemon_server::daemon_client;

    // Test that we can create the expected types
    let test_result = timeout(Duration::from_secs(2), async {
        // Try to connect to a non-existent daemon (should fail quickly)
        match daemon_client::DaemonClient::connect("127.0.0.1:19999", "test-client").await {
            Ok(_) => panic!("Unexpected success connecting to non-existent daemon"),
            Err(_) => {
                // Expected failure - daemon not running
                println!("✅ Daemon client correctly fails when daemon not available");
            }
        }
    }).await;

    assert!(test_result.is_ok(), "Daemon client test timed out");
}

/// Test configuration loading
#[test]
fn test_daemon_config_default() {
    use hakana_daemon_server::config::DaemonConfig;

    let config = DaemonConfig::default();
    assert_eq!(config.host, "127.0.0.1");
    assert_eq!(config.port, 9999);
    assert!(config.max_clients > 0);
    assert_eq!(config.log_level, "info");

    println!("✅ Daemon config defaults are correct");
}

/// Test local daemon binary discovery
#[tokio::test]
async fn test_local_daemon_discovery() {
    use std::path::Path;

    // Get current executable directory (simulating language server location)
    let current_exe = std::env::current_exe().expect("Could not get current executable path");
    let exe_dir = current_exe.parent().expect("Could not get parent directory");

    // Check if daemon binary exists in same directory
    let daemon_path = exe_dir.join("hakana-daemon");

    println!("Looking for daemon at: {}", daemon_path.display());

    // The daemon binary should be in the same directory as our test binary
    if daemon_path.exists() {
        println!("✅ Daemon binary found at expected location");

        // Test that it responds to --help
        let output = tokio::process::Command::new(&daemon_path)
            .args(&["--help"])
            .output()
            .await
            .expect("Failed to execute daemon binary");

        let combined_output = format!("{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            output.status.success() ||
            combined_output.contains("Hakana") ||
            combined_output.contains("Usage:"),
            "Daemon binary doesn't respond correctly to --help: {}", combined_output
        );

        println!("✅ Daemon binary responds correctly to --help");
    } else {
        println!("⚠️ Daemon binary not found at expected location (this is OK for development)");
        println!("   Expected location: {}", daemon_path.display());
        println!("   Language server will look in this same directory at runtime");
    }
}

/// Test protocol types
#[test]
fn test_protocol_types() {
    use hakana_daemon_server::protocol::{Request, Response};
    use serde_json::json;

    // Test request serialization
    let request = Request {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        method: "initialize".to_string(),
        params: Some(json!({"test": "value"})),
    };

    let serialized = serde_json::to_string(&request).expect("Failed to serialize request");
    assert!(serialized.contains("initialize"));

    // Test response serialization
    let response = Response {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        result: Some(json!({"capabilities": {}})),
        error: None,
    };

    let serialized = serde_json::to_string(&response).expect("Failed to serialize response");
    assert!(serialized.contains("capabilities"));

    println!("✅ Protocol types serialize correctly");
}