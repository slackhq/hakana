use serde::{Deserialize, Serialize};

use crate::{
    attribute_info::AttributeInfo, code_location::HPos, issue::IssueKind, t_union::TUnion,
    taint::SinkType, VarId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedConstantComponent {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum DefaultType {
    NormalData(TUnion),
    Unresolved(UnresolvedConstantComponent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionLikeParameter {
    pub name: VarId,

    pub is_inout: bool,

    pub signature_type: Option<TUnion>,

    pub is_optional: bool,

    pub is_nullable: bool,

    pub default_type: Option<DefaultType>,

    pub location: HPos,

    pub name_location: HPos,

    pub signature_type_location: Option<HPos>,

    pub is_variadic: bool,

    pub taint_sinks: Option<Vec<SinkType>>,

    pub removed_taints_when_returning_true: Option<Vec<SinkType>>,

    pub assert_untainted: bool,

    /**
     * Was the type inferred in a closure (e.g. one passed to Vec\Map)
     */
    pub type_inferred: bool,

    /**
     * Warn if passed an explicit value
     */
    pub expect_variable: bool,

    pub promoted_property: bool,

    pub attributes: Vec<AttributeInfo>,

    pub suppressed_issues: Option<Vec<(IssueKind, HPos)>>,
}

impl FunctionLikeParameter {
    pub fn new(name: VarId, location: HPos, name_location: HPos) -> Self {
        Self {
            name,
            is_inout: false,
            signature_type: None,
            is_optional: false,
            is_nullable: false,
            default_type: None,
            location,
            name_location,
            signature_type_location: None,
            is_variadic: false,
            taint_sinks: None,
            assert_untainted: false,
            type_inferred: false,
            expect_variable: false,
            promoted_property: false,
            attributes: Vec::new(),
            removed_taints_when_returning_true: None,
            suppressed_issues: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct FnParameter {
    pub signature_type: Option<Box<TUnion>>,
    pub is_inout: bool,
    pub is_variadic: bool,
    pub is_optional: bool,
}
