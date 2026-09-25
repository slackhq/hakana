use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::{io, process};

use hakana_protocol::SocketPath;

use super::{ServerFactory, ServerProcess, wait_for_server};

#[derive(Debug)]
pub struct SystemdServerFactory {
    systemd_run: PathBuf,
}

impl SystemdServerFactory {
    pub fn new(systemd_run: PathBuf) -> Self {
        Self { systemd_run }
    }

    fn command(&self, project_root: &Path, hakana_binary: &Path, unit: &str) -> Command {
        let mut command = Command::new(&self.systemd_run);
        command
            .arg("--user")
            .arg("--slice=hack.slice")
            .arg("--property=Nice=10")
            .arg("--property=Type=exec")
            .arg("--wait")
            .arg("--collect")
            .arg(format!("--unit={unit}"))
            .arg(format!("--working-directory={}", project_root.display()))
            .arg(hakana_binary)
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

impl ServerFactory for SystemdServerFactory {
    fn spawn_server(&self, project_root: &Path, hakana_binary: &Path) -> io::Result<ServerProcess> {
        log::info!(
            "Spawning hakana server through systemd-run: {} server --root {}",
            hakana_binary.display(),
            project_root.display()
        );

        let unit = next_systemd_unit_name();
        let child = self
            .command(project_root, hakana_binary, &unit)
            .spawn()
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to spawn hakana server through systemd-run (binary: {}): {}",
                        hakana_binary.display(),
                        e
                    ),
                )
            })?;

        Ok(ServerProcess {
            child,
            systemd_unit: Some(unit),
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

fn next_systemd_unit_name() -> String {
    static UNIT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    format!(
        "hakana-server-{}-{}",
        process::id(),
        UNIT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_includes_service_and_hakana_arguments() {
        let factory = SystemdServerFactory::new(PathBuf::from("/usr/bin/systemd-run"));
        let project_root = Path::new("/tmp/project root");
        let hakana_binary = Path::new("/opt/hakana");
        let command = factory.command(project_root, hakana_binary, "hakana-server-test");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(command.get_program(), "/usr/bin/systemd-run");
        assert_eq!(
            args,
            vec![
                "--user",
                "--slice=hack.slice",
                "--property=Nice=10",
                "--property=Type=exec",
                "--wait",
                "--collect",
                "--unit=hakana-server-test",
                "--working-directory=/tmp/project root",
                "/opt/hakana",
                "server",
                "--root",
                "/tmp/project root",
            ]
        );
        assert_eq!(command.get_current_dir(), Some(project_root));
    }
}
