use hakana_str::StrId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use strum_macros::Display;

use crate::method_identifier::MethodIdentifier;
use crate::taint::{SinkType, SourceType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodReceiver {
    pub callee: MethodIdentifier,
    pub caller: Option<(MethodIdentifier, std::sync::Arc<Vec<StrId>>)>,
    pub classes: Vec<StrId>,
}

pub fn method_receiver_bindings<'a>(
    paths: impl Iterator<Item = &'a PathKind>,
) -> Option<BTreeMap<MethodIdentifier, Vec<StrId>>> {
    let mut bindings = BTreeMap::<MethodIdentifier, Vec<StrId>>::new();
    for path in paths {
        if let PathKind::MethodReturn(receiver) = path {
            if receiver.caller.is_none() {
                if receiver.classes.is_empty()
                    || bindings.remove(&receiver.callee).is_some_and(|previous| {
                        !previous
                            .iter()
                            .any(|class| receiver.classes.binary_search(class).is_ok())
                    })
                {
                    return None;
                }
                continue;
            }
            if let Some((caller, possible_classes)) = &receiver.caller
                && receiver.classes == **possible_classes
                && !bindings.contains_key(&receiver.callee)
                && !bindings.contains_key(caller)
            {
                continue;
            }
            let mut classes = receiver.classes.clone();
            if let Some(previous) = bindings.remove(&receiver.callee) {
                classes.retain(|class| previous.binary_search(class).is_ok());
            }
            if let Some((caller, possible_classes)) = &receiver.caller {
                if let Some(previous) = bindings.remove(caller) {
                    classes.retain(|class| previous.binary_search(class).is_ok());
                }
                classes.retain(|class| possible_classes.binary_search(class).is_ok());
                if classes.is_empty() {
                    return None;
                }
                if classes != **possible_classes {
                    bindings.insert(*caller, classes);
                }
            }
        }
    }
    Some(bindings)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum ArrayDataKind {
    ArrayKey,
    ArrayValue,
}

/// Representation established by an encoder. This is independent of both the
/// Hack type and which security sinks a source can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueEncoding {
    HtmlSafeJson,
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
    Aggregate,
    Superglobal(String),
    Encode(ValueEncoding),
    StringComposition,
    /// Only the suffix edge: preserve taint already carried by the prefix.
    UriSuffix(super::uri_prefix::UriPrefix),
    /// A consumer contract on this edge, not a sanitizer of the returned value.
    AcceptHtmlSafeJson,
    StringTransform,
    MethodReturn(std::sync::Arc<MethodReceiver>),
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
            PathKind::Aggregate => "aggregate".to_string(),
            PathKind::Superglobal(name) => format!("superglobal({name})"),
            PathKind::Encode(ValueEncoding::HtmlSafeJson) => "html-safe-json".to_string(),
            PathKind::StringComposition => "string-composition".to_string(),
            PathKind::UriSuffix(prefix) => format!("uri-suffix({:?})", prefix.dependencies),
            PathKind::AcceptHtmlSafeJson => "accept-html-safe-json".to_string(),
            PathKind::StringTransform => "string-transform".to_string(),
            PathKind::MethodReturn(receiver) => format!("method-return({receiver:?})"),
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
            PathKind::Aggregate => write!(f, "aggregate"),
            PathKind::Superglobal(name) => write!(f, "superglobal({name})"),
            PathKind::Encode(ValueEncoding::HtmlSafeJson) => write!(f, "serialize"),
            PathKind::StringComposition
            | PathKind::UriSuffix(_)
            | PathKind::AcceptHtmlSafeJson
            | PathKind::StringTransform => Ok(()),
            PathKind::MethodReturn(_) => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFlowPath {
    pub kind: PathKind,
    pub added_taints: Vec<SinkType>,
    pub removed_taints: Vec<SinkType>,
    pub source_transforms: Vec<(SourceType, SourceType)>,
}
