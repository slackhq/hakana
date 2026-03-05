//! Protocol module for hakana client-server communication.
//!
//! This module defines the binary protocol used for communication between
//! the hakana server and its clients (CLI and LSP).

mod serialize;
mod socket;
mod types;

pub use serialize::{
    Deserialize, ProtocolError, Serialize, decode_message, encode_message, read_message,
    write_message,
};
pub use socket::{ClientConnection, ClientSocket, ServerSocket, SocketPath};
pub use types::*;
