use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::taint::TaintType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum PathExpressionKind {
    ArrayKey,
    ArrayValue,
    Property,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum PathKind {
    Default,
    Inout,
    UnknownExpressionFetch(PathExpressionKind),
    UnknownExpressionAssignment(PathExpressionKind),
    ExpressionFetch(PathExpressionKind, String),
    ExpressionAssignment(PathExpressionKind, String),
    RemoveDictKey(String),
}

#[derive(Debug, Clone)]
pub struct DataFlowPath {
    pub kind: PathKind,
    pub added_taints: Option<HashSet<TaintType>>,
    pub removed_taints: Option<HashSet<TaintType>>,
}
