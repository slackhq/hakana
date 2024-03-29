use super::{
    node::{DataFlowNode, DataFlowNodeId, DataFlowNodeKind},
    path::PathKind,
};

use core::panic;
use std::{collections::BTreeSet, sync::Arc};

use hakana_str::Interner;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{
    code_location::{FilePath, HPos},
    taint::{self, SinkType, SourceType},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaintedNode {
    pub id: DataFlowNodeId,
    pub pos: Option<Arc<HPos>>,
    pub specialization_key: Option<(FilePath, u32)>,
    pub taint_sources: Vec<SourceType>,
    pub taint_sinks: Vec<SinkType>,
    pub previous: Option<Arc<TaintedNode>>,
    pub path_types: Vec<PathKind>,
    pub specialized_calls: FxHashMap<(FilePath, u32), FxHashSet<DataFlowNodeId>>,
}

impl TaintedNode {
    pub fn get_trace(&self, interner: &Interner, root_dir: &str) -> String {
        let mut source_descriptor = format!(
            "{}{}",
            self.id.to_label(interner),
            if let Some(pos) = &self.pos {
                format!(
                    " ({}:{}:{})",
                    &pos.file_path.get_relative_path(interner, root_dir),
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
                previous_source.get_trace(interner, root_dir),
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

    pub fn get_taint_sources(&self) -> &Vec<SourceType> {
        if let Some(previous_source) = &self.previous {
            return previous_source.get_taint_sources();
        }

        &self.taint_sources
    }

    pub fn from(node: &DataFlowNode) -> Self {
        match &node.kind {
            DataFlowNodeKind::Vertex {
                pos,
                specialization_key,
                ..
            } => TaintedNode {
                id: node.id.clone(),
                pos: pos.as_ref().map(|p| Arc::new(*p)),
                specialization_key: *specialization_key,
                taint_sinks: vec![],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
            DataFlowNodeKind::TaintSource { pos, types, .. } => {
                let mut sinks = vec![];

                for source_type in types {
                    sinks.extend(taint::get_sinks_for_sources(source_type));
                }

                TaintedNode {
                    id: node.id.clone(),
                    pos: pos.as_ref().map(|p| Arc::new(*p)),
                    specialization_key: None,
                    taint_sinks: sinks,
                    previous: None,
                    path_types: Vec::new(),
                    specialized_calls: FxHashMap::default(),
                    taint_sources: types.clone(),
                }
            }
            DataFlowNodeKind::TaintSink { pos, types, .. } => TaintedNode {
                id: node.id.clone(),
                pos: pos.as_ref().map(|p| Arc::new(*p)),
                specialization_key: None,
                taint_sinks: types.clone(),
                taint_sources: vec![],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
            },
            DataFlowNodeKind::DataSource { pos, target_id, .. } => TaintedNode {
                id: node.id.clone(),
                pos: Some(Arc::new(*pos)),
                specialization_key: None,
                taint_sinks: vec![SinkType::Custom(target_id.clone())],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
            _ => {
                panic!();
            }
        }
    }

    pub fn get_unique_source_id(&self, interner: &Interner) -> String {
        let mut id = self.id.to_string(interner)
            + "|"
            + self
                .path_types
                .iter()
                .filter(|t| !matches!(t, PathKind::Default))
                .map(|k| k.to_unique_string())
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
            .map(|t| format!("{}:{}", t.0 .0 .0 .0, t.0 .1))
            .collect::<BTreeSet<_>>()
        {
            id += "-";
            id += &specialization;
        }

        id
    }
}
