use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::{taint::SinkType, StrId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum ArrayDataKind {
    ArrayKey,
    ArrayValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
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

#[derive(Debug, Clone)]
pub struct DataFlowPath {
    pub kind: PathKind,
    pub added_taints: Option<FxHashSet<SinkType>>,
    pub removed_taints: Option<FxHashSet<SinkType>>,
}
