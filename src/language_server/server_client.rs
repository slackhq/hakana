//! Client for connecting to the hakana server from the language server.
//!
//! This module provides functionality to:
//! - Connect to an existing hakana server
//! - Spawn a new server if one isn't running
//! - Forward file changes and analysis requests
//!
//! Note: The server handles one request per connection, so the client
//! reconnects for each request.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use hakana_protocol::{
    ClientSocket, FileChange, GetIssuesRequest, GetIssuesResponse,
    GotoDefinitionRequest, GotoDefinitionResponse, Message, ShutdownRequest,
    SocketPath, StatusRequest, StatusResponse,
};
use rustc_hash::FxHashMap;

/// Connection to the hakana server.
///
/// The server handles one request per connection, so each method
/// creates a fresh connection to the server. If the server dies,
/// it will be automatically respawned on the next request.
#[derive(Debug)]
pub struct ServerConnection {
    socket_path: SocketPath,
    project_root: PathBuf,
    /// Child process handle if we spawned the server
    server_process: Option<Child>,
    /// Binary path used to spawn the server
    hakana_binary: Option<String>,
}

impl ServerConnection {
    /// Connect to an existing server, returning None if no server is running.
    pub fn try_connect(project_root: &Path) -> Option<Self> {
        let socket_path = SocketPath::for_project(project_root);

        if !socket_path.server_exists() {
            return None;
        }

        // Verify we can actually connect
        if ClientSocket::connect(&socket_path).is_ok() {
            Some(Self {
                socket_path,
                project_root: project_root.to_path_buf(),
                server_process: None,
                hakana_binary: None,
            })
        } else {
            None
        }
    }

    /// Connect to an existing server or spawn a new one.
    /// This is a blocking operation.
    pub fn connect_or_spawn(
        project_root: &Path,
        hakana_binary: Option<&str>,
    ) -> io::Result<Self> {
        let socket_path = SocketPath::for_project(project_root);

        // Try to connect to existing server
        if socket_path.server_exists() {
            if ClientSocket::connect(&socket_path).is_ok() {
                return Ok(Self {
                    socket_path,
                    project_root: project_root.to_path_buf(),
                    server_process: None,
                    hakana_binary: hakana_binary.map(|s| s.to_string()),
                });
            }
        }

        // Spawn a new server
        let mut server_process = Self::spawn_server(project_root, hakana_binary)?;

        // Wait for server to be ready (also checks if process died during startup)
        Self::wait_for_server(&socket_path, &mut server_process, Duration::from_secs(120))?;

        Ok(Self {
            socket_path,
            project_root: project_root.to_path_buf(),
            server_process: Some(server_process),
            hakana_binary: hakana_binary.map(|s| s.to_string()),
        })
    }

    /// Create a fresh connection to the server for a single request.
    /// If the connection fails, attempts to respawn the server.
    fn connect(&mut self) -> io::Result<ClientSocket> {
        // First, try to connect directly
        if let Ok(socket) = ClientSocket::connect(&self.socket_path) {
            return Ok(socket);
        }

        // Connection failed - server may have died. Try to respawn.
        eprintln!("Server connection failed, attempting to respawn...");

        // Clean up old process handle if any
        self.server_process = None;

        // Spawn a new server
        let mut server_process = Self::spawn_server(
            &self.project_root,
            self.hakana_binary.as_deref(),
        )?;

        // Wait for it to be ready (also checks if process died during startup)
        Self::wait_for_server(&self.socket_path, &mut server_process, Duration::from_secs(120))?;

        self.server_process = Some(server_process);

        // Try to connect again
        ClientSocket::connect(&self.socket_path)
            .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, format!("{}", e)))
    }

    /// Spawn a new hakana server process.
    fn spawn_server(
        project_root: &Path,
        hakana_binary: Option<&str>,
    ) -> io::Result<Child> {
        // Find the hakana binary
        let binary = if let Some(bin) = hakana_binary {
            PathBuf::from(bin)
        } else {
            Self::find_hakana_binary()?
        };

        eprintln!("Spawning hakana server: {} server --root {}", binary.display(), project_root.display());

        // Create a log file for server output so we can see errors
        let log_path = std::env::temp_dir().join("hakana-server.log");
        let log_file = std::fs::File::create(&log_path).ok();
        let log_file2 = std::fs::OpenOptions::new()
            .append(true)
            .open(&log_path)
            .ok();

        eprintln!("Server log file: {}", log_path.display());

        // Capture both stdout and stderr to the log file
        // Set current_dir to project_root to ensure consistent behavior
        // Environment (including PATH) is inherited by default from the parent process
        let child = Command::new(&binary)
            .arg("server")
            .arg("--root")
            .arg(project_root)
            .current_dir(project_root)
            .stdin(Stdio::null())
            .stdout(log_file.map(Stdio::from).unwrap_or(Stdio::null()))
            .stderr(log_file2.map(Stdio::from).unwrap_or(Stdio::null()))
            .spawn()
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("Failed to spawn hakana server (binary: {}): {}", binary.display(), e),
                )
            })?;

        Ok(child)
    }

    /// Find the hakana binary.
    /// The hakana binary is always in the same directory as hakana-language-server.
    fn find_hakana_binary() -> io::Result<PathBuf> {
        let current_exe = std::env::current_exe().map_err(|e| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Could not determine current executable path: {}", e),
            )
        })?;

        let exe_dir = current_exe.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine executable directory",
            )
        })?;

        // hakana binary is in the same directory as hakana-language-server
        let hakana_path = exe_dir.join("hakana");
        if hakana_path.exists() {
            return Ok(hakana_path);
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Could not find hakana binary at {}. Ensure hakana is built and in the same directory as hakana-language-server.",
                hakana_path.display()
            ),
        ))
    }

    /// Wait for the server to be ready.
    /// Also checks if the child process has died during startup.
    fn wait_for_server(
        socket_path: &SocketPath,
        child: &mut Child,
        timeout: Duration,
    ) -> io::Result<()> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(100);

        loop {
            // Check if child process has exited (died)
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Try to read the server log file to see what went wrong
                    let log_path = std::env::temp_dir().join("hakana-server.log");
                    let log_contents = std::fs::read_to_string(&log_path)
                        .unwrap_or_else(|_| "<no log available>".to_string());
                    eprintln!("Server log contents:\n{}", log_contents);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Server process exited during startup with status: {}. Check {} for details.", status, log_path.display()),
                    ));
                }
                Ok(None) => {
                    // Process still running, continue waiting
                }
                Err(e) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to check server process status: {}", e),
                    ));
                }
            }

            if start.elapsed() > timeout {
                // Kill the child process since we're giving up
                let _ = child.kill();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Timed out waiting for server to start",
                ));
            }

            if socket_path.server_exists() {
                if ClientSocket::connect(socket_path).is_ok() {
                    return Ok(());
                }
                // Server socket exists but not ready yet
                thread::sleep(poll_interval);
            } else {
                thread::sleep(poll_interval);
            }
        }
    }

    /// Check server status.
    pub fn status(&mut self) -> io::Result<StatusResponse> {
        let mut socket = self.connect()?;
        let request = Message::Status(StatusRequest);
        match socket.request(&request) {
            Ok(Message::StatusResult(response)) => Ok(response),
            Ok(Message::Error(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Server error: {}", e.message),
            )),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("{}", e))),
        }
    }

    /// Get current issues from the server.
    pub fn get_issues(
        &mut self,
        filter: Option<String>,
        find_unused_expressions: bool,
        find_unused_definitions: bool,
    ) -> io::Result<GetIssuesResponse> {
        let mut socket = self.connect()?;
        let request = Message::GetIssues(GetIssuesRequest {
            filter,
            find_unused_expressions,
            find_unused_definitions,
        });

        match socket.request(&request) {
            Ok(Message::GetIssuesResult(response)) => Ok(response),
            Ok(Message::Error(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Server error: {}", e.message),
            )),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("{}", e))),
        }
    }

    /// Send file changes to the server.
    pub fn notify_file_changes(
        &mut self,
        changes: FxHashMap<String, hakana_orchestrator::file::FileStatus>,
    ) -> io::Result<()> {
        let mut socket = self.connect()?;
        let file_changes: Vec<FileChange> = changes
            .into_iter()
            .map(|(path, status)| FileChange {
                path,
                status: status.into(),
            })
            .collect();

        let request = Message::FileChanged(file_changes);

        match socket.request(&request) {
            Ok(Message::Ack(_)) => Ok(()),
            Ok(Message::Error(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Server error: {}", e.message),
            )),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("{}", e))),
        }
    }

    /// Request goto-definition.
    pub fn goto_definition(
        &mut self,
        file_path: String,
        line: u32,
        column: u32,
    ) -> io::Result<GotoDefinitionResponse> {
        let mut socket = self.connect()?;
        let request = Message::GotoDefinition(GotoDefinitionRequest {
            file_path,
            line,
            column,
        });

        match socket.request(&request) {
            Ok(Message::GotoDefinitionResult(response)) => Ok(response),
            Ok(Message::Error(e)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Server error: {}", e.message),
            )),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("{}", e))),
        }
    }

    /// Shutdown the server.
    pub fn shutdown(&mut self) -> io::Result<()> {
        let mut socket = self.connect()?;
        let request = Message::Shutdown(ShutdownRequest);

        match socket.request(&request) {
            Ok(Message::Ack(_)) => Ok(()),
            Ok(_) => Ok(()), // Any response is fine for shutdown
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("{}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Test that find_hakana_binary returns a proper error when binary doesn't exist
    #[test]
    fn test_find_hakana_binary_error_message() {
        // This test verifies the error message is helpful
        // In a real scenario, the binary should exist next to hakana-language-server
        let result = ServerConnection::find_hakana_binary();

        // Either it finds the binary (in dev environment) or gives a clear error
        match result {
            Ok(path) => {
                assert!(path.exists(), "Found binary path should exist");
                assert!(
                    path.to_string_lossy().contains("hakana"),
                    "Binary path should contain 'hakana'"
                );
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("hakana") && msg.contains("binary"),
                    "Error message should mention hakana binary: {}",
                    msg
                );
            }
        }
    }

    /// Test that wait_for_server detects when a process dies
    #[test]
    fn test_wait_for_server_detects_dead_process() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let socket_path = SocketPath::for_project(temp_dir.path());

        // Spawn a process that immediately exits
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("exit 1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        // Give it a moment to exit
        thread::sleep(Duration::from_millis(100));

        // wait_for_server should detect the process died
        let result = ServerConnection::wait_for_server(
            &socket_path,
            &mut child,
            Duration::from_secs(1),
        );

        assert!(result.is_err(), "Should return error when process dies");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("exited"),
            "Error should mention process exited: {}",
            err
        );
    }

    /// Test that wait_for_server times out appropriately
    #[test]
    fn test_wait_for_server_timeout() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let socket_path = SocketPath::for_project(temp_dir.path());

        // Spawn a process that sleeps (stays alive but doesn't create socket)
        let mut child = Command::new("sleep")
            .arg("10")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        // wait_for_server should timeout since no socket is created
        let result = ServerConnection::wait_for_server(
            &socket_path,
            &mut child,
            Duration::from_millis(200),
        );

        // Clean up the sleep process
        let _ = child.kill();
        let _ = child.wait();

        assert!(result.is_err(), "Should return error on timeout");
        let err = result.unwrap_err();
        assert!(
            err.kind() == io::ErrorKind::TimedOut,
            "Error should be TimedOut, got: {:?}",
            err.kind()
        );
    }

    /// Test that try_connect returns None when no server exists
    #[test]
    fn test_try_connect_no_server() {
        let temp_dir = tempdir().expect("Failed to create temp dir");

        let result = ServerConnection::try_connect(temp_dir.path());

        assert!(
            result.is_none(),
            "Should return None when no server is running"
        );
    }

    /// Test ServerConnection respawn behavior (integration test)
    /// This test requires the hakana binary to be built
    #[test]
    #[ignore] // Run with --ignored flag, requires built hakana binary
    fn test_server_respawn_on_death() {
        let temp_dir = tempdir().expect("Failed to create temp dir");

        // Create a minimal hakana.json
        let config_path = temp_dir.path().join("hakana.json");
        fs::write(&config_path, r#"{"ignore_files": []}"#)
            .expect("Failed to write config");

        // Connect or spawn should work
        let mut conn = ServerConnection::connect_or_spawn(temp_dir.path(), None)
            .expect("Should be able to spawn server");

        // Get status to verify server is working
        let status = conn.status().expect("Should get status");
        assert!(status.ready || !status.analysis_in_progress);

        // Kill the server process
        if let Some(ref mut child) = conn.server_process {
            child.kill().expect("Should kill server");
            child.wait().expect("Should wait for server");
        }
        conn.server_process = None;

        // Give it a moment
        thread::sleep(Duration::from_millis(100));

        // Next request should respawn the server
        let status = conn.status().expect("Should respawn and get status");
        assert!(status.ready || !status.analysis_in_progress);

        // Clean up
        let _ = conn.shutdown();
    }
}
