use std::fmt::Display;
use std::pin::pin;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use rustc_hash::FxHashMap;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::sync::{mpsc, oneshot};
use tower::{Service, ServiceExt};

use super::client::{ClientSocket, Outgoing};
use super::jsonrpc::{self, Error, Id, Message, Request, Response, read_message, write_message};

/// Drives a JSON-RPC service over a byte stream until the client sends `exit` or
/// closes its end of the stream.
pub struct Server<R, W> {
    reader: R,
    writer: W,
    socket: ClientSocket,
}

impl<R, W> Server<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite,
{
    pub fn new(reader: R, writer: W, socket: ClientSocket) -> Self {
        Self {
            reader,
            writer,
            socket,
        }
    }

    pub async fn serve<T>(self, mut service: T)
    where
        T: Service<Request, Response = Option<Response>>,
        T::Error: Display,
    {
        let Self {
            reader,
            writer,
            mut socket,
        } = self;
        let mut writer = pin!(writer);

        // Reading a framed message isn't cancel-safe, so read in a separate future
        // that is polled to completion rather than restarted on each loop iteration.
        let (incoming_tx, mut incoming_rx) = mpsc::unbounded_channel();
        let mut read_loop = pin!(async move {
            let mut reader = BufReader::new(reader);
            loop {
                match read_message(&mut reader).await {
                    Ok(Some(message)) => {
                        if incoming_tx.send(message).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        eprintln!("Failed to read LSP message: {error}");
                        break;
                    }
                }
            }
        });
        let mut read_loop_done = false;

        let mut in_flight = FuturesUnordered::new();
        let mut pending_client_requests: FxHashMap<Id, oneshot::Sender<jsonrpc::Result<Value>>> =
            FxHashMap::default();
        let mut next_request_id = 0;

        loop {
            let outgoing_message = tokio::select! {
                _ = &mut read_loop, if !read_loop_done => {
                    read_loop_done = true;
                    continue;
                }
                message = incoming_rx.recv() => {
                    let Some(message) = message else {
                        // The client closed the stream.
                        break;
                    };

                    match Message::parse(&message) {
                        Ok(Message::Request(request)) => {
                            if request.method == "exit" {
                                break;
                            }

                            let service = match service.ready().await {
                                Ok(service) => service,
                                Err(error) => {
                                    eprintln!("Language server stopped: {error}");
                                    break;
                                }
                            };

                            let id = request.id.clone();
                            let response = service.call(request);
                            in_flight.push(async move {
                                response.await.unwrap_or_else(|error| {
                                    id.map(|id| {
                                        Response::new(
                                            Some(id),
                                            Err(Error::internal_error(error.to_string())),
                                        )
                                    })
                                })
                            });
                            continue;
                        }
                        Ok(Message::Response(id, result)) => {
                            if let Some(response_tx) = pending_client_requests.remove(&id) {
                                let _ = response_tx.send(result);
                            }
                            continue;
                        }
                        Err(error) => serde_json::to_value(Response::new(None, Err(error))),
                    }
                }
                Some(response) = in_flight.next() => {
                    let Some(response) = response else {
                        continue;
                    };
                    serde_json::to_value(response)
                }
                Some(outgoing) = socket.outgoing_rx.recv() => match outgoing {
                    Outgoing::Notification { method, params } => Ok(json!({
                        "jsonrpc": "2.0",
                        "method": method,
                        "params": params,
                    })),
                    Outgoing::Request { method, params, response_tx } => {
                        let id = next_request_id;
                        next_request_id += 1;
                        pending_client_requests.insert(Id::Number(id), response_tx);
                        Ok(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": method,
                            "params": params,
                        }))
                    }
                },
            };

            let result = match outgoing_message {
                Ok(message) => write_message(&mut writer, &message).await,
                Err(error) => Err(error.into()),
            };

            if let Err(error) = result {
                eprintln!("Failed to write LSP message: {error}");
                break;
            }
        }
    }
}
