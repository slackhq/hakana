//! MCP (Model Context Protocol) server for Hakana.
//!
//! This module provides an MCP server that allows LLMs to query the Hakana codebase
//! for symbol usages (find-references). The server communicates via stdio using
//! JSON-RPC messages.
//!
//! The server integrates with the existing hakana server infrastructure to maintain
//! warm codebase state.

mod protocol;
mod tools;

pub use protocol::{McpServer, run_mcp_server};
