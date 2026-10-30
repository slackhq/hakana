use serde::{Deserialize, Serialize};

use crate::{ast_signature::DefSignatureNode, code_location::HPos};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub enum ParserError {
    CannotReadFile,
    NotAHackFile,
    SyntaxError { message: String, pos: HPos },
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct FileInfo {
    pub ast_nodes: Vec<DefSignatureNode>,
    pub closure_refs: Vec<u32>,
    pub parser_errors: Vec<ParserError>,
}
