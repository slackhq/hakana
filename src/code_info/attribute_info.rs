use serde::{Deserialize, Serialize};

use hakana_str::StrId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeInfo {
    pub name: StrId,
}
