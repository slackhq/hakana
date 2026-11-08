use std::path::{Path, PathBuf};
use std::{env, io};

use hakana_protocol::{
    ClientSocket, FileChange, GetIssuesRequest, GetIssuesResponse, GotoDefinitionRequest,
    GotoDefinitionResponse, Message, ShutdownRequest, SocketPath, StatusRequest, StatusResponse,
};
use rustc_hash::FxHashMap;

use crate::server_factory::{PreferredServerFactory, ServerFactory};

#[derive(Debug)]
pub struct ServerConnection {
    socket_path: SocketPath,
    project_root: PathBuf,
    hakana_binary: Option<String>,
    factory: PreferredServerFactory,
    /// Held while starting a server, so that requests which find the server gone
    /// at the same time start one replacement between them.
    starting_server: tokio::sync::Mutex<()>,
}

impl ServerConnection {
    pub async fn try_connect(project_root: &Path) -> Option<Self> {
        let socket_path = SocketPath::for_project(project_root);

        if !socket_path.server_exists() {
            return None;
        }

        if ClientSocket::connect(&socket_path).await.is_ok() {
            Some(Self {
                socket_path,
                project_root: project_root.to_path_buf(),
                hakana_binary: None,
                factory: PreferredServerFactory::discover(),
                starting_server: Default::default(),
            })
        } else {
            None
        }
    }

    pub async fn connect_or_spawn(
        project_root: &Path,
        hakana_binary: Option<&str>,
    ) -> io::Result<Self> {
        let connection = Self {
            socket_path: SocketPath::for_project(project_root),
            project_root: project_root.to_path_buf(),
            hakana_binary: hakana_binary.map(|s| s.to_string()),
            factory: PreferredServerFactory::discover(),
            starting_server: Default::default(),
        };

        if connection.socket_path.server_exists()
            && ClientSocket::connect(&connection.socket_path).await.is_ok()
        {
            return Ok(connection);
        }

        connection.start_server().await?;

        Ok(connection)
    }

    /// A connection to whatever listens on `socket_path`. Should that server go
    /// away, starting a replacement fails rather than launching a real one.
    #[cfg(test)]
    pub(crate) fn for_tests(socket_path: SocketPath) -> Self {
        Self {
            socket_path,
            project_root: PathBuf::from("/hakana-fake-project"),
            hakana_binary: Some("/nonexistent/hakana".to_string()),
            factory: PreferredServerFactory::Direct(
                crate::server_factory::DirectProcessServerFactory,
            ),
            starting_server: Default::default(),
        }
    }

    async fn connect(&self) -> io::Result<ClientSocket> {
        if let Ok(socket) = ClientSocket::connect(&self.socket_path).await {
            return Ok(socket);
        }

        let _starting_server = self.starting_server.lock().await;

        // Another request may have started the server while this one waited.
        if let Ok(socket) = ClientSocket::connect(&self.socket_path).await {
            return Ok(socket);
        }

        log::info!("Server connection failed, attempting to respawn...");

        self.start_server().await?;

        ClientSocket::connect(&self.socket_path)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, format!("{}", e)))
    }

    async fn start_server(&self) -> io::Result<()> {
        let binary = if let Some(bin) = &self.hakana_binary {
            PathBuf::from(bin)
        } else {
            Self::find_hakana_binary()?
        };

        let mut server_process = self.factory.spawn_server(&self.project_root, &binary)?;
        self.factory
            .wait_for_server(
                &self.socket_path,
                &mut server_process,
                std::time::Duration::from_secs(120),
            )
            .await
    }

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

        let hakana_path = exe_dir.join("hakana");
        if hakana_path.exists() {
            return Ok(hakana_path);
        }

        env::var_os("PATH")
            .map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .and_then(|paths| paths.iter().map(|p| p.join("hakana")).find(|p| p.exists()))
            .ok_or(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Could not find hakana binary in the PATH or at {}. Ensure hakana is built and in the PATH the same directory as hakana-language-server.",
                    hakana_path.display()
                ),
            ))
    }

    pub async fn status(&self) -> io::Result<StatusResponse> {
        let mut socket = self.connect().await?;
        let request = Message::Status(StatusRequest);
        match socket.request(&request).await {
            Ok(Message::StatusResult(response)) => Ok(response),
            Ok(Message::Error(e)) => Err(io::Error::other(format!("Server error: {}", e.message))),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::other(format!("{}", e))),
        }
    }

    pub async fn get_issues(
        &self,
        filter: Option<String>,
        find_unused_expressions: bool,
        find_unused_definitions: bool,
        block_until_next_analysis: bool,
        last_seen_generation: Option<u64>,
    ) -> io::Result<GetIssuesResponse> {
        let mut socket = self.connect().await?;
        let request = Message::GetIssues(GetIssuesRequest {
            filter,
            find_unused_expressions,
            find_unused_definitions,
            block_until_next_analysis,
            send_progress_report: false,
            last_seen_generation,
        });

        match socket.request(&request).await {
            Ok(Message::GetIssuesResult(response)) => Ok(response),
            Ok(Message::Error(e)) => Err(io::Error::other(format!("Server error: {}", e.message))),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::other(format!("{}", e))),
        }
    }

    pub async fn notify_file_changes(
        &self,
        changes: FxHashMap<String, hakana_orchestrator::file::FileStatus>,
    ) -> io::Result<()> {
        let mut socket = self.connect().await?;
        let file_changes: Vec<FileChange> = changes
            .into_iter()
            .map(|(path, status)| FileChange {
                path,
                status: status.into(),
            })
            .collect();

        let request = Message::FileChanged(file_changes);

        match socket.request(&request).await {
            Ok(Message::Ack(_)) => Ok(()),
            Ok(Message::Error(e)) => Err(io::Error::other(format!("Server error: {}", e.message))),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::other(format!("{}", e))),
        }
    }

    pub async fn goto_definition(
        &self,
        file_path: String,
        line: u32,
        column: u32,
    ) -> io::Result<GotoDefinitionResponse> {
        let mut socket = self.connect().await?;
        let request = Message::GotoDefinition(GotoDefinitionRequest {
            file_path,
            line,
            column,
        });

        match socket.request(&request).await {
            Ok(Message::GotoDefinitionResult(response)) => Ok(response),
            Ok(Message::Error(e)) => Err(io::Error::other(format!("Server error: {}", e.message))),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected response type",
            )),
            Err(e) => Err(io::Error::other(format!("{}", e))),
        }
    }

    pub async fn shutdown(&self) -> io::Result<()> {
        let mut socket = self.connect().await?;
        let request = Message::Shutdown(ShutdownRequest);

        match socket.request(&request).await {
            Ok(Message::Ack(_)) => Ok(()),
            Ok(_) => Ok(()),
            Err(e) => Err(io::Error::other(format!("{}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_find_hakana_binary_error_message() {
        let result = ServerConnection::find_hakana_binary();

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

    /// Race 4: the server is gone, and two requests find that out at the same
    /// time. Between them they start one replacement, not one each.
    #[tokio::test]
    async fn concurrent_requests_start_one_server() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::Arc;

        let temp_dir = tempdir().expect("Failed to create temp dir");
        let launches = temp_dir.path().join("launches");

        // Stands in for the hakana binary: it notes that it was launched, and
        // stays around as a server starting up would.
        let binary = temp_dir.path().join("hakana");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\necho launched >> {}\nsleep 2\n",
                launches.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();

        let connection = Arc::new(ServerConnection {
            socket_path: SocketPath::from_path(temp_dir.path().join("hakana.sock")),
            project_root: temp_dir.path().to_path_buf(),
            hakana_binary: Some(binary.to_string_lossy().into_owned()),
            factory: PreferredServerFactory::Direct(
                crate::server_factory::DirectProcessServerFactory,
            ),
            starting_server: Default::default(),
        });

        let requests: Vec<_> = (0..2)
            .map(|_| {
                let connection = connection.clone();
                tokio::spawn(async move { connection.connect().await.map(|_| ()) })
            })
            .collect();

        // The server takes a moment to come up after it is launched.
        while !launches.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let _server = hakana_protocol::ServerSocket::bind(connection.socket_path.clone())
            .expect("Failed to bind");

        for request in requests {
            request.await.unwrap().expect("Failed to connect");
        }

        assert_eq!(
            std::fs::read_to_string(&launches).unwrap().lines().count(),
            1,
            "each request started a server of its own"
        );
    }

    #[tokio::test]
    async fn test_try_connect_no_server() {
        let temp_dir = tempdir().expect("Failed to create temp dir");

        let result = ServerConnection::try_connect(temp_dir.path()).await;

        assert!(
            result.is_none(),
            "Should return None when no server is running"
        );
    }
}
