//! MCP protocol implementation using JSON-RPC over stdio.

use crate::tools::Tool;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_protocol::{
    ClientSocket, FindSymbolReferencesRequest, Message, SocketPath, StatusRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// MCP protocol version
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Server information
const SERVER_NAME: &str = "hakana-mcp";
const SERVER_VERSION: &str = "0.1.0";

/// JSON-RPC request structure
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// JSON-RPC response structure
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

/// JSON-RPC error structure
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

/// MCP Server state
pub struct McpServer {
    /// Root directory of the project
    root_dir: String,
    /// Number of threads for analysis
    threads: u8,
    /// Analysis config path
    config_path: Option<String>,
    /// Analysis plugins (unused but kept for API compatibility)
    #[allow(dead_code)]
    plugins: Vec<Arc<dyn CustomHook>>,
    /// Socket path for connecting to hakana server
    socket_path: SocketPath,
    /// Whether server is ready
    server_ready: bool,
    /// Whether the server has been initialized
    initialized: bool,
}

impl McpServer {
    /// Create a new MCP server
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

    /// Try to connect to existing hakana server, or spawn one and wait for it
    fn ensure_server_ready(&mut self) -> Result<(), String> {
        if self.server_ready {
            return Ok(());
        }

        if !self.socket_path.server_exists() {
            // Spawn a new server
            eprintln!("Spawning hakana server...");
            self.spawn_server()?;

            // Wait for server socket to appear
            eprintln!("Waiting for server to start...");
            let start = Instant::now();
            let timeout = Duration::from_secs(300); // 5 minutes for initial analysis

            loop {
                if self.socket_path.server_exists() {
                    break;
                }
                if start.elapsed() > timeout {
                    return Err("Timed out waiting for hakana server to start".to_string());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            eprintln!("Server socket appeared, waiting for analysis to complete...");
        }

        // Poll server until analysis is complete
        self.wait_for_server_ready()?;
        self.server_ready = true;

        Ok(())
    }

    /// Spawn a hakana server in the background
    fn spawn_server(&self) -> Result<(), String> {
        // Find the hakana binary (should be in the same directory as hakana-mcp)
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

        // Spawn the server as a background process
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

        // Don't wait for the child - let it run in the background
        std::mem::forget(child);

        Ok(())
    }

    /// Wait for the server to complete its initial analysis
    fn wait_for_server_ready(&self) -> Result<(), String> {
        let start = Instant::now();
        let timeout = Duration::from_secs(300); // 5 minutes

        loop {
            if start.elapsed() > timeout {
                return Err("Timed out waiting for server to complete analysis".to_string());
            }

            match ClientSocket::connect(&self.socket_path) {
                Ok(mut client) => {
                    match client.request(&Message::Status(StatusRequest)) {
                        Ok(Message::StatusResult(status)) => {
                            if status.ready && !status.analysis_in_progress {
                                eprintln!(
                                    "\nServer analysis complete: {} files, {} symbols",
                                    status.files_count, status.symbols_count
                                );
                                return Ok(());
                            }
                            // Still analyzing, show progress
                            let elapsed = start.elapsed().as_secs();
                            eprint!(
                                "\rAnalysis in progress... {}s (files: {})",
                                elapsed, status.files_count
                            );
                        }
                        Ok(_) => {
                            // Unexpected response, wait and retry
                        }
                        Err(_) => {
                            // Connection error, wait and retry
                        }
                    }
                }
                Err(_) => {
                    // Server not ready yet, wait and retry
                }
            }

            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Handle an incoming JSON-RPC request
    fn handle_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params),
            "initialized" => {
                // Notification, no response needed
                JsonRpcResponse::success(request.id, json!({}))
            }
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params),
            "shutdown" => JsonRpcResponse::success(request.id, json!({})),
            _ => JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            ),
        }
    }

    /// Handle initialize request
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

    /// Handle tools/list request
    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let tools = vec![Tool::find_symbol_usages_definition()];

        JsonRpcResponse::success(
            id,
            json!({
                "tools": tools
            }),
        )
    }

    /// Handle tools/call request
    fn handle_tools_call(&mut self, id: Option<Value>, params: Value) -> JsonRpcResponse {
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
            "find_symbol_usages" => self.handle_find_symbol_usages(id, call_params.arguments),
            _ => JsonRpcResponse::error(id, -32602, format!("Unknown tool: {}", call_params.name)),
        }
    }

    /// Handle find_symbol_usages tool call
    fn handle_find_symbol_usages(
        &mut self,
        id: Option<Value>,
        arguments: Value,
    ) -> JsonRpcResponse {
        // Ensure server is running
        if let Err(e) = self.ensure_server_ready() {
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

        // Connect to server and make the request
        let mut client = match ClientSocket::connect(&self.socket_path) {
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

        match client.request(&request) {
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
}

/// Run the MCP server, reading from stdin and writing to stdout
pub fn run_mcp_server(
    root_dir: String,
    threads: u8,
    config_path: Option<String>,
    plugins: Vec<Arc<dyn CustomHook>>,
    header: String,
) -> io::Result<()> {
    let mut server = McpServer::new(root_dir, threads, config_path, plugins, header);

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let error_response =
                    JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                let response_json = serde_json::to_string(&error_response)?;
                writeln!(stdout, "{}", response_json)?;
                stdout.flush()?;
                continue;
            }
        };

        // Handle shutdown specially
        if request.method == "shutdown" {
            let response = server.handle_request(request);
            let response_json = serde_json::to_string(&response)?;
            writeln!(stdout, "{}", response_json)?;
            stdout.flush()?;
            break;
        }

        let response = server.handle_request(request);
        let response_json = serde_json::to_string(&response)?;
        writeln!(stdout, "{}", response_json)?;
        stdout.flush()?;
    }

    eprintln!("Hakana MCP server shutting down");
    Ok(())
}
