use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use hakana_analyzer::config::{self, Config};
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_str::Interner;
use hakana_orchestrator::file::FileStatus;
use hakana_orchestrator::{scan_and_analyze_async, SuccessfulScanData};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

#[derive(Debug)]
pub struct Backend {
    client: Client,
    analysis_config: Arc<Config>,
    starter_interner: Arc<Interner>,
    previous_scan_data: RwLock<Option<SuccessfulScanData>>,
    previous_analysis_result: RwLock<Option<AnalysisResult>>,
    all_diagnostics: RwLock<Option<FxHashMap<Url, Vec<Diagnostic>>>>,
    file_changes: RwLock<Option<FxHashMap<String, FileStatus>>>,
    files_with_errors: RwLock<FxHashSet<Url>>,
}

impl Backend {
    pub fn new(client: Client, analysis_config: Config, starter_interner: Interner) -> Self {
        Self {
            client,
            analysis_config: Arc::new(analysis_config),
            starter_interner: Arc::new(starter_interner),
            previous_scan_data: RwLock::new(None),
            previous_analysis_result: RwLock::new(None),
            all_diagnostics: RwLock::new(None),
            file_changes: RwLock::new(None),
            files_with_errors: RwLock::new(FxHashSet::default()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        self.do_analysis().await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::NONE),
                        will_save: Some(false),
                        will_save_wait_until: Some(false),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                    },
                )),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let registration = Registration {
            id: "watch-hack-files".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(
                serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![
                        FileSystemWatcher {
                            glob_pattern: GlobPattern::String("**/*.{hack,php,hhi}".to_string()),
                            kind: None,
                        },
                        FileSystemWatcher {
                            glob_pattern: GlobPattern::String("**/.git/index.lock".to_string()),
                            kind: Some(WatchKind::Delete),
                        },
                        FileSystemWatcher {
                            glob_pattern: GlobPattern::String("**/[!.]*/**/".to_string()),
                            kind: Some(WatchKind::Delete),
                        },
                    ],
                })
                .unwrap(),
            ),
        };

        let registrations = vec![registration];
        self.client
            .register_capability(registrations)
            .await
            .unwrap();

        self.emit_issues().await;

        let mut all_diagnostics = self.all_diagnostics.write().await;
        *all_diagnostics = None;

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut new_file_statuses = FxHashMap::default();

        self.client
            .log_message(MessageType::INFO, "watched files changed")
            .await;

        // self.client
        //     .log_message(
        //         MessageType::INFO,
        //         format!("receiving changes {:?}", params.changes),
        //     )
        //     .await;

        for file_event in params.changes {
            //let uri = file_event.uri;
            let change_type = file_event.typ;

            let file_path = file_event.uri.path().to_string();

            if file_path.ends_with(".php")
                || file_path.ends_with(".hack")
                || file_path.ends_with(".hhi")
            {
                match change_type {
                    FileChangeType::CREATED => {
                        new_file_statuses.insert(file_path, FileStatus::Added(0, 0));
                    }
                    FileChangeType::CHANGED => {
                        new_file_statuses.insert(file_path, FileStatus::Modified(0, 0));
                    }
                    FileChangeType::DELETED => {
                        new_file_statuses.insert(file_path, FileStatus::Deleted);
                    }
                    _ => {}
                }
            } else if Path::new(&file_path).extension().is_none() && !file_path.contains("/.git/") {
                if let FileChangeType::DELETED = change_type {
                    new_file_statuses.insert(file_path, FileStatus::DeletedDir);
                }
            }
        }

        if !new_file_statuses.is_empty() {
            let mut existing_file_changes = self.file_changes.write().await;

            if let Some(existing_file_changes) = existing_file_changes.as_mut() {
                existing_file_changes.extend(new_file_statuses);
            } else {
                *existing_file_changes = Some(new_file_statuses);
            }
        } else {
            let file_changes_guard = self.file_changes.read().await;

            if file_changes_guard.is_none() {
                self.client
                    .log_message(MessageType::INFO, "No files updated")
                    .await;
                return;
            }
        }

        if Path::new(".git/index.lock").exists() {
            self.client
                .log_message(MessageType::INFO, "Waiting a sec while git is doing stuff")
                .await;
        } else {
            self.do_analysis().await;
            self.emit_issues().await;
        }
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl Backend {
    async fn do_analysis(&self) {
        let mut previous_scan_data_guard = self.previous_scan_data.write().await;
        let mut previous_analysis_result_guard = self.previous_analysis_result.write().await;
        let mut all_diagnostics_guard = self.all_diagnostics.write().await;

        let successful_scan_data = previous_scan_data_guard.take();

        let analysis_result = previous_analysis_result_guard.take();

        let mut file_changes_guard = self.file_changes.write().await;

        let file_changes = file_changes_guard.take();

        self.client
            .log_message(
                MessageType::INFO,
                format!("scan & analyze changes — {:?}", file_changes),
            )
            .await;

        sleep(Duration::from_millis(10)).await;

        let result = scan_and_analyze_async(
            Vec::new(),
            None,
            None,
            self.analysis_config.clone(),
            8,
            &self.client,
            "",
            self.starter_interner.clone(),
            successful_scan_data,
            analysis_result,
            file_changes,
        )
        .await;

        *file_changes_guard = None;

        match result {
            Ok((analysis_result, successful_scan_data)) => {
                let mut all_diagnostics = FxHashMap::default();

                for (file, emitted_issues) in analysis_result.get_all_issues(
                    &successful_scan_data.interner,
                    &self.analysis_config.root_dir,
                    false,
                ) {
                    let mut diagnostics = vec![];
                    for emitted_issue in emitted_issues {
                        diagnostics.push(Diagnostic::new(
                            Range {
                                start: Position {
                                    line: emitted_issue.pos.start_line - 1,
                                    character: emitted_issue.pos.start_column as u32 - 1,
                                },
                                end: Position {
                                    line: emitted_issue.pos.end_line - 1,
                                    character: emitted_issue.pos.end_column as u32 - 1,
                                },
                            },
                            Some(DiagnosticSeverity::ERROR),
                            Some(NumberOrString::String(emitted_issue.kind.to_string())),
                            Some("Hakana".to_string()),
                            emitted_issue.description.clone(),
                            None,
                            None,
                        ));
                    }

                    match Url::from_file_path(&file) {
                        Ok(url) => {
                            all_diagnostics.insert(url, diagnostics);
                        }
                        Err(_) => {
                            self.client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Failure to get url from file {}", file),
                                )
                                .await;
                        }
                    }
                }

                *all_diagnostics_guard = Some(all_diagnostics);
                *previous_scan_data_guard = Some(successful_scan_data);
                *previous_analysis_result_guard = Some(analysis_result);
            }
            Err(error) => {
                *previous_scan_data_guard = None;
                *previous_analysis_result_guard = None;
                *all_diagnostics_guard = None;

                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Analysis failed with error {}", error),
                    )
                    .await;
            }
        }
    }

    async fn emit_issues(&self) {
        if let Some(all_diagnostics) = self.all_diagnostics.write().await.as_mut() {
            let mut new_files_with_errors = FxHashSet::default();

            for (uri, diagnostics) in all_diagnostics.drain() {
                self.client
                    .publish_diagnostics(uri.clone(), diagnostics, None)
                    .await;
                new_files_with_errors.insert(uri);
            }

            let mut files_with_errors = self.files_with_errors.write().await;

            for old_uri in files_with_errors.iter() {
                if !new_files_with_errors.contains(old_uri) {
                    self.client
                        .publish_diagnostics(old_uri.clone(), vec![], None)
                        .await;
                }
            }

            *files_with_errors = new_files_with_errors;

            self.client
                .log_message(MessageType::INFO, "Diagnostics sent")
                .await;
        }
    }
}

pub fn get_config(
    plugins: Vec<Box<dyn CustomHook>>,
    cwd: &String,
    interner: &mut Interner,
) -> std::result::Result<Config, Box<dyn Error>> {
    let mut all_custom_issues = vec![];

    for analysis_hook in &plugins {
        all_custom_issues.extend(analysis_hook.get_custom_issue_names());
    }

    let mut config = config::Config::new(
        cwd.clone(),
        all_custom_issues
            .into_iter()
            .map(|i| i.to_string())
            .collect(),
    );

    config.find_unused_expressions = true;
    config.find_unused_definitions = true;
    config.ignore_mixed_issues = true;
    config.ast_diff = true;

    config.hooks = plugins;

    let config_path_str = format!("{}/hakana.json", cwd);

    let config_path = Path::new(&config_path_str);

    if config_path.exists() {
        config.update_from_file(cwd, config_path, interner)?;
    }

    Ok(config)
}
