use serde::{Deserialize, Serialize};

use crate::{
    VarId, attribute_info::AttributeInfo, code_location::HPos, issue::IssueKind, t_atomic::TAtomic,
    t_union::TUnion, taint::SinkType,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct UnresolvedConstantComponent {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub enum DefaultType {
    NormalData(TAtomic),
    Unresolved(UnresolvedConstantComponent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct FunctionLikeParameter {
    pub name: VarId,

    pub is_inout: bool,

    pub signature_type: Option<TUnion>,

    pub is_optional: bool,

    pub is_nullable: bool,

    // Most parameters have no default; keep the large atomic type out of line.
    pub default_type: Option<Box<DefaultType>>,
    pub default_value_location: Option<HPos>,

    pub location: HPos,

    pub name_location: HPos,

    pub signature_type_location: Option<HPos>,

    pub is_variadic: bool,

    pub is_splat: bool,

    pub is_named: bool,

    pub taint_sinks: Option<Vec<SinkType>>,
    pub removed_taints: Vec<SinkType>,

    pub removed_taints_when_returning_true: Option<Vec<SinkType>>,

    pub propagate_taint: bool,

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
            default_value_location: None,
            location,
            name_location,
            signature_type_location: None,
            is_variadic: false,
            is_splat: false,
            is_named: false,
            taint_sinks: None,
            removed_taints: vec![],
            propagate_taint: false,
            assert_untainted: false,
            type_inferred: false,
            expect_variable: false,
            promoted_property: false,
            attributes: Vec::new(),
            removed_taints_when_returning_true: None,
            suppressed_issues: None,
        }
    }

    pub fn is_required(&self) -> bool {
        !(self.is_optional || self.is_splat || self.is_variadic || self.is_named)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct FnParameter {
    pub signature_type: Option<Box<TUnion>>,
    pub is_inout: bool,
    pub is_variadic: bool,
    pub is_optional: bool,
}
