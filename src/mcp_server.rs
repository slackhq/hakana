
use hakana_daemon_server::mcp_client::{McpClient, McpRequest, McpResponse};
use mimalloc::MiMalloc;
use serde_json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mcp_client = McpClient::new().await?;

    // Handle stdin/stdout communication
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();

    let mut reader = AsyncBufReader::new(stdin);
    let mut line = String::new();

    stderr.write_all(b"Hakana MCP server started\n").await?;

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                match serde_json::from_str::<McpRequest>(trimmed) {
                    Ok(request) => {
                        let response = mcp_client.handle_request(request).await;
                        if let Ok(response_json) = serde_json::to_string(&response) {
                            stdout.write_all(response_json.as_bytes()).await?;
                            stdout.write_all(b"\n").await?;
                            stdout.flush().await?;
                        }
                    }
                    Err(e) => {
                        stderr.write_all(format!("Error parsing request: {}\n", e).as_bytes()).await?;

                        let error_response = McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: serde_json::Value::Null,
                            result: None,
                            error: Some(serde_json::json!({
                                "code": -32700,
                                "message": "Parse error"
                            })),
                        };

                        if let Ok(response_json) = serde_json::to_string(&error_response) {
                            stdout.write_all(response_json.as_bytes()).await?;
                            stdout.write_all(b"\n").await?;
                            stdout.flush().await?;
                        }
                    }
                }
            }
            Err(e) => {
                stderr.write_all(format!("Error reading input: {}\n", e).as_bytes()).await?;
                break;
            }
        }
    }

    stderr.write_all(b"Hakana MCP server shutting down\n").await?;
    Ok(())
}