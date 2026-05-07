//! Watchman integration for the hakana server.
//!
//! Watches for changes in Hack/PHP files and config file changes
//! using watchman subscriptions, triggering re-analysis as needed.

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

/// Get the current watchman clock. Called BEFORE initial analysis
/// so that any file changes during analysis are captured by the subscription.
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
            log::error!("Watchman subscription error: {}", e);
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
    log::info!("Connecting to watchman...");

    let watchman = Connector::new().connect().await?;

    log::info!("Connected to watchman, resolving root...");

    let canonical_path =
        CanonicalPath::canonicalize(&root_dir).map_err(watchman_client::Error::ConnectionError)?;

    let resolved = watchman.resolve_root(canonical_path).await?;

    log::info!(
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
        log::info!("Watching config file: {:?}", rel_path);
    }

    let expression = build_expression(&ignore_files, &project_root, config_relative_path.as_ref());

    // Use `since` to only get changes after the clock obtained before initial analysis
    let subscribe_request = SubscribeRequest {
        since: Some(Clock::Spec(since_clock)),
        expression: Some(expression),
        defer_vcs: true,
        ..Default::default()
    };

    let (mut subscription, _initial_response) = watchman
        .subscribe::<NameOnly>(&resolved, subscribe_request)
        .await?;

    log::info!("Watchman subscription created: {}", subscription.name());

    loop {
        let event = subscription.next().await;
        if !handle_subscription_event(&project_root, &config_relative_path, &tx, event).await {
            break;
        }
    }

    Ok(())
}

/// Handle a single event from Watchman.
/// Returns `true` if processing should continue, `false` otherwise.
async fn handle_subscription_event(
    project_root: &Path,
    config_relative_path: &Option<PathBuf>,
    tx: &mpsc::Sender<WatchmanEvent>,
    event: Result<SubscriptionData<NameOnly>, watchman_client::Error>,
) -> bool {
    match event {
        Ok(SubscriptionData::FilesChanged(result)) => {
            if let Some(files) = result.files {
                let mut new_statuses = FxHashMap::default();
                let mut config_changed = false;

                for file in files {
                    let file_name = file.name.into_inner();
                    let file_path = project_root.join(&file_name);
                    let file_path_str = file_path.to_string_lossy().to_string();

                    if let Some(config_rel) = config_relative_path {
                        if Path::new(&file_name) == config_rel.as_path() {
                            log::info!("Config file changed: {:?}", file_name);
                            config_changed = true;
                            continue;
                        }
                    }

                    // For existing files, treat all as Modified —
                    // the hash comparison in the orchestrator handles deduplication
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

                // Config changed triggers full re-analysis, so send it first
                if config_changed {
                    if tx.send(WatchmanEvent::ConfigChanged).await.is_err() {
                        log::info!("Server shut down, stopping watchman subscription");
                        return false;
                    }
                }

                if !new_statuses.is_empty() {
                    log::info!("Watchman detected {} file change(s)", new_statuses.len());
                    if tx
                        .send(WatchmanEvent::FileChanges(new_statuses))
                        .await
                        .is_err()
                    {
                        log::info!("Server shut down, stopping watchman subscription");
                        return false;
                    }
                }
            }
        }
        Ok(SubscriptionData::StateEnter { state_name, .. }) => {
            log::info!("Watchman state enter: {}", state_name);
        }
        Ok(SubscriptionData::StateLeave { state_name, .. }) => {
            log::info!("Watchman state leave: {}", state_name);
        }
        Ok(SubscriptionData::Canceled) => {
            log::info!("Watchman subscription canceled");
            return false;
        }
        Err(e) => {
            log::error!("Watchman subscription error: {}", e);
            return false;
        }
    }

    true
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use watchman_client::pdu::{Clock, ClockSpec, QueryResult};

    fn make_name_only(path: &str) -> NameOnly {
        NameOnly::from(PathBuf::from(path))
    }

    fn make_files_changed(
        files: Vec<NameOnly>,
    ) -> Result<SubscriptionData<NameOnly>, watchman_client::Error> {
        Ok(SubscriptionData::FilesChanged(QueryResult {
            version: "test".to_string(),
            is_fresh_instance: false,
            files: Some(files),
            clock: Clock::Spec(ClockSpec::null()),
            state_enter: None,
            state_leave: None,
            state_metadata: None,
            saved_state_info: None,
            debug: None,
        }))
    }

    // ── build_expression tests ──

    #[test]
    fn build_expression_no_ignores_no_config() {
        let expr = build_expression(&[], Path::new("/project"), None);
        let dbg = format!("{:?}", expr);
        assert!(dbg.contains("FileType(Regular)"));
        assert!(dbg.contains("Suffix"));
        assert!(dbg.contains("hack"));
        assert!(dbg.contains("php"));
        assert!(dbg.contains("hhi"));
        assert!(dbg.contains(".git"));
        assert!(
            !dbg.contains("Any"),
            "no ignores means no Any wrapper for exclusions"
        );
    }

    #[test]
    fn build_expression_with_dir_ignore() {
        let ignores = vec!["vendor/**".to_string()];
        let expr = build_expression(&ignores, Path::new("/project"), None);
        let dbg = format!("{:?}", expr);
        assert!(dbg.contains("vendor"), "should exclude vendor dir");
        assert!(dbg.contains("Any"), "multiple exclusions wrapped in Any");
    }

    #[test]
    fn build_expression_with_file_ignore() {
        let ignores = vec!["some/file.hack".to_string()];
        let expr = build_expression(&ignores, Path::new("/project"), None);
        let dbg = format!("{:?}", expr);
        assert!(dbg.contains("some/file.hack"));
    }

    #[test]
    fn build_expression_strips_project_root_prefix() {
        let ignores = vec!["/project/vendor/**".to_string()];
        let expr = build_expression(&ignores, Path::new("/project"), None);
        let dbg = format!("{:?}", expr);
        assert!(dbg.contains("vendor"));
        assert!(!dbg.contains("/project/vendor"));
    }

    #[test]
    fn build_expression_with_config_path() {
        let config = PathBuf::from("hakana.json");
        let expr = build_expression(&[], Path::new("/project"), Some(&config));
        let dbg = format!("{:?}", expr);
        assert!(dbg.contains("hakana.json"));
        assert!(dbg.contains("Suffix"), "still matches hack/php/hhi files");
    }

    #[test]
    fn build_expression_with_ignores_and_config() {
        let ignores = vec!["build/**".to_string(), "tmp/cache.txt".to_string()];
        let config = PathBuf::from("hakana.json");
        let expr = build_expression(&ignores, Path::new("/root"), Some(&config));
        let dbg = format!("{:?}", expr);
        assert!(dbg.contains("build"));
        assert!(dbg.contains("tmp/cache.txt"));
        assert!(dbg.contains("hakana.json"));
    }

    // ── handle_subscription_event tests ──

    #[tokio::test]
    async fn handle_hack_file_modified() {
        let tmp = TempDir::new().unwrap();
        let hack_file = tmp.path().join("test.hack");
        fs::write(&hack_file, "content").unwrap();

        let (tx, mut rx) = mpsc::channel(16);
        let event = make_files_changed(vec![make_name_only("test.hack")]);

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);

        match rx.try_recv().unwrap() {
            WatchmanEvent::FileChanges(changes) => {
                let key = hack_file.to_string_lossy().to_string();
                assert!(changes.contains_key(&key));
                assert!(matches!(changes[&key], FileStatus::Modified(_, _)));
            }
            _ => panic!("expected FileChanges"),
        }
    }

    #[tokio::test]
    async fn handle_deleted_hack_file() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let event = make_files_changed(vec![make_name_only("removed.hack")]);

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);

        match rx.try_recv().unwrap() {
            WatchmanEvent::FileChanges(changes) => {
                let key = tmp
                    .path()
                    .join("removed.hack")
                    .to_string_lossy()
                    .to_string();
                assert!(matches!(changes[&key], FileStatus::Deleted));
            }
            _ => panic!("expected FileChanges"),
        }
    }

    #[tokio::test]
    async fn handle_deleted_non_hack_file_is_deleted_dir() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let event = make_files_changed(vec![make_name_only("somedir")]);

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);

        match rx.try_recv().unwrap() {
            WatchmanEvent::FileChanges(changes) => {
                let key = tmp.path().join("somedir").to_string_lossy().to_string();
                assert!(matches!(changes[&key], FileStatus::DeletedDir));
            }
            _ => panic!("expected FileChanges"),
        }
    }

    #[tokio::test]
    async fn handle_non_hack_existing_file_ignored() {
        let tmp = TempDir::new().unwrap();
        let txt_file = tmp.path().join("readme.txt");
        fs::write(&txt_file, "hello").unwrap();

        let (tx, mut rx) = mpsc::channel(16);
        let event = make_files_changed(vec![make_name_only("readme.txt")]);

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);
        assert!(
            rx.try_recv().is_err(),
            "non-hack existing files should not produce events"
        );
    }

    #[tokio::test]
    async fn handle_directory_skipped() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let (tx, mut rx) = mpsc::channel(16);
        let event = make_files_changed(vec![make_name_only("subdir")]);

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);
        assert!(rx.try_recv().is_err(), "directories should be skipped");
    }

    #[tokio::test]
    async fn handle_config_change_sends_config_event() {
        let tmp = TempDir::new().unwrap();
        let config_file = tmp.path().join("hakana.json");
        fs::write(&config_file, "{}").unwrap();

        let config_rel = Some(PathBuf::from("hakana.json"));
        let (tx, mut rx) = mpsc::channel(16);
        let event = make_files_changed(vec![make_name_only("hakana.json")]);

        let cont = handle_subscription_event(tmp.path(), &config_rel, &tx, event).await;
        assert!(cont);

        match rx.try_recv().unwrap() {
            WatchmanEvent::ConfigChanged => {}
            _ => panic!("expected ConfigChanged"),
        }
        assert!(
            rx.try_recv().is_err(),
            "config-only change should not send FileChanges"
        );
    }

    #[tokio::test]
    async fn handle_config_and_hack_changes_together() {
        let tmp = TempDir::new().unwrap();
        let hack_file = tmp.path().join("main.hack");
        fs::write(&hack_file, "code").unwrap();

        let config_rel = Some(PathBuf::from("hakana.json"));
        let (tx, mut rx) = mpsc::channel(16);
        let event = make_files_changed(vec![
            make_name_only("hakana.json"),
            make_name_only("main.hack"),
        ]);

        let cont = handle_subscription_event(tmp.path(), &config_rel, &tx, event).await;
        assert!(cont);

        match rx.try_recv().unwrap() {
            WatchmanEvent::ConfigChanged => {}
            _ => panic!("expected ConfigChanged first"),
        }
        match rx.try_recv().unwrap() {
            WatchmanEvent::FileChanges(changes) => {
                assert_eq!(changes.len(), 1);
            }
            _ => panic!("expected FileChanges second"),
        }
    }

    #[tokio::test]
    async fn handle_no_files_in_result() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let event = Ok(SubscriptionData::FilesChanged(QueryResult {
            version: "test".to_string(),
            is_fresh_instance: false,
            files: None,
            clock: Clock::Spec(ClockSpec::null()),
            state_enter: None,
            state_leave: None,
            state_metadata: None,
            saved_state_info: None,
            debug: None,
        }));

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn handle_canceled_returns_false() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let event: Result<SubscriptionData<NameOnly>, _> = Ok(SubscriptionData::Canceled);

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(!cont);
    }

    #[tokio::test]
    async fn handle_error_returns_false() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let event: Result<SubscriptionData<NameOnly>, _> =
            Err(watchman_client::Error::WatchmanResponseError {
                message: "test error".to_string(),
            });

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(!cont);
    }

    #[tokio::test]
    async fn handle_state_enter_continues() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let event: Result<SubscriptionData<NameOnly>, _> = Ok(SubscriptionData::StateEnter {
            state_name: "hg.update".to_string(),
            metadata: None,
        });

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);
    }

    #[tokio::test]
    async fn handle_state_leave_continues() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let event: Result<SubscriptionData<NameOnly>, _> = Ok(SubscriptionData::StateLeave {
            state_name: "hg.update".to_string(),
            metadata: None,
        });

        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(cont);
    }

    #[tokio::test]
    async fn handle_closed_receiver_returns_false() {
        let tmp = TempDir::new().unwrap();
        let hack_file = tmp.path().join("test.hack");
        fs::write(&hack_file, "content").unwrap();

        let (tx, rx) = mpsc::channel(16);
        drop(rx);

        let event = make_files_changed(vec![make_name_only("test.hack")]);
        let cont = handle_subscription_event(tmp.path(), &None, &tx, event).await;
        assert!(!cont, "should return false when receiver is dropped");
    }
}
