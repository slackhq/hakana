use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub enum MemberVisibility {
    Public,
    Protected,
    Private,
}
