use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{
    code_location::HPos, issue::IssueKind, member_visibility::MemberVisibility, t_union::TUnion,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub enum PropertyKind {
    Property,
    XhpAttribute { is_required: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct PropertyInfo {
    pub is_static: bool,

    pub visibility: MemberVisibility,

    pub kind: PropertyKind,

    pub pos: Option<HPos>,

    pub stmt_pos: Option<HPos>,

    pub type_pos: Option<HPos>,

    pub type_: TUnion,

    pub has_default: bool,

    // distinct from syntax-defined readonly properties, which require
    // different runtime handling
    pub soft_readonly: bool,

    pub is_promoted: bool,

    pub is_internal: bool,

    #[cfg_attr(feature = "memory-profile", allocative(visit = hakana_str::memory_profile::option_hash_map))]
    pub suppressed_issues: Option<FxHashMap<IssueKind, HPos>>,
}
