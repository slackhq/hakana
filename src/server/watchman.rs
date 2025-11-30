//! Watchman integration for the hakana server.
//!
//! This module handles file system watching using watchman to detect changes
//! in Hack/PHP files and trigger re-analysis.

use hakana_orchestrator::file::FileStatus;
use rustc_hash::FxHashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use watchman_client::prelude::*;
use watchman_client::SubscriptionData;

/// Check if watchman is available.
pub fn check_available() -> Result<(), String> {
    match Command::new("watchman").arg("version").output() {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                Err("watchman command failed".to_string())
            }
        }
        Err(e) => Err(format!("watchman not found: {}", e)),
    }
}

/// Get the current watchman clock. This should be called BEFORE initial analysis
/// so that any file changes during analysis are captured by the subscription.
pub fn get_clock(root_dir: &Path) -> io::Result<ClockSpec> {
    // Create a temporary tokio runtime to get the clock synchronously
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to build tokio runtime: {}", e),
            )
        })?;

    rt.block_on(async {
        let watchman = Connector::new().connect().await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("Failed to connect to watchman: {}", e),
            )
        })?;

        let canonical_path = CanonicalPath::canonicalize(root_dir).map_err(|e| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Failed to canonicalize path: {}", e),
            )
        })?;

        let resolved = watchman.resolve_root(canonical_path).await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to resolve watchman root: {}", e),
            )
        })?;

        watchman.clock(&resolved, SyncTimeout::Default).await.map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to get watchman clock: {}", e),
            )
        })
    })
}

/// Handle for receiving file changes from watchman.
pub struct WatchmanHandle {
    rx: mpsc::Receiver<FxHashMap<String, FileStatus>>,
}

impl WatchmanHandle {
    /// Poll for file changes (non-blocking).
    /// Returns all pending changes accumulated since last poll.
    pub fn poll_changes(&self) -> FxHashMap<String, FileStatus> {
        let mut all_changes = FxHashMap::default();
        while let Ok(changes) = self.rx.try_recv() {
            all_changes.extend(changes);
        }
        all_changes
    }
}

/// Start watchman subscription for file changes.
///
/// Returns a handle that can be used to poll for changes.
pub fn start_subscription(
    root_dir: PathBuf,
    ignore_files: Vec<String>,
    since_clock: ClockSpec,
) -> WatchmanHandle {
    // Create channel for communicating file changes from watchman thread
    let (tx, rx) = mpsc::channel::<FxHashMap<String, FileStatus>>();

    // Spawn a thread with its own tokio runtime for watchman
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        rt.block_on(async move {
            if let Err(e) = run_subscription(root_dir, tx, ignore_files, since_clock).await {
                eprintln!("Watchman subscription error: {}", e);
            }
        });
    });

    WatchmanHandle { rx }
}

/// Run the watchman subscription in an async context.
/// Sends file changes through the channel when detected.
async fn run_subscription(
    root_dir: PathBuf,
    tx: mpsc::Sender<FxHashMap<String, FileStatus>>,
    ignore_files: Vec<String>,
    since_clock: ClockSpec,
) -> Result<(), watchman_client::Error> {
    eprintln!("Connecting to watchman...");

    let watchman = Connector::new().connect().await?;

    eprintln!("Connected to watchman, resolving root...");

    let canonical_path =
        CanonicalPath::canonicalize(&root_dir).map_err(watchman_client::Error::ConnectionError)?;

    let resolved = watchman.resolve_root(canonical_path).await?;

    eprintln!(
        "Watchman watching: {:?} (watcher: {})",
        resolved.path(),
        resolved.watcher()
    );

    // Build the watchman expression with ignore patterns
    let expression = build_expression(&ignore_files, &resolved.path());

    // Create subscription request for Hack/PHP files
    // Use `since` to only get changes after the clock obtained before initial analysis
    let subscribe_request = SubscribeRequest {
        since: Some(Clock::Spec(since_clock)),
        expression: Some(expression),
        // Debounce to avoid rapid-fire changes
        defer_vcs: true,
        ..Default::default()
    };

    let (mut subscription, _initial_response) = watchman
        .subscribe::<NameOnly>(&resolved, subscribe_request)
        .await?;

    eprintln!("Watchman subscription created: {}", subscription.name());

    let project_root = resolved.path();

    // Process subscription events
    loop {
        match subscription.next().await {
            Ok(SubscriptionData::FilesChanged(result)) => {
                if let Some(files) = result.files {
                    let mut new_statuses = FxHashMap::default();

                    for file in files {
                        let file_path = project_root.join(&file.name.into_inner());
                        let file_path_str = file_path.to_string_lossy().to_string();

                        // Determine file status based on existence
                        let status = if file_path.exists() {
                            if file_path.is_dir() {
                                // Skip directories that exist - we only care about deleted dirs
                                continue;
                            }
                            // For existing files, treat all as Modified
                            // The hash comparison in the orchestrator will handle this correctly
                            FileStatus::Modified(0, 0)
                        } else {
                            // File doesn't exist - it was deleted
                            if file_path_str.ends_with(".hack")
                                || file_path_str.ends_with(".php")
                                || file_path_str.ends_with(".hhi")
                            {
                                FileStatus::Deleted
                            } else {
                                // Could be a deleted directory
                                FileStatus::DeletedDir
                            }
                        };

                        // Only include hack/php/hhi files
                        if file_path_str.ends_with(".hack")
                            || file_path_str.ends_with(".php")
                            || file_path_str.ends_with(".hhi")
                            || matches!(status, FileStatus::DeletedDir)
                        {
                            new_statuses.insert(file_path_str, status);
                        }
                    }

                    if !new_statuses.is_empty() {
                        eprintln!("Watchman detected {} file change(s)", new_statuses.len());
                        // Send changes through channel; ignore errors if receiver is dropped
                        if tx.send(new_statuses).is_err() {
                            eprintln!("Server shut down, stopping watchman subscription");
                            break;
                        }
                    }
                }
            }
            Ok(SubscriptionData::StateEnter { state_name, .. }) => {
                eprintln!("Watchman state enter: {}", state_name);
            }
            Ok(SubscriptionData::StateLeave { state_name, .. }) => {
                eprintln!("Watchman state leave: {}", state_name);
            }
            Ok(SubscriptionData::Canceled) => {
                eprintln!("Watchman subscription canceled");
                break;
            }
            Err(e) => {
                eprintln!("Watchman subscription error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Build a watchman expression that matches Hack/PHP files while excluding ignored paths.
///
/// Creates an expression like:
/// ["allof",
///   ["type", "f"],
///   ["suffix", ["hack", "php", "hhi"]],
///   ["not", ["anyof",
///     ["dirname", ".git"],
///     ["dirname", "ignored_dir"],
///     ["name", "specific/file.hack"],
///     ...
///   ]]
/// ]
fn build_expression(ignore_files: &[String], project_root: &Path) -> Expr {
    let project_root_str = project_root.to_string_lossy();

    // Build list of exclusions
    let mut exclusions: Vec<Expr> = vec![
        // Always exclude .git directory
        Expr::DirName(DirNameTerm {
            path: ".git".into(),
            depth: None,
        }),
    ];

    for ignore_pattern in ignore_files {
        // Strip the project root prefix to get relative path
        let relative_pattern = if ignore_pattern.starts_with(project_root_str.as_ref()) {
            ignore_pattern[project_root_str.len()..].trim_start_matches('/')
        } else {
            ignore_pattern.as_str()
        };

        // Handle directory patterns (ending with /**)
        if let Some(dir_path) = relative_pattern.strip_suffix("/**") {
            exclusions.push(Expr::DirName(DirNameTerm {
                path: dir_path.into(),
                depth: None,
            }));
        } else {
            // Handle specific file patterns
            exclusions.push(Expr::Name(NameTerm {
                paths: vec![relative_pattern.into()],
                wholename: true,
            }));
        }
    }

    // Build the full expression:
    // Match files with hack/php/hhi suffix, excluding ignored paths
    if exclusions.len() > 1 {
        // We have exclusions beyond just .git
        Expr::All(vec![
            Expr::FileType(FileType::Regular),
            Expr::Suffix(vec!["hack".into(), "php".into(), "hhi".into()]),
            Expr::Not(Box::new(Expr::Any(exclusions))),
        ])
    } else {
        // Just the .git exclusion
        Expr::All(vec![
            Expr::FileType(FileType::Regular),
            Expr::Suffix(vec!["hack".into(), "php".into(), "hhi".into()]),
            Expr::Not(Box::new(Expr::DirName(DirNameTerm {
                path: ".git".into(),
                depth: None,
            }))),
        ])
    }
}
