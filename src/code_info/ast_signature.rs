use serde::{Deserialize, Serialize};

use crate::StrId;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DefSignatureNode {
    pub name: StrId,

    pub is_function: bool,
    pub is_constant: bool,

    pub start_offset: u32,
    pub end_offset: u32,
    pub start_line: u32,
    pub end_line: u32,

    pub children: Vec<DefSignatureNode>,

    pub signature_hash: u64,
    pub body_hash: Option<u64>,
}
