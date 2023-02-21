use super::{
    node::{DataFlowNode, DataFlowNodeKind},
    path::{DataFlowPath, ArrayDataKind, PathKind},
};
use crate::taint::SinkType;
use oxidized::ast_defs::Pos;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WholeProgramKind {
    Taint,
    Query,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphKind {
    FunctionBody,
    WholeProgram(WholeProgramKind),
}

#[derive(Debug, Clone)]
pub struct DataFlowGraph {
    pub kind: GraphKind,
    pub vertices: FxHashMap<String, DataFlowNode>,
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
            vertices: FxHashMap::default(),
            forward_edges: FxHashMap::default(),
            backward_edges: FxHashMap::default(),
            sources: FxHashMap::default(),
            sinks: FxHashMap::default(),
            mixed_source_counts: FxHashMap::default(),
            specializations: FxHashMap::default(),
            specialized_calls: FxHashMap::default(),
        }
    }

    pub fn add_node(&mut self, node: DataFlowNode) {
        match &node.kind {
            DataFlowNodeKind::Vertex {
                unspecialized_id,
                specialization_key,
                ..
            } => {
                if let GraphKind::WholeProgram(_) = &self.kind {
                    if let (Some(unspecialized_id), Some(specialization_key)) =
                        (&unspecialized_id, &specialization_key)
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

                self.vertices.insert(node.id.clone(), node);
            }
            DataFlowNodeKind::TaintSource { .. }
            | DataFlowNodeKind::VariableUseSource { .. }
            | DataFlowNodeKind::DataSource { .. }
            | DataFlowNodeKind::ForLoopInit { .. } => {
                self.sources.insert(node.id.clone(), node);
            }
            DataFlowNodeKind::TaintSink { .. } | DataFlowNodeKind::VariableUseSink { .. } => {
                self.sinks.insert(node.id.clone(), node);
            }
        };
    }

    pub fn add_path(
        &mut self,
        from: &DataFlowNode,
        to: &DataFlowNode,
        path_kind: PathKind,
        added_taints: Option<FxHashSet<SinkType>>,
        removed_taints: Option<FxHashSet<SinkType>>,
    ) {
        if matches!(
            path_kind,
            PathKind::UnknownArrayFetch(ArrayDataKind::ArrayKey)
        ) {
            return;
        }

        let from_id = from.get_id();
        let to_id = to.get_id();

        if from_id == to_id {
            return;
        }

        if let GraphKind::FunctionBody = self.kind {
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

        if self.kind == GraphKind::FunctionBody {
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

        self.vertices.extend(graph.vertices);
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
                visited_child_ids.insert(child_node.get_id().clone());

                let mut new_parent_nodes = FxHashSet::default();
                let mut has_visited_a_parent_already = false;

                if let Some(backward_edges) = self.backward_edges.get(child_node.get_id()) {
                    for from_id in backward_edges {
                        if let Some(node) = self.vertices.get(from_id) {
                            if !visited_child_ids.contains(from_id) {
                                new_parent_nodes.insert(node.clone());
                            } else {
                                has_visited_a_parent_already = true;
                            }
                        } else if let Some(node) = self.sources.get(from_id) {
                            origin_nodes.push(node.clone());
                        }
                    }
                }

                if new_parent_nodes.len() == 0 {
                    if !has_visited_a_parent_already {
                        origin_nodes.push(child_node);
                    }
                } else {
                    new_parent_nodes.retain(|f| !visited_child_ids.contains(f.get_id()));
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
            if origin_node.get_label().contains("()") {
                if let Some(entry) = self.mixed_source_counts.get_mut(origin_node.get_id()) {
                    entry.insert(pos.to_string());
                } else {
                    self.mixed_source_counts.insert(
                        origin_node.get_id().clone(),
                        FxHashSet::from_iter([pos.to_string()]),
                    );
                }
            }
        }
    }
}
