use std::fmt::Display;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use super::jsonrpc::{self, Error};
use super::types::{
    Diagnostic, LogMessageParams, MessageType, PublishDiagnosticsParams, Registration,
    RegistrationParams, Url,
};

/// A message from the server to the client, written out by the server loop.
pub(crate) enum Outgoing {
    Notification {
        method: &'static str,
        params: Value,
    },
    Request {
        method: &'static str,
        params: Value,
        response_tx: oneshot::Sender<jsonrpc::Result<Value>>,
    },
}

/// Handle for sending notifications and requests to the client.
#[derive(Debug, Clone)]
pub struct Client {
    outgoing_tx: mpsc::UnboundedSender<Outgoing>,
}

/// The receiving end of all messages sent through [`Client`] handles.
pub struct ClientSocket {
    pub(crate) outgoing_rx: mpsc::UnboundedReceiver<Outgoing>,
}

impl Client {
    pub(crate) fn new() -> (Self, ClientSocket) {
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        (Self { outgoing_tx }, ClientSocket { outgoing_rx })
    }

    pub fn log_message(&self, typ: MessageType, message: impl Display) {
        self.send_notification(
            "window/logMessage",
            LogMessageParams {
                typ,
                message: message.to_string(),
            },
        );
    }

    pub fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.send_notification(
            "textDocument/publishDiagnostics",
            PublishDiagnosticsParams { uri, diagnostics },
        );
    }

    pub async fn register_capability(
        &self,
        registrations: Vec<Registration>,
    ) -> jsonrpc::Result<()> {
        self.send_request(
            "client/registerCapability",
            RegistrationParams { registrations },
        )
        .await
        .map(|_| ())
    }

    /// Messages sent after the server loop has stopped are dropped.
    fn send_notification(&self, method: &'static str, params: impl Serialize) {
        let _ = self.outgoing_tx.send(Outgoing::Notification {
            method,
            params: to_value(params),
        });
    }

    async fn send_request(
        &self,
        method: &'static str,
        params: impl Serialize,
    ) -> jsonrpc::Result<Value> {
        let (response_tx, response_rx) = oneshot::channel();
        let _ = self.outgoing_tx.send(Outgoing::Request {
            method,
            params: to_value(params),
            response_tx,
        });

        response_rx
            .await
            .unwrap_or_else(|_| Err(Error::internal_error("Connection to client closed")))
    }
}

fn to_value(params: impl Serialize) -> Value {
    serde_json::to_value(params).expect("LSP params should always serialize to JSON")
}
