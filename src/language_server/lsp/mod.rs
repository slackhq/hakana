//! A minimal Language Server Protocol implementation built on [`tower::Service`].

mod client;
pub mod jsonrpc;
mod server;
mod service;
pub mod types;

pub use client::{Client, ClientSocket};
pub use server::Server;
pub use service::{LanguageServer, LspService, WorkerExited};

#[cfg(test)]
mod tests;
