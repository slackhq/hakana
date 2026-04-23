use hakana_orchestrator::file::FileStatus;
use rustc_hash::FxHashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::mpsc;
use watchman_client::SubscriptionData;
use watchman_client::prelude::*;

#[derive(Debug)]
pub enum WatchmanEvent {
    FileChanges(FxHashMap<String, FileStatus>),
    ConfigChanged,
}

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

pub async fn get_clock(root_dir: &Path) -> io::Result<ClockSpec> {
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

    watchman
        .clock(&resolved, SyncTimeout::Default)
        .await
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to get watchman clock: {}", e),
            )
        })
}

pub struct WatchmanHandle {
    rx: mpsc::Receiver<WatchmanEvent>,
}

impl WatchmanHandle {
    pub async fn recv(&mut self) -> Option<WatchmanEvent> {
        self.rx.recv().await
    }
}

pub fn start_subscription(
    root_dir: PathBuf,
    ignore_files: Vec<String>,
    since_clock: ClockSpec,
    config_path: Option<PathBuf>,
) -> WatchmanHandle {
    let (tx, rx) = mpsc::channel::<WatchmanEvent>(64);

    tokio::spawn(async move {
        if let Err(e) = run_subscription(root_dir, tx, ignore_files, since_clock, config_path).await
        {
            eprintln!("Watchman subscription error: {}", e);
        }
    });

    WatchmanHandle { rx }
}

async fn run_subscription(
    root_dir: PathBuf,
    tx: mpsc::Sender<WatchmanEvent>,
    ignore_files: Vec<String>,
    since_clock: ClockSpec,
    config_path: Option<PathBuf>,
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

    let project_root = resolved.path();

    let config_relative_path = config_path.as_ref().and_then(|p| {
        if p.is_absolute() {
            p.strip_prefix(&project_root).ok().map(|p| p.to_path_buf())
        } else {
            Some(p.clone())
        }
    });

    if let Some(ref rel_path) = config_relative_path {
        eprintln!("Watching config file: {:?}", rel_path);
    }

    let expression = build_expression(&ignore_files, &project_root, config_relative_path.as_ref());

    let subscribe_request = SubscribeRequest {
        since: Some(Clock::Spec(since_clock)),
        expression: Some(expression),
        defer_vcs: true,
        ..Default::default()
    };

    let (mut subscription, _initial_response) = watchman
        .subscribe::<NameOnly>(&resolved, subscribe_request)
        .await?;

    eprintln!("Watchman subscription created: {}", subscription.name());

    loop {
        match subscription.next().await {
            Ok(SubscriptionData::FilesChanged(result)) => {
                if let Some(files) = result.files {
                    let mut new_statuses = FxHashMap::default();
                    let mut config_changed = false;

                    for file in files {
                        let file_name = file.name.into_inner();
                        let file_path = project_root.join(&file_name);
                        let file_path_str = file_path.to_string_lossy().to_string();

                        if let Some(ref config_rel) = config_relative_path {
                            if Path::new(&file_name) == config_rel.as_path() {
                                eprintln!("Config file changed: {:?}", file_name);
                                config_changed = true;
                                continue;
                            }
                        }

                        let status = if file_path.exists() {
                            if file_path.is_dir() {
                                continue;
                            }
                            FileStatus::Modified(0, 0)
                        } else {
                            if file_path_str.ends_with(".hack")
                                || file_path_str.ends_with(".php")
                                || file_path_str.ends_with(".hhi")
                            {
                                FileStatus::Deleted
                            } else {
                                FileStatus::DeletedDir
                            }
                        };

                        if file_path_str.ends_with(".hack")
                            || file_path_str.ends_with(".php")
                            || file_path_str.ends_with(".hhi")
                            || matches!(status, FileStatus::DeletedDir)
                        {
                            new_statuses.insert(file_path_str, status);
                        }
                    }

                    if config_changed {
                        if tx.send(WatchmanEvent::ConfigChanged).await.is_err() {
                            eprintln!("Server shut down, stopping watchman subscription");
                            break;
                        }
                    }

                    if !new_statuses.is_empty() {
                        eprintln!("Watchman detected {} file change(s)", new_statuses.len());
                        if tx
                            .send(WatchmanEvent::FileChanges(new_statuses))
                            .await
                            .is_err()
                        {
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

fn build_expression(
    ignore_files: &[String],
    project_root: &Path,
    config_path: Option<&PathBuf>,
) -> Expr {
    let project_root_str = project_root.to_string_lossy();

    let mut exclusions: Vec<Expr> = vec![Expr::DirName(DirNameTerm {
        path: ".git".into(),
        depth: None,
    })];

    for ignore_pattern in ignore_files {
        let relative_pattern = if ignore_pattern.starts_with(project_root_str.as_ref()) {
            ignore_pattern[project_root_str.len()..].trim_start_matches('/')
        } else {
            ignore_pattern.as_str()
        };

        if let Some(dir_path) = relative_pattern.strip_suffix("/**") {
            exclusions.push(Expr::DirName(DirNameTerm {
                path: dir_path.into(),
                depth: None,
            }));
        } else {
            exclusions.push(Expr::Name(NameTerm {
                paths: vec![relative_pattern.into()],
                wholename: true,
            }));
        }
    }

    let file_match = if let Some(config_rel_path) = config_path {
        let config_path_str = config_rel_path.to_string_lossy().to_string();
        Expr::Any(vec![
            Expr::Suffix(vec!["hack".into(), "php".into(), "hhi".into()]),
            Expr::Name(NameTerm {
                paths: vec![config_path_str.into()],
                wholename: true,
            }),
        ])
    } else {
        Expr::Suffix(vec!["hack".into(), "php".into(), "hhi".into()])
    };

    if exclusions.len() > 1 {
        Expr::All(vec![
            Expr::FileType(FileType::Regular),
            file_match,
            Expr::Not(Box::new(Expr::Any(exclusions))),
        ])
    } else {
        Expr::All(vec![
            Expr::FileType(FileType::Regular),
            file_match,
            Expr::Not(Box::new(Expr::DirName(DirNameTerm {
                path: ".git".into(),
                depth: None,
            }))),
        ])
    }
}
