use crate::tools::Tool;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_protocol::{
    ClientSocket, FindSymbolReferencesRequest, GotoDefinitionRequest, Message, SocketPath,
    StatusRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::Instant;

const PROTOCOL_VERSION: &str = "2024-11-05";

const SERVER_NAME: &str = "hakana-mcp";
const SERVER_VERSION: &str = "0.1.0";

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

pub struct McpServer {
    root_dir: String,
    threads: u8,
    config_path: Option<String>,
    #[allow(dead_code)]
    plugins: Vec<Arc<dyn CustomHook>>,
    socket_path: SocketPath,
    server_ready: bool,
    initialized: bool,
}

impl McpServer {
    pub fn new(
        root_dir: String,
        threads: u8,
        config_path: Option<String>,
        plugins: Vec<Arc<dyn CustomHook>>,
        _header: String,
    ) -> Self {
        let socket_path = SocketPath::for_project(Path::new(&root_dir));
        Self {
            root_dir,
            threads,
            config_path,
            plugins,
            socket_path,
            server_ready: false,
            initialized: false,
        }
    }

    async fn ensure_server_ready(&mut self) -> Result<(), String> {
        if self.server_ready {
            return Ok(());
        }

        if !self.socket_path.server_exists() {
            eprintln!("Spawning hakana server...");
            self.spawn_server()?;

            eprintln!("Waiting for server to start...");
            let start = Instant::now();
            let timeout = Duration::from_secs(300);

            loop {
                if self.socket_path.server_exists() {
                    break;
                }
                if start.elapsed() > timeout {
                    return Err("Timed out waiting for hakana server to start".to_string());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            eprintln!("Server socket appeared, waiting for analysis to complete...");
        }

        self.wait_for_server_ready().await?;
        self.server_ready = true;

        Ok(())
    }

    fn spawn_server(&self) -> Result<(), String> {
        let current_exe = std::env::current_exe()
            .map_err(|e| format!("Could not determine current executable: {}", e))?;

        let hakana_exe = current_exe
            .parent()
            .map(|p| p.join("hakana"))
            .unwrap_or_else(|| std::path::PathBuf::from("hakana"));

        let config_path = self
            .config_path
            .clone()
            .unwrap_or_else(|| format!("{}/hakana.json", self.root_dir));

        eprintln!(
            "Spawning: {} server --root {}",
            hakana_exe.display(),
            self.root_dir
        );

        let child = Command::new(&hakana_exe)
            .arg("server")
            .arg("--root")
            .arg(&self.root_dir)
            .arg("--config")
            .arg(&config_path)
            .arg("--threads")
            .arg(self.threads.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn server: {}", e))?;

        std::mem::forget(child);

        Ok(())
    }

    async fn wait_for_server_ready(&self) -> Result<(), String> {
        let start = Instant::now();
        let timeout = Duration::from_secs(300);

        loop {
            if start.elapsed() > timeout {
                return Err("Timed out waiting for server to complete analysis".to_string());
            }

            if let Ok(mut client) = ClientSocket::connect(&self.socket_path).await
                && let Ok(Message::StatusResult(status)) =
                    client.request(&Message::Status(StatusRequest)).await
            {
                if status.ready && !status.analysis_in_progress {
                    eprintln!(
                        "\nServer analysis complete: {} files, {} symbols",
                        status.files_count, status.symbols_count
                    );
                    return Ok(());
                }
                let elapsed = start.elapsed().as_secs();
                eprint!(
                    "\rAnalysis in progress... {}s (files: {})",
                    elapsed, status.files_count
                );
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn handle_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params),
            "initialized" | "notifications/initialized" => {
                self.initialized = true;
                JsonRpcResponse::success(request.id, json!({}))
            }
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "shutdown" => JsonRpcResponse::success(request.id, json!({})),
            _ => JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            ),
        }
    }

    fn handle_initialize(&mut self, id: Option<Value>, _params: Value) -> JsonRpcResponse {
        self.initialized = true;

        JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            }),
        )
    }

    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let tools = vec![
            Tool::find_symbol_usages_definition(),
            Tool::goto_definition_definition(),
        ];

        JsonRpcResponse::success(
            id,
            json!({
                "tools": tools
            }),
        )
    }

    async fn handle_tools_call(&mut self, id: Option<Value>, params: Value) -> JsonRpcResponse {
        #[derive(Deserialize)]
        struct ToolCallParams {
            name: String,
            #[serde(default)]
            arguments: Value,
        }

        let call_params: ToolCallParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                return JsonRpcResponse::error(id, -32602, format!("Invalid params: {}", e));
            }
        };

        match call_params.name.as_str() {
            "find_symbol_usages" => {
                self.handle_find_symbol_usages(id, call_params.arguments)
                    .await
            }
            "goto_definition" => self.handle_goto_definition(id, call_params.arguments).await,
            _ => JsonRpcResponse::error(id, -32602, format!("Unknown tool: {}", call_params.name)),
        }
    }

    async fn handle_find_symbol_usages(
        &mut self,
        id: Option<Value>,
        arguments: Value,
    ) -> JsonRpcResponse {
        if let Err(e) = self.ensure_server_ready().await {
            return JsonRpcResponse::error(id, -32603, e);
        }

        #[derive(Deserialize)]
        struct SymbolUsagesParams {
            symbol_name: String,
        }

        let params: SymbolUsagesParams = match serde_json::from_value(arguments) {
            Ok(p) => p,
            Err(e) => {
                return JsonRpcResponse::error(id, -32602, format!("Invalid arguments: {}", e));
            }
        };

        let mut client = match ClientSocket::connect(&self.socket_path).await {
            Ok(c) => c,
            Err(e) => {
                return JsonRpcResponse::error(
                    id,
                    -32603,
                    format!("Failed to connect to server: {}", e),
                );
            }
        };

        let request = Message::FindSymbolReferences(FindSymbolReferencesRequest {
            symbol_name: params.symbol_name.clone(),
        });

        match client.request(&request).await {
            Ok(Message::FindSymbolReferencesResult(response)) => {
                let content = if !response.symbol_found {
                    format!("Symbol not found: {}", params.symbol_name)
                } else if response.references.is_empty() {
                    format!("No usages found for symbol: {}", params.symbol_name)
                } else {
                    let mut output = format!(
                        "Found {} usage(s) of {}:\n\n",
                        response.references.len(),
                        params.symbol_name
                    );
                    for r in response.references {
                        output.push_str(&format!("{}:{}:{}\n", r.file_path, r.line, r.column,));
                    }
                    output
                };

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": content
                        }]
                    }),
                )
            }
            Ok(Message::Error(e)) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Server error: {}", e.message)
                    }],
                    "isError": true
                }),
            ),
            Ok(_) => {
                JsonRpcResponse::error(id, -32603, "Unexpected response from server".to_string())
            }
            Err(e) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error finding usages: {}", e)
                    }],
                    "isError": true
                }),
            ),
        }
    }

    async fn handle_goto_definition(
        &mut self,
        id: Option<Value>,
        arguments: Value,
    ) -> JsonRpcResponse {
        if let Err(e) = self.ensure_server_ready().await {
            return JsonRpcResponse::error(id, -32603, e);
        }

        #[derive(Deserialize)]
        struct GotoDefinitionParams {
            file_path: String,
            line: u32,
            column: u32,
        }

        let params: GotoDefinitionParams = match serde_json::from_value(arguments) {
            Ok(p) => p,
            Err(e) => {
                return JsonRpcResponse::error(id, -32602, format!("Invalid arguments: {}", e));
            }
        };

        let mut client = match ClientSocket::connect(&self.socket_path).await {
            Ok(c) => c,
            Err(e) => {
                return JsonRpcResponse::error(
                    id,
                    -32603,
                    format!("Failed to connect to server: {}", e),
                );
            }
        };

        let request = Message::GotoDefinition(GotoDefinitionRequest {
            file_path: params.file_path.clone(),
            line: params.line,
            column: params.column,
        });

        match client.request(&request).await {
            Ok(Message::GotoDefinitionResult(response)) => {
                let content = if !response.found {
                    format!(
                        "No definition found at {}:{}:{}",
                        params.file_path, params.line, params.column
                    )
                } else {
                    let file_path = response.file_path.unwrap_or_default();
                    let start_line = response.start_line.unwrap_or(0);
                    let start_column = response.start_column.unwrap_or(0);
                    let end_line = response.end_line.unwrap_or(0);
                    let end_column = response.end_column.unwrap_or(0);

                    format!(
                        "Definition found:\n\nFile: {}\nStart: line {}, column {}\nEnd: line {}, column {}",
                        file_path, start_line, start_column, end_line, end_column
                    )
                };

                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": content
                        }]
                    }),
                )
            }
            Ok(Message::Error(e)) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Server error: {}", e.message)
                    }],
                    "isError": true
                }),
            ),
            Ok(_) => {
                JsonRpcResponse::error(id, -32603, "Unexpected response from server".to_string())
            }
            Err(e) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error looking up definition: {}", e)
                    }],
                    "isError": true
                }),
            ),
        }
    }
}

async fn run_mcp_server_io(
    server: &mut McpServer,
    reader: impl tokio::io::AsyncBufRead + Unpin,
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
) -> io::Result<()> {
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let error_response =
                    JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                let response_json = serde_json::to_string(&error_response)?;
                writer
                    .write_all(format!("{}\n", response_json).as_bytes())
                    .await?;
                writer.flush().await?;
                continue;
            }
        };

        let is_notification = request.id.is_none();

        if request.method == "shutdown" {
            let response = server.handle_request(request).await;
            let response_json = serde_json::to_string(&response)?;
            writer
                .write_all(format!("{}\n", response_json).as_bytes())
                .await?;
            writer.flush().await?;
            break;
        }

        let response = server.handle_request(request).await;

        if is_notification {
            continue;
        }

        let response_json = serde_json::to_string(&response)?;
        writer
            .write_all(format!("{}\n", response_json).as_bytes())
            .await?;
        writer.flush().await?;
    }

    Ok(())
}

pub async fn run_mcp_server(
    root_dir: String,
    threads: u8,
    config_path: Option<String>,
    plugins: Vec<Arc<dyn CustomHook>>,
    header: String,
) -> io::Result<()> {
    let mut server = McpServer::new(root_dir, threads, config_path, plugins, header);
    let stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();

    run_mcp_server_io(&mut server, stdin, &mut stdout).await?;

    eprintln!("Hakana MCP server shutting down");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run_session(messages: &[&str]) -> Vec<serde_json::Value> {
        let input = messages.join("\n") + "\n";
        let reader = BufReader::new(input.as_bytes());
        let mut output = Vec::new();

        let mut server = McpServer::new("/tmp/fake".to_string(), 1, None, vec![], String::new());
        run_mcp_server_io(&mut server, reader, &mut output)
            .await
            .unwrap();

        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn notifications_initialized_gets_no_response() {
        let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#;
        let notification = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

        let responses = run_session(&[initialize, notification]).await;

        assert_eq!(
            responses.len(),
            1,
            "expected 1 response, got: {responses:?}"
        );
        assert_eq!(responses[0]["id"], 1);
        assert!(responses[0]["result"].is_object());
        assert!(responses[0]["error"].is_null());
    }

    #[tokio::test]
    async fn unknown_method_with_id_gets_error_response() {
        let request = r#"{"jsonrpc":"2.0","id":42,"method":"bogus/method"}"#;
        let responses = run_session(&[request]).await;

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["id"], 42);
        assert_eq!(responses[0]["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn unknown_notification_is_silently_ignored() {
        let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#;
        let notification =
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":99}}"#;

        let responses = run_session(&[initialize, notification]).await;

        assert_eq!(
            responses.len(),
            1,
            "expected 1 response, got: {responses:?}"
        );
        assert_eq!(responses[0]["id"], 1);
    }
}
