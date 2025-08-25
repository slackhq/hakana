use hakana_str::StrId;
use serde::{Deserialize, Serialize};

use crate::{
    code_location::HPos, functionlike_parameter::UnresolvedConstantComponent, issue::IssueKind,
    t_atomic::TAtomic, t_union::TUnion,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstantInfo {
    pub pos: HPos,

    pub type_pos: Option<HPos>,

    pub provided_type: Option<TUnion>,

    pub inferred_type: Option<TAtomic>,

    pub unresolved_value: Option<UnresolvedConstantComponent>,

    pub is_abstract: bool,

    pub allow_non_exclusive_enum_values: bool,

    pub suppressed_issues: Vec<(IssueKind, HPos)>,

    pub defining_class: StrId,
}
