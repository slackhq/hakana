//! JSON-RPC 2.0 messages and the LSP base protocol framing (`Content-Length` headers).

use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Error {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(-32700, message)
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(-32600, message)
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("Method not found: {method}"))
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }

    pub fn server_not_initialized() -> Self {
        Self::new(-32002, "Server not initialized")
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)
    }
}

impl std::error::Error for Error {}

/// An incoming request, or a notification if `id` is `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: Option<Id>,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Response {
    jsonrpc: &'static str,
    /// `None` only for errors about messages whose id couldn't be determined.
    id: Option<Id>,
    #[serde(flatten)]
    payload: ResponsePayload,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ResponsePayload {
    Result(Value),
    Error(Error),
}

impl Response {
    pub fn new(id: Option<Id>, result: Result<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            payload: match result {
                Ok(value) => ResponsePayload::Result(value),
                Err(error) => ResponsePayload::Error(error),
            },
        }
    }
}

/// A message received from the client.
#[derive(Debug, PartialEq)]
pub(crate) enum Message {
    Request(Request),
    /// The client's response to a request sent by the server.
    Response(Id, Result<Value>),
}

impl Message {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        #[derive(Deserialize)]
        struct RawMessage {
            id: Option<Id>,
            method: Option<String>,
            #[serde(default)]
            params: Value,
            result: Option<Value>,
            error: Option<Error>,
        }

        let value: Value =
            serde_json::from_slice(bytes).map_err(|e| Error::parse_error(e.to_string()))?;
        let raw: RawMessage =
            serde_json::from_value(value).map_err(|e| Error::invalid_request(e.to_string()))?;

        match (raw.method, raw.id) {
            (Some(method), id) => Ok(Self::Request(Request {
                id,
                method,
                params: raw.params,
            })),
            (None, Some(id)) => Ok(Self::Response(
                id,
                match raw.error {
                    Some(error) => Err(error),
                    None => Ok(raw.result.unwrap_or(Value::Null)),
                },
            )),
            (None, None) => Err(Error::invalid_request("Message has neither method nor id")),
        }
    }
}

/// Read the body of the next framed message, or `None` at end of input.
pub(crate) async fn read_message<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }

        let header = line.trim_end();
        if header.is_empty() {
            if content_length.is_some() {
                break;
            }
            continue;
        }

        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            content_length = Some(value.trim().parse::<usize>().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid Content-Length: {e}"),
                )
            })?);
        }
    }

    let mut body = vec![0; content_length.unwrap_or_default()];
    reader.read_exact(&mut body).await?;

    Ok(Some(body))
}

pub(crate) async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &impl Serialize,
) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    writer.write_all(&body).await?;
    writer.flush().await
}
