use serde::{Deserialize, Serialize};

use hakana_str::StrId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct AttributeInfo {
    pub name: StrId,
}
