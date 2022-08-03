use serde::{Deserialize, Serialize};

use crate::{
    code_location::HPos, functionlike_parameter::UnresolvedConstantComponent, t_union::TUnion,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstantInfo {
    pub pos: Option<HPos>,

    pub type_pos: Option<HPos>,

    pub provided_type: Option<TUnion>,

    pub inferred_type: Option<TUnion>,

    pub unresolved_value: Option<UnresolvedConstantComponent>,

    pub is_abstract: bool,
}
