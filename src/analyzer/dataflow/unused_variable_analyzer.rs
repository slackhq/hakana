use hakana_reflection_info::analysis_result::Replacement;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::data_flow::node::DataFlowNodeKind;
use hakana_reflection_info::data_flow::node::VariableSourceKind;
use hakana_reflection_info::data_flow::path::PathKind;
use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
};
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::dataflow::program_analyzer::{should_ignore_array_fetch, should_ignore_property_fetch};
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::ArrayDataKind;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;

pub fn check_variables_used(graph: &DataFlowGraph) -> Vec<DataFlowNode> {
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

    // for (from_id, to) in &graph.forward_edges {
    //     for (to_id, _) in to {
    //         println!("{} -> {}", from_id, to_id);
    //     }
    // }

    let mut unused_nodes = Vec::new();

    for (_, source_node) in vars {
        if !is_variable_used(graph, source_node) {
            unused_nodes.push(source_node.clone());
        }
    }

    unused_nodes
}

fn is_variable_used(graph: &DataFlowGraph, source_node: &DataFlowNode) -> bool {
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
                return true;
            }
        }

        sources = new_child_nodes;

        i += 1;
    }

    false
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
            if let Some(_) = graph.sinks.get(to_id) {
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
    type Params = AstParams<TastInfo, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_expr(
        &mut self,
        tast_info: &mut TastInfo,
        expr: &aast::Expr<(), ()>,
    ) -> Result<(), ()> {
        if let aast::Expr_::List(exprs) = &expr.2 {
            for list_expr in exprs {
                let has_matching_node = self.unused_variable_nodes.iter().any(|n| match &n.kind {
                    DataFlowNodeKind::VariableUseSource { pos, .. } => {
                        pos.start_offset == list_expr.1.start_offset()
                    }
                    _ => false,
                });

                if has_matching_node {
                    tast_info.replacements.insert(
                        (list_expr.1.start_offset(), list_expr.1.end_offset()),
                        Replacement::Substitute("$_".to_string()),
                    );
                }
            }
        }
        expr.recurse(tast_info, self)
    }

    fn visit_stmt(
        &mut self,
        tast_info: &mut TastInfo,
        stmt: &aast::Stmt<(), ()>,
    ) -> Result<(), ()> {
        if let aast::Stmt_::If(_) = &stmt.1 {
            let span = stmt.0.to_raw_span();

            if span.end.line() - span.start.line() <= 1 {
                self.in_single_block = true;

                let result = stmt.recurse(tast_info, self);

                self.in_single_block = false;

                return result;
            }
        }

        let has_matching_node = self.unused_variable_nodes.iter().any(|n| match &n.kind {
            DataFlowNodeKind::VariableUseSource { pos, .. } => {
                pos.start_offset == stmt.0.start_offset()
            }
            _ => false,
        });

        if has_matching_node {
            if let aast::Stmt_::Expr(boxed) = &stmt.1 {
                if let aast::Expr_::Binop(boxed) = &boxed.2 {
                    if let oxidized::ast_defs::Bop::Eq(_) = &boxed.0 {
                        let expression_effects = tast_info
                            .expr_effects
                            .get(&(boxed.2 .1.start_offset(), boxed.2 .1.end_offset()))
                            .unwrap_or(&0);

                        if let crate::typed_ast::PURE
                        | crate::typed_ast::READ_GLOBALS
                        | crate::typed_ast::READ_PROPS = *expression_effects
                        {
                            if !self.in_single_block {
                                let span = stmt.0.to_raw_span();
                                tast_info.replacements.insert(
                                    ((span.start.beg_of_line() as usize) - 1, stmt.0.end_offset()),
                                    Replacement::Remove,
                                );
                            }
                        } else {
                            tast_info.replacements.insert(
                                (stmt.0.start_offset(), boxed.2 .1.start_offset()),
                                Replacement::Remove,
                            );

                            // remove trailing array fetches
                            if let aast::Expr_::ArrayGet(array_get) = &boxed.2 .2 {
                                if let Some(array_offset_expr) = &array_get.1 {
                                    let array_offset_effects = tast_info
                                        .expr_effects
                                        .get(&(
                                            array_offset_expr.1.start_offset(),
                                            array_offset_expr.1.end_offset(),
                                        ))
                                        .unwrap_or(&0);

                                    if let crate::typed_ast::PURE
                                    | crate::typed_ast::READ_GLOBALS
                                    | crate::typed_ast::READ_PROPS = *array_offset_effects
                                    {
                                        tast_info.replacements.insert(
                                            (
                                                array_offset_expr.pos().start_offset() - 1,
                                                array_offset_expr.pos().end_offset() + 1,
                                            ),
                                            Replacement::Remove,
                                        );
                                    }
                                }
                            }

                            for (pos, comment) in self.comments {
                                if pos.line() == stmt.0.line() {
                                    match comment {
                                        Comment::CmtBlock(block) => {
                                            if block.trim() == "HHAST_FIXME[UnusedVariable]" {
                                                tast_info.replacements.insert(
                                                    (pos.start_offset(), stmt.0.start_offset()),
                                                    Replacement::Remove,
                                                );
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        stmt.recurse(tast_info, self)
    }
}

pub(crate) fn add_unused_expression_replacements(
    stmts: &Vec<aast::Stmt<(), ()>>,
    tast_info: &mut TastInfo,
    unused_source_nodes: &Vec<DataFlowNode>,
    statements_analyzer: &StatementsAnalyzer,
) {
    let mut scanner = Scanner {
        unused_variable_nodes: unused_source_nodes,
        comments: &statements_analyzer
            .get_file_analyzer()
            .get_file_source()
            .comments,
        in_single_block: false,
    };

    for stmt in stmts {
        visit(&mut scanner, tast_info, stmt).unwrap();
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
                    pos: Rc::new(pos.clone().unwrap()),
                    path_types: Vec::new(),
                    kind: VariableSourceKind::Default,
                    name: "".to_string(),
                },
                DataFlowNodeKind::VariableUseSource { kind, label, pos } => Self {
                    pos: Rc::new(pos.clone()),
                    path_types: Vec::new(),
                    kind: kind.clone(),
                    name: label.clone(),
                },
                DataFlowNodeKind::VariableUseSink { pos } => Self {
                    pos: Rc::new(pos.clone()),
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
