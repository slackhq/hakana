use serde::{Deserialize, Serialize};

use crate::ast_signature::DefSignatureNode;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct FileInfo {
    pub ast_nodes: Vec<DefSignatureNode>,
    pub closure_refs: Vec<u32>,
    pub valid_file: bool,
}
