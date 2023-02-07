use serde::{Deserialize, Serialize};

use crate::StrId;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DefSignatureNode {
    pub name: StrId,

    pub is_function: bool,
    pub is_constant: bool,

    pub start_offset: usize,
    pub end_offset: usize,
    pub start_line: usize,
    pub end_line: usize,

    pub children: Vec<DefSignatureNode>,

    pub signature_hash: u64,
    pub body_hash: Option<u64>,
}
