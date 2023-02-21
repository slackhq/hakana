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
    RemoveDictKey(String),
    RefineSymbol(StrId),
    ScalarTypeGuard,
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct DataFlowPath {
    pub kind: PathKind,
    pub added_taints: Option<FxHashSet<SinkType>>,
    pub removed_taints: Option<FxHashSet<SinkType>>,
}
