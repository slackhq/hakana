use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use hakana_analyzer::config::{self, Config};
use hakana_analyzer::custom_hook::CustomHook;
use hakana_logger::{Logger, Verbosity};
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_workhorse::file::FileStatus;
use hakana_workhorse::{scan_and_analyze, SuccessfulScanData};
use rustc_hash::FxHashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub analysis_config: Arc<Config>,
    pub previous_scan_data: Arc<Option<SuccessfulScanData>>,
    pub previous_analysis_result: Arc<Option<AnalysisResult>>,
    pub all_diagnostics: Option<FxHashMap<Url, Vec<Diagnostic>>>,
    pub file_changes: Option<FxHashMap<String, FileStatus>>,
}

#[tower_lsp::async_trait(?Send)]
impl LanguageServer for Backend {
    async fn initialize(&mut self, _: InitializeParams) -> Result<InitializeResult> {
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

    async fn initialized(&mut self, _: InitializedParams) {
        let registration = Registration {
            id: "watch-hack-files".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(
                serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/*.{hack,php,hhi}".to_string()),
                        kind: None,
                    }],
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

        self.all_diagnostics = None;

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn did_change_watched_files(&mut self, params: DidChangeWatchedFilesParams) {
        let mut new_file_statuses = FxHashMap::default();

        for file_event in params.changes {
            //let uri = file_event.uri;
            let change_type = file_event.typ;

            let file_path = file_event.uri.path().to_string();

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
        }

        self.file_changes = Some(new_file_statuses);

        self.do_analysis().await;
        self.emit_issues().await;
    }

    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Backend {
    async fn do_analysis(&mut self) {
        let previous_scan_data = self.previous_scan_data.clone();
        let previous_analysis_result = self.previous_analysis_result.clone();

        self.previous_scan_data = Arc::new(None);
        self.previous_analysis_result = Arc::new(None);

        let successful_scan_data = Arc::try_unwrap(previous_scan_data).unwrap();

        let analysis_result = Arc::try_unwrap(previous_analysis_result).unwrap();

        let file_changes = if let Some(ref mut file_changes) = self.file_changes {
            let file_changes = file_changes
                .drain()
                .into_iter()
                .collect::<FxHashMap<_, _>>();
            Some(file_changes)
        } else {
            None
        };

        let result = scan_and_analyze(
            Vec::new(),
            None,
            None,
            self.analysis_config.clone(),
            None,
            8,
            Arc::new(Logger::LanguageServer(
                self.client.clone(),
                Verbosity::Simple,
            )),
            "",
            successful_scan_data,
            analysis_result,
            file_changes,
        )
        .await;

        self.file_changes = None;

        match result {
            Ok((analysis_result, successful_scan_data)) => {
                self.client
                    .log_message(MessageType::INFO, "Analysis succeeded, sending diagnostics")
                    .await;

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
                                    line: emitted_issue.pos.start_line as u32 - 1,
                                    character: emitted_issue.pos.start_column as u32 - 1,
                                },
                                end: Position {
                                    line: emitted_issue.pos.end_line as u32 - 1,
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

                    all_diagnostics.insert(Url::from_file_path(&file).unwrap(), diagnostics);
                }

                self.all_diagnostics = Some(all_diagnostics);
                self.previous_scan_data = Arc::new(Some(successful_scan_data));
                self.previous_analysis_result = Arc::new(Some(analysis_result));
            }
            Err(error) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Analysis failed with error {}", error.to_string()),
                    )
                    .await;
            }
        }
    }

    async fn emit_issues(&mut self) {
        if let Some(ref mut all_diagnostics) = self.all_diagnostics {
            for (uri, diagnostics) in all_diagnostics.drain() {
                self.client
                    .publish_diagnostics(uri.clone(), diagnostics.clone(), None)
                    .await;
            }
        }
    }
}

pub fn get_config(
    plugins: Vec<Box<dyn CustomHook>>,
    cwd: &String,
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
    config.find_unused_definitions = false;
    config.ignore_mixed_issues = true;
    config.ast_diff = true;

    config.hooks = plugins;

    let config_path_str = format!("{}/hakana.json", cwd);

    let config_path = Path::new(&config_path_str);

    if config_path.exists() {
        config.update_from_file(&cwd, config_path)?;
    }

    Ok(config)
}
