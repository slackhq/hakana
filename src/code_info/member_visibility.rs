use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberVisibility {
    Public,
    Protected,
    Private,
}
