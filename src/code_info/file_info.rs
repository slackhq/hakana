use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::ast_signature::DefSignatureNode;
use crate::functionlike_info::FunctionLikeInfo;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileInfo {
    pub ast_nodes: Vec<DefSignatureNode>,
    pub closure_infos: FxHashMap<usize, FunctionLikeInfo>,
}

impl FileInfo {
    pub fn new() -> Self {
        Self {
            closure_infos: FxHashMap::default(),
            ast_nodes: vec![],
        }
    }
}
