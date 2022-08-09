use super::{
    node::DataFlowNode,
    path::{DataFlowPath, PathExpressionKind, PathKind},
};
use crate::taint::TaintType;
use oxidized::ast_defs::Pos;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphKind {
    Variable,
    Taint,
}

#[derive(Debug, Clone)]
pub struct DataFlowGraph {
    pub kind: GraphKind,
    pub nodes: FxHashMap<String, DataFlowNode>,
    pub forward_edges: FxHashMap<String, FxHashMap<String, DataFlowPath>>,
    pub backward_edges: FxHashMap<String, FxHashSet<String>>,
    pub sources: FxHashMap<String, DataFlowNode>,
    pub sinks: FxHashMap<String, DataFlowNode>,
    pub mixed_source_counts: FxHashMap<String, FxHashSet<String>>,
    pub specializations: FxHashMap<String, FxHashSet<String>>,
    specialized_calls: FxHashMap<String, FxHashSet<String>>,
}

impl DataFlowGraph {
    pub fn new(kind: GraphKind) -> Self {
        Self {
            kind,
            nodes: FxHashMap::default(),
            forward_edges: FxHashMap::default(),
            backward_edges: FxHashMap::default(),
            sources: FxHashMap::default(),
            sinks: FxHashMap::default(),
            mixed_source_counts: FxHashMap::default(),
            specializations: FxHashMap::default(),
            specialized_calls: FxHashMap::default(),
        }
    }

    pub fn add_source(&mut self, node: DataFlowNode) {
        self.nodes.insert(node.id.clone(), node.clone());
        self.sources.insert(node.id.clone(), node);
    }

    pub fn add_sink(&mut self, node: DataFlowNode) {
        self.nodes.insert(node.id.clone(), node.clone());
        self.sinks.insert(node.id.clone(), node);
    }

    pub fn add_node(&mut self, node: DataFlowNode) {
        if self.kind == GraphKind::Taint {
            if let (Some(unspecialized_id), Some(specialization_key)) =
                (&node.unspecialized_id, &node.specialization_key)
            {
                self.specializations
                    .entry(unspecialized_id.clone())
                    .or_insert_with(FxHashSet::default)
                    .insert(specialization_key.clone());

                self.specialized_calls
                    .entry(specialization_key.clone())
                    .or_insert_with(FxHashSet::default)
                    .insert(unspecialized_id.clone());
            }
        }

        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_path(
        &mut self,
        from: &DataFlowNode,
        to: &DataFlowNode,
        path_kind: PathKind,
        added_taints: Option<FxHashSet<TaintType>>,
        removed_taints: Option<FxHashSet<TaintType>>,
    ) {
        if matches!(
            path_kind,
            PathKind::UnknownExpressionFetch(PathExpressionKind::ArrayKey)
        ) {
            return;
        }
        let from_id = &from.id;
        let to_id = &to.id;

        if from_id == to_id && !matches!(path_kind, PathKind::Inout) {
            return;
        }

        if let GraphKind::Variable = self.kind {
            self.backward_edges
                .entry(to_id.clone())
                .or_insert_with(FxHashSet::default)
                .insert(from_id.clone());
        }

        self.forward_edges
            .entry(from_id.clone())
            .or_insert_with(FxHashMap::default)
            .insert(
                to_id.clone(),
                DataFlowPath {
                    kind: path_kind,
                    added_taints,
                    removed_taints,
                },
            );
    }

    pub fn add_graph(&mut self, graph: DataFlowGraph) {
        if self.kind != graph.kind {
            panic!("Graph kinds are different");
        }

        for (key, edges) in graph.forward_edges {
            self.forward_edges
                .entry(key)
                .or_insert_with(FxHashMap::default)
                .extend(edges);
        }

        if self.kind == GraphKind::Variable {
            for (key, edges) in graph.backward_edges {
                self.backward_edges
                    .entry(key)
                    .or_insert_with(FxHashSet::default)
                    .extend(edges);
            }
            for (key, count) in graph.mixed_source_counts {
                if let Some(existing_count) = self.mixed_source_counts.get_mut(&key) {
                    existing_count.extend(count);
                } else {
                    self.mixed_source_counts.insert(key, count);
                }
            }
        } else {
            for (key, specializations) in graph.specializations {
                self.specializations
                    .entry(key)
                    .or_insert_with(FxHashSet::default)
                    .extend(specializations);
            }
        }

        self.nodes.extend(graph.nodes);
        self.sources.extend(graph.sources);
        self.sinks.extend(graph.sinks);
    }

    pub fn get_origin_nodes(&self, assignment_node: &DataFlowNode) -> Vec<DataFlowNode> {
        let mut visited_child_ids = FxHashSet::default();

        let mut origin_nodes = vec![];

        let mut child_nodes = vec![assignment_node.clone()];

        for _ in 0..50 {
            let mut all_parent_nodes = vec![];

            for child_node in child_nodes {
                visited_child_ids.insert(child_node.id.clone());

                let mut new_parent_nodes = vec![];
                let mut has_visited_a_parent_already = false;

                if let Some(backward_edges) = self.backward_edges.get(&child_node.id) {
                    for from_id in backward_edges {
                        if let Some(node) = self.nodes.get(from_id) {
                            if !visited_child_ids.contains(from_id) {
                                new_parent_nodes.push(node.clone());
                            } else {
                                has_visited_a_parent_already = true;
                            }
                        }
                    }
                }

                if new_parent_nodes.len() == 0 {
                    if !has_visited_a_parent_already {
                        origin_nodes.push(child_node);
                    }
                } else {
                    new_parent_nodes.retain(|f| !visited_child_ids.contains(&f.id));
                    all_parent_nodes.extend(new_parent_nodes);
                }
            }

            child_nodes = all_parent_nodes;

            if child_nodes.len() == 0 {
                break;
            }
        }

        origin_nodes
    }

    pub fn add_mixed_data(&mut self, assignment_node: &DataFlowNode, pos: &Pos) {
        let origin_nodes = self.get_origin_nodes(assignment_node);

        for origin_node in origin_nodes {
            if origin_node.label.contains("()") {
                if let Some(entry) = self.mixed_source_counts.get_mut(&origin_node.id) {
                    entry.insert(pos.to_string());
                } else {
                    self.mixed_source_counts
                        .insert(origin_node.id, FxHashSet::from_iter([pos.to_string()]));
                }
            }
        }
    }
}
