use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}

impl Notification {
    pub fn new(method: String, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        }
    }

    pub fn file_changed(file_path: String) -> Self {
        Self::new(
            "workspace/didChangeWatchedFiles".to_string(),
            Some(serde_json::json!({
                "changes": [{
                    "uri": format!("file://{}", file_path),
                    "type": 2  // Changed
                }]
            })),
        )
    }

    pub fn diagnostics_published(uri: String, diagnostics: Value) -> Self {
        Self::new(
            "textDocument/publishDiagnostics".to_string(),
            Some(serde_json::json!({
                "uri": uri,
                "diagnostics": diagnostics
            })),
        )
    }
}