//! Unix socket utilities for hakana client-server communication.

use std::fs;
use std::io::{self, BufReader, BufWriter};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::serialize::{read_message, write_message, ProtocolError};
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

/// Server-side socket wrapper.
pub struct ServerSocket {
    listener: UnixListener,
    socket_path: SocketPath,
}

impl ServerSocket {
    /// Create and bind a new server socket.
    pub fn bind(socket_path: SocketPath) -> io::Result<Self> {
        // Clean up any stale socket file
        socket_path.cleanup()?;

        let listener = UnixListener::bind(socket_path.path())?;

        // Set socket permissions to owner-only
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

    /// Accept a new client connection.
    pub fn accept(&self) -> io::Result<ClientConnection> {
        let (stream, _addr) = self.listener.accept()?;
        // Ensure the connection stream is in blocking mode
        stream.set_nonblocking(false)?;
        Ok(ClientConnection::new(stream))
    }

    /// Set the accept timeout.
    pub fn set_accept_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.listener.set_nonblocking(timeout.is_some())?;
        Ok(())
    }

    /// Get the socket path.
    pub fn socket_path(&self) -> &SocketPath {
        &self.socket_path
    }

}

impl Drop for ServerSocket {
    fn drop(&mut self) {
        let _ = self.socket_path.cleanup();
    }
}

/// A connection to a client (server-side).
pub struct ClientConnection {
    reader: BufReader<UnixStream>,
    writer: BufWriter<UnixStream>,
}

impl ClientConnection {
    fn new(stream: UnixStream) -> Self {
        // Set a generous write timeout for large responses
        let _ = stream.set_write_timeout(Some(Duration::from_secs(300)));
        let reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
        let writer = BufWriter::new(stream);
        Self { reader, writer }
    }

    /// Read a message from the client.
    pub fn read_message(&mut self) -> Result<Message, ProtocolError> {
        read_message(&mut self.reader)
    }

    /// Write a message to the client.
    pub fn write_message(&mut self, msg: &Message) -> Result<(), ProtocolError> {
        write_message(&mut self.writer, msg)
    }

    /// Set read timeout.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.reader.get_ref().set_read_timeout(timeout)
    }

    /// Set write timeout.
    pub fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.writer.get_ref().set_write_timeout(timeout)
    }
}

/// Client-side socket wrapper.
pub struct ClientSocket {
    reader: BufReader<UnixStream>,
    writer: BufWriter<UnixStream>,
}

impl ClientSocket {
    /// Connect to a server.
    pub fn connect(socket_path: &SocketPath) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path.path())?;
        // Set generous timeouts for large responses
        stream.set_read_timeout(Some(Duration::from_secs(300)))?;
        stream.set_write_timeout(Some(Duration::from_secs(60)))?;
        let reader = BufReader::new(stream.try_clone()?);
        let writer = BufWriter::new(stream);
        Ok(Self { reader, writer })
    }

    /// Connect with a timeout.
    pub fn connect_timeout(socket_path: &SocketPath, timeout: Duration) -> io::Result<Self> {
        // Unix sockets don't have a direct connect_timeout, so we use non-blocking connect
        // For simplicity, we'll just use regular connect with a read/write timeout
        let stream = UnixStream::connect(socket_path.path())?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let reader = BufReader::new(stream.try_clone()?);
        let writer = BufWriter::new(stream);
        Ok(Self { reader, writer })
    }

    /// Send a request and wait for a response.
    pub fn request(&mut self, msg: &Message) -> Result<Message, ProtocolError> {
        write_message(&mut self.writer, msg)?;
        read_message(&mut self.reader)
    }

    /// Send a message without waiting for a response (for notifications).
    pub fn send(&mut self, msg: &Message) -> Result<(), ProtocolError> {
        write_message(&mut self.writer, msg)
    }

    /// Read a message from the server.
    pub fn read_message(&mut self) -> Result<Message, ProtocolError> {
        read_message(&mut self.reader)
    }

    /// Set read timeout.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.reader.get_ref().set_read_timeout(timeout)
    }

    /// Set write timeout.
    pub fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.writer.get_ref().set_write_timeout(timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::*;
    use std::thread;

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

    #[test]
    fn test_client_server_roundtrip() {
        let socket_path = SocketPath::from_path(PathBuf::from("/tmp/hakana-test-roundtrip.sock"));

        // Clean up any stale socket
        let _ = socket_path.cleanup();

        // Start server in a thread
        let server_socket_path = socket_path.clone();
        let server_handle = thread::spawn(move || {
            let server = ServerSocket::bind(server_socket_path).expect("Failed to bind");
            let mut conn = server.accept().expect("Failed to accept");

            // Read request
            let msg = conn.read_message().expect("Failed to read");
            if let Message::Status(_) = msg {
                // Send response
                let response = Message::StatusResult(StatusResponse {
                    ready: true,
                    files_count: 100,
                    symbols_count: 5000,
                    uptime_secs: 3600,
                    analysis_in_progress: false,
                    pending_requests: 0,
                    project_root: "/home/user/project".to_string(),
                });
                conn.write_message(&response).expect("Failed to write");
            }
        });

        // Give server time to start
        thread::sleep(Duration::from_millis(100));

        // Connect client
        let mut client = ClientSocket::connect(&socket_path).expect("Failed to connect");

        // Send request and get response
        let request = Message::Status(StatusRequest);
        let response = client.request(&request).expect("Failed to request");

        if let Message::StatusResult(status) = response {
            assert!(status.ready);
            assert_eq!(status.files_count, 100);
        } else {
            panic!("Expected StatusResponse");
        }

        server_handle.join().unwrap();
    }
}
