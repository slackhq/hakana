use std::sync::Arc;

use hakana_str::StrId;
use rustc_hash::FxHashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::t_union::TUnion;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeResolutionContext {
    pub template_type_map: IndexMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,
    pub template_supers: FxHashMap<StrId, TUnion>,
}

impl Default for TypeResolutionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeResolutionContext {
    pub fn new() -> Self {
        Self {
            template_type_map: IndexMap::new(),
            template_supers: FxHashMap::default(),
        }
    }
}
