use std::error::Error;
use std::fs;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::signal;
use tokio::time::{sleep, Duration};

use crate::config::DaemonConfig;

#[derive(Debug)]
pub struct LifecycleManager {
    _config: Arc<DaemonConfig>,
    shutdown_signal: Arc<AtomicBool>,
    pid_file_path: Option<String>,
}

impl LifecycleManager {
    pub fn new(config: Arc<DaemonConfig>) -> Self {
        Self {
            _config: Arc::clone(&config),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            pid_file_path: config.pid_file.clone(),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn Error>> {
        // Write PID file if configured
        if let Some(pid_file) = &self.pid_file_path {
            self.write_pid_file(pid_file)?;
        }

        // Set up signal handlers
        self.setup_signal_handlers().await?;

        println!("Hakana daemon started with PID {}", process::id());

        Ok(())
    }

    pub async fn wait_for_shutdown(&self) {
        while !self.shutdown_signal.load(Ordering::Relaxed) {
            sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_signal.load(Ordering::Relaxed)
    }

    pub async fn shutdown(&self) -> Result<(), Box<dyn Error>> {
        println!("Initiating graceful shutdown...");

        self.shutdown_signal.store(true, Ordering::Relaxed);

        // Give some time for cleanup
        sleep(Duration::from_millis(1000)).await;

        // Remove PID file
        if let Some(pid_file) = &self.pid_file_path {
            self.remove_pid_file(pid_file)?;
        }

        println!("Daemon shutdown complete");

        Ok(())
    }

    async fn setup_signal_handlers(&self) -> Result<(), Box<dyn Error>> {
        let shutdown_signal = Arc::clone(&self.shutdown_signal);

        tokio::spawn(async move {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap();
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt()).unwrap();

            tokio::select! {
                _ = sigterm.recv() => {
                    println!("Received SIGTERM");
                    shutdown_signal.store(true, Ordering::Relaxed);
                }
                _ = sigint.recv() => {
                    println!("Received SIGINT");
                    shutdown_signal.store(true, Ordering::Relaxed);
                }
            }
        });

        Ok(())
    }

    fn write_pid_file(&self, pid_file: &str) -> Result<(), Box<dyn Error>> {
        let pid = process::id();
        fs::write(pid_file, pid.to_string())?;
        println!("PID file written to: {}", pid_file);
        Ok(())
    }

    fn remove_pid_file(&self, pid_file: &str) -> Result<(), Box<dyn Error>> {
        if Path::new(pid_file).exists() {
            fs::remove_file(pid_file)?;
            println!("PID file removed: {}", pid_file);
        }
        Ok(())
    }

    pub fn check_if_daemon_running(pid_file: &str) -> Result<Option<u32>, Box<dyn Error>> {
        if !Path::new(pid_file).exists() {
            return Ok(None);
        }

        let pid_str = fs::read_to_string(pid_file)?;
        let pid: u32 = pid_str.trim().parse()?;

        // Check if process is still running
        if Self::is_process_running(pid) {
            Ok(Some(pid))
        } else {
            // Clean up stale PID file
            fs::remove_file(pid_file)?;
            Ok(None)
        }
    }

    fn is_process_running(pid: u32) -> bool {
        // On Unix systems, sending signal 0 checks if process exists
        #[cfg(unix)]
        {
            use std::process::Command;
            let output = Command::new("kill")
                .args(&["-0", &pid.to_string()])
                .output();

            match output {
                Ok(output) => output.status.success(),
                Err(_) => false,
            }
        }

        #[cfg(not(unix))]
        {
            // Fallback for non-Unix systems
            false
        }
    }

    pub async fn stop_daemon(pid_file: &str) -> Result<(), Box<dyn Error>> {
        if let Some(pid) = Self::check_if_daemon_running(pid_file)? {
            println!("Stopping daemon with PID {}", pid);

            #[cfg(unix)]
            {
                use std::process::Command;

                // Try graceful shutdown first
                let _ = Command::new("kill")
                    .args(&["-TERM", &pid.to_string()])
                    .output();

                // Wait a bit for graceful shutdown
                sleep(Duration::from_secs(5)).await;

                // Check if still running and force kill if necessary
                if Self::is_process_running(pid) {
                    println!("Daemon did not stop gracefully, force killing...");
                    let _ = Command::new("kill")
                        .args(&["-KILL", &pid.to_string()])
                        .output();

                    sleep(Duration::from_secs(1)).await;
                }

                if !Self::is_process_running(pid) {
                    println!("Daemon stopped successfully");
                    // Clean up PID file
                    let _ = fs::remove_file(pid_file);
                } else {
                    return Err("Failed to stop daemon".into());
                }
            }

            #[cfg(not(unix))]
            {
                return Err("Daemon stopping not supported on this platform".into());
            }
        } else {
            println!("No daemon running");
        }

        Ok(())
    }

    pub fn get_daemon_status(pid_file: &str) -> Result<DaemonStatus, Box<dyn Error>> {
        match Self::check_if_daemon_running(pid_file)? {
            Some(pid) => Ok(DaemonStatus::Running(pid)),
            None => Ok(DaemonStatus::Stopped),
        }
    }
}

#[derive(Debug)]
pub enum DaemonStatus {
    Running(u32),
    Stopped,
}

impl std::fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonStatus::Running(pid) => write!(f, "Running (PID: {})", pid),
            DaemonStatus::Stopped => write!(f, "Stopped"),
        }
    }
}