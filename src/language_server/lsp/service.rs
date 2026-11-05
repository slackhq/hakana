use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tower::Service;

use super::client::{Client, ClientSocket};
use super::jsonrpc::{Error, Request, Response, Result};
use super::types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, GotoDefinitionParams,
    InitializeResult, Location, MessageType,
};

/// Handlers for the LSP methods Hakana supports.
///
/// Handlers run one at a time, in the order the client sent the messages, so they
/// get exclusive access to the server state.
pub trait LanguageServer: Send + 'static {
    fn initialize(&mut self) -> impl Future<Output = Result<InitializeResult>> + Send;

    fn initialized(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }

    fn shutdown(&mut self) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    fn did_change(
        &mut self,
        params: DidChangeTextDocumentParams,
    ) -> impl Future<Output = ()> + Send {
        let _ = params;
        async {}
    }

    fn did_change_watched_files(
        &mut self,
        params: DidChangeWatchedFilesParams,
    ) -> impl Future<Output = ()> + Send {
        let _ = params;
        async {}
    }

    fn goto_definition(
        &mut self,
        params: GotoDefinitionParams,
    ) -> impl Future<Output = Result<Option<Location>>> + Send {
        let _ = params;
        async { Ok(None) }
    }
}

type Job = (Request, oneshot::Sender<Option<Response>>);

/// A [`Service`] that forwards requests to a worker task owning the [`LanguageServer`].
///
/// Resolves to `None` for notifications.
#[derive(Debug, Clone)]
pub struct LspService {
    jobs_tx: mpsc::UnboundedSender<Job>,
}

/// The worker task owning the language server has stopped, e.g. because a handler panicked.
#[derive(Debug)]
pub struct WorkerExited;

impl fmt::Display for WorkerExited {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Language server worker exited")
    }
}

impl std::error::Error for WorkerExited {}

impl LspService {
    pub fn new<S: LanguageServer>(init: impl FnOnce(Client) -> S) -> (Self, ClientSocket) {
        let (client, socket) = Client::new();
        let server = init(client.clone());
        let (jobs_tx, jobs_rx) = mpsc::unbounded_channel();

        tokio::spawn(run_worker(server, client, jobs_rx));

        (Self { jobs_tx }, socket)
    }
}

impl Service<Request> for LspService {
    type Response = Option<Response>;
    type Error = WorkerExited;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Option<Response>, WorkerExited>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<std::result::Result<(), WorkerExited>> {
        if self.jobs_tx.is_closed() {
            Poll::Ready(Err(WorkerExited))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let (response_tx, response_rx) = oneshot::channel();
        // If the worker is gone, the sender is dropped and the receiver reports it.
        let _ = self.jobs_tx.send((request, response_tx));

        Box::pin(async move { response_rx.await.map_err(|_| WorkerExited) })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Uninitialized,
    Initialized,
    ShutDown,
}

async fn run_worker<S: LanguageServer>(
    mut server: S,
    client: Client,
    mut jobs_rx: mpsc::UnboundedReceiver<Job>,
) {
    let mut state = State::Uninitialized;

    while let Some((request, response_tx)) = jobs_rx.recv().await {
        let response = match request.id {
            Some(id) => Some(Response::new(
                Some(id),
                handle_request(&mut server, &mut state, &request.method, request.params).await,
            )),
            None => {
                // Like other servers, ignore notifications outside the initialized state.
                if state == State::Initialized
                    && let Err(error) =
                        handle_notification(&mut server, &request.method, request.params).await
                {
                    client.log_message(
                        MessageType::ERROR,
                        format!("Failed to handle {}: {}", request.method, error),
                    );
                }
                None
            }
        };

        let _ = response_tx.send(response);
    }
}

async fn handle_request<S: LanguageServer>(
    server: &mut S,
    state: &mut State,
    method: &str,
    params: Value,
) -> Result<Value> {
    match (*state, method) {
        (State::Uninitialized, "initialize") => {
            let result = server.initialize().await?;
            *state = State::Initialized;
            to_value(result)
        }
        (State::Uninitialized, _) => Err(Error::server_not_initialized()),
        (State::Initialized, "initialize") => {
            Err(Error::invalid_request("Server is already initialized"))
        }
        (State::Initialized, "shutdown") => {
            *state = State::ShutDown;
            server.shutdown().await?;
            Ok(Value::Null)
        }
        (State::Initialized, "textDocument/definition") => {
            to_value(server.goto_definition(from_params(params)?).await?)
        }
        (State::Initialized, _) => Err(Error::method_not_found(method)),
        (State::ShutDown, _) => Err(Error::invalid_request("Server is shutting down")),
    }
}

async fn handle_notification<S: LanguageServer>(
    server: &mut S,
    method: &str,
    params: Value,
) -> Result<()> {
    match method {
        "initialized" => server.initialized().await,
        "textDocument/didChange" => server.did_change(from_params(params)?).await,
        "workspace/didChangeWatchedFiles" => {
            server.did_change_watched_files(from_params(params)?).await
        }
        // Includes `$/` notifications, which servers may ignore.
        _ => {}
    }

    Ok(())
}

fn from_params<P: DeserializeOwned>(params: Value) -> Result<P> {
    serde_json::from_value(params).map_err(|e| Error::invalid_params(e.to_string()))
}

fn to_value(result: impl Serialize) -> Result<Value> {
    serde_json::to_value(result).map_err(|e| Error::internal_error(e.to_string()))
}
