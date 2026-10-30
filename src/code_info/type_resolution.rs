use std::sync::{Arc, LazyLock};

use hakana_str::StrId;

use serde::{Deserialize, Serialize};

use crate::{GenericParent, t_union::TUnion};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
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
    /// Most declarations have no generic context. Share their empty value,
    /// while allowing population to update nonempty contexts with Arc::make_mut.
    pub fn into_shared(self) -> Arc<Self> {
        static EMPTY: LazyLock<Arc<TypeResolutionContext>> =
            LazyLock::new(|| Arc::new(TypeResolutionContext::new()));

        if self.template_type_map.is_empty() && self.template_supers.is_empty() {
            Arc::clone(&EMPTY)
        } else {
            Arc::new(self)
        }
    }

    pub fn deserialize_shared_option<'de, D>(deserializer: D) -> Result<Option<Arc<Self>>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Option::<Self>::deserialize(deserializer).map(|context| context.map(Self::into_shared))
    }

    pub fn new() -> Self {
        Self {
            template_type_map: vec![],
            template_supers: vec![],
        }
    }
}
