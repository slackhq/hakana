use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::code_location::HPos;
use hakana_code_info::data_flow::node::DataFlowNodeId;
use hakana_code_info::data_flow::node::DataFlowNodeKind;
use hakana_code_info::data_flow::node::VariableSourceKind;
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::EFFECT_PURE;
use hakana_code_info::EFFECT_READ_GLOBALS;
use hakana_code_info::EFFECT_READ_PROPS;
use hakana_str::Interner;
use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
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

pub fn check_variables_used(
    graph: &DataFlowGraph,
    _interner: &Interner,
) -> (Vec<DataFlowNode>, Vec<DataFlowNode>) {
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

pub fn check_variables_scoped_incorrectly(
    graph: &DataFlowGraph,
    if_block_boundaries: &[(u32, u32)],
    loop_boundaries: &[(u32, u32)],
    for_loop_init_boundaries: &[(u32, u32)],
    interner: &Interner,
) -> (Vec<DataFlowNode>, Vec<DataFlowNode>) {
    let mut incorrectly_scoped = Vec::new();
    let mut async_incorrectly_scoped = Vec::new();

    // Skip if there are no if blocks to analyze
    if if_block_boundaries.is_empty() {
        return (incorrectly_scoped, async_incorrectly_scoped);
    }

    let variable_sources = get_sources_grouped_by_var_name(graph, interner);

    // Check each variable's sources collectively
    for (_, sources) in variable_sources {
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

        if all_sources_outside_if && !any_source_in_foreach_init {
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

                if let Some(sink_positions) = get_all_variable_uses(graph, source) {
                    // Check if ALL uses are within if blocks AND not used in multiple if blocks
                    // BUT skip if the variable is defined before a loop and used inside an if within that loop
                    if !sink_positions.is_empty()
                        && sink_positions.iter().all(|sink_pos| {
                            is_position_within_any_if_block(sink_pos, if_block_boundaries)
                        })
                        && !is_used_in_multiple_if_blocks(&sink_positions, if_block_boundaries)
                        && !is_optimization_pattern(
                            &sources,
                            &sink_positions,
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

fn get_sources_grouped_by_var_name<'a>(
    graph: &'a DataFlowGraph,
    interner: &Interner,
) -> FxHashMap<String, Vec<&'a DataFlowNode>> {
    let mut variable_sources: FxHashMap<String, Vec<&DataFlowNode>> = FxHashMap::default();

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

            // Extract variable name from the node ID
            if let Some(var_name) = get_variable_name_from_node(&source_node.id, interner) {
                variable_sources
                    .entry(var_name)
                    .or_default()
                    .push(source_node);
            }
        }
    }
    variable_sources
}

fn get_variable_name_from_node(node_id: &DataFlowNodeId, interner: &Interner) -> Option<String> {
    match node_id {
        DataFlowNodeId::Var(var_id, ..) => Some(interner.lookup(&var_id.0).to_string()),
        DataFlowNodeId::Param(var_id, ..) => Some(interner.lookup(&var_id.0).to_string()),
        _ => None,
    }
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

fn get_all_variable_uses(graph: &DataFlowGraph, source_node: &DataFlowNode) -> Option<Vec<HPos>> {
    let mut visited_nodes = FxHashSet::default();
    let mut sink_positions = Vec::new();
    let mut to_visit = vec![source_node.id.clone()];

    while let Some(node_id) = to_visit.pop() {
        if visited_nodes.contains(&node_id) {
            continue;
        }
        visited_nodes.insert(node_id.clone());

        // Check if this node is a sink
        if let Some(sink_node) = graph.sinks.get(&node_id) {
            if let DataFlowNodeKind::VariableUseSink { pos } = &sink_node.kind {
                sink_positions.push(*pos);
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

    if sink_positions.is_empty() {
        None
    } else {
        Some(sink_positions)
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
                boxed.1 .0.len() == 1 && matches!(boxed.1 .0[0].1, aast::Stmt_::Expr(_));

            let result = boxed.1.recurse(analysis_data, self);
            if result.is_err() {
                self.in_single_block = false;
                return result;
            }

            self.in_single_block =
                boxed.2 .0.len() == 1 && matches!(boxed.2 .0[0].1, aast::Stmt_::Expr(_));
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
                            boxed.2 .1.start_offset() as u32,
                            boxed.2 .1.end_offset() as u32,
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
                                boxed.2 .1.start_offset() as u32,
                            ),
                            Replacement::Remove,
                        );

                        // remove trailing array fetches
                        if let aast::Expr_::ArrayGet(array_get) = &boxed.2 .2 {
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
