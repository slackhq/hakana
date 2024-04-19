use serde::{Deserialize, Serialize};

use crate::member_visibility::MemberVisibility;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    pub is_static: bool,

    pub visibility: MemberVisibility,

    pub is_final: bool,

    pub is_abstract: bool,
}

impl Default for MethodInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl MethodInfo {
    pub fn new() -> Self {
        Self {
            is_static: false,
            visibility: MemberVisibility::Public,
            is_final: false,
            is_abstract: false,
        }
    }
}
