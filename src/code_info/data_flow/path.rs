use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::{taint::SinkType, StrId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum ArrayDataKind {
    ArrayKey,
    ArrayValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathKind {
    Default,
    UnknownArrayFetch(ArrayDataKind),
    UnknownArrayAssignment(ArrayDataKind),
    ArrayFetch(ArrayDataKind, String),
    ArrayAssignment(ArrayDataKind, String),
    PropertyFetch(StrId, StrId),
    PropertyAssignment(StrId, StrId),
    UnknownPropertyFetch,
    UnknownPropertyAssignment,
    Serialize,
    RemoveDictKey(String),
    RefineSymbol(StrId),
    ScalarTypeGuard,
}

impl PathKind {
    pub fn to_unique_string(&self) -> String {
        match &self {
            PathKind::Default => "".to_string(),
            PathKind::UnknownArrayFetch(a) => {
                format!(
                    "array-{}-fetch",
                    match a {
                        ArrayDataKind::ArrayKey => "key",
                        ArrayDataKind::ArrayValue => "value",
                    }
                )
            }
            PathKind::ArrayFetch(a, b) => {
                format!(
                    "array-{}-fetch({})",
                    match a {
                        ArrayDataKind::ArrayKey => "key",
                        ArrayDataKind::ArrayValue => "value",
                    },
                    b
                )
            }
            PathKind::UnknownArrayAssignment(a) => {
                format!(
                    "array-{}-assignment",
                    match a {
                        ArrayDataKind::ArrayKey => "key",
                        ArrayDataKind::ArrayValue => "value",
                    }
                )
            }
            PathKind::ArrayAssignment(a, b) => {
                format!(
                    "array-{}-assignment({})",
                    match a {
                        ArrayDataKind::ArrayKey => "key",
                        ArrayDataKind::ArrayValue => "value",
                    },
                    b
                )
            }
            PathKind::UnknownPropertyFetch => "property-fetch".to_string(),
            PathKind::PropertyFetch(a, b) => {
                format!("property-fetch({},{})", a.0, b.0)
            }
            PathKind::UnknownPropertyAssignment => "property-assignment".to_string(),
            PathKind::PropertyAssignment(a, b) => {
                format!("property-assignment({},{})", a.0, b.0)
            }
            PathKind::RemoveDictKey(_) => "remove-dict-key".to_string(),
            PathKind::RefineSymbol(_) => "refine-symbol".to_string(),
            PathKind::ScalarTypeGuard => "scalar-type-guard".to_string(),
            PathKind::Serialize => "serialize".to_string(),
        }
    }
}

impl std::fmt::Display for PathKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self {
            PathKind::Default => std::fmt::Result::Ok(()),
            PathKind::UnknownArrayFetch(_) | PathKind::ArrayFetch(_, _) => {
                write!(f, "array-fetch")
            }
            PathKind::UnknownArrayAssignment(_) | PathKind::ArrayAssignment(_, _) => {
                write!(f, "array-assignment")
            }
            PathKind::PropertyFetch(_, _) | PathKind::UnknownPropertyFetch => {
                write!(f, "property-fetch")
            }
            PathKind::PropertyAssignment(_, _) | PathKind::UnknownPropertyAssignment => {
                write!(f, "property-assignment")
            }
            PathKind::RemoveDictKey(_) => write!(f, "remove-dict-key"),
            PathKind::RefineSymbol(_) => write!(f, "refine-symbol"),
            PathKind::ScalarTypeGuard => write!(f, "scalar-type-guard"),
            PathKind::Serialize => write!(f, "serialize"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DataFlowPath {
    pub kind: PathKind,
    pub added_taints: Option<FxHashSet<SinkType>>,
    pub removed_taints: Option<FxHashSet<SinkType>>,
}
