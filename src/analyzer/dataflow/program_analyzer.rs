use hakana_reflection_info::data_flow::node::DataFlowNodeKind;
use hakana_reflection_info::Interner;
use rustc_hash::FxHashSet;
use std::sync::Arc;
use std::time::Instant;

use crate::config::Config;
use crate::config::Verbosity;
use hakana_reflection_info::data_flow::graph::DataFlowGraph;
use hakana_reflection_info::data_flow::path::PathExpressionKind;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::data_flow::tainted_node::TaintedNode;
use hakana_reflection_info::issue::Issue;
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::taint::SinkType;

pub fn find_tainted_data(
    graph: &DataFlowGraph,
    config: &Config,
    verbosity: Verbosity,
    interner: &Interner,
) -> Vec<Issue> {
    let mut new_issues = vec![];

    let sources = graph
        .sources
        .iter()
        .map(|(_, v)| Arc::new(TaintedNode::from(v)))
        .collect::<Vec<_>>();

    if !matches!(verbosity, Verbosity::Quiet) {
        println!("Security analysis: detecting paths");

        println!(" - initial sources count: {}", sources.len());
        println!(" - initial sinks count:   {}", graph.sinks.len());
    }

    // for (from_id, to) in &graph.forward_edges {
    //     for (to_id, _) in to {
    //         println!("{} -> {}", from_id, to_id);
    //     }
    // }

    find_paths_to_sinks(
        sources,
        graph,
        config,
        verbosity,
        &mut new_issues,
        true,
        interner,
    );

    new_issues
}

pub fn find_connections(
    graph: &DataFlowGraph,
    config: &Config,
    verbosity: Verbosity,
    interner: &Interner,
) -> Vec<Issue> {
    let mut new_issues = vec![];

    let sources = graph
        .sources
        .iter()
        .filter(|(_, v)| matches!(v.kind, DataFlowNodeKind::DataSource { .. }))
        .map(|(_, v)| Arc::new(TaintedNode::from(v)))
        .collect::<Vec<_>>();

    if !matches!(verbosity, Verbosity::Quiet) {
        println!(" - initial sources count: {}", sources.len());
    }

    // for (from_id, to) in &graph.forward_edges {
    //     for (to_id, _) in to {
    //         println!("{} -> {}", from_id, to_id);
    //     }
    // }

    find_paths_to_sinks(
        sources,
        graph,
        config,
        verbosity,
        &mut new_issues,
        false,
        interner,
    );

    new_issues
}

#[inline]
fn find_paths_to_sinks(
    mut sources: Vec<Arc<TaintedNode>>,
    graph: &DataFlowGraph,
    config: &Config,
    verbosity: Verbosity,
    new_issues: &mut Vec<Issue>,
    match_sinks: bool,
    interner: &Interner,
) {
    let mut seen_sources = FxHashSet::default();

    for source in &sources {
        seen_sources.insert(source.get_unique_source_id());
    }

    if !match_sinks || !graph.sinks.is_empty() {
        for i in 0..config.security_config.max_depth {
            if !sources.is_empty() {
                let now = if matches!(verbosity, Verbosity::Debugging) {
                    Some(Instant::now())
                } else {
                    None
                };
                let mut actual_source_count = 0;
                let mut new_sources = Vec::new();

                for source in sources {
                    let inow = if matches!(verbosity, Verbosity::Debugging) {
                        Some(Instant::now())
                    } else {
                        None
                    };
                    let source_taints = source.taint_sinks.clone();
                    let source_id = source.id.clone();

                    let generated_sources = get_specialized_sources(graph, source);
                    actual_source_count += generated_sources.len();

                    for generated_source in generated_sources {
                        new_sources.extend(get_child_nodes(
                            graph,
                            config,
                            &generated_source,
                            &source_taints,
                            &mut seen_sources,
                            new_issues,
                            i == config.security_config.max_depth - 1,
                            match_sinks,
                            interner,
                        ))
                    }

                    if let Some(inow) = inow {
                        let ielapsed = inow.elapsed();
                        if ielapsed.as_millis() > 100 {
                            println!("    - took {:.2?} to generate from {}", ielapsed, source_id);
                        }
                    }
                }

                if !matches!(verbosity, Verbosity::Quiet) {
                    println!(
                        " - generated {}{}",
                        actual_source_count,
                        if let Some(now) = now {
                            let elapsed = now.elapsed();
                            format!(" sources in {:.2?}", elapsed)
                        } else {
                            "".to_string()
                        }
                    );
                }

                sources = new_sources;
            }
        }
    }
}

fn get_specialized_sources(
    graph: &DataFlowGraph,
    source: Arc<TaintedNode>,
) -> Vec<Arc<TaintedNode>> {
    let mut generated_sources = vec![];

    if graph.forward_edges.contains_key(&source.id) {
        generated_sources.push(source.clone());
    }

    if let (Some(specialization_key), Some(unspecialized_id)) =
        (&source.specialization_key, &source.unspecialized_id)
    {
        if graph.forward_edges.contains_key(unspecialized_id) {
            let mut new_source = (*source).clone();

            new_source.id = unspecialized_id.clone();
            new_source.unspecialized_id = None;
            new_source.specialization_key = None;

            new_source
                .specialized_calls
                .entry(specialization_key.clone())
                .or_insert_with(FxHashSet::default)
                .insert(new_source.id.clone());

            generated_sources.push(Arc::new(new_source));
        }
    } else if let Some(specializations) = graph.specializations.get(&source.id) {
        for specialization in specializations {
            if source.specialized_calls.is_empty()
                || source.specialized_calls.contains_key(specialization)
            {
                let new_id = format!("{}-{}", source.id, specialization);

                if graph.forward_edges.contains_key(&new_id) {
                    let mut new_source = (*source).clone();
                    new_source.id = new_id;

                    new_source.unspecialized_id = Some(source.id.clone());
                    new_source.specialized_calls.remove(specialization);

                    generated_sources.push(Arc::new(new_source));
                }
            }
        }
    } else {
        for (key, map) in &source.specialized_calls {
            if map.contains(&source.id) {
                let new_forward_edge_id = format!("{}-{}", source.id, key);

                if graph.forward_edges.contains_key(&new_forward_edge_id) {
                    let mut new_source = (*source).clone();
                    new_source.id = new_forward_edge_id;
                    new_source.unspecialized_id = Some(source.id.clone());
                    generated_sources.push(Arc::new(new_source));
                }
            }
        }
    }

    return generated_sources;
}

fn get_child_nodes(
    graph: &DataFlowGraph,
    config: &Config,
    generated_source: &Arc<TaintedNode>,
    source_taints: &FxHashSet<SinkType>,
    seen_sources: &mut FxHashSet<String>,
    new_issues: &mut Vec<Issue>,
    is_last: bool,
    match_sinks: bool,
    interner: &Interner,
) -> Vec<Arc<TaintedNode>> {
    let mut new_child_nodes = Vec::new();

    if let Some(forward_edges) = graph.forward_edges.get(&generated_source.id) {
        if !match_sinks {
            for t in source_taints {
                if let SinkType::Custom(target_id) = t {
                    if &generated_source.id == target_id {
                        let message = format!(
                            "Data found its way to {} using path {}",
                            target_id,
                            generated_source.get_trace(interner)
                        );
                        new_issues.push(Issue::new(
                            IssueKind::TaintedData(t.clone()),
                            message,
                            (**generated_source.pos.as_ref().unwrap()).clone(),
                        ));
                    }
                }
            }
        }

        for (to_id, path) in forward_edges {
            let destination_node = if let Some(n) = graph.vertices.get(to_id) {
                n
            } else if let Some(n) = graph.sinks.get(to_id) {
                n
            } else {
                println!("nothing found for {}", to_id);
                panic!();
            };

            // skip Exception::__construct, which looks too noisy
            if to_id == "Exception::__construct#1" {
                continue;
            }

            // if we're going through a scalar type guard and the last non-default path was
            // an array or property assignment, skip
            if let PathKind::ScalarTypeGuard = &path.kind {
                let foo = has_recent_assignment(&generated_source.path_types);

                if foo {
                    continue;
                }
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

            if !match_sinks {
                for t in source_taints {
                    if let SinkType::Custom(target_id) = t {
                        if to_id == target_id {
                            let message = format!(
                                "Data found its way to {} using path {}",
                                target_id,
                                generated_source.get_trace(interner)
                            );
                            new_issues.push(Issue::new(
                                IssueKind::TaintedData(t.clone()),
                                message,
                                (**generated_source.pos.as_ref().unwrap()).clone(),
                            ));
                        }
                    }
                }
            }

            let mut new_taints = source_taints.clone();
            if let Some(added_taints) = &path.added_taints {
                new_taints.extend(added_taints.clone());
            }

            if let Some(removed_taints) = &path.removed_taints {
                new_taints.retain(|t| !removed_taints.contains(t));
            }

            let mut new_destination = TaintedNode::from(destination_node);

            new_destination.previous = Some(generated_source.clone());
            new_destination.taint_sinks = new_taints.clone();
            new_destination.specialized_calls = generated_source.specialized_calls.clone();

            let mut new_path_types = generated_source.path_types.clone();

            new_path_types.push(match &path.kind {
                PathKind::RemoveDictKey(_) => PathKind::Default,
                _ => path.kind.clone(),
            });

            new_destination.path_types = new_path_types;

            if match_sinks {
                if let Some(sink) = graph.sinks.get(to_id) {
                    match &sink.kind {
                        DataFlowNodeKind::TaintSink { types, .. } => {
                            let mut matching_taints = types.clone();
                            matching_taints.retain(|t| new_taints.contains(t));

                            if !matching_taints.is_empty() {
                                if let Some(issue_pos) = &generated_source.pos {
                                    let taint_sources = generated_source.get_taint_sources();
                                    for taint_source in taint_sources {
                                        for matching_taint in &matching_taints {
                                            if let Some(pos) = &new_destination.pos {
                                                if !config.allow_sink_in_file(
                                                    &matching_taint,
                                                    interner.lookup(&pos.file_path),
                                                ) {
                                                    continue;
                                                }
                                            }

                                            new_destination.taint_sinks.remove(&matching_taint);

                                            let message = format!(
                                                "Data from {} found its way to {} using path {}",
                                                taint_source.get_error_message(),
                                                matching_taint.get_error_message(),
                                                new_destination.get_trace(interner)
                                            );
                                            new_issues.push(Issue::new(
                                                IssueKind::TaintedData(matching_taint.clone()),
                                                message,
                                                (**issue_pos).clone(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            let source_id = new_destination.get_unique_source_id();

            if seen_sources.contains(&source_id) {
                continue;
            }

            seen_sources.insert(source_id);

            if !is_last {
                new_child_nodes.push(Arc::new(new_destination));
            }
        }
    }

    new_child_nodes
}

fn has_recent_assignment(generated_path_types: &Vec<PathKind>) -> bool {
    let filtered_paths = generated_path_types
        .iter()
        .rev()
        .filter(|t| !matches!(t, PathKind::Default));

    let mut nesting = 0;

    for filtered_path in filtered_paths {
        if let PathKind::ExpressionAssignment(_, _) | PathKind::UnknownExpressionAssignment(_) =
            filtered_path
        {
            if nesting == 0 {
                return true;
            }

            nesting -= 1;
        }

        if let PathKind::ExpressionFetch(_, _) | PathKind::UnknownExpressionFetch(_) = filtered_path
        {
            nesting += 1;
        }
    }

    false
}

pub(crate) fn should_ignore_fetch(
    path_type: &PathKind,
    match_type: &PathExpressionKind,
    previous_path_types: &Vec<PathKind>,
) -> bool {
    // arraykey-fetch requires a matching arraykey-assignment at the same level
    // otherwise the tainting is not valid
    if match path_type {
        PathKind::ExpressionFetch(inner_expression_type, _) => inner_expression_type == match_type,
        PathKind::UnknownExpressionFetch(PathExpressionKind::ArrayKey) => {
            match_type == &PathExpressionKind::ArrayValue
        }
        _ => false,
    } {
        let mut fetch_nesting = 0;

        for previous_path_type in previous_path_types.iter().rev() {
            match &previous_path_type {
                PathKind::UnknownExpressionAssignment(inner) => {
                    if inner == match_type {
                        if fetch_nesting == 0 {
                            return false;
                        }

                        fetch_nesting -= 1;
                    }
                }
                PathKind::ExpressionAssignment(inner, previous_assignment_value) => {
                    if inner == match_type {
                        if fetch_nesting > 0 {
                            fetch_nesting -= 1;
                            continue;
                        }

                        if let PathKind::ExpressionFetch(_, fetch_value) = &path_type {
                            if fetch_value == previous_assignment_value {
                                return false;
                            }
                        }

                        return true;
                    }
                }
                PathKind::UnknownExpressionFetch(inner) | PathKind::ExpressionFetch(inner, _) => {
                    if inner == match_type {
                        fetch_nesting += 1;
                    }
                }
                _ => {}
            }
        }
    }

    if let PathKind::RemoveDictKey(key_name) = path_type {
        if match_type == &PathExpressionKind::ArrayValue {
            if let Some(PathKind::ExpressionAssignment(
                PathExpressionKind::ArrayValue,
                assigned_name,
            )) = previous_path_types
                .iter()
                .filter(|t| !matches!(t, PathKind::Default))
                .last()
            {
                if assigned_name == key_name {
                    return true;
                }
            }
        }
    }

    false
}
