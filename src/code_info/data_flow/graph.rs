use super::{
    node::{DataFlowNode, DataFlowNodeId, DataFlowNodeKind},
    path::{DataFlowPath, PathKind},
};
use crate::{code_location::FilePath, taint::SinkType};
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
    pub vertices: FxHashMap<DataFlowNodeId, DataFlowNode>,
    pub forward_edges: FxHashMap<DataFlowNodeId, FxHashMap<DataFlowNodeId, DataFlowPath>>,
    pub backward_edges: FxHashMap<DataFlowNodeId, FxHashSet<DataFlowNodeId>>,
    pub sources: FxHashMap<DataFlowNodeId, DataFlowNode>,
    pub sinks: FxHashMap<DataFlowNodeId, DataFlowNode>,
    pub mixed_source_counts: FxHashMap<DataFlowNodeId, FxHashSet<String>>,
    pub specializations: FxHashMap<DataFlowNodeId, FxHashSet<(FilePath, u32)>>,
    specialized_calls: FxHashMap<(FilePath, u32), FxHashSet<DataFlowNodeId>>,
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
            DataFlowNodeKind::Vertex { is_specialized, .. } => {
                if let GraphKind::WholeProgram(_) = &self.kind {
                    if *is_specialized {
                        let (unspecialized_id, specialization_key) = node.id.unspecialize();
                        self.specializations
                            .entry(unspecialized_id.clone())
                            .or_default()
                            .insert(specialization_key);

                        self.specialized_calls
                            .entry(specialization_key)
                            .or_default()
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
        added_taints: Vec<SinkType>,
        removed_taints: Vec<SinkType>,
    ) {
        let from_id = &from.id;
        let to_id = &to.id;

        if from_id == to_id {
            return;
        }

        if let GraphKind::FunctionBody = self.kind {
            self.backward_edges
                .entry(to_id.clone())
                .or_default()
                .insert(from_id.clone());
        }

        self.forward_edges
            .entry(from_id.clone())
            .or_default()
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
            self.forward_edges.entry(key).or_default().extend(edges);
        }

        if self.kind == GraphKind::FunctionBody {
            for (key, edges) in graph.backward_edges {
                self.backward_edges.entry(key).or_default().extend(edges);
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
                    .or_default()
                    .extend(specializations);
            }
        }

        self.vertices.extend(graph.vertices);
        self.sources.extend(graph.sources);
        self.sinks.extend(graph.sinks);
    }

    /// Returns a set of nodes that are origin nodes for the given assignment
    pub fn get_origin_node_ids(
        &self,
        assignment_node_id: &DataFlowNodeId,
        ignore_paths: Vec<PathKind>,
    ) -> Vec<DataFlowNodeId> {
        let mut visited_child_ids = FxHashSet::default();

        let mut origin_nodes = vec![];

        let mut child_node_ids = vec![assignment_node_id.clone()];

        for _ in 0..50 {
            let mut all_parent_nodes = vec![];

            for child_node_id in child_node_ids {
                if visited_child_ids.contains(&child_node_id) {
                    continue;
                }

                visited_child_ids.insert(child_node_id.clone());

                let mut new_parent_nodes = FxHashSet::default();
                let mut has_visited_a_parent_already = false;

                if let Some(backward_edges) = self.backward_edges.get(&child_node_id) {
                    for from_id in backward_edges {
                        if let Some(forward_flows) = self.forward_edges.get(from_id) {
                            if let Some(path) = forward_flows.get(&child_node_id) {
                                if ignore_paths.contains(&path.kind) {
                                    break;
                                }
                            }
                        }

                        if self.get_node(from_id).is_some() {
                            if !visited_child_ids.contains(from_id) {
                                new_parent_nodes.insert(from_id.clone());
                            } else {
                                has_visited_a_parent_already = true;
                            }
                        }
                    }
                }

                if new_parent_nodes.is_empty() {
                    if !has_visited_a_parent_already {
                        origin_nodes.push(child_node_id);
                    }
                } else {
                    new_parent_nodes.retain(|f| !visited_child_ids.contains(f));
                    all_parent_nodes.extend(new_parent_nodes);
                }
            }

            child_node_ids = all_parent_nodes;

            if child_node_ids.is_empty() {
                break;
            }
        }

        origin_nodes
    }

    #[inline]
    pub fn get_node(&self, id: &DataFlowNodeId) -> Option<&DataFlowNode> {
        if let Some(node) = self.vertices.get(id) {
            Some(node)
        } else if let Some(node) = self.sources.get(id) {
            Some(node)
        } else if let Some(node) = self.sinks.get(id) {
            Some(node)
        } else {
            None
        }
    }

    pub fn add_mixed_data(&mut self, assignment_node: &DataFlowNode, pos: &Pos) {
        let origin_node_ids = self.get_origin_node_ids(&assignment_node.id, vec![]);

        for origin_node_id in origin_node_ids {
            if let DataFlowNodeId::CallTo(..) | DataFlowNodeId::SpecializedCallTo(..) =
                origin_node_id
            {
                if let Some(entry) = self.mixed_source_counts.get_mut(&origin_node_id) {
                    entry.insert(pos.to_string());
                } else {
                    self.mixed_source_counts.insert(
                        origin_node_id.clone(),
                        FxHashSet::from_iter([pos.to_string()]),
                    );
                }
            }
        }
    }
}
