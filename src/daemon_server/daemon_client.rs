use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, RwLock, Mutex};
use tokio::time::timeout;

use crate::protocol::{Request, Response, Notification};

#[derive(Debug)]
pub struct DaemonClient {
    stream: Arc<Mutex<TcpStream>>,
    request_id_counter: Arc<Mutex<u64>>,
    pending_requests: Arc<RwLock<std::collections::HashMap<u64, oneshot::Sender<Response>>>>,
    notification_tx: mpsc::Sender<Notification>,
}

impl DaemonClient {
    pub async fn connect(addr: &str, client_name: &str) -> Result<(Self, mpsc::Receiver<Notification>), Box<dyn Error>> {
        let stream = TcpStream::connect(addr).await?;
        let stream = Arc::new(Mutex::new(stream));

        let (notification_tx, notification_rx) = mpsc::channel::<Notification>(100);

        let client = Self {
            stream: Arc::clone(&stream),
            request_id_counter: Arc::new(Mutex::new(0)),
            pending_requests: Arc::new(RwLock::new(std::collections::HashMap::new())),
            notification_tx,
        };

        // Start message handling task
        let stream_clone = Arc::clone(&stream);
        let pending_requests_clone = Arc::clone(&client.pending_requests);
        let notification_tx_clone = client.notification_tx.clone();

        tokio::spawn(async move {
            Self::handle_messages(stream_clone, pending_requests_clone, notification_tx_clone).await;
        });

        // Initialize connection
        client.initialize(client_name).await?;

        Ok((client, notification_rx))
    }

    async fn initialize(&self, client_name: &str) -> Result<(), Box<dyn Error>> {
        let params = json!({
            "clientInfo": {
                "name": client_name,
                "version": "0.1.0"
            }
        });

        let _response = self.send_request("initialize", Some(params)).await?;
        Ok(())
    }

    pub async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Response, Box<dyn Error>> {
        let id = {
            let mut counter = self.request_id_counter.lock().await;
            *counter += 1;
            *counter
        };

        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: json!(id),
            method: method.to_string(),
            params,
        };

        let (response_tx, response_rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(id, response_tx);
        }

        // Send request
        let request_json = serde_json::to_string(&request)?;
        let message = format!("Content-Length: {}\r\n\r\n{}", request_json.len(), request_json);

        {
            let mut stream = self.stream.lock().await;
            stream.write_all(message.as_bytes()).await?;
        }

        // Wait for response with timeout
        match timeout(Duration::from_secs(30), response_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err("Request cancelled".into()),
            Err(_) => Err("Request timeout".into()),
        }
    }

    async fn handle_messages(
        stream: Arc<Mutex<TcpStream>>,
        pending_requests: Arc<RwLock<std::collections::HashMap<u64, oneshot::Sender<Response>>>>,
        notification_tx: mpsc::Sender<Notification>,
    ) {
        let mut buffer = Vec::new();

        loop {
            let mut temp_buffer = [0; 4096];
            let n = match {
                let mut stream_guard = stream.lock().await;
                stream_guard.read(&mut temp_buffer).await
            } {
                Ok(0) => break, // Connection closed
                Ok(n) => n,
                Err(e) => {
                    eprintln!("Error reading from daemon: {}", e);
                    break;
                }
            };

            buffer.extend_from_slice(&temp_buffer[..n]);

            // Process complete messages
            while let Some(message) = Self::extract_message(&mut buffer) {
                if let Ok(response) = serde_json::from_str::<Response>(&message) {
                    // Handle response
                    if let Some(id) = response.id.as_u64() {
                        let mut pending = pending_requests.write().await;
                        if let Some(sender) = pending.remove(&id) {
                            let _ = sender.send(response);
                        }
                    }
                } else if let Ok(notification) = serde_json::from_str::<Notification>(&message) {
                    // Handle notification
                    if let Err(e) = notification_tx.send(notification).await {
                        eprintln!("Failed to forward notification: {}", e);
                        break;
                    }
                }
            }
        }
    }

    fn extract_message(buffer: &mut Vec<u8>) -> Option<String> {
        let buffer_str = String::from_utf8_lossy(buffer);
        if let Some(header_end) = buffer_str.find("\r\n\r\n") {
            let header = &buffer_str[..header_end];
            if let Some(content_length_line) = header.lines().find(|line| line.starts_with("Content-Length:")) {
                if let Ok(length) = content_length_line[15..].trim().parse::<usize>() {
                    let message_start = header_end + 4;
                    if buffer.len() >= message_start + length {
                        let message = String::from_utf8_lossy(&buffer[message_start..message_start + length]).to_string();
                        buffer.drain(..message_start + length);
                        return Some(message);
                    }
                }
            }
        }
        None
    }

    pub async fn get_definition(&self, uri: &str, line: u32, character: u32) -> Result<Value, Box<dyn Error>> {
        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });

        let response = self.send_request("textDocument/definition", Some(params)).await?;
        Ok(response.result.unwrap_or(json!(null)))
    }

    pub async fn get_hover(&self, uri: &str, line: u32, character: u32) -> Result<Value, Box<dyn Error>> {
        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });

        let response = self.send_request("textDocument/hover", Some(params)).await?;
        Ok(response.result.unwrap_or(json!(null)))
    }

    pub async fn get_references(&self, uri: &str, line: u32, character: u32, include_declaration: bool) -> Result<Value, Box<dyn Error>> {
        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": include_declaration }
        });

        let response = self.send_request("textDocument/references", Some(params)).await?;
        Ok(response.result.unwrap_or(json!([])))
    }

    pub async fn search_symbols(&self, query: &str) -> Result<Value, Box<dyn Error>> {
        let params = json!({
            "query": query
        });

        let response = self.send_request("workspace/symbol", Some(params)).await?;
        Ok(response.result.unwrap_or(json!([])))
    }

    pub async fn get_diagnostics(&self, uri: &str) -> Result<Value, Box<dyn Error>> {
        let params = json!({
            "textDocument": { "uri": uri }
        });

        let response = self.send_request("textDocument/diagnostics", Some(params)).await?;
        Ok(response.result.unwrap_or(json!([])))
    }
}