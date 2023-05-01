use serde::{Deserialize, Serialize};

use crate::StrId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeInfo {
    pub name: StrId,
}
