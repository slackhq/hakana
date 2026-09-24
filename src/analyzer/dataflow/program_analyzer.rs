use hakana_code_info::data_flow::node::DataFlowNodeId;
use hakana_code_info::data_flow::node::DataFlowNodeKind;
use hakana_str::Interner;
use hakana_str::StrId;
use itertools::Itertools;
use log::{Level, info, log_enabled};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::sync::Arc;
use std::time::Instant;

use crate::config::Config;
use hakana_code_info::data_flow::graph::DataFlowGraph;
use hakana_code_info::data_flow::path::ArrayDataKind;
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::data_flow::tainted_node::TaintedNode;
use hakana_code_info::issue::Issue;
use hakana_code_info::issue::IssueKind;
use hakana_code_info::taint::{SinkType, get_sinks_for_sources};

pub fn find_tainted_data(
    graph: &DataFlowGraph,
    config: &Config,
    interner: &Interner,
    threads: u8,
) -> Vec<Issue> {
    let mut new_issues = vec![];

    let sources = graph
        .sources
        .values()
        .map(|v| Arc::new(TaintedNode::from(v)))
        .collect::<Vec<_>>();

    info!("Security analysis: detecting paths");
    info!(" - initial sources count: {}", sources.len());
    info!(" - initial sinks count:   {}", graph.sinks.len());

    find_paths_to_sinks(
        sources,
        graph,
        config,
        &mut new_issues,
        true,
        interner,
        true,
        threads,
    );

    // Graph/interner insertion order can vary between scans. Keep independent
    // source findings at the same sink stable in reports and snapshots.
    new_issues.sort_by(|a, b| {
        a.pos
            .cmp(&b.pos)
            .then_with(|| a.description.cmp(&b.description))
    });

    new_issues
}

pub fn find_connections(
    graph: &DataFlowGraph,
    config: &Config,
    interner: &Interner,
    threads: u8,
) -> Vec<Issue> {
    let mut new_issues = vec![];

    let sources = graph
        .sources
        .iter()
        .filter(|(_, v)| matches!(v.kind, DataFlowNodeKind::DataSource { .. }))
        .map(|(_, v)| Arc::new(TaintedNode::from(v)))
        .collect::<Vec<_>>();

    info!(" - initial sources count: {}", sources.len());

    find_paths_to_sinks(
        sources,
        graph,
        config,
        &mut new_issues,
        false,
        interner,
        false,
        threads,
    );

    new_issues
}

fn find_paths_to_sinks(
    mut sources: Vec<Arc<TaintedNode>>,
    graph: &DataFlowGraph,
    config: &Config,
    new_issues: &mut Vec<Issue>,
    match_sinks: bool,
    interner: &Interner,
    prune_unreachable: bool,
    threads: u8,
) {
    // Backward BFS is deliberately context-insensitive: it over-approximates
    // reachability. The forward BFS still validates every field, sanitizer and
    // call-context transition, so pruning cannot create an invalid witness.
    let sink_reachable = (match_sinks && prune_unreachable).then(|| nodes_reaching_sinks(graph));
    if let Some(reachable) = &sink_reachable {
        let before = sources.len();
        sources.retain(|source| reachable.contains(&reachability_id(&source.id)));
        info!(
            " - backward reachability retained {} of {} sources",
            sources.len(),
            before
        );
    }
    let mut seen_sources = FxHashSet::default();

    for source in &sources {
        seen_sources.insert(source.get_unique_source_id(interner));
    }

    let executor = FrontierExecutor::new(threads);
    info!(" - forward traversal workers: {}", executor.workers());

    if !match_sinks || !graph.sinks.is_empty() {
        for _ in 0..config.security_config.max_depth {
            if !sources.is_empty() {
                let now = if log_enabled!(Level::Debug) {
                    Some(Instant::now())
                } else {
                    None
                };
                let mut actual_source_count = 0;
                let mut new_sources = Vec::new();

                let mut file_nodes = FxHashMap::default();

                if executor.workers() == 1 {
                    // Merge immediately and release each old frontier reference
                    // as we go. The synchronous path needs no candidate buffers.
                    for source in sources {
                        actual_source_count += expand_source(
                            source,
                            graph,
                            config,
                            sink_reachable.as_ref(),
                            match_sinks,
                            interner,
                            new_issues,
                            &mut |id, destination| {
                                if seen_sources.insert(id) {
                                    if let Some(pos) = &destination.pos {
                                        *file_nodes
                                            .entry((pos.file_path, pos.start_line))
                                            .or_insert(0) += 1;
                                    }
                                    new_sources.push(Arc::new(destination));
                                }
                            },
                        );
                    }
                } else {
                    // Bound speculative expansion to a batch rather than buffering
                    // the entire next frontier. The visited set is read-only while
                    // workers run, then updated in original source/edge order.
                    for batch in sources.chunks(1024) {
                        let expansions = executor.expand(batch, |source| {
                            let mut expansion = Expansion::default();
                            expansion.source_count = expand_source(
                                source.clone(),
                                graph,
                                config,
                                sink_reachable.as_ref(),
                                match_sinks,
                                interner,
                                &mut expansion.issues,
                                &mut |id, destination| {
                                    if !seen_sources.contains(&id) {
                                        expansion.destinations.push((id, Arc::new(destination)));
                                    }
                                },
                            );
                            expansion
                        });
                        for expansion in expansions {
                            actual_source_count += expansion.source_count;
                            new_issues.extend(expansion.issues);
                            for (id, destination) in expansion.destinations {
                                if seen_sources.insert(id) {
                                    if let Some(pos) = &destination.pos {
                                        *file_nodes
                                            .entry((pos.file_path, pos.start_line))
                                            .or_insert(0) += 1;
                                    }
                                    new_sources.push(destination);
                                }
                            }
                        }
                    }
                }

                info!(
                    " - generated {} new destinations from {} sources{}",
                    new_sources.len(),
                    actual_source_count,
                    if let Some(now) = now {
                        let elapsed = now.elapsed();
                        format!(" in {:.2?}", elapsed)
                    } else {
                        "".to_string()
                    }
                );

                let top_files = file_nodes.iter().sorted_by(|a, b| b.1.cmp(a.1)).take(5);

                for top_file in top_files {
                    if *top_file.1 > 10000 {
                        info!(
                            "   - {} in {}:{}",
                            top_file.1,
                            top_file.0.0.get_relative_path(interner, &config.root_dir),
                            top_file.0.1,
                        );
                    }
                }

                sources = new_sources;
            } else {
                break;
            }
        }
    }

    if match_sinks {
        let unfinished = sources
            .iter()
            .filter(|source| {
                get_specialized_sources(graph, (*source).clone())
                    .iter()
                    .any(|generated| {
                        graph.forward_edges.get(&generated.id).is_some_and(|edges| {
                            edges.iter().any(|(id, paths)| {
                                paths.iter().any(|path| path.kind != PathKind::Aggregate)
                                    && sink_reachable.as_ref().is_none_or(|reachable| {
                                        reachable.contains(&reachability_id(id))
                                    })
                            })
                        })
                    })
            })
            .collect::<Vec<_>>();
        if !unfinished.is_empty()
            && let Some(pos) = unfinished
                .iter()
                .find_map(|source| source.pos.as_deref().copied())
                .or_else(|| graph.sinks.values().find_map(|sink| sink.get_pos()))
        {
            new_issues.push(Issue::new(
                IssueKind::TaintAnalysisIncomplete,
                format!("Security analysis reached max-depth {} with {} unfinished states that may reach a sink. Increase --max-depth; this run is incomplete.",
                    config.security_config.max_depth, unfinished.len()),
                pos, &None,
            ));
        }
    }
}

/// A private pool respects --threads without changing Rayon's process-wide pool.
/// WebAssembly and --threads 1 merge destinations synchronously as they expand.
struct FrontierExecutor {
    #[cfg(not(target_arch = "wasm32"))]
    pool: Option<rayon::ThreadPool>,
}

impl FrontierExecutor {
    fn new(threads: u8) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let pool = if threads > 1 {
                match rayon::ThreadPoolBuilder::new()
                    .num_threads(usize::from(threads))
                    .thread_name(|i| format!("hakana-dataflow-{i}"))
                    .build()
                {
                    Ok(pool) => Some(pool),
                    Err(error) => {
                        log::warn!("Cannot start dataflow workers; using one thread: {error}");
                        None
                    }
                }
            } else {
                None
            };
            Self { pool }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = threads;
            Self {}
        }
    }

    fn workers(&self) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(pool) = &self.pool {
            return pool.current_num_threads();
        }
        1
    }

    fn expand(
        &self,
        sources: &[Arc<TaintedNode>],
        expand: impl Fn(&Arc<TaintedNode>) -> Expansion + Sync + Send,
    ) -> Vec<Expansion> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(pool) = &self.pool {
            // Indexed collection preserves frontier order independently of
            // scheduling. Racing to insert visited states would change witnesses.
            return pool.install(|| sources.par_iter().map(expand).collect());
        }
        sources.iter().map(expand).collect()
    }
}

#[derive(Default)]
struct Expansion {
    destinations: Vec<(String, Arc<TaintedNode>)>,
    issues: Vec<Issue>,
    source_count: usize,
}

fn expand_source(
    source: Arc<TaintedNode>,
    graph: &DataFlowGraph,
    config: &Config,
    sink_reachable: Option<&FxHashSet<DataFlowNodeId>>,
    match_sinks: bool,
    interner: &Interner,
    new_issues: &mut Vec<Issue>,
    visit: &mut impl FnMut(String, TaintedNode),
) -> usize {
    let now = log_enabled!(Level::Debug).then(Instant::now);
    let generated_sources = get_specialized_sources(graph, source.clone());
    let source_count = generated_sources.len();
    for generated_source in generated_sources {
        get_child_nodes(
            graph,
            config,
            &generated_source,
            &source.taint_sinks,
            new_issues,
            sink_reachable,
            match_sinks,
            interner,
            visit,
        );
    }
    if let Some(now) = now {
        let elapsed = now.elapsed();
        if elapsed.as_millis() > 100 {
            info!(
                "    - took {:.2?} to generate from {}",
                elapsed,
                source.id.to_string(interner)
            );
        }
    }
    source_count
}

fn reachability_id(id: &DataFlowNodeId) -> DataFlowNodeId {
    match id {
        DataFlowNodeId::SpecializedCallTo(..)
        | DataFlowNodeId::SpecializedFunctionLikeArg(..)
        | DataFlowNodeId::SpecializedFunctionLikeOut(..)
        | DataFlowNodeId::SpecializedThisBeforeMethod(..)
        | DataFlowNodeId::SpecializedThisAfterMethod(..) => id.unspecialize().0,
        _ => id.clone(),
    }
}

fn nodes_reaching_sinks(graph: &DataFlowGraph) -> FxHashSet<DataFlowNodeId> {
    let mut predecessors: FxHashMap<DataFlowNodeId, Vec<DataFlowNodeId>> = FxHashMap::default();
    for (from, edges) in &graph.forward_edges {
        for (to, paths) in edges {
            if paths.iter().any(|path| path.kind != PathKind::Aggregate) {
                predecessors
                    .entry(reachability_id(to))
                    .or_default()
                    .push(reachability_id(from));
            }
        }
    }
    let mut reachable: FxHashSet<_> = graph.sinks.keys().map(reachability_id).collect();
    let mut queue: std::collections::VecDeque<_> = reachable.iter().cloned().collect();
    while let Some(node) = queue.pop_front() {
        if let Some(parents) = predecessors.get(&node) {
            for parent in parents {
                if reachable.insert(parent.clone()) {
                    queue.push_back(parent.clone());
                }
            }
        }
    }
    reachable
}

fn get_specialized_sources(
    graph: &DataFlowGraph,
    source: Arc<TaintedNode>,
) -> Vec<Arc<TaintedNode>> {
    let mut generated_sources = vec![];

    if graph.forward_edges.contains_key(&source.id) {
        generated_sources.push(source.clone());
    }

    if source.is_specialized {
        let (unspecialized_id, specialization_key) = source.id.unspecialize();
        if graph.forward_edges.contains_key(&unspecialized_id) {
            let mut new_source = (*source).clone();

            new_source.id = unspecialized_id;
            new_source.is_specialized = false;

            new_source
                .specialized_calls
                .entry(specialization_key)
                .or_default()
                .insert(new_source.id.clone());

            generated_sources.push(Arc::new(new_source));
        }
    } else if let Some(specializations) = graph.specializations.get(&source.id) {
        for specialization in specializations {
            if source.specialized_calls.is_empty()
                || source.specialized_calls.contains_key(specialization)
            {
                let new_id = source.id.specialize(specialization.0, specialization.1);

                if graph.forward_edges.contains_key(&new_id) {
                    let mut new_source = (*source).clone();
                    new_source.id = new_id;

                    new_source.is_specialized = false;
                    new_source.specialized_calls.remove(specialization);

                    generated_sources.push(Arc::new(new_source));
                }
            }
        }
    } else {
        for (key, map) in &source.specialized_calls {
            if map.contains(&source.id) {
                let new_forward_edge_id = source.id.specialize(key.0, key.1);

                if graph.forward_edges.contains_key(&new_forward_edge_id) {
                    let mut new_source = (*source).clone();
                    new_source.id = new_forward_edge_id;
                    new_source.is_specialized = false;
                    generated_sources.push(Arc::new(new_source));
                }
            }
        }
    }

    generated_sources
}

fn get_child_nodes(
    graph: &DataFlowGraph,
    config: &Config,
    generated_source: &Arc<TaintedNode>,
    source_taints: &Vec<SinkType>,
    new_issues: &mut Vec<Issue>,
    sink_reachable: Option<&FxHashSet<DataFlowNodeId>>,
    match_sinks: bool,
    interner: &Interner,
    visit: &mut impl FnMut(String, TaintedNode),
) {
    if let Some(forward_edges) = graph.forward_edges.get(&generated_source.id) {
        if !match_sinks {
            for t in source_taints {
                if let SinkType::Custom(target_id) = t
                    && &generated_source.id.to_string(interner) == target_id
                {
                    let message = format!(
                        "Data found its way to {} using path {}",
                        target_id,
                        generated_source.get_trace(interner, &config.root_dir)
                    );
                    new_issues.push(Issue::new(
                        IssueKind::TaintedData(Box::new(t.clone())),
                        message,
                        **generated_source.pos.as_ref().unwrap(),
                        &None,
                    ));
                }
            }
        }

        for (to_id, path) in forward_edges
            .iter()
            .flat_map(|(to_id, paths)| paths.iter().map(move |path| (to_id, path)))
        {
            if sink_reachable.is_some_and(|reachable| !reachable.contains(&reachability_id(to_id)))
            {
                continue;
            }
            let destination_node = if let Some(n) = graph.vertices.get(to_id) {
                n
            } else if let Some(n) = graph.sinks.get(to_id) {
                n
            } else {
                println!("nothing found for {}", to_id.to_string(interner));
                panic!();
            };

            if let PathKind::Aggregate = &path.kind {
                continue;
            }

            if should_ignore_superglobal_fetch(&path.kind, &generated_source.path_types) {
                continue;
            }

            // if we're going through a scalar type guard and the last non-default path was
            // an array or property assignment, skip
            if let PathKind::ScalarTypeGuard = &path.kind
                && has_recent_assignment(&generated_source.path_types)
            {
                continue;
            }

            if let PathKind::RefineSymbol(symbol_id) = &path.kind
                && has_unmatched_property_assignment(symbol_id, &generated_source.path_types)
            {
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

            if !match_sinks {
                for t in source_taints {
                    if let SinkType::Custom(target_id) = t
                        && &to_id.to_string(interner) == target_id
                    {
                        let message = format!(
                            "Data found its way to {} using path {}",
                            target_id,
                            generated_source.get_trace(interner, &config.root_dir)
                        );
                        new_issues.push(Issue::new(
                            IssueKind::TaintedData(Box::new(t.clone())),
                            message,
                            **generated_source.pos.as_ref().unwrap(),
                            &None,
                        ));
                    }
                }
            }

            let (transformed_sources, mut new_taints) = transform_sources(
                generated_source.get_taint_sources(),
                source_taints,
                &path.source_transforms,
            );
            new_taints.extend(path.added_taints.clone());
            new_taints.retain(|t| !path.removed_taints.contains(t));

            let mut new_destination = TaintedNode::from(destination_node);

            new_destination.previous = Some(generated_source.clone());
            new_destination.taint_sinks.clone_from(&new_taints);
            if !transformed_sources.is_empty() {
                new_destination.taint_sources = transformed_sources;
            }
            new_destination
                .specialized_calls
                .clone_from(&generated_source.specialized_calls);

            let mut new_path_types = generated_source.path_types.clone();

            new_path_types.push(match &path.kind {
                PathKind::RemoveDictKey(_) => PathKind::Default,
                _ => path.kind.clone(),
            });

            new_destination.path_types = new_path_types;

            if match_sinks
                && let Some(sink) = graph.sinks.get(to_id)
                && let DataFlowNodeKind::TaintSink {
                    types,
                    pos: sink_pos,
                    ..
                } = &sink.kind
            {
                let mut matching_sinks = types.clone();
                matching_sinks.retain(|t| new_taints.contains(t));
                if new_destination.has_html_safe_javascript(
                    matches!(path.kind, PathKind::AcceptHtmlSafeJson),
                ) {
                    matching_sinks.retain(|t| *t != SinkType::JavaScript);
                }

                if !matching_sinks.is_empty() {
                    let taint_sources = new_destination.get_taint_sources().to_vec();
                    for taint_source in &taint_sources {
                        for matching_sink in &matching_sinks {
                            if !config.allow_data_from_source_in_file(
                                taint_source,
                                matching_sink,
                                &new_destination,
                                interner,
                            ) {
                                continue;
                            }

                            new_destination.taint_sinks.retain(|s| s != matching_sink);

                            let message = format!(
                                "Data from {} found its way to {} using path {}",
                                taint_source.get_error_message(),
                                matching_sink.get_error_message(),
                                new_destination.get_trace(interner, &config.root_dir)
                            );
                            new_issues.push(Issue::new(
                                IssueKind::TaintedData(Box::new(matching_sink.clone())),
                                message,
                                *sink_pos,
                                &None,
                            ));
                        }
                    }
                }
            }

            let source_id = new_destination.get_unique_source_id(interner);

            // Reporting and clearing matched sinks must happen before either
            // visited check, including for paths merged in an earlier batch.
            visit(source_id, new_destination);
        }
    }
}

fn transform_sources(
    sources: &[hakana_code_info::taint::SourceType],
    sinks: &[SinkType],
    transforms: &[(
        hakana_code_info::taint::SourceType,
        hakana_code_info::taint::SourceType,
    )],
) -> (Vec<hakana_code_info::taint::SourceType>, Vec<SinkType>) {
    if transforms.is_empty() {
        return (vec![], sinks.to_vec());
    }
    let transformed: Vec<_> = sources
        .iter()
        .map(|source| {
            transforms
                .iter()
                .find(|(from, _)| from == source)
                .map_or_else(|| source.clone(), |(_, to)| to.clone())
        })
        .collect();
    let old_possible: FxHashSet<_> = sources.iter().flat_map(get_sinks_for_sources).collect();
    let new_possible: FxHashSet<_> = transformed.iter().flat_map(get_sinks_for_sources).collect();
    // Keep prior sanitization for sinks shared by the old and new source kinds,
    // and keep obligations belonging to source kinds that were not transformed.
    let mut new_sinks: Vec<_> = sinks
        .iter()
        .filter(|sink| !old_possible.contains(*sink) || new_possible.contains(*sink))
        .cloned()
        .collect();
    new_sinks.extend(new_possible.difference(&old_possible).cloned());
    (transformed, new_sinks)
}

fn should_ignore_superglobal_fetch(path: &PathKind, previous: &[PathKind]) -> bool {
    let PathKind::ArrayFetch(ArrayDataKind::ArrayValue, key) = path else {
        return false;
    };
    let mut depth = 0i32;
    for step in previous.iter().rev() {
        match step {
            PathKind::ArrayFetch(ArrayDataKind::ArrayValue, _)
            | PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue) => depth += 1,
            PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, _)
            | PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue) => depth -= 1,
            PathKind::Serialize | PathKind::Encode(hakana_code_info::data_flow::path::ValueEncoding::HtmlSafeJson) => return false,
            PathKind::Superglobal(name) => {
                return match name.as_str() {
                    "_SERVER" if depth == 0 => !hakana_code_info::taint::is_request_server_key(key),
                    "_FILES" if depth == 1 => {
                        !matches!(key.as_str(), "name" | "type" | "full_path")
                    }
                    _ => false,
                };
            }
            _ => {}
        }
    }
    false
}

fn has_recent_assignment(generated_path_types: &[PathKind]) -> bool {
    let filtered_paths = generated_path_types
        .iter()
        .rev()
        .filter(|t| !matches!(t, PathKind::Default));

    let mut nesting = 0;

    for filtered_path in filtered_paths {
        match filtered_path {
            PathKind::ArrayAssignment(_, _)
            | PathKind::UnknownArrayAssignment(_)
            | PathKind::PropertyAssignment(_, _)
            | PathKind::UnknownPropertyAssignment => {
                if nesting == 0 {
                    return true;
                }

                nesting -= 1;
            }
            PathKind::ArrayFetch(_, _)
            | PathKind::UnknownArrayFetch(_)
            | PathKind::PropertyFetch(_, _)
            | PathKind::UnknownPropertyFetch => {
                nesting += 1;
            }
            PathKind::Serialize | PathKind::Encode(hakana_code_info::data_flow::path::ValueEncoding::HtmlSafeJson) => {
                return false;
            }
            _ => (),
        }
    }

    false
}

fn has_unmatched_property_assignment(symbol: &StrId, generated_path_types: &[PathKind]) -> bool {
    let filtered_paths = generated_path_types
        .iter()
        .rev()
        .filter(|t| !matches!(t, PathKind::Default));

    let mut nesting = 0;

    for filtered_path in filtered_paths {
        match filtered_path {
            PathKind::PropertyAssignment(assignment_symbol, _) => {
                if assignment_symbol == symbol {
                    if nesting == 0 {
                        return false;
                    }

                    nesting -= 1;
                }
            }
            PathKind::UnknownPropertyAssignment => {
                if nesting == 0 {
                    return false;
                }

                nesting -= 1;
            }
            PathKind::PropertyFetch(fetch_symbol, _) => {
                if fetch_symbol == symbol {
                    nesting += 1;
                }
            }
            PathKind::UnknownPropertyFetch => {
                nesting += 1;
            }
            PathKind::Serialize | PathKind::Encode(hakana_code_info::data_flow::path::ValueEncoding::HtmlSafeJson) => {
                return false;
            }
            _ => (),
        }
    }

    true
}

pub(crate) fn should_ignore_array_fetch(
    path_type: &PathKind,
    match_type: &ArrayDataKind,
    previous_path_types: &[PathKind],
) -> bool {
    // arraykey-fetch requires a matching arraykey-assignment at the same level
    // otherwise the tainting is not valid
    if match path_type {
        PathKind::ArrayFetch(inner_expression_type, _) => inner_expression_type == match_type,
        PathKind::UnknownArrayFetch(ArrayDataKind::ArrayKey) => {
            match_type == &ArrayDataKind::ArrayValue
        }
        _ => false,
    } {
        let mut fetch_nesting = 0;

        for previous_path_type in previous_path_types.iter().rev() {
            match &previous_path_type {
                PathKind::UnknownArrayAssignment(inner) => {
                    if inner == match_type {
                        if fetch_nesting == 0 {
                            return false;
                        }

                        fetch_nesting -= 1;
                    }
                }
                PathKind::ArrayAssignment(inner, previous_assignment_value) => {
                    if inner == match_type {
                        if fetch_nesting > 0 {
                            fetch_nesting -= 1;
                            continue;
                        }

                        if let PathKind::ArrayFetch(_, fetch_value) = &path_type
                            && fetch_value == previous_assignment_value
                        {
                            return false;
                        }

                        return true;
                    }
                }
                PathKind::UnknownArrayFetch(inner) | PathKind::ArrayFetch(inner, _) => {
                    if inner == match_type {
                        fetch_nesting += 1;
                    }
                }
                _ => {}
            }
        }
    }

    if let PathKind::RemoveDictKey(key_name) = path_type
        && match_type == &ArrayDataKind::ArrayValue
        && let Some(PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, assigned_name)) =
            previous_path_types
                .iter()
                .rfind(|t| !matches!(t, PathKind::Default | PathKind::StringTransform | PathKind::StringComposition | PathKind::AcceptHtmlSafeJson))
        && assigned_name == key_name
    {
        return true;
    }

    false
}

pub(crate) fn should_ignore_property_fetch(
    path_type: &PathKind,
    previous_path_types: &[PathKind],
) -> bool {
    // arraykey-fetch requires a matching arraykey-assignment at the same level
    // otherwise the tainting is not valid
    if let PathKind::PropertyFetch(_, _) = path_type {
        let mut fetch_nesting = 0;

        for previous_path_type in previous_path_types.iter().rev() {
            match &previous_path_type {
                PathKind::UnknownPropertyAssignment => {
                    if fetch_nesting == 0 {
                        return false;
                    }

                    fetch_nesting -= 1;
                }
                PathKind::PropertyAssignment(_, previous_assignment_value) => {
                    if fetch_nesting > 0 {
                        fetch_nesting -= 1;
                        continue;
                    }

                    if let PathKind::PropertyFetch(_, fetch_value) = &path_type
                        && fetch_value == previous_assignment_value
                    {
                        return false;
                    }

                    return true;
                }
                PathKind::UnknownPropertyFetch | PathKind::PropertyFetch(_, _) => {
                    fetch_nesting += 1;
                }
                _ => {}
            }
        }
    }

    false
}

#[cfg(test)]
mod security_search_tests {
    use super::*;
    use hakana_code_info::code_location::{FilePath, HPos};
    use hakana_code_info::data_flow::graph::{GraphKind, WholeProgramKind};
    use hakana_code_info::data_flow::node::DataFlowNode;
    use hakana_code_info::function_context::FunctionLikeIdentifier;
    use hakana_code_info::taint::SourceType;

    fn position(interner: &mut Interner) -> HPos {
        HPos {
            file_path: FilePath(interner.intern("test.hack".into())),
            start_offset: 0,
            end_offset: 1,
            start_line: 1,
            end_line: 1,
            start_column: 1,
            end_column: 2,
        }
    }

    fn vertex(graph: &mut DataFlowGraph, id: DataFlowNodeId, pos: HPos) -> DataFlowNodeId {
        graph.add_node(DataFlowNode {
            id: id.clone(),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(pos),
                is_specialized: false,
            },
        });
        id
    }

    fn findings(graph: &DataFlowGraph, interner: &Interner, prune: bool) -> Vec<String> {
        let config = Config::new("project".into(), FxHashSet::default());
        let sources = graph
            .sources
            .values()
            .map(|node| Arc::new(TaintedNode::from(node)))
            .collect();
        let mut issues = vec![];
        find_paths_to_sinks(
            sources,
            graph,
            &config,
            &mut issues,
            true,
            interner,
            prune,
            1,
        );
        for threads in [2, 4] {
            let mut parallel = vec![];
            find_paths_to_sinks(
                graph
                    .sources
                    .values()
                    .map(|node| Arc::new(TaintedNode::from(node)))
                    .collect(),
                graph,
                &config,
                &mut parallel,
                true,
                interner,
                prune,
                threads,
            );
            assert_eq!(issues, parallel);
        }
        let mut messages: Vec<_> = issues.into_iter().map(|issue| issue.description).collect();
        messages.sort();
        messages
    }

    #[test]
    fn reverse_pruning_preserves_specialized_field_paths_and_parallel_edges() {
        let mut interner = Interner::default();
        let pos = position(&mut interner);
        let mut graph = DataFlowGraph::new(GraphKind::WholeProgram(WholeProgramKind::Taint));
        let source = DataFlowNodeId::String("request".into());
        graph.add_node(DataFlowNode {
            id: source.clone(),
            kind: DataFlowNodeKind::TaintSource {
                pos: Some(pos),
                types: vec![SourceType::UriRequestHeader],
            },
        });
        let sink = DataFlowNodeId::String("html".into());
        graph.add_node(DataFlowNode {
            id: sink.clone(),
            kind: DataFlowNodeKind::TaintSink {
                pos,
                types: vec![SinkType::HtmlTag],
            },
        });
        let method = FunctionLikeIdentifier::Function(interner.intern("identity".into()));
        let arg = DataFlowNode::get_for_method_argument(&method, 0, Some(pos), Some(pos));
        graph.add_node(arg.clone());
        let base = vertex(&mut graph, arg.id.unspecialize().0, pos);
        let data = vertex(&mut graph, DataFlowNodeId::String("fields".into()), pos);
        graph.add_path(&source, &arg.id, PathKind::Default, vec![], vec![]);
        graph.add_path(
            &base,
            &data,
            PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, "unsafe".into()),
            vec![],
            vec![],
        );
        // Same endpoints from another file/summary must not overwrite "unsafe".
        let mut extra = DataFlowGraph::new(graph.kind);
        extra.add_path(
            &base,
            &data,
            PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, "safe".into()),
            vec![],
            vec![SinkType::HtmlTag],
        );
        graph.add_graph(extra);
        graph.add_path(
            &data,
            &sink,
            PathKind::ArrayFetch(ArrayDataKind::ArrayValue, "unsafe".into()),
            vec![],
            vec![],
        );
        // A node-only meeting at this field would incorrectly create a second issue.
        graph.add_path(
            &data,
            &sink,
            PathKind::ArrayFetch(ArrayDataKind::ArrayValue, "other".into()),
            vec![],
            vec![],
        );
        let dead = vertex(&mut graph, DataFlowNodeId::String("dead".into()), pos);
        graph.add_path(&source, &dead, PathKind::Default, vec![], vec![]);
        graph.add_path(&dead, &sink, PathKind::Aggregate, vec![], vec![]);
        let reachable = nodes_reaching_sinks(&graph);
        assert!(reachable.contains(&source));
        assert!(!reachable.contains(&dead));
        let pruned = findings(&graph, &interner, true);
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned, findings(&graph, &interner, false));
    }

    #[test]
    fn source_transforms_preserve_other_sources_and_prior_sanitization() {
        let (sources, sinks) = transform_sources(
            &[SourceType::UriRequestHeader, SourceType::SystemSecret],
            &[SinkType::Sql, SinkType::Logging, SinkType::Output],
            &[(
                SourceType::UriRequestHeader,
                SourceType::NonUriRequestHeader,
            )],
        );
        assert_eq!(
            sources,
            vec![SourceType::NonUriRequestHeader, SourceType::SystemSecret]
        );
        assert!(sinks.contains(&SinkType::Logging));
        assert!(sinks.contains(&SinkType::Output));
        assert!(sinks.contains(&SinkType::Sql));
        assert!(!sinks.contains(&SinkType::HtmlTag));
    }

    #[test]
    fn source_kinds_with_the_same_sink_policy_have_distinct_visit_keys() {
        let mut interner = Interner::default();
        let pos = position(&mut interner);
        let node = |source| {
            TaintedNode::from(&DataFlowNode {
                id: DataFlowNodeId::String("shared".into()),
                kind: DataFlowNodeKind::TaintSource {
                    pos: Some(pos),
                    types: vec![source],
                },
            })
        };
        assert_ne!(
            node(SourceType::UserEmail).get_unique_source_id(&interner),
            node(SourceType::UserPII).get_unique_source_id(&interner),
        );
    }

    fn check_worker_equivalence(
        sources: Vec<Arc<TaintedNode>>,
        graph: &DataFlowGraph,
        config: &Config,
        interner: &Interner,
        match_sinks: bool,
    ) -> Vec<Issue> {
        let run = |threads| {
            let mut issues = vec![];
            find_paths_to_sinks(
                sources.clone(),
                graph,
                config,
                &mut issues,
                match_sinks,
                interner,
                match_sinks,
                threads,
            );
            issues
        };
        let serial = run(1);
        // Compare complete ordered witnesses and warning positions, not just counts.
        for threads in [2, 4, 4] {
            assert_eq!(serial, run(threads));
        }
        serial
    }

    #[test]
    fn parallel_batches_preserve_global_deduplication_and_report_before_dedup() {
        let mut interner = Interner::default();
        let pos = position(&mut interner);
        let mut graph = DataFlowGraph::new(GraphKind::WholeProgram(WholeProgramKind::Taint));
        let join = vertex(&mut graph, DataFlowNodeId::String("join".into()), pos);
        let tail = vertex(&mut graph, DataFlowNodeId::String("tail".into()), pos);
        let sink = DataFlowNodeId::String("html".into());
        graph.add_node(DataFlowNode {
            id: sink.clone(),
            kind: DataFlowNodeKind::TaintSink {
                pos,
                types: vec![SinkType::HtmlTag],
            },
        });
        let mut sources = vec![];
        // Cross several bounded parallel batches; equivalent origins must merge
        // globally, but every direct source-to-sink witness must still report.
        for i in 0..2050 {
            let node = DataFlowNode {
                id: DataFlowNodeId::String(format!("request-{i}")),
                kind: DataFlowNodeKind::TaintSource {
                    pos: Some(pos),
                    types: vec![SourceType::UriRequestHeader],
                },
            };
            sources.push(Arc::new(TaintedNode::from(&node)));
            graph.add_path(&node.id, &join, PathKind::Default, vec![], vec![]);
            graph.add_path(&node.id, &sink, PathKind::Default, vec![], vec![]);
            graph.add_node(node);
        }
        graph.add_path(&join, &tail, PathKind::Default, vec![], vec![]);
        graph.add_path(&tail, &join, PathKind::Default, vec![], vec![]);
        graph.add_path(&tail, &sink, PathKind::Default, vec![], vec![]);
        let mut config = Config::new("project".into(), FxHashSet::default());
        config.security_config.max_depth = 2;
        let issues = check_worker_equivalence(sources.clone(), &graph, &config, &interner, true);
        assert_eq!(issues.len(), 2051);
        let warning = issues.last().unwrap();
        assert_eq!(warning.kind, IssueKind::TaintAnalysisIncomplete);
        assert!(warning.description.contains("1 unfinished states"));

        config.security_config.max_depth = 10;
        let issues = check_worker_equivalence(sources, &graph, &config, &interner, true);
        assert_eq!(issues.len(), 2051);
        assert!(issues.last().unwrap().description.contains("request-0"));
        assert!(
            issues
                .iter()
                .all(|issue| issue.kind != IssueKind::TaintAnalysisIncomplete)
        );
    }

    #[test]
    fn parallel_queries_preserve_target_reports_and_cycles() {
        let mut interner = Interner::default();
        let pos = position(&mut interner);
        let mut graph = DataFlowGraph::new(GraphKind::WholeProgram(WholeProgramKind::Query));
        let join = vertex(&mut graph, DataFlowNodeId::String("join".into()), pos);
        let target = vertex(&mut graph, DataFlowNodeId::String("target".into()), pos);
        let mut sources = vec![];
        for i in 0..1050 {
            let node = DataFlowNode {
                id: DataFlowNodeId::String(format!("origin-{i}")),
                kind: DataFlowNodeKind::DataSource {
                    pos,
                    target_id: target.to_string(&interner),
                },
            };
            sources.push(Arc::new(TaintedNode::from(&node)));
            graph.add_path(&node.id, &join, PathKind::Default, vec![], vec![]);
            graph.add_path(&node.id, &target, PathKind::Default, vec![], vec![]);
            graph.add_node(node);
        }
        graph.add_path(&join, &target, PathKind::Default, vec![], vec![]);
        graph.add_path(&target, &join, PathKind::Default, vec![], vec![]);
        let config = Config::new("project".into(), FxHashSet::default());
        let issues = check_worker_equivalence(sources, &graph, &config, &interner, false);
        assert_eq!(issues.len(), 1052);
    }

    #[test]
    fn parallel_reporting_preserves_path_specific_ignores() {
        let mut interner = Interner::default();
        let pos = position(&mut interner);
        let ignored_pos = HPos {
            file_path: FilePath(interner.intern("project/ignored.hack".into())),
            ..pos
        };
        let mut config = Config::new("project".into(), FxHashSet::default());
        let config_path = std::env::temp_dir().join(format!(
            "hakana-parallel-ignore-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::write(
            &config_path,
            r#"{"security_analysis":{"ignore_files":[],"ignore_sink_files":{
                "UriRequestHeader -> HtmlTag":["ignored.hack"]
            }}}"#,
        )
        .unwrap();
        let loaded = config.update_from_file(&"project".into(), &config_path, &mut interner);
        std::fs::remove_file(config_path).unwrap();
        loaded.unwrap();

        let mut graph = DataFlowGraph::new(GraphKind::WholeProgram(WholeProgramKind::Taint));
        let mut sources = vec![];
        for (id, source_pos) in [("ignored", ignored_pos), ("reported", pos)] {
            let node = DataFlowNode {
                id: DataFlowNodeId::String(id.into()),
                kind: DataFlowNodeKind::TaintSource {
                    pos: Some(source_pos),
                    types: vec![SourceType::UriRequestHeader],
                },
            };
            sources.push(Arc::new(TaintedNode::from(&node)));
            graph.add_node(node);
        }
        let sink = DataFlowNodeId::String("sink".into());
        graph.add_node(DataFlowNode {
            id: sink.clone(),
            kind: DataFlowNodeKind::TaintSink {
                pos,
                types: vec![SinkType::HtmlTag],
            },
        });
        for source in &sources {
            graph.add_path(&source.id, &sink, PathKind::Default, vec![], vec![]);
        }
        let issues = check_worker_equivalence(sources, &graph, &config, &interner, true);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].description.contains("reported"));
    }
}
