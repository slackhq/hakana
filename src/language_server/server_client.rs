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
            })
        } else {
            None
        }
    }

    pub async fn connect_or_spawn(
        project_root: &Path,
        hakana_binary: Option<&str>,
    ) -> io::Result<Self> {
        let socket_path = SocketPath::for_project(project_root);

        if socket_path.server_exists() && ClientSocket::connect(&socket_path).await.is_ok() {
            return Ok(Self {
                socket_path,
                project_root: project_root.to_path_buf(),
                hakana_binary: hakana_binary.map(|s| s.to_string()),
            });
        }

        Self::start_server(project_root, hakana_binary, &socket_path).await?;

        Ok(Self {
            socket_path,
            project_root: project_root.to_path_buf(),
            hakana_binary: hakana_binary.map(|s| s.to_string()),
        })
    }

    async fn connect(&self) -> io::Result<ClientSocket> {
        if let Ok(socket) = ClientSocket::connect(&self.socket_path).await {
            return Ok(socket);
        }

        log::info!("Server connection failed, attempting to respawn...");

        Self::start_server(
            &self.project_root,
            self.hakana_binary.as_deref(),
            &self.socket_path,
        )
        .await?;

        ClientSocket::connect(&self.socket_path)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, format!("{}", e)))
    }

    async fn start_server(
        project_root: &Path,
        hakana_binary: Option<&str>,
        socket_path: &SocketPath,
    ) -> io::Result<()> {
        let binary = if let Some(bin) = hakana_binary {
            PathBuf::from(bin)
        } else {
            Self::find_hakana_binary()?
        };

        let factory = PreferredServerFactory::discover();
        let mut server_process = factory.spawn_server(project_root, &binary)?;
        factory
            .wait_for_server(
                socket_path,
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
    ) -> io::Result<GetIssuesResponse> {
        let mut socket = self.connect().await?;
        let request = Message::GetIssues(GetIssuesRequest {
            filter,
            find_unused_expressions,
            find_unused_definitions,
            block_until_next_analysis,
            send_progress_report: false,
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
