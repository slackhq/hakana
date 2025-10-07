use std::sync::Arc;

use hakana_str::StrId;

use serde::{Deserialize, Serialize};

use crate::{GenericParent, t_union::TUnion};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeResolutionContext {
    pub template_type_map: Vec<(StrId, Vec<(GenericParent, Arc<TUnion>)>)>,
    pub template_supers: Vec<(StrId, TUnion)>,
}

impl Default for TypeResolutionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeResolutionContext {
    pub fn new() -> Self {
        Self {
            template_type_map: vec![],
            template_supers: vec![],
        }
    }
}
