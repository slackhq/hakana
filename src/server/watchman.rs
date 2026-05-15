//! Watchman integration for the hakana server.
//!
//! Watches for changes in Hack/PHP files and config file changes
//! using watchman subscriptions, triggering re-analysis as needed.

use hakana_orchestrator::file::FileStatus;
use rustc_hash::FxHashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use watchman_client::SubscriptionData;
use watchman_client::prelude::*;

#[derive(Debug)]
pub enum WatchmanEvent {
    FileChanges(FxHashMap<String, FileStatus>),
    ConfigChanged,
    FreshInstance,
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

pub struct WatchmanHandle {
    rx: mpsc::Receiver<WatchmanEvent>,
}

impl WatchmanHandle {
    pub async fn recv(&mut self) -> Option<WatchmanEvent> {
        self.rx.recv().await
    }
}

pub async fn start_subscription(
    root_dir: PathBuf,
    ignore_files: Vec<String>,
    config_path: Option<PathBuf>,
) -> WatchmanHandle {
    let (tx, rx) = mpsc::channel::<WatchmanEvent>(64);
    let (startup_tx, startup_rx) = oneshot::channel();

    tokio::spawn(async move {
        let mut startup_tx = Some(startup_tx);
        let mut last_clock = None;

        while let Err(e) = run_subscription(
            &root_dir,
            &tx,
            &mut startup_tx,
            &mut last_clock,
            &ignore_files,
            &config_path,
        )
        .await
        {
            log::error!("watchman subscriber error: {}. Trying to reconnect...", e);
        }
    });

    startup_rx
        .await
        .expect("failed to start watchman subscription");

    WatchmanHandle { rx }
}

/// Connect to watchman and establish a subscription on the given root.
/// Returns `Ok` if the subscriber should terminate, `Err` to indicate
/// a reconnect should be attempted.
async fn run_subscription(
    root_dir: &Path,
    tx: &mpsc::Sender<WatchmanEvent>,
    startup_tx: &mut Option<oneshot::Sender<bool>>,
    last_clock: &mut Option<Clock>,
    ignore_files: &Vec<String>,
    config_path: &Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
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

    let subscribe_request = SubscribeRequest {
        since: last_clock.take(),
        expression: Some(expression.clone()),
        defer_vcs: true,
        ..Default::default()
    };

    let (mut subscription, initial_response) = watchman
        .subscribe::<NameOnly>(&resolved, subscribe_request)
        .await?;

    *last_clock = Some(initial_response.clock);

    log::info!("Watchman subscription created: {}", subscription.name());

    // Send startup notification on initial subscription only
    if let Some(startup_tx) = startup_tx.take() {
        startup_tx
            .send(true)
            .expect("failed to send startup notification");
    }

    loop {
        match handle_subscription_event(
            &project_root,
            last_clock,
            &config_relative_path,
            &tx,
            subscription.next().await,
        )
        .await
        {
            Ok(true) => continue,
            // The server itself is exiting, so exit the subscriber itself.
            Ok(false) => return Ok(()),
            // Try to reconnect on every other error kind.
            Err(e) => return Err(e),
        }
    }
}

/// Handle a single event from Watchman.
/// Returns `Ok(true)` if processing should continue, `Ok(false)` to indicate the subscriber should terminate,
/// and `Err` to indicate a reconnect should be attempted.
async fn handle_subscription_event(
    project_root: &Path,
    last_clock: &mut Option<Clock>,
    config_relative_path: &Option<PathBuf>,
    tx: &mpsc::Sender<WatchmanEvent>,
    event: Result<SubscriptionData<NameOnly>, watchman_client::Error>,
) -> Result<bool, Box<dyn Error>> {
    match event {
        Ok(SubscriptionData::FilesChanged(result)) => {
            // Keep track of the last watchman clock seen so that we can use it
            // in reconnect attempts to avoid losing changes.
            *last_clock = Some(result.clock);

            // `is_fresh_instance` means the set of changed files in the response is the full set of files
            // matching the subscription query, rather than an incremental changeset.
            // Trigger a full reanalysis in this case.
            if result.is_fresh_instance {
                log::info!("non-incremental watchman notification, triggering full reanalysis");
                if tx.send(WatchmanEvent::FreshInstance).await.is_err() {
                    return Ok(false);
                }

                return Ok(true);
            }

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
                    log::info!("Watchman detected config file change, triggering full reanalysis");
                    if tx.send(WatchmanEvent::ConfigChanged).await.is_err() {
                        return Ok(false);
                    }
                }

                if !new_statuses.is_empty() {
                    log::info!("Watchman detected {} file change(s)", new_statuses.len());
                    if tx
                        .send(WatchmanEvent::FileChanges(new_statuses))
                        .await
                        .is_err()
                    {
                        return Ok(false);
                    }
                }
            } else {
                log::info!("Received watchman file changes event without files");
            }
        }
        Ok(SubscriptionData::StateEnter { state_name, .. }) => {
            log::info!("Watchman state enter: {}", state_name);
        }
        Ok(SubscriptionData::StateLeave { state_name, .. }) => {
            log::info!("Watchman state leave: {}", state_name);
        }
        Ok(SubscriptionData::Canceled) => return Err("Watchman subscription canceled".into()),
        Err(e) => {
            return Err(e.into());
        }
    }

    Ok(true)
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
    use std::{fs, io::Error};
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

    // ── subscription tests ──

    #[tokio::test]
    async fn handle_hack_file_modified() -> Result<(), Error> {
        let tmp = TempDir::new().expect("could not create temp dir");
        let hack_file = tmp.path().canonicalize()?.join("test.hack");
        fs::write(&hack_file, "content").expect("could not create test file");

        {
            let mut watchman = start_subscription(tmp.path().to_path_buf(), vec![], None).await;

            assert!(
                matches!(
                    watchman
                        .recv()
                        .await
                        .expect("could not receive watchman event"),
                    WatchmanEvent::FreshInstance,
                ),
                "expected initial notification with is_fresh_instance"
            );

            fs::write(&hack_file, "new content").expect("could not update file");

            match watchman
                .recv()
                .await
                .expect("could not receive watchman event")
            {
                WatchmanEvent::FileChanges(changes) => {
                    let key = hack_file.to_string_lossy().to_string();
                    assert!(changes.contains_key(&key));
                    assert!(matches!(changes[&key], FileStatus::Modified(_, _)));
                }
                _ => panic!("expected FileChanges"),
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn handle_deleted_hack_file() -> Result<(), Error> {
        let tmp = TempDir::new().expect("could not create temp dir");
        let hack_file = tmp.path().canonicalize()?.join("test.hack");
        fs::write(&hack_file, "content").expect("could not create test file");

        {
            let mut watchman = start_subscription(tmp.path().to_path_buf(), vec![], None).await;

            assert!(
                matches!(
                    watchman
                        .recv()
                        .await
                        .expect("could not receive watchman event"),
                    WatchmanEvent::FreshInstance,
                ),
                "expected initial notification with is_fresh_instance"
            );

            fs::remove_file(&hack_file).expect("could not delete file");

            match watchman
                .recv()
                .await
                .expect("could not receive watchman event")
            {
                WatchmanEvent::FileChanges(changes) => {
                    let key = hack_file.to_string_lossy().to_string();
                    assert!(matches!(changes[&key], FileStatus::Deleted));
                }
                _ => panic!("expected FileChanges"),
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn handle_deleted_non_hack_file_is_deleted_dir() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let mut last_clock = None;
        let event = make_files_changed(vec![make_name_only("somedir")]);

        let cont = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event)
            .await
            .unwrap();
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
        let mut last_clock = None;
        let event = make_files_changed(vec![make_name_only("readme.txt")]);

        let cont = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event)
            .await
            .unwrap();
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
        let mut last_clock = None;
        let event = make_files_changed(vec![make_name_only("subdir")]);

        let cont = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event)
            .await
            .unwrap();
        assert!(cont);
        assert!(rx.try_recv().is_err(), "directories should be skipped");
    }

    #[tokio::test]
    async fn handle_config_change_sends_config_event() -> Result<(), Error> {
        let tmp = TempDir::new().expect("could not create temp dir");
        let config_file = tmp.path().canonicalize()?.join("hakana.json");
        fs::write(&config_file, "{}").expect("could not create config file");

        {
            let config_path = Some(config_file.clone());
            let mut watchman =
                start_subscription(tmp.path().to_path_buf(), vec![], config_path).await;

            assert!(
                matches!(
                    watchman
                        .recv()
                        .await
                        .expect("could not receive watchman event"),
                    WatchmanEvent::FreshInstance,
                ),
                "expected initial notification with is_fresh_instance"
            );

            fs::write(&config_file, "{\"updated\": true}").expect("could not update config file");

            match watchman
                .recv()
                .await
                .expect("could not receive watchman event")
            {
                WatchmanEvent::ConfigChanged => {}
                _ => panic!("expected ConfigChanged"),
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn handle_config_and_hack_changes_together() -> Result<(), Error> {
        let tmp = TempDir::new().expect("could not create temp dir");
        let canonical = tmp.path().canonicalize()?;
        let config_file = canonical.join("hakana.json");
        let hack_file = canonical.join("main.hack");
        fs::write(&config_file, "{}").expect("could not create config file");
        fs::write(&hack_file, "code").expect("could not create hack file");

        {
            let config_path = Some(config_file.clone());
            let mut watchman =
                start_subscription(tmp.path().to_path_buf(), vec![], config_path).await;

            assert!(
                matches!(
                    watchman
                        .recv()
                        .await
                        .expect("could not receive watchman event"),
                    WatchmanEvent::FreshInstance,
                ),
                "expected initial notification with is_fresh_instance"
            );

            fs::write(&config_file, "{\"updated\": true}").expect("could not update config file");
            fs::write(&hack_file, "new code").expect("could not update hack file");

            let mut saw_config_changed = false;
            let mut saw_file_changes = false;

            while !saw_config_changed || !saw_file_changes {
                match watchman
                    .recv()
                    .await
                    .expect("could not receive watchman event")
                {
                    WatchmanEvent::ConfigChanged => saw_config_changed = true,
                    WatchmanEvent::FileChanges(_) => saw_file_changes = true,
                    _ => panic!("unexpected event"),
                }
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn handle_no_files_in_result() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let mut last_clock = None;
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

        let cont = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event)
            .await
            .unwrap();
        assert!(cont);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn handle_canceled_returns_error() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let mut last_clock = None;
        let event: Result<SubscriptionData<NameOnly>, _> = Ok(SubscriptionData::Canceled);

        let res = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event).await;
        assert!(res.is_err(), "should return an error");
    }

    #[tokio::test]
    async fn handle_error_returns_error() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let mut last_clock = None;
        let event: Result<SubscriptionData<NameOnly>, _> =
            Err(watchman_client::Error::WatchmanResponseError {
                message: "test error".to_string(),
            });

        let res = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event).await;
        assert!(res.is_err(), "should return an error");
    }

    #[tokio::test]
    async fn handle_state_enter_continues() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let mut last_clock = None;
        let event: Result<SubscriptionData<NameOnly>, _> = Ok(SubscriptionData::StateEnter {
            state_name: "hg.update".to_string(),
            metadata: None,
        });

        let cont = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event)
            .await
            .unwrap();
        assert!(cont);
    }

    #[tokio::test]
    async fn handle_state_leave_continues() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let mut last_clock = None;
        let event: Result<SubscriptionData<NameOnly>, _> = Ok(SubscriptionData::StateLeave {
            state_name: "hg.update".to_string(),
            metadata: None,
        });

        let cont = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event)
            .await
            .unwrap();
        assert!(cont);
    }

    #[tokio::test]
    async fn handle_closed_receiver_returns_false() {
        let tmp = TempDir::new().unwrap();
        let hack_file = tmp.path().join("test.hack");
        fs::write(&hack_file, "content").unwrap();

        let (tx, rx) = mpsc::channel(16);
        drop(rx);

        let mut last_clock = None;
        let event = make_files_changed(vec![make_name_only("test.hack")]);
        let result = handle_subscription_event(tmp.path(), &mut last_clock, &None, &tx, event)
            .await
            .unwrap();
        assert!(!result, "should return false when receiver is dropped");
    }
}
