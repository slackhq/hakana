use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::taint::SinkType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum PathExpressionKind {
    ArrayKey,
    ArrayValue,
    Property,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum PathKind {
    Default,
    UnknownExpressionFetch(PathExpressionKind),
    UnknownExpressionAssignment(PathExpressionKind),
    ExpressionFetch(PathExpressionKind, String),
    ExpressionAssignment(PathExpressionKind, String),
    RemoveDictKey(String),
    ScalarTypeGuard,
}

#[derive(Debug, Clone)]
pub struct DataFlowPath {
    pub kind: PathKind,
    pub added_taints: Option<FxHashSet<SinkType>>,
    pub removed_taints: Option<FxHashSet<SinkType>>,
}
