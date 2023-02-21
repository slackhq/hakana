use super::{
    node::{DataFlowNode, DataFlowNodeKind},
    path::PathKind,
};

use core::panic;
use std::{collections::BTreeSet, sync::Arc};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{
    code_location::HPos,
    taint::{self, SinkType, SourceType},
    Interner,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaintedNode {
    pub id: String,
    pub unspecialized_id: Option<String>,
    pub label: String,
    pub pos: Option<Arc<HPos>>,
    pub specialization_key: Option<String>,
    pub taint_sources: FxHashSet<SourceType>,
    pub taint_sinks: FxHashSet<SinkType>,
    pub previous: Option<Arc<TaintedNode>>,
    pub path_types: Vec<PathKind>,
    pub specialized_calls: FxHashMap<String, FxHashSet<String>>,
}

impl TaintedNode {
    pub fn get_trace(&self, interner: &Interner) -> String {
        let mut source_descriptor = format!(
            "{}{}",
            self.label,
            if let Some(pos) = &self.pos {
                format!(
                    " ({}:{}:{})",
                    interner.lookup(&pos.file_path),
                    pos.start_line,
                    pos.start_column
                )
            } else {
                "".to_string()
            }
        );

        if let Some(previous_source) = &self.previous {
            let path = self.path_types.iter().last();
            source_descriptor = format!(
                "{} {} {}",
                previous_source.get_trace(interner),
                if let Some(path) = path {
                    format!("--{}-->", path)
                } else {
                    "-->".to_string()
                },
                source_descriptor
            );
        }

        source_descriptor
    }

    pub fn get_taint_sources(&self) -> &FxHashSet<SourceType> {
        if let Some(previous_source) = &self.previous {
            return previous_source.get_taint_sources();
        }

        return &self.taint_sources;
    }

    pub fn from(node: &DataFlowNode) -> Self {
        match &node.kind {
            DataFlowNodeKind::Vertex {
                pos,
                unspecialized_id,
                label,
                specialization_key,
            } => TaintedNode {
                id: node.id.clone(),
                unspecialized_id: unspecialized_id.clone(),
                label: label.clone(),
                pos: if let Some(p) = &pos {
                    Some(Arc::new(p.clone()))
                } else {
                    None
                },
                specialization_key: specialization_key.clone(),
                taint_sinks: FxHashSet::default(),
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: FxHashSet::default(),
            },
            DataFlowNodeKind::TaintSource { pos, label, types } => {
                let mut sinks = FxHashSet::default();

                for source_type in types {
                    sinks.extend(taint::get_sinks_for_sources(source_type));
                }

                TaintedNode {
                    id: node.id.clone(),
                    unspecialized_id: None,
                    label: label.clone(),
                    pos: if let Some(p) = &pos {
                        Some(Arc::new(p.clone()))
                    } else {
                        None
                    },
                    specialization_key: None,
                    taint_sinks: sinks,
                    previous: None,
                    path_types: Vec::new(),
                    specialized_calls: FxHashMap::default(),
                    taint_sources: types.clone(),
                }
            }
            DataFlowNodeKind::TaintSink { pos, label, types } => TaintedNode {
                id: node.id.clone(),
                unspecialized_id: None,
                label: label.clone(),
                pos: if let Some(p) = &pos {
                    Some(Arc::new(p.clone()))
                } else {
                    None
                },
                specialization_key: None,
                taint_sinks: types.clone(),
                taint_sources: FxHashSet::default(),
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
            },
            DataFlowNodeKind::DataSource {
                pos,
                label,
                target_id,
            } => TaintedNode {
                id: node.id.clone(),
                unspecialized_id: None,
                label: label.clone(),
                pos: Some(Arc::new(pos.clone())),
                specialization_key: None,
                taint_sinks: FxHashSet::from_iter([SinkType::Custom(target_id.clone())]),
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: FxHashSet::default(),
            },
            _ => {
                panic!();
            }
        }
    }

    pub fn get_unique_source_id(&self) -> String {
        let mut id = self.id.clone()
            + "|"
            + self
                .path_types
                .iter()
                .filter(|t| !matches!(t, PathKind::Default))
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join("-")
                .as_str()
            + "|";

        for taint_type in self
            .taint_sinks
            .iter()
            .map(|t| t.to_string())
            .collect::<BTreeSet<_>>()
        {
            id += "-";
            id += taint_type.as_str();
        }

        id += "|";

        for specialization in self
            .specialized_calls
            .iter()
            .map(|t| t.0.as_str())
            .collect::<BTreeSet<_>>()
        {
            id += "-";
            id += specialization;
        }

        id
    }
}
