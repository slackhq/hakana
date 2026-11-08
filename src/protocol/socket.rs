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
    /// Held for as long as this server owns the socket path.
    _lock: SocketLock,
}

/// The claim a server has on a socket path: a lock on a file next to it.
///
/// Binding has to replace the socket file a dead server left behind, and seeing
/// that the file is stale and then replacing it is not atomic: of two servers
/// starting together, the second would remove the socket the first had just
/// bound. So a server claims the path first. The kernel drops the lock when the
/// process holding it ends, however it ends.
struct SocketLock {
    path: PathBuf,
    _file: fs::File,
}

impl SocketLock {
    /// Fails with `AddrInUse` if a live server has the path.
    fn acquire(socket_path: &SocketPath) -> io::Result<Self> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

        let mut path = socket_path.path().as_os_str().to_owned();
        path.push(".lock");
        let path = PathBuf::from(path);

        loop {
            let file = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o600)
                .open(&path)?;

            match file.try_lock() {
                Ok(()) => {}
                Err(fs::TryLockError::WouldBlock) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        format!(
                            "Server already running on socket: {}",
                            socket_path.path().display()
                        ),
                    ));
                }
                Err(fs::TryLockError::Error(e)) => return Err(e),
            }

            // A server that shuts down removes its lock file. If one did between
            // this file being opened and locked, the lock is on a file that is no
            // longer at the path, and keeps nobody out: start again.
            let locked = file.metadata()?;
            if fs::metadata(&path)
                .is_ok_and(|at_path| at_path.dev() == locked.dev() && at_path.ino() == locked.ino())
            {
                return Ok(Self { path, _file: file });
            }
        }
    }
}

impl Drop for SocketLock {
    fn drop(&mut self) {
        // Removed while still locked, so that it is this server's file that goes.
        let _ = fs::remove_file(&self.path);
    }
}

impl ServerSocket {
    /// Listen on the socket path, replacing the socket file a dead server left
    /// behind. Fails with `AddrInUse` if a live server has the path.
    pub fn bind(socket_path: SocketPath) -> io::Result<Self> {
        let lock = SocketLock::acquire(&socket_path)?;

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
            _lock: lock,
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
        // The lock is still held here, so the file at the path is this server's.
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

    fn temp_socket_path(dir: &tempfile::TempDir) -> SocketPath {
        SocketPath::from_path(dir.path().join("hakana.sock"))
    }

    /// Race 4: two servers started at once for the same project both find no
    /// server and go on to bind. The second must not take the socket from under
    /// the first, which would be left running where no client can reach it.
    #[tokio::test]
    async fn second_server_cannot_take_a_live_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = temp_socket_path(&dir);

        let first = ServerSocket::bind(socket_path.clone()).expect("Failed to bind");

        let second = ServerSocket::bind(socket_path.clone());
        assert_eq!(
            second.err().map(|e| e.kind()),
            Some(io::ErrorKind::AddrInUse),
            "a second server bound the socket of a running one"
        );

        // The first server is still the one that answers.
        let accepted = tokio::spawn(async move { first.accept().await.is_ok() });
        ClientSocket::connect(&socket_path)
            .await
            .expect("Failed to connect");
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), accepted)
                .await
                .is_ok_and(|accepted| accepted.unwrap()),
            "the first server no longer receives connections"
        );
    }

    /// A server that died without cleaning up leaves its socket file behind.
    #[tokio::test]
    async fn socket_of_a_dead_server_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = temp_socket_path(&dir);

        // Dropping a std listener closes it and leaves the file.
        drop(std::os::unix::net::UnixListener::bind(socket_path.path()).unwrap());
        assert!(socket_path.path().exists());
        assert!(!socket_path.server_exists());

        let server = ServerSocket::bind(socket_path.clone()).expect("Failed to bind");
        assert!(socket_path.server_exists());

        // And once this one is gone, the next can bind.
        drop(server);
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "a server that shut down left files behind"
        );
        ServerSocket::bind(socket_path).expect("Failed to bind after a clean shutdown");
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
