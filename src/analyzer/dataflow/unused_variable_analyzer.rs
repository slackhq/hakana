use hakana_reflection_info::analysis_result::Replacement;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::data_flow::node::DataFlowNodeKind;
use hakana_reflection_info::data_flow::node::VariableSourceKind;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::EFFECT_PURE;
use hakana_reflection_info::EFFECT_READ_GLOBALS;
use hakana_reflection_info::EFFECT_READ_PROPS;
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
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::ArrayDataKind;
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
    //         println!("{} -> {}", from_id, to_id);
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
    generated_source_id: &String,
    generated_source: &VariableUseNode,
    visited_source_ids: &FxHashSet<String>,
) -> Option<FxHashMap<String, VariableUseNode>> {
    let mut new_child_nodes = FxHashMap::default();

    if let Some(forward_edges) = graph.forward_edges.get(generated_source_id) {
        for (to_id, path) in forward_edges {
            if graph.sinks.get(to_id).is_some() {
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
                name: generated_source.name.clone(),
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
                if let aast::Expr_::Binop(boxed) = &boxed.2 {
                    if let oxidized::ast_defs::Bop::Eq(_) = &boxed.bop {
                        let expression_effects = analysis_data
                            .expr_effects
                            .get(&(
                                boxed.rhs.1.start_offset() as u32,
                                boxed.rhs.1.end_offset() as u32,
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

                                self.remove_fixme_comments(
                                    stmt,
                                    analysis_data,
                                    stmt.0.start_offset(),
                                );
                            }
                        } else {
                            analysis_data.add_replacement(
                                (
                                    stmt.0.start_offset() as u32,
                                    boxed.rhs.1.start_offset() as u32,
                                ),
                                Replacement::Remove,
                            );

                            // remove trailing array fetches
                            if let aast::Expr_::ArrayGet(array_get) = &boxed.rhs.2 {
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
        comments: statements_analyzer
            .get_file_analyzer()
            .get_file_source()
            .comments,
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
    pub name: String,
}

impl VariableUseNode {
    pub fn from(node: &DataFlowNode) -> (String, Self) {
        (
            node.id.clone(),
            match &node.kind {
                DataFlowNodeKind::Vertex { pos, .. } => Self {
                    pos: Rc::new((*pos).unwrap()),
                    path_types: Vec::new(),
                    kind: VariableSourceKind::Default,
                    name: "".to_string(),
                },
                DataFlowNodeKind::VariableUseSource {
                    kind, label, pos, ..
                } => Self {
                    pos: Rc::new(*pos),
                    path_types: Vec::new(),
                    kind: kind.clone(),
                    name: label.clone(),
                },
                DataFlowNodeKind::VariableUseSink { pos } => Self {
                    pos: Rc::new(*pos),
                    path_types: Vec::new(),
                    kind: VariableSourceKind::Default,
                    name: "".to_string(),
                },
                _ => {
                    panic!();
                }
            },
        )
    }
}
