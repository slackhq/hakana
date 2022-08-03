use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::taint_analyzer::should_ignore_fetch;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::node::NodeKind;
use hakana_reflection_info::data_flow::path::PathExpressionKind;
use hakana_reflection_info::data_flow::tainted_node::TaintedNode;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;

pub fn check_variables_used(graph: &DataFlowGraph) -> Vec<DataFlowNode> {
    let vars = graph
        .sources
        .iter()
        .filter(|(_, source)| {
            matches!(
                source.kind,
                NodeKind::Default | NodeKind::PrivateParam | NodeKind::NonPrivateParam
            )
        })
        .map(|(_, value)| (value.pos.clone().unwrap().start_offset, value))
        .collect::<BTreeMap<_, _>>();

    let mut unused_nodes = Vec::new();

    for (_, source_node) in vars {
        if !is_variable_used(graph, source_node) {
            unused_nodes.push(source_node.clone());
        }
    }

    unused_nodes
}

fn is_variable_used(graph: &DataFlowGraph, source_node: &DataFlowNode) -> bool {
    let mut visited_source_ids = HashSet::new();

    let mut sources = HashMap::new();

    let source_node = TaintedNode::from(source_node);
    sources.insert(source_node.id.clone(), source_node.clone());

    let mut i = 0;

    while i < 200 {
        if sources.is_empty() {
            break;
        }

        let mut new_child_nodes = HashMap::new();

        for (_, source) in &sources {
            visited_source_ids.insert(source.id.clone());

            let child_nodes = get_variable_child_nodes(graph, source, &visited_source_ids);

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
    generated_source: &TaintedNode,
    visited_source_ids: &HashSet<String>,
) -> Option<HashMap<String, TaintedNode>> {
    let mut new_child_nodes = HashMap::new();

    if let Some(forward_edges) = graph.forward_edges.get(&generated_source.id) {
        for (to_id, path) in forward_edges {
            if let Some(_) = graph.sinks.get(to_id) {
                return None;
            }

            if visited_source_ids.contains(to_id) {
                continue;
            }

            if should_ignore_fetch(
                &path.kind,
                &PathExpressionKind::ArrayKey,
                &generated_source.path_types,
            ) {
                continue;
            }

            if should_ignore_fetch(
                &path.kind,
                &PathExpressionKind::ArrayValue,
                &generated_source.path_types,
            ) {
                continue;
            }

            if should_ignore_fetch(
                &path.kind,
                &PathExpressionKind::Property,
                &generated_source.path_types,
            ) {
                continue;
            }

            let mut new_destination = TaintedNode {
                kind: NodeKind::Default,
                id: to_id.clone(),
                unspecialized_id: None,
                label: to_id.clone(),
                pos: None,
                specialization_key: None,
                taints: HashSet::new(),
                previous: None,
                path_types: generated_source.clone().path_types,
                specialized_calls: HashMap::new(),
            };

            new_destination.path_types.push(path.kind.clone());

            new_child_nodes.insert(to_id.clone(), new_destination);
        }
    }

    Some(new_child_nodes)
}

use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
};

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
                let has_matching_node = self
                    .unused_variable_nodes
                    .iter()
                    .any(|n| n.pos.as_ref().unwrap().start_offset == list_expr.1.start_offset());

                if has_matching_node {
                    tast_info.replacements.insert(
                        (list_expr.1.start_offset(), list_expr.1.end_offset()),
                        "$_".to_string(),
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

        let has_matching_node = self
            .unused_variable_nodes
            .iter()
            .any(|n| n.pos.as_ref().unwrap().start_offset == stmt.0.start_offset());

        if has_matching_node {
            if let aast::Stmt_::Expr(boxed) = &stmt.1 {
                if let aast::Expr_::Binop(boxed) = &boxed.2 {
                    if let oxidized::ast_defs::Bop::Eq(_) = &boxed.0 {
                        if tast_info
                            .pure_exprs
                            .contains(&(boxed.2 .1.start_offset(), boxed.2 .1.end_offset()))
                        {
                            if !self.in_single_block {
                                let span = stmt.0.to_raw_span();
                                tast_info.replacements.insert(
                                    ((span.start.beg_of_line() as usize) - 1, stmt.0.end_offset()),
                                    "".to_string(),
                                );
                            }
                        } else {
                            tast_info.replacements.insert(
                                (stmt.0.start_offset(), boxed.2 .1.start_offset()),
                                "".to_string(),
                            );

                            // remove trailing array fetches
                            if let aast::Expr_::ArrayGet(array_get) = &boxed.2 .2 {
                                if let Some(array_offset_expr) = &array_get.1 {
                                    if tast_info.pure_exprs.contains(&(
                                        array_offset_expr.1.start_offset(),
                                        array_offset_expr.1.end_offset(),
                                    )) {
                                        tast_info.replacements.insert(
                                            (
                                                array_offset_expr.pos().start_offset() - 1,
                                                array_offset_expr.pos().end_offset() + 1,
                                            ),
                                            "".to_string(),
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
                                                    "".to_string(),
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
