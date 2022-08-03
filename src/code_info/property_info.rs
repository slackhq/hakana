use serde::{Deserialize, Serialize};

use crate::{code_location::HPos, member_visibility::MemberVisibility, t_union::TUnion};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyInfo {
    pub is_static: bool,

    pub visibility: MemberVisibility,

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
}
