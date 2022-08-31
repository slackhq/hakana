use std::sync::Arc;

use rustc_hash::FxHashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::t_union::TUnion;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeResolutionContext {
    pub template_type_map: IndexMap<String, FxHashMap<String, Arc<TUnion>>>,
    pub template_supers: FxHashMap<String, TUnion>,
}

impl TypeResolutionContext {
    pub fn new() -> Self {
        Self {
            template_type_map: IndexMap::new(),
            template_supers: FxHashMap::default(),
        }
    }
}
