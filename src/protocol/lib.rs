//! Protocol module for hakana client-server communication.
//!
//! This module defines the binary protocol used for communication between
//! the hakana server and its clients (CLI and LSP).

mod types;
mod serialize;
mod socket;

pub use types::*;
pub use serialize::{Serialize, Deserialize, ProtocolError, encode_message, decode_message, read_message, write_message};
pub use socket::{ServerSocket, ClientSocket, SocketPath, ClientConnection};
