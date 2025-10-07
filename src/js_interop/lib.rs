use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_orchestrator::wasm::{get_single_file_codebase, scan_and_analyze_single_file};
use hakana_str::Interner;
use serde_json::json;
use wasm_bindgen::prelude::*;
extern crate console_error_panic_hook;

#[wasm_bindgen]
pub struct ScannerAndAnalyzer {
    codebase: CodebaseInfo,
    interner: Interner,
}

#[wasm_bindgen]
impl ScannerAndAnalyzer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_results(&mut self, file_contents: String) -> String {
        let result = scan_and_analyze_single_file(
            &mut self.codebase,
            &self.interner,
            "hello.hack".to_string(),
            file_contents.clone(),
            true,
        );
        match result {
            Ok((analysis_result, interner)) => {
                self.interner = interner;

                let mut issue_json_objects = vec![];
                for (file_path, issues) in analysis_result.get_all_issues(&self.interner, "", true)
                {
                    for issue in issues {
                        issue_json_objects.push(json!({
                            "severity": "ERROR",
                            "line_from": issue.pos.start_line,
                            "line_to": issue.pos.end_line,
                            "type": format!("{}", issue.kind),
                            "message": issue.description,
                            "file_name": issue.pos.file_path,
                            "file_path": file_path.clone(),
                            "snippet": "",
                            "selected_text": "",
                            "from": issue.pos.start_offset,
                            "to": issue.pos.end_offset,
                            "snippet_from": issue.pos.start_offset,
                            "snippet_to": issue.pos.end_offset,
                            "column_from": issue.pos.start_column,
                            "column_to": issue.pos.end_column,
                            "shortcode": 0,
                            "taint_trace": serde_json::Value::Null,
                            "other_references": serde_json::Value::Null,
                        }));
                    }
                }

                let json = json!({
                    "results": issue_json_objects,
                });

                json.to_string()
            }
            Err(err) => json!({
                "error": err,
            })
            .to_string(),
        }
    }
}

impl Default for ScannerAndAnalyzer {
    fn default() -> Self {
        console_error_panic_hook::set_once();

        let (codebase, interner, _) = get_single_file_codebase(vec![]);

        Self { codebase, interner }
    }
}
