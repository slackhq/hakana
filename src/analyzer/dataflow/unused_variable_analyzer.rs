use hakana_code_info::EFFECT_PURE;
use hakana_code_info::EFFECT_READ_GLOBALS;
use hakana_code_info::EFFECT_READ_PROPS;
use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::code_location::HPos;
use hakana_code_info::data_flow::node::DataFlowNodeId;
use hakana_code_info::data_flow::node::DataFlowNodeKind;
use hakana_code_info::data_flow::node::VariableSourceKind;
use hakana_code_info::data_flow::path::PathKind;
use oxidized::{
    aast,
    aast_visitor::{AstParams, Node, Visitor, visit},
};
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::dataflow::program_analyzer::{should_ignore_array_fetch, should_ignore_property_fetch};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_code_info::data_flow::graph::DataFlowGraph;
use hakana_code_info::data_flow::node::DataFlowNode;
use hakana_code_info::data_flow::path::ArrayDataKind;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;

enum VariableUsage {
    NeverReferenced,
    ReferencedButNotUsed,
    Used,
}

pub fn check_variables_used(graph: &DataFlowGraph) -> (Vec<DataFlowNode>, Vec<DataFlowNode>) {
    let vars = graph
        .sources
        .iter()
        .filter(|(_, source)| matches!(source.kind, DataFlowNodeKind::VariableUseSource { .. }))
        .map(|(_, value)| match &value.kind {
            DataFlowNodeKind::VariableUseSource { pos, .. } => {
                ((pos.start_offset, pos.end_offset), value)
            }
            _ => {
                panic!();
            }
        })
        .collect::<BTreeMap<_, _>>();

    //println!("{:#?}", graph);

    // println!("printing variable map");

    // for (from_id, to) in &graph.forward_edges {
    //     for (to_id, _) in to {
    //         println!(
    //             "{} -> {}",
    //             from_id.to_string(_interner),
    //             to_id.to_string(_interner)
    //         );
    //     }
    // }

    let mut unused_nodes = Vec::new();
    let mut unused_but_referenced_nodes = Vec::new();

    for (_, source_node) in vars {
        match is_variable_used(graph, source_node) {
            VariableUsage::NeverReferenced => {
                if let DataFlowNode {
                    kind:
                        DataFlowNodeKind::VariableUseSource {
                            pure: true,
                            kind: VariableSourceKind::Default,
                            ..
                        },
                    ..
                } = source_node
                {
                    unused_nodes.push(source_node.clone());
                } else {
                    unused_but_referenced_nodes.push(source_node.clone());
                }
            }
            VariableUsage::ReferencedButNotUsed => {
                unused_but_referenced_nodes.push(source_node.clone());
            }
            VariableUsage::Used => {}
        }
    }

    (unused_nodes, unused_but_referenced_nodes)
}

pub fn check_variables_redefined_in_loop(
    graph: &DataFlowGraph,
    loop_boundaries: &[(u32, u32)],
    variable_assignments: &FxHashMap<String, FxHashSet<u32>>,
) -> Vec<DataFlowNode> {
    // Early exits for common cases
    if loop_boundaries.is_empty() || variable_assignments.is_empty() {
        return Vec::new();
    }

    // Filter to only variables with multiple assignments (potential redefinitions)
    let multi_assignment_vars: FxHashMap<&String, &FxHashSet<u32>> = variable_assignments
        .iter()
        .filter(|(_, offsets)| offsets.len() > 1)
        .map(|(name, offsets)| (name, offsets))
        .collect();

    if multi_assignment_vars.is_empty() {
        return Vec::new();
    }

    // Build a reverse index: offset -> source node for quick lookup
    let mut offset_to_source: FxHashMap<u32, &DataFlowNode> = FxHashMap::default();
    for (_, source_node) in &graph.sources {
        if let (DataFlowNodeKind::VariableUseSource { pos, .. }, DataFlowNodeId::Var(..)) =
            (&source_node.kind, &source_node.id)
        {
            offset_to_source.insert(pos.start_offset, source_node);
        }
    }

    let mut redefined_nodes = Vec::new();

    // For each variable with multiple assignments
    for (var_name, assignment_offsets) in multi_assignment_vars {
        // For each loop
        for &(loop_start, loop_end) in loop_boundaries {
            // Partition assignments into inside/outside loop
            let (mut assignments_inside, assignments_outside): (Vec<u32>, Vec<u32>) =
                assignment_offsets
                    .iter()
                    .partition(|&&offset| offset >= loop_start && offset <= loop_end);

            // Skip if no assignments on both sides of the loop boundary
            if assignments_inside.is_empty() || assignments_outside.is_empty() {
                continue;
            }

            assignments_inside.sort_unstable();

            // Check if any outside assignment is used inside the loop
            for &outside_offset in &assignments_outside {
                let Some(outside_source) = offset_to_source.get(&outside_offset) else {
                    continue;
                };

                // Only check sources outside the loop
                if let DataFlowNodeKind::VariableUseSource { pos, .. } = &outside_source.kind {
                    if pos.start_offset >= loop_start {
                        continue;
                    }
                }

                let Some(sink_ids) = get_all_variable_uses(graph, outside_source) else {
                    continue;
                };

                // Collect all uses of the outside source that are inside the loop
                let uses_in_loop: Vec<u32> = sink_ids
                    .iter()
                    .filter_map(|sink_id| {
                        let sink = graph.sinks.get(sink_id)?;
                        if let DataFlowNodeKind::VariableUseSink { pos: sink_pos } = &sink.kind {
                            if sink_pos.start_offset >= loop_start
                                && sink_pos.start_offset <= loop_end
                            {
                                return Some(sink_pos.start_offset);
                            }
                        }
                        None
                    })
                    .collect();

                if uses_in_loop.is_empty() {
                    continue;
                }

                // Find the earliest use in the loop
                let earliest_use = *uses_in_loop.iter().min().unwrap();

                // Check each assignment inside the loop that comes after this use
                for &inside_offset in &assignments_inside {
                    if inside_offset <= earliest_use {
                        continue;
                    }

                    let Some(inside_source) = offset_to_source.get(&inside_offset) else {
                        continue;
                    };

                    // Check if this redefined variable is used again after redefinition
                    if let Some(inside_sink_ids) = get_all_variable_uses(graph, inside_source) {
                        let has_use_after_redef = inside_sink_ids.iter().any(|sink_id| {
                            if let Some(DataFlowNode {
                                kind: DataFlowNodeKind::VariableUseSink { pos: sink_pos },
                                ..
                            }) = graph.sinks.get(sink_id)
                            {
                                sink_pos.start_offset > inside_offset
                                    && sink_pos.start_offset >= loop_start
                                    && sink_pos.start_offset <= loop_end
                            } else {
                                false
                            }
                        });

                        if has_use_after_redef {
                            redefined_nodes.push((*inside_source).clone());
                            break; // Only report once per inside assignment
                        }
                    }
                }
            }
        }
    }

    redefined_nodes
}

pub fn check_variables_scoped_incorrectly(
    graph: &DataFlowGraph,
    if_block_boundaries: &[(u32, u32)],
    loop_boundaries: &[(u32, u32)],
    for_loop_init_boundaries: &[(u32, u32)],
    concurrent_block_boundaries: &[(u32, u32)],
    function_pos: HPos,
) -> (Vec<DataFlowNode>, Vec<DataFlowNode>) {
    let mut incorrectly_scoped = Vec::new();
    let mut async_incorrectly_scoped = Vec::new();

    // Skip if there are no if blocks to analyze
    if if_block_boundaries.is_empty() {
        return (incorrectly_scoped, async_incorrectly_scoped);
    }

    let sources_by_usage = get_sources_grouped_by_usage(graph, function_pos);

    // Check each group of sources that feed the same set of sinks
    for (sources, sink_positions_map) in sources_by_usage {
        // Check if ALL sources are defined outside if blocks
        let all_sources_outside_if = sources.iter().all(|source_node| {
            if let DataFlowNodeKind::VariableUseSource { pos, .. } = &source_node.kind {
                !is_position_within_any_if_block(pos, if_block_boundaries)
            } else {
                false
            }
        });

        // Check if any source is defined within foreach iterator bounds (should be exempted)
        let any_source_in_foreach_init = sources.iter().any(|source_node| {
            if let DataFlowNodeKind::VariableUseSource { pos, .. } = &source_node.kind {
                is_position_within_foreach_init_bounds(pos, for_loop_init_boundaries)
            } else {
                false
            }
        });

        // Check if any source is defined within concurrent block bounds (should be exempted)
        let any_source_in_concurrent_block = sources.iter().any(|source_node| {
            if let DataFlowNodeKind::VariableUseSource { pos, .. } = &source_node.kind {
                is_position_within_concurrent_block_bounds(pos, concurrent_block_boundaries)
            } else {
                false
            }
        });

        if all_sources_outside_if && !any_source_in_foreach_init && !any_source_in_concurrent_block
        {
            for source in &sources {
                if matches!(
                    source.kind,
                    DataFlowNodeKind::VariableUseSource {
                        kind: VariableSourceKind::InoutArg,
                        ..
                    }
                ) {
                    continue;
                }

                if let Some(sink_positions) = sink_positions_map.get(source) {
                    // Check if ALL uses are within if blocks AND not used in multiple if blocks
                    // BUT skip if the variable is defined before a loop and used inside an if within that loop
                    if !sink_positions.is_empty()
                        && sink_positions.iter().all(|sink_pos| {
                            is_position_within_any_if_block(sink_pos, if_block_boundaries)
                        })
                        && !is_used_in_multiple_if_blocks(sink_positions, if_block_boundaries)
                        && !is_optimization_pattern(
                            &sources,
                            sink_positions,
                            loop_boundaries,
                            if_block_boundaries,
                        )
                    {
                        // Check if any of the sources are from await expressions
                        let has_await_source = sources.iter().any(|source_node| {
                            if let DataFlowNodeKind::VariableUseSource { has_await_call, .. } =
                                &source_node.kind
                            {
                                *has_await_call
                            } else {
                                false
                            }
                        });

                        if has_await_source {
                            async_incorrectly_scoped.push((*source).clone());
                        } else {
                            incorrectly_scoped.push((*source).clone());
                        }
                    }
                }
            }
        }
    }

    (incorrectly_scoped, async_incorrectly_scoped)
}

fn get_sources_grouped_by_usage<'a>(
    graph: &'a DataFlowGraph,
    function_pos: HPos,
) -> Vec<(
    Vec<&'a DataFlowNode>,
    FxHashMap<&'a DataFlowNode, Vec<HPos>>,
)> {
    let mut sink_to_sources: FxHashMap<DataFlowNodeId, Vec<&DataFlowNode>> = FxHashMap::default();
    let mut source_to_sinks = FxHashMap::default();

    // First, collect all relevant sources
    let mut relevant_sources = Vec::new();
    for (_, source_node) in &graph.sources {
        if let DataFlowNodeKind::VariableUseSource { kind, pure, .. } = &source_node.kind {
            // Skip function parameters - they're not "defined outside if blocks" in the problematic sense
            if matches!(
                kind,
                VariableSourceKind::NonPrivateParam
                    | VariableSourceKind::PrivateParam
                    | VariableSourceKind::ClosureParam
            ) {
                continue;
            }

            // Skip pure sources - only flag variables with impure sources
            if *pure {
                continue;
            }

            relevant_sources.push(source_node);
        }
    }

    // For each source, find all its sinks and map sink -> sources
    for source_node in &relevant_sources {
        if let Some(sink_ids) = get_all_variable_uses(graph, source_node) {
            let mut sink_positions = vec![];

            // Find the sink nodes that correspond to these positions
            for sink_id in sink_ids {
                // Find the sink node with this position
                if let Some(DataFlowNode {
                    kind: DataFlowNodeKind::VariableUseSink { pos },
                    ..
                }) = &graph.sinks.get(&sink_id)
                {
                    // only count sinks inside the function bounds
                    // Hakana will add function param locations as sinks
                    // when vars passed as args to those functions
                    if pos.start_offset > function_pos.start_offset
                        && pos.end_offset <= function_pos.end_offset
                        && pos.file_path == function_pos.file_path
                    {
                        sink_positions.push(*pos);
                        sink_to_sources
                            .entry(sink_id.clone())
                            .or_default()
                            .push(source_node);
                    }
                }
            }

            // Store sink positions for this source
            source_to_sinks.insert(source_node, sink_positions.clone());
        }
    }

    // Group sources by their set of sinks
    let mut usage_groups: FxHashMap<Vec<DataFlowNodeId>, Vec<&DataFlowNode>> = FxHashMap::default();

    for source_node in &relevant_sources {
        // Find all sinks that this source feeds
        let mut source_sinks = Vec::new();
        for (sink_id, sources) in &sink_to_sources {
            if sources.iter().any(|s| std::ptr::eq(*s, *source_node)) {
                source_sinks.push(sink_id.clone());
            }
        }

        // Sort sinks for consistent grouping
        source_sinks.sort();

        // Group sources by their sink set
        usage_groups
            .entry(source_sinks)
            .or_default()
            .push(*source_node);
    }

    // Return the groups of sources along with their sink positions
    usage_groups
        .into_values()
        .map(|sources| {
            // Create a map of source -> sink positions for this group
            let mut group_sink_map = FxHashMap::default();
            for source in &sources {
                if let Some(sink_positions) = source_to_sinks.get(source) {
                    group_sink_map.insert(*source, sink_positions.clone());
                }
            }
            (sources, group_sink_map)
        })
        .collect()
}

fn is_position_within_any_if_block(pos: &HPos, if_block_boundaries: &[(u32, u32)]) -> bool {
    let pos_offset = pos.start_offset;
    if_block_boundaries
        .iter()
        .any(|(start, end)| pos_offset >= *start && pos_offset <= *end)
}

fn is_position_within_foreach_init_bounds(
    pos: &HPos,
    for_loop_init_boundaries: &[(u32, u32)],
) -> bool {
    let pos_offset = pos.start_offset;
    for_loop_init_boundaries
        .iter()
        .any(|(start, end)| pos_offset >= *start && pos_offset <= *end)
}

fn is_position_within_concurrent_block_bounds(
    pos: &HPos,
    concurrent_block_boundaries: &[(u32, u32)],
) -> bool {
    let pos_offset = pos.start_offset;
    concurrent_block_boundaries
        .iter()
        .any(|(start, end)| pos_offset >= *start && pos_offset <= *end)
}

fn is_used_in_multiple_if_blocks(
    sink_positions: &[HPos],
    if_block_boundaries: &[(u32, u32)],
) -> bool {
    // Group sink positions by which if block they belong to
    let mut blocks_with_usage = FxHashSet::default();

    for sink_pos in sink_positions {
        let pos_offset = sink_pos.start_offset;
        for (i, (start, end)) in if_block_boundaries.iter().enumerate() {
            if pos_offset >= *start && pos_offset <= *end {
                blocks_with_usage.insert(i);
                break;
            }
        }
    }

    // If the variable is used in more than one if block, it's not "only used inside" a single if block
    blocks_with_usage.len() > 1
}

fn is_optimization_pattern(
    sources: &[&DataFlowNode],
    sink_positions: &[HPos],
    loop_boundaries: &[(u32, u32)],
    if_block_boundaries: &[(u32, u32)],
) -> bool {
    // If there are no loop boundaries, this pattern doesn't apply
    if loop_boundaries.is_empty() {
        return false;
    }

    // Check if any source is defined before a loop and any sink is used inside an if block within that loop
    for source_node in sources {
        if let DataFlowNodeKind::VariableUseSource {
            pos: source_pos, ..
        } = &source_node.kind
        {
            // Check if the source is defined before any loop
            for (loop_start, loop_end) in loop_boundaries {
                if source_pos.start_offset < *loop_start {
                    // Check if any sink is used inside an if block within this loop
                    for sink_pos in sink_positions {
                        if sink_pos.start_offset >= *loop_start
                            && sink_pos.start_offset <= *loop_end
                        {
                            // The sink is within the loop, check if it's also within an if block
                            if is_position_within_any_if_block(sink_pos, if_block_boundaries) {
                                return true; // This is an optimization pattern
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

fn get_all_variable_uses(
    graph: &DataFlowGraph,
    source_node: &DataFlowNode,
) -> Option<Vec<DataFlowNodeId>> {
    let mut visited_nodes = FxHashSet::default();
    let mut sink_nodes = Vec::new();
    let mut to_visit = vec![source_node.id.clone()];

    while let Some(node_id) = to_visit.pop() {
        if visited_nodes.contains(&node_id) {
            continue;
        }
        visited_nodes.insert(node_id.clone());

        // Check if this node is a sink
        if let Some(sink_node) = graph.sinks.get(&node_id) {
            if let DataFlowNodeKind::VariableUseSink { .. } = &sink_node.kind {
                sink_nodes.push(sink_node.id.clone());
            }
        }

        // Add connected nodes to visit
        if let Some(edges) = graph.forward_edges.get(&node_id) {
            for (to_id, _) in edges {
                if !visited_nodes.contains(to_id) {
                    to_visit.push(to_id.clone());
                }
            }
        }
    }

    if sink_nodes.is_empty() {
        None
    } else {
        Some(sink_nodes)
    }
}

fn is_variable_used(graph: &DataFlowGraph, source_node: &DataFlowNode) -> VariableUsage {
    let mut visited_source_ids = FxHashSet::default();

    let mut sources = FxHashMap::default();

    let source_node = VariableUseNode::from(source_node);
    sources.insert(source_node.0.clone(), source_node.1);

    let mut i = 0;

    while i < 200 {
        if sources.is_empty() {
            break;
        }

        let mut new_child_nodes = FxHashMap::default();

        for (id, source) in &sources {
            visited_source_ids.insert(id.clone());

            let child_nodes = get_variable_child_nodes(graph, id, source, &visited_source_ids);

            if let Some(child_nodes) = child_nodes {
                new_child_nodes.extend(child_nodes);
            } else {
                return VariableUsage::Used;
            }
        }

        sources = new_child_nodes;

        i += 1;
    }

    if i == 1 {
        VariableUsage::NeverReferenced
    } else {
        VariableUsage::ReferencedButNotUsed
    }
}

fn get_variable_child_nodes(
    graph: &DataFlowGraph,
    generated_source_id: &DataFlowNodeId,
    generated_source: &VariableUseNode,
    visited_source_ids: &FxHashSet<DataFlowNodeId>,
) -> Option<FxHashMap<DataFlowNodeId, VariableUseNode>> {
    let mut new_child_nodes = FxHashMap::default();

    if let Some(forward_edges) = graph.forward_edges.get(generated_source_id) {
        for (to_id, path) in forward_edges {
            if graph.sinks.contains_key(to_id) {
                return None;
            }

            if visited_source_ids.contains(to_id) {
                continue;
            }

            if should_ignore_array_fetch(
                &path.kind,
                &ArrayDataKind::ArrayKey,
                &generated_source.path_types,
            ) {
                continue;
            }

            if should_ignore_array_fetch(
                &path.kind,
                &ArrayDataKind::ArrayValue,
                &generated_source.path_types,
            ) {
                continue;
            }

            if should_ignore_property_fetch(&path.kind, &generated_source.path_types) {
                continue;
            }

            let mut new_destination = VariableUseNode {
                path_types: generated_source.clone().path_types,
                kind: generated_source.kind.clone(),
                pos: generated_source.pos.clone(),
            };

            new_destination.path_types.push(path.kind.clone());

            new_child_nodes.insert(to_id.clone(), new_destination);
        }
    }

    Some(new_child_nodes)
}

struct Scanner<'a> {
    pub unused_variable_nodes: &'a Vec<DataFlowNode>,
    pub comments: &'a Vec<(Pos, Comment)>,
    pub in_single_block: bool,
}

impl<'ast> Visitor<'ast> for Scanner<'_> {
    type Params = AstParams<FunctionAnalysisData, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_expr(
        &mut self,
        analysis_data: &mut FunctionAnalysisData,
        expr: &aast::Expr<(), ()>,
    ) -> Result<(), ()> {
        if let aast::Expr_::List(exprs) = &expr.2 {
            for list_expr in exprs {
                let has_matching_node = self.unused_variable_nodes.iter().any(|n| match &n.kind {
                    DataFlowNodeKind::VariableUseSource { pos, .. } => {
                        pos.start_offset == list_expr.1.start_offset() as u32
                    }
                    _ => false,
                });

                if has_matching_node {
                    analysis_data.add_replacement(
                        (
                            list_expr.1.start_offset() as u32,
                            list_expr.1.end_offset() as u32,
                        ),
                        Replacement::Substitute("$_".to_string()),
                    );
                }
            }
        }
        expr.recurse(analysis_data, self)
    }

    fn visit_stmt(
        &mut self,
        analysis_data: &mut FunctionAnalysisData,
        stmt: &aast::Stmt<(), ()>,
    ) -> Result<(), ()> {
        if let aast::Stmt_::If(boxed) = &stmt.1 {
            self.in_single_block =
                boxed.1.0.len() == 1 && matches!(boxed.1.0[0].1, aast::Stmt_::Expr(_));

            let result = boxed.1.recurse(analysis_data, self);
            if result.is_err() {
                self.in_single_block = false;
                return result;
            }

            self.in_single_block =
                boxed.2.0.len() == 1 && matches!(boxed.2.0[0].1, aast::Stmt_::Expr(_));
            let result = boxed.2.recurse(analysis_data, self);
            self.in_single_block = false;
            return result;
        }

        let has_matching_node = self.unused_variable_nodes.iter().any(|n| match &n.kind {
            DataFlowNodeKind::VariableUseSource { pos, .. } => {
                pos.start_offset == stmt.0.start_offset() as u32
            }
            _ => false,
        });

        if has_matching_node {
            if let aast::Stmt_::Expr(boxed) = &stmt.1 {
                if let aast::Expr_::Assign(boxed) = &boxed.2 {
                    let expression_effects = analysis_data
                        .expr_effects
                        .get(&(
                            boxed.2.1.start_offset() as u32,
                            boxed.2.1.end_offset() as u32,
                        ))
                        .unwrap_or(&0);

                    if let EFFECT_PURE | EFFECT_READ_GLOBALS | EFFECT_READ_PROPS =
                        *expression_effects
                    {
                        if !self.in_single_block {
                            let span = stmt.0.to_raw_span();
                            analysis_data.add_replacement(
                                (stmt.0.start_offset() as u32, stmt.0.end_offset() as u32),
                                Replacement::TrimPrecedingWhitespace(
                                    span.start.beg_of_line() as u32
                                ),
                            );

                            self.remove_fixme_comments(stmt, analysis_data, stmt.0.start_offset());
                        }
                    } else {
                        analysis_data.add_replacement(
                            (
                                stmt.0.start_offset() as u32,
                                boxed.2.1.start_offset() as u32,
                            ),
                            Replacement::Remove,
                        );

                        // remove trailing array fetches
                        if let aast::Expr_::ArrayGet(array_get) = &boxed.2.2 {
                            if let Some(array_offset_expr) = &array_get.1 {
                                let array_offset_effects = analysis_data
                                    .expr_effects
                                    .get(&(
                                        array_offset_expr.1.start_offset() as u32,
                                        array_offset_expr.1.end_offset() as u32,
                                    ))
                                    .unwrap_or(&0);

                                if let EFFECT_PURE | EFFECT_READ_GLOBALS | EFFECT_READ_PROPS =
                                    *array_offset_effects
                                {
                                    analysis_data.add_replacement(
                                        (
                                            array_offset_expr.pos().start_offset() as u32 - 1,
                                            array_offset_expr.pos().end_offset() as u32 + 1,
                                        ),
                                        Replacement::Remove,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        stmt.recurse(analysis_data, self)
    }
}

impl<'a> Scanner<'a> {
    fn remove_fixme_comments(
        &mut self,
        stmt: &aast::Stmt<(), ()>,
        analysis_data: &mut FunctionAnalysisData,
        limit: usize,
    ) {
        for (comment_pos, comment) in self.comments {
            if comment_pos.line() == stmt.0.line() {
                if let Comment::CmtBlock(block) = comment {
                    if block.trim() == "HHAST_FIXME[UnusedVariable]" {
                        analysis_data.add_replacement(
                            (comment_pos.start_offset() as u32, limit as u32),
                            Replacement::TrimPrecedingWhitespace(
                                comment_pos.to_raw_span().start.beg_of_line() as u32,
                            ),
                        );

                        return;
                    }
                }
            } else if comment_pos.line() == stmt.0.line() - 1 {
                if let Comment::CmtBlock(block) = comment {
                    if let "HAKANA_FIXME[UnusedAssignment]"
                    | "HAKANA_FIXME[UnusedAssignmentStatement]" = block.trim()
                    {
                        let stmt_start = stmt.0.to_raw_span().start;
                        analysis_data.add_replacement(
                            (
                                comment_pos.start_offset() as u32,
                                (stmt_start.beg_of_line() as u32) - 1,
                            ),
                            Replacement::TrimPrecedingWhitespace(
                                comment_pos.to_raw_span().start.beg_of_line() as u32,
                            ),
                        );
                        return;
                    }
                }
            }
        }
    }
}

pub(crate) fn add_unused_expression_replacements(
    stmts: &Vec<aast::Stmt<(), ()>>,
    analysis_data: &mut FunctionAnalysisData,
    unused_source_nodes: &Vec<DataFlowNode>,
    statements_analyzer: &StatementsAnalyzer,
) {
    let mut scanner = Scanner {
        unused_variable_nodes: unused_source_nodes,
        comments: statements_analyzer.file_analyzer.file_source.comments,
        in_single_block: false,
    };

    for stmt in stmts {
        visit(&mut scanner, analysis_data, stmt).unwrap();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableUseNode {
    pub pos: Rc<HPos>,
    pub path_types: Vec<PathKind>,
    pub kind: VariableSourceKind,
}

impl VariableUseNode {
    pub fn from(node: &DataFlowNode) -> (DataFlowNodeId, Self) {
        (
            node.id.clone(),
            match &node.kind {
                DataFlowNodeKind::Vertex { pos, .. } => Self {
                    pos: Rc::new((*pos).unwrap()),
                    path_types: Vec::new(),
                    kind: VariableSourceKind::Default,
                },
                DataFlowNodeKind::VariableUseSource { kind, pos, .. } => Self {
                    pos: Rc::new(*pos),
                    path_types: Vec::new(),
                    kind: kind.clone(),
                },
                DataFlowNodeKind::VariableUseSink { pos } => Self {
                    pos: Rc::new(*pos),
                    path_types: Vec::new(),
                    kind: VariableSourceKind::Default,
                },
                _ => {
                    panic!();
                }
            },
        )
    }
}
