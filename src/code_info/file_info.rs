use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{ast_signature::DefSignatureNode, functionlike_info::FunctionLikeInfo};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileInfo {
    pub ast_nodes: Vec<DefSignatureNode>,
    pub closure_infos: FxHashMap<usize, FunctionLikeInfo>,
}
