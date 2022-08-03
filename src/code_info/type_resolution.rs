use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::t_union::TUnion;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeResolutionContext {
    pub template_type_map: IndexMap<String, HashMap<String, TUnion>>,
    pub template_supers: HashMap<String, TUnion>,
}

impl TypeResolutionContext {
    pub fn new() -> Self {
        Self {
            template_type_map: IndexMap::new(),
            template_supers: HashMap::new(),
        }
    }
}
