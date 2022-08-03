use super::{
    node::{DataFlowNode, NodeKind},
    path::PathKind,
};

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    rc::Rc,
};

use serde::{Deserialize, Serialize};

use crate::{code_location::HPos, taint::TaintType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaintedNode {
    pub kind: NodeKind,
    pub id: String,
    pub unspecialized_id: Option<String>,
    pub label: String,
    pub pos: Option<Rc<HPos>>,
    pub specialization_key: Option<String>,
    pub taints: HashSet<TaintType>,
    pub previous: Option<Rc<TaintedNode>>,
    pub path_types: Vec<PathKind>,
    pub specialized_calls: HashMap<String, HashSet<String>>,
}

impl TaintedNode {
    pub fn get_trace(&self) -> String {
        let mut source_descriptor = format!(
            "{}{}",
            self.label,
            if let Some(pos) = &self.pos {
                format!(
                    " ({}:{}:{})",
                    pos.file_path, pos.start_line, pos.start_column
                )
            } else {
                "".to_string()
            }
        );

        if let Some(previous_source) = &self.previous {
            let path = self.path_types.iter().last();
            source_descriptor = format!(
                "{} {} {}",
                previous_source.get_trace(),
                if let Some(path) = path {
                    format!("--{:?}-->", path)
                } else {
                    "-->".to_string()
                },
                source_descriptor
            );
        }

        source_descriptor
    }

    pub fn from(node: &DataFlowNode) -> Self {
        TaintedNode {
            kind: node.kind.clone(),
            id: node.id.clone(),
            unspecialized_id: node.unspecialized_id.clone(),
            label: node.label.clone(),
            pos: if let Some(p) = &node.pos {
                Some(Rc::new(p.clone()))
            } else {
                None
            },
            specialization_key: node.specialization_key.clone(),
            taints: node.taints.clone().unwrap_or(HashSet::new()),
            previous: None,
            path_types: Vec::new(),
            specialized_calls: HashMap::new(),
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
            .taints
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
