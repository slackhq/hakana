use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// TCP port for daemon server
    pub port: u16,

    /// TCP host for daemon server
    pub host: String,

    /// Path to Unix socket (if using Unix socket instead of TCP)
    pub socket_path: Option<String>,

    /// Path to PID file for daemon process management
    pub pid_file: Option<String>,

    /// Path to log file
    pub log_file: Option<String>,

    /// Log level (error, warn, info, debug, trace)
    pub log_level: String,

    /// Maximum number of concurrent clients
    pub max_clients: usize,

    /// File watching configuration
    pub file_watcher: FileWatcherConfig,

    /// Analysis configuration
    pub analysis: AnalysisConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatcherConfig {
    /// Whether to use watchman if available
    pub use_watchman: bool,

    /// Polling interval in seconds when not using watchman
    pub poll_interval: u64,

    /// Debounce delay in milliseconds for batching file changes
    pub debounce_delay: u64,

    /// File patterns to watch
    pub watch_patterns: Vec<String>,

    /// File patterns to ignore
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Number of threads to use for analysis
    pub threads: u8,

    /// Whether to enable incremental analysis
    pub incremental: bool,

    /// Maximum number of files to analyze in parallel
    pub max_parallel_files: usize,

    /// Analysis timeout in seconds
    pub timeout: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            port: 9999,
            host: "127.0.0.1".to_string(),
            socket_path: None,
            pid_file: None,
            log_file: None,
            log_level: "info".to_string(),
            max_clients: 100,
            file_watcher: FileWatcherConfig::default(),
            analysis: AnalysisConfig::default(),
        }
    }
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            use_watchman: true,
            poll_interval: 2,
            debounce_delay: 500,
            watch_patterns: vec![
                "**/*.hack".to_string(),
                "**/*.php".to_string(),
                "**/*.hhi".to_string(),
            ],
            ignore_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/vendor/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
            ],
        }
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            threads: 8,
            incremental: true,
            max_parallel_files: 1000,
            timeout: 300, // 5 minutes
        }
    }
}

impl DaemonConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn Error>> {
        let contents = fs::read_to_string(path)?;
        let config: DaemonConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn load_from_project_dir<P: AsRef<Path>>(project_dir: P) -> Result<Self, Box<dyn Error>> {
        let config_path = project_dir.as_ref().join("hakana-daemon.toml");
        if config_path.exists() {
            Self::load_from_file(config_path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn create_default_config_file<P: AsRef<Path>>(path: P) -> Result<(), Box<dyn Error>> {
        let config = Self::default();
        config.save_to_file(path)
    }

    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.port == 0 {
            return Err("Port cannot be 0".into());
        }

        if self.max_clients == 0 {
            return Err("max_clients must be greater than 0".into());
        }

        if self.analysis.threads == 0 {
            return Err("analysis.threads must be greater than 0".into());
        }

        if !matches!(
            self.log_level.as_str(),
            "error" | "warn" | "info" | "debug" | "trace"
        ) {
            return Err("log_level must be one of: error, warn, info, debug, trace".into());
        }

        Ok(())
    }
}