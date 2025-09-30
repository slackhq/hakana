use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use hakana_analyzer::config::Config;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::code_location::{FilePath, HPos};
use hakana_orchestrator::{scan_and_analyze, SuccessfulScanData, file::FileStatus};
use hakana_logger::Logger;
use hakana_str::Interner;
use rustc_hash::FxHashMap;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Position;

#[derive(Debug)]
pub struct AnalysisManager {
    config: Arc<Config>,
    scan_data: RwLock<Option<SuccessfulScanData>>,
    analysis_result: RwLock<Option<AnalysisResult>>,
    last_analysis: RwLock<Option<Instant>>,
}

impl AnalysisManager {
    pub async fn new(
        config: Arc<Config>,
        interner: Interner,
    ) -> Result<Self, Box<dyn Error>> {
        let manager = Self {
            config,
            scan_data: RwLock::new(None),
            analysis_result: RwLock::new(None),
            last_analysis: RwLock::new(None),
        };

        // Perform initial analysis
        manager.perform_analysis(None, Arc::new(interner)).await?;

        Ok(manager)
    }

    pub async fn perform_analysis(
        &self,
        file_changes: Option<FxHashMap<String, FileStatus>>,
        starter_interner: Arc<Interner>,
    ) -> Result<(), Box<dyn Error>> {
        let mut scan_data_guard = self.scan_data.write().await;
        let mut analysis_result_guard = self.analysis_result.write().await;
        let mut last_analysis_guard = self.last_analysis.write().await;

        let previous_scan_data = scan_data_guard.take();
        let previous_analysis_result = analysis_result_guard.take();

        let config_clone = self.config.clone();
        let result = tokio::task::spawn_blocking(move || {
            scan_and_analyze(
                Vec::new(),
                None,
                None,
                config_clone,
                None, // cache_dir
                8,
                Arc::new(Logger::DevNull),
                "",
                (*starter_interner).clone(),
                previous_scan_data,
                previous_analysis_result,
                file_changes,
                || {}, // chaos_monkey function
            )
        }).await.unwrap();

        match result {
            Ok((analysis_result, scan_data)) => {
                *scan_data_guard = Some(scan_data);
                *analysis_result_guard = Some(analysis_result);
                *last_analysis_guard = Some(Instant::now());
                Ok(())
            }
            Err(e) => {
                *scan_data_guard = None;
                *analysis_result_guard = None;
                Err(e.into())
            }
        }
    }

    pub async fn get_definition(&self, params: &Option<Value>) -> Result<Value, Box<dyn Error>> {
        let params = params.as_ref().ok_or("Missing parameters")?;
        let position = params.get("position").ok_or("Missing position")?;
        let text_document = params.get("textDocument").ok_or("Missing textDocument")?;
        let uri = text_document.get("uri").and_then(|u| u.as_str()).ok_or("Missing URI")?;

        let line = position.get("line").and_then(|l| l.as_u64()).ok_or("Missing line")? as u32;
        let character = position.get("character").and_then(|c| c.as_u64()).ok_or("Missing character")? as u32;

        let file_path = uri.trim_start_matches("file://").to_string();

        let scan_data_guard = self.scan_data.read().await;
        let analysis_result_guard = self.analysis_result.read().await;

        if let (Some(scan_data), Some(analysis_result)) = (scan_data_guard.as_ref(), analysis_result_guard.as_ref()) {
            // Convert LSP position to offset
            if let Ok(file_contents) = std::fs::read_to_string(&file_path) {
                let offset = self.position_to_offset(&file_contents, Position { line, character });

                if let Some(file_path_id) = scan_data.interner.get(&file_path) {
                    let file_path_obj = FilePath(file_path_id);

                    if let Some(definition_locations) = analysis_result.definition_locations.get(&file_path_obj) {
                        for ((start_offset, end_offset), (classlike_name, member_name)) in definition_locations {
                            if (offset as u32) >= *start_offset && (offset as u32) <= *end_offset {
                                if let Some(pos) = scan_data.codebase.get_symbol_pos(classlike_name, member_name) {
                                    return Ok(self.pos_to_location(pos, &scan_data.interner)?);
                                }
                                return Ok(json!(null));
                            }
                        }
                    }
                }
            }
        }

        Ok(json!(null))
    }

    pub async fn get_references(&self, _params: &Option<Value>) -> Result<Value, Box<dyn Error>> {
        // TODO: Implement reference finding
        Ok(json!([]))
    }

    pub async fn get_hover(&self, _params: &Option<Value>) -> Result<Value, Box<dyn Error>> {
        // TODO: Implement hover information
        Ok(json!(null))
    }

    pub async fn search_symbols(&self, _params: &Option<Value>) -> Result<Value, Box<dyn Error>> {
        // TODO: Implement symbol search
        Ok(json!([]))
    }

    pub async fn get_diagnostics(&self, params: &Option<Value>) -> Result<Value, Box<dyn Error>> {
        let params = params.as_ref().ok_or("Missing parameters")?;
        let text_document = params.get("textDocument").ok_or("Missing textDocument")?;
        let uri = text_document.get("uri").and_then(|u| u.as_str()).ok_or("Missing URI")?;

        let file_path = uri.trim_start_matches("file://").to_string();

        let scan_data_guard = self.scan_data.read().await;
        let analysis_result_guard = self.analysis_result.read().await;

        if let (Some(scan_data), Some(analysis_result)) = (scan_data_guard.as_ref(), analysis_result_guard.as_ref()) {
            let all_issues = analysis_result.get_all_issues(
                &scan_data.interner,
                &self.config.root_dir,
                false,
            );

            if let Some((_file, emitted_issues)) = all_issues.into_iter().find(|(f, _)| f == &file_path) {
                let mut diagnostics = Vec::new();
                for emitted_issue in emitted_issues {
                    diagnostics.push(json!({
                        "range": {
                            "start": {
                                "line": emitted_issue.pos.start_line - 1,
                                "character": emitted_issue.pos.start_column - 1
                            },
                            "end": {
                                "line": emitted_issue.pos.end_line - 1,
                                "character": emitted_issue.pos.end_column - 1
                            }
                        },
                        "severity": 1, // Error
                        "code": emitted_issue.kind.to_string(),
                        "source": "Hakana",
                        "message": emitted_issue.description
                    }));
                }
                return Ok(json!(diagnostics));
            }
        }

        Ok(json!([]))
    }

    pub async fn get_all_diagnostics(&self) -> Result<FxHashMap<String, Vec<Value>>, Box<dyn Error>> {
        let scan_data_guard = self.scan_data.read().await;
        let analysis_result_guard = self.analysis_result.read().await;

        let mut all_diagnostics = FxHashMap::default();

        if let (Some(scan_data), Some(analysis_result)) = (scan_data_guard.as_ref(), analysis_result_guard.as_ref()) {
            for (file, emitted_issues) in analysis_result.get_all_issues(
                &scan_data.interner,
                &self.config.root_dir,
                false,
            ) {
                let mut diagnostics = Vec::new();
                for emitted_issue in emitted_issues {
                    diagnostics.push(json!({
                        "range": {
                            "start": {
                                "line": emitted_issue.pos.start_line - 1,
                                "character": emitted_issue.pos.start_column - 1
                            },
                            "end": {
                                "line": emitted_issue.pos.end_line - 1,
                                "character": emitted_issue.pos.end_column - 1
                            }
                        },
                        "severity": 1, // Error
                        "code": emitted_issue.kind.to_string(),
                        "source": "Hakana",
                        "message": emitted_issue.description
                    }));
                }
                all_diagnostics.insert(file, diagnostics);
            }
        }

        Ok(all_diagnostics)
    }

    fn position_to_offset(&self, file_contents: &str, position: Position) -> usize {
        let lines: Vec<&str> = file_contents.lines().collect();
        let mut offset = 0;

        // Add offset for complete lines before the target line
        for (_line_idx, line) in lines.iter().enumerate().take(position.line as usize) {
            offset += line.len() + 1; // +1 for newline character
        }

        // Add offset for characters in the target line
        if let Some(target_line) = lines.get(position.line as usize) {
            offset += std::cmp::min(position.character as usize, target_line.len());
        }

        offset
    }

    fn pos_to_location(&self, def_pos: HPos, interner: &Interner) -> Result<Value, Box<dyn Error>> {
        let file_path = interner.lookup(&def_pos.file_path.0);
        Ok(json!({
            "uri": format!("file://{}", file_path),
            "range": {
                "start": {
                    "line": def_pos.start_line - 1,
                    "character": def_pos.start_column - 1
                },
                "end": {
                    "line": def_pos.end_line - 1,
                    "character": def_pos.end_column - 1
                }
            }
        }))
    }
}