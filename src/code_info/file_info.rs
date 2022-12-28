use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::functionlike_info::FunctionLikeInfo;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileInfo {
    pub closure_infos: FxHashMap<usize, FunctionLikeInfo>,
}

impl FileInfo {
    pub fn new() -> Self {
        Self {
            closure_infos: FxHashMap::default(),
        }
    }
}
