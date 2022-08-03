use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    attribute_info::AttributeInfo, code_location::HPos, t_union::TUnion, taint::TaintType,
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
    pub name: String,

    pub is_inout: bool,

    pub signature_type: Option<TUnion>,

    pub is_optional: bool,

    pub is_nullable: bool,

    pub default_type: Option<DefaultType>,

    pub location: Option<HPos>,

    pub signature_type_location: Option<HPos>,

    pub is_variadic: bool,

    pub taint_sinks: Option<HashSet<TaintType>>,

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
}

impl FunctionLikeParameter {
    pub fn new(name: String) -> Self {
        Self {
            name,
            is_inout: false,
            signature_type: None,
            is_optional: false,
            is_nullable: false,
            default_type: None,
            location: None,
            signature_type_location: None,
            is_variadic: false,
            taint_sinks: None,
            assert_untainted: false,
            type_inferred: false,
            expect_variable: false,
            promoted_property: false,
            attributes: Vec::new(),
        }
    }

    pub fn get_id(&self) -> String {
        let mut str = String::new();

        if let Some(t) = &self.signature_type {
            str += t.get_id().as_str();
        } else {
            str += "mixed";
        }

        str += if self.is_variadic { "..." } else { "" };
        str += if self.is_optional { "=" } else { "" };
        return str;
    }
}
