use std::future::Future;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{io, pin::Pin};

use hakana_protocol::SocketPath;

use super::{ServerFactory, ServerProcess, wait_for_server};

#[derive(Debug, Default)]
pub struct DirectProcessServerFactory;

impl DirectProcessServerFactory {
    fn command(project_root: &Path, hakana_binary: &Path) -> Command {
        let mut command = Command::new(hakana_binary);
        command
            .arg("server")
            .arg("--root")
            .arg(project_root)
            .current_dir(project_root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }
}

impl ServerFactory for DirectProcessServerFactory {
    fn spawn_server(&self, project_root: &Path, hakana_binary: &Path) -> io::Result<ServerProcess> {
        log::info!(
            "Spawning hakana server: {} server --root {}",
            hakana_binary.display(),
            project_root.display()
        );

        let child = Self::command(project_root, hakana_binary)
            .spawn()
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to spawn hakana server (binary: {}): {}",
                        hakana_binary.display(),
                        e
                    ),
                )
            })?;

        Ok(ServerProcess {
            child,
            systemd_unit: None,
        })
    }

    fn wait_for_server<'a>(
        &'a self,
        socket_path: &'a SocketPath,
        server_process: &'a mut ServerProcess,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
        Box::pin(wait_for_server(socket_path, server_process, timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn wait_for_server_detects_dead_process() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let socket_path = SocketPath::for_project(temp_dir.path());
        let factory = DirectProcessServerFactory;
        let child = Command::new("sh")
            .arg("-c")
            .arg("exit 1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut server_process = ServerProcess {
            child,
            systemd_unit: None,
        };
        let result = factory
            .wait_for_server(&socket_path, &mut server_process, Duration::from_secs(1))
            .await;

        assert!(result.is_err(), "Should return error when process dies");
        assert!(result.unwrap_err().to_string().contains("exited"));
    }

    #[tokio::test]
    async fn wait_for_server_times_out() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let socket_path = SocketPath::for_project(temp_dir.path());
        let factory = DirectProcessServerFactory;
        let child = Command::new("sleep")
            .arg("10")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        let mut server_process = ServerProcess {
            child,
            systemd_unit: None,
        };
        let result = factory
            .wait_for_server(
                &socket_path,
                &mut server_process,
                Duration::from_millis(200),
            )
            .await;

        let _ = server_process.child.kill();
        let _ = server_process.child.wait();

        assert!(result.is_err(), "Should return error on timeout");
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }
}
