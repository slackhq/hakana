//! Unix socket utilities for hakana client-server communication.

use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use crate::types::Message;

/// Socket path configuration.
#[derive(Debug, Clone)]
pub struct SocketPath {
    path: PathBuf,
}

impl SocketPath {
    /// Create a socket path for a project.
    /// Path format: /tmp/hakana-{hash}.sock
    pub fn for_project(project_root: &Path) -> Self {
        let canonical = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let hash = Self::hash_path(&canonical);
        let path = PathBuf::from(format!("/tmp/hakana-{}.sock", hash));
        Self { path }
    }

    /// Create a socket path from an explicit path.
    pub fn from_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Get the socket path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if a server is listening on this socket.
    pub fn server_exists(&self) -> bool {
        self.path.exists() && UnixStream::connect(&self.path).is_ok()
    }

    /// Remove the socket file if it exists.
    pub fn cleanup(&self) -> io::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    /// Hash a path to create a unique identifier.
    fn hash_path(path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

pub struct ServerSocket {
    listener: tokio::net::UnixListener,
    socket_path: SocketPath,
}

impl ServerSocket {
    pub fn bind(socket_path: SocketPath) -> io::Result<Self> {
        socket_path.cleanup()?;

        let listener = tokio::net::UnixListener::bind(socket_path.path())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(socket_path.path(), perms)?;
        }

        Ok(Self {
            listener,
            socket_path,
        })
    }

    pub async fn accept(&self) -> io::Result<ClientConnection> {
        let (stream, _addr) = self.listener.accept().await?;
        Ok(ClientConnection::new(stream))
    }

    pub fn socket_path(&self) -> &SocketPath {
        &self.socket_path
    }
}

impl Drop for ServerSocket {
    fn drop(&mut self) {
        let _ = self.socket_path.cleanup();
    }
}

pub struct ClientConnection {
    reader: tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::io::BufWriter<tokio::net::unix::OwnedWriteHalf>,
}

impl ClientConnection {
    fn new(stream: tokio::net::UnixStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        let reader = tokio::io::BufReader::new(read_half);
        let writer = tokio::io::BufWriter::new(write_half);
        Self { reader, writer }
    }

    pub async fn read_message(&mut self) -> Result<Message, crate::serialize::ProtocolError> {
        crate::serialize::read_message(&mut self.reader).await
    }

    pub async fn write_message(
        &mut self,
        msg: &Message,
    ) -> Result<(), crate::serialize::ProtocolError> {
        crate::serialize::write_message(&mut self.writer, msg).await
    }
}

pub struct ClientSocket {
    reader: tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::io::BufWriter<tokio::net::unix::OwnedWriteHalf>,
}

impl ClientSocket {
    pub async fn connect(socket_path: &SocketPath) -> io::Result<Self> {
        let stream = tokio::net::UnixStream::connect(socket_path.path()).await?;
        let (read_half, write_half) = stream.into_split();
        let reader = tokio::io::BufReader::new(read_half);
        let writer = tokio::io::BufWriter::new(write_half);
        Ok(Self { reader, writer })
    }

    pub async fn request(
        &mut self,
        msg: &Message,
    ) -> Result<Message, crate::serialize::ProtocolError> {
        crate::serialize::write_message(&mut self.writer, msg).await?;
        crate::serialize::read_message(&mut self.reader).await
    }

    pub async fn send(&mut self, msg: &Message) -> Result<(), crate::serialize::ProtocolError> {
        crate::serialize::write_message(&mut self.writer, msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_socket_path_hash() {
        let path1 = SocketPath::for_project(Path::new("/home/user/project"));
        let path2 = SocketPath::for_project(Path::new("/home/user/project"));
        let path3 = SocketPath::for_project(Path::new("/home/user/other"));

        // Same path should produce same hash
        assert_eq!(path1.path(), path2.path());
        // Different path should produce different hash
        assert_ne!(path1.path(), path3.path());
    }

    #[tokio::test]
    async fn test_client_server_roundtrip() {
        let socket_path = SocketPath::from_path(PathBuf::from("/tmp/hakana-test-roundtrip.sock"));

        let _ = socket_path.cleanup();

        let server_socket_path = socket_path.clone();
        let server_handle = tokio::spawn(async move {
            let server = ServerSocket::bind(server_socket_path).expect("Failed to bind");
            let mut conn = server.accept().await.expect("Failed to accept");

            let msg = conn.read_message().await.expect("Failed to read");
            if let Message::Status(_) = msg {
                let response = Message::StatusResult(StatusResponse {
                    ready: true,
                    files_count: 100,
                    symbols_count: 5000,
                    uptime_secs: 3600,
                    analysis_in_progress: false,
                    pending_requests: 0,
                    project_root: "/home/user/project".to_string(),
                });
                conn.write_message(&response)
                    .await
                    .expect("Failed to write");
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let mut client = ClientSocket::connect(&socket_path)
            .await
            .expect("Failed to connect");

        let request = Message::Status(StatusRequest);
        let response = client.request(&request).await.expect("Failed to request");

        if let Message::StatusResult(status) = response {
            assert!(status.ready);
            assert_eq!(status.files_count, 100);
        } else {
            panic!("Expected StatusResponse");
        }

        server_handle.await.unwrap();
    }
}
