use std::error::Error;
use std::sync::Arc;

use crate::daemon_client::DaemonClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub struct McpClient {
    daemon_client: Arc<Mutex<Option<DaemonClient>>>,
    notification_rx: Arc<Mutex<Option<mpsc::Receiver<crate::protocol::Notification>>>>,
}

impl McpClient {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            daemon_client: Arc::new(Mutex::new(None)),
            notification_rx: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn connect(&self) -> Result<(), Box<dyn Error>> {
        let mut daemon_client_guard = self.daemon_client.lock().await;
        if daemon_client_guard.is_none() {
            let (client, notification_rx) = DaemonClient::connect("127.0.0.1:9999", "hakana-mcp").await?;
            *daemon_client_guard = Some(client);

            let mut notification_rx_guard = self.notification_rx.lock().await;
            *notification_rx_guard = Some(notification_rx);
        }
        Ok(())
    }

    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        if let Err(_) = self.connect().await {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(json!({
                    "code": -1,
                    "message": "Failed to connect to Hakana daemon"
                })),
            };
        }

        match request.method.as_str() {
            "initialize" => self.handle_initialize(request).await,
            "resources/list" => self.handle_list_resources(request).await,
            "resources/read" => self.handle_read_resource(request).await,
            "tools/list" => self.handle_list_tools(request).await,
            "tools/call" => self.handle_call_tool(request).await,
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(json!({
                    "code": -32601,
                    "message": "Method not found"
                })),
            },
        }
    }

    async fn handle_initialize(&self, request: McpRequest) -> McpResponse {
        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "resources": {
                        "subscribe": true,
                        "listChanged": true
                    },
                    "tools": {
                        "listChanged": true
                    }
                },
                "serverInfo": {
                    "name": "hakana-mcp",
                    "version": "0.1.0"
                }
            })),
            error: None,
        }
    }

    async fn handle_list_resources(&self, request: McpRequest) -> McpResponse {
        // List available codebase resources
        let resources = vec![
            McpResource {
                uri: "hakana://codebase/classes".to_string(),
                name: "All Classes".to_string(),
                description: Some("List of all classes in the codebase".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            McpResource {
                uri: "hakana://codebase/functions".to_string(),
                name: "All Functions".to_string(),
                description: Some("List of all functions in the codebase".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            McpResource {
                uri: "hakana://codebase/methods".to_string(),
                name: "All Methods".to_string(),
                description: Some("List of all methods in the codebase".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            McpResource {
                uri: "hakana://codebase/constants".to_string(),
                name: "All Constants".to_string(),
                description: Some("List of all constants in the codebase".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ];

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "resources": resources
            })),
            error: None,
        }
    }

    async fn handle_read_resource(&self, request: McpRequest) -> McpResponse {
        if let Some(params) = &request.params {
            if let Some(uri) = params.get("uri").and_then(|u| u.as_str()) {
                match uri {
                    "hakana://codebase/classes" => {
                        // TODO: Get all classes from daemon
                        return McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id,
                            result: Some(json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "application/json",
                                    "text": json!({
                                        "classes": []
                                    }).to_string()
                                }]
                            })),
                            error: None,
                        };
                    }
                    "hakana://codebase/functions" => {
                        // TODO: Get all functions from daemon
                        return McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id,
                            result: Some(json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "application/json",
                                    "text": json!({
                                        "functions": []
                                    }).to_string()
                                }]
                            })),
                            error: None,
                        };
                    }
                    _ => {}
                }
            }
        }

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(json!({
                "code": -32602,
                "message": "Invalid resource URI"
            })),
        }
    }

    async fn handle_list_tools(&self, request: McpRequest) -> McpResponse {
        let tools = vec![
            McpTool {
                name: "get_definition".to_string(),
                description: "Get the definition location of a symbol at a specific file position".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file": {
                            "type": "string",
                            "description": "File path"
                        },
                        "line": {
                            "type": "integer",
                            "description": "Line number (0-based)"
                        },
                        "character": {
                            "type": "integer",
                            "description": "Character position (0-based)"
                        }
                    },
                    "required": ["file", "line", "character"]
                }),
            },
            McpTool {
                name: "get_references".to_string(),
                description: "Get all references to a symbol at a specific file position".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file": {
                            "type": "string",
                            "description": "File path"
                        },
                        "line": {
                            "type": "integer",
                            "description": "Line number (0-based)"
                        },
                        "character": {
                            "type": "integer",
                            "description": "Character position (0-based)"
                        },
                        "include_declaration": {
                            "type": "boolean",
                            "description": "Whether to include the declaration in results",
                            "default": true
                        }
                    },
                    "required": ["file", "line", "character"]
                }),
            },
            McpTool {
                name: "search_symbols".to_string(),
                description: "Search for symbols in the codebase".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query"
                        }
                    },
                    "required": ["query"]
                }),
            },
            McpTool {
                name: "get_diagnostics".to_string(),
                description: "Get diagnostics (errors, warnings) for a specific file".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file": {
                            "type": "string",
                            "description": "File path"
                        }
                    },
                    "required": ["file"]
                }),
            },
        ];

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "tools": tools
            })),
            error: None,
        }
    }

    async fn handle_call_tool(&self, request: McpRequest) -> McpResponse {
        if let Some(params) = &request.params {
            if let Some(name) = params.get("name").and_then(|n| n.as_str()) {
                if let Some(arguments) = params.get("arguments") {
                    let daemon_client_guard = self.daemon_client.lock().await;
                    if let Some(daemon_client) = daemon_client_guard.as_ref() {
                        return match name {
                            "get_definition" => self.call_get_definition(daemon_client, arguments, &request.id).await,
                            "get_references" => self.call_get_references(daemon_client, arguments, &request.id).await,
                            "search_symbols" => self.call_search_symbols(daemon_client, arguments, &request.id).await,
                            "get_diagnostics" => self.call_get_diagnostics(daemon_client, arguments, &request.id).await,
                            _ => McpResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id,
                                result: None,
                                error: Some(json!({
                                    "code": -32602,
                                    "message": "Unknown tool"
                                })),
                            },
                        };
                    }
                }
            }
        }

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(json!({
                "code": -32602,
                "message": "Invalid tool call parameters"
            })),
        }
    }

    async fn call_get_definition(&self, daemon_client: &DaemonClient, arguments: &Value, id: &Value) -> McpResponse {
        if let (Some(file), Some(line), Some(character)) = (
            arguments.get("file").and_then(|f| f.as_str()),
            arguments.get("line").and_then(|l| l.as_u64()),
            arguments.get("character").and_then(|c| c.as_u64()),
        ) {
            let uri = format!("file://{}", file);
            match daemon_client.get_definition(&uri, line as u32, character as u32).await {
                Ok(result) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Definition: {}", result)
                        }]
                    })),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: None,
                    error: Some(json!({
                        "code": -1,
                        "message": e.to_string()
                    })),
                },
            }
        } else {
            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: None,
                error: Some(json!({
                    "code": -32602,
                    "message": "Invalid arguments for get_definition"
                })),
            }
        }
    }

    async fn call_get_references(&self, daemon_client: &DaemonClient, arguments: &Value, id: &Value) -> McpResponse {
        if let (Some(file), Some(line), Some(character)) = (
            arguments.get("file").and_then(|f| f.as_str()),
            arguments.get("line").and_then(|l| l.as_u64()),
            arguments.get("character").and_then(|c| c.as_u64()),
        ) {
            let include_declaration = arguments.get("include_declaration").and_then(|b| b.as_bool()).unwrap_or(true);
            let uri = format!("file://{}", file);

            match daemon_client.get_references(&uri, line as u32, character as u32, include_declaration).await {
                Ok(result) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(json!({
                        "content": [{
                            "type": "text",
                            "text": format!("References: {}", result)
                        }]
                    })),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: None,
                    error: Some(json!({
                        "code": -1,
                        "message": e.to_string()
                    })),
                },
            }
        } else {
            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: None,
                error: Some(json!({
                    "code": -32602,
                    "message": "Invalid arguments for get_references"
                })),
            }
        }
    }

    async fn call_search_symbols(&self, daemon_client: &DaemonClient, arguments: &Value, id: &Value) -> McpResponse {
        if let Some(query) = arguments.get("query").and_then(|q| q.as_str()) {
            match daemon_client.search_symbols(query).await {
                Ok(result) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Symbols: {}", result)
                        }]
                    })),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: None,
                    error: Some(json!({
                        "code": -1,
                        "message": e.to_string()
                    })),
                },
            }
        } else {
            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: None,
                error: Some(json!({
                    "code": -32602,
                    "message": "Invalid arguments for search_symbols"
                })),
            }
        }
    }

    async fn call_get_diagnostics(&self, daemon_client: &DaemonClient, arguments: &Value, id: &Value) -> McpResponse {
        if let Some(file) = arguments.get("file").and_then(|f| f.as_str()) {
            let uri = format!("file://{}", file);
            match daemon_client.get_diagnostics(&uri).await {
                Ok(result) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: Some(json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Diagnostics: {}", result)
                        }]
                    })),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: id.clone(),
                    result: None,
                    error: Some(json!({
                        "code": -1,
                        "message": e.to_string()
                    })),
                },
            }
        } else {
            McpResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: None,
                error: Some(json!({
                    "code": -32602,
                    "message": "Invalid arguments for get_diagnostics"
                })),
            }
        }
    }
}