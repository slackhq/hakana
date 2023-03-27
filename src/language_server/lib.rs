use std::path::Path;
use std::sync::Arc;

use hakana_analyzer::config::{self, Config};
use hakana_analyzer::custom_hook::CustomHook;
use hakana_workhorse::scanner::ScanFilesResult;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub analysis_config: Arc<Config>,
    pub scan_result: tokio::sync::Mutex<ScanFilesResult>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
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

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for file_event in params.changes {
            //let uri = file_event.uri;
            let change_type = file_event.typ;

            match change_type {
                FileChangeType::CREATED => {
                    // Handle file creation
                    // ...
                }
                FileChangeType::CHANGED => {
                    // Handle file modification
                    // ...
                }
                FileChangeType::DELETED => {
                    // Handle file deletion
                    // ...
                }
                _ => {}
            }
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

pub fn get_config(plugins: Vec<Box<dyn CustomHook>>, cwd: &String) -> Config {
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
        config.update_from_file(&cwd, config_path);
    }

    config
}
