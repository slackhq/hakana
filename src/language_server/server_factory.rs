mod direct_process;
mod systemd;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use std::{env, io};

use hakana_protocol::{ClientSocket, SocketPath};

pub use direct_process::DirectProcessServerFactory;
pub use systemd::SystemdServerFactory;

pub trait ServerFactory {
    fn spawn_server(&self, project_root: &Path, hakana_binary: &Path) -> io::Result<ServerProcess>;

    fn wait_for_server<'a>(
        &'a self,
        socket_path: &'a SocketPath,
        server_process: &'a mut ServerProcess,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;
}

pub struct ServerProcess {
    pub(super) child: Child,
    pub(super) systemd_unit: Option<String>,
}

#[derive(Debug)]
pub enum PreferredServerFactory {
    Direct(DirectProcessServerFactory),
    Systemd(SystemdServerFactory),
}

impl PreferredServerFactory {
    pub fn discover() -> Self {
        Self::with_executable_resolver(resolve_executable)
    }

    fn with_executable_resolver<F>(resolver: F) -> Self
    where
        F: Fn(&str) -> Option<PathBuf>,
    {
        match resolver("systemd-run") {
            Some(systemd_run) => Self::Systemd(SystemdServerFactory::new(systemd_run)),
            None => Self::Direct(DirectProcessServerFactory),
        }
    }
}

impl ServerFactory for PreferredServerFactory {
    fn spawn_server(&self, project_root: &Path, hakana_binary: &Path) -> io::Result<ServerProcess> {
        match self {
            Self::Direct(factory) => factory.spawn_server(project_root, hakana_binary),
            Self::Systemd(factory) => factory.spawn_server(project_root, hakana_binary),
        }
    }

    fn wait_for_server<'a>(
        &'a self,
        socket_path: &'a SocketPath,
        server_process: &'a mut ServerProcess,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
        match self {
            Self::Direct(factory) => factory.wait_for_server(socket_path, server_process, timeout),
            Self::Systemd(factory) => factory.wait_for_server(socket_path, server_process, timeout),
        }
    }
}

fn resolve_executable(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .and_then(|paths| {
            paths
                .iter()
                .map(|path| path.join(name))
                .find(|path| path.is_file())
        })
}

pub(super) async fn wait_for_server(
    socket_path: &SocketPath,
    server_process: &mut ServerProcess,
    timeout: Duration,
) -> io::Result<()> {
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match server_process.child.try_wait() {
            Ok(Some(status)) => {
                let log_path = std::env::temp_dir().join("hakana-server.log");
                let log_contents = std::fs::read_to_string(&log_path)
                    .unwrap_or_else(|_| "<no log available>".to_string());
                log::info!("Server log contents:\n{}", log_contents);
                return Err(io::Error::other(format!(
                    "Server process exited during startup with status: {}. Check {} for details.",
                    status,
                    log_path.display()
                )));
            }
            Ok(None) => {}
            Err(e) => {
                return Err(io::Error::other(format!(
                    "Failed to check server process status: {}",
                    e
                )));
            }
        }

        if start.elapsed() > timeout {
            if let Some(unit) = &server_process.systemd_unit {
                let _ = Command::new("systemctl")
                    .arg("--user")
                    .arg("stop")
                    .arg(unit)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = server_process.child.kill();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Timed out waiting for server to start",
            ));
        }

        if socket_path.server_exists() && ClientSocket::connect(socket_path).await.is_ok() {
            return Ok(());
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_systemd_when_discoverable() {
        let factory = PreferredServerFactory::with_executable_resolver(|name| {
            assert_eq!(name, "systemd-run");
            Some(PathBuf::from("/usr/bin/systemd-run"))
        });

        assert!(matches!(factory, PreferredServerFactory::Systemd(_)));
    }

    #[test]
    fn selects_direct_process_when_systemd_is_unavailable() {
        let factory = PreferredServerFactory::with_executable_resolver(|_| None);

        assert!(matches!(factory, PreferredServerFactory::Direct(_)));
    }
}
