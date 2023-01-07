pub(crate) mod populator;

use crate::file_cache_provider::FileStatus;
use analyzer::analyze_files;
use diff::mark_safe_symbols_from_diff;
use hakana_aast_helper::get_aast_for_path_and_contents;
use hakana_analyzer::config::{Config, Verbosity};
use hakana_analyzer::dataflow::program_analyzer::{find_connections, find_tainted_data};
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::Interner;
use indexmap::IndexMap;
use indicatif::ProgressBar;
use oxidized::aast;
use oxidized::scoured_comments::ScouredComments;
use populator::populate_codebase;
use rust_embed::RustEmbed;
use rustc_hash::{FxHashMap, FxHashSet};
use scanner::scan_files;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};
use unused_symbols::find_unused_definitions;

mod analyzer;
mod ast_differ;
mod cache;
mod diff;
mod file_cache_provider;
mod scanner;
mod unused_symbols;
pub mod wasm;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../third-party/hhvm/hphp/hack/hhi"]
#[prefix = "hhi_embedded_"]
#[include = "*.hhi"]
#[include = "*.php"]
#[include = "*.hack"]
struct HhiAsset;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../third-party/hhvm/hphp/hsl/src"]
#[prefix = "hsl_embedded_"]
#[include = "*.hhi"]
#[include = "*.php"]
#[include = "*.hack"]
struct HslAsset;

pub fn scan_and_analyze(
    include_core_libs: bool,
    stubs_dirs: Vec<String>,
    filter: Option<String>,
    ignored_paths: Option<FxHashSet<String>>,
    config: Arc<Config>,
    cache_dir: Option<&String>,
    threads: u8,
    verbosity: Verbosity,
    header: &str,
    starter_data: Option<(CodebaseInfo, Interner)>,
) -> io::Result<AnalysisResult> {
    let mut all_scanned_dirs = stubs_dirs.clone();
    all_scanned_dirs.push(config.root_dir.clone());

    let now = Instant::now();

    let mut files_to_analyze = vec![];

    let (mut codebase, mut interner, file_statuses, resolved_names, codebase_diff) = scan_files(
        &all_scanned_dirs,
        include_core_libs,
        cache_dir,
        &mut files_to_analyze,
        &config,
        threads,
        verbosity,
        header,
        starter_data,
    )?;

    if let Some(cache_dir) = cache_dir {
        let timestamp_path = format!("{}/buildinfo", cache_dir);
        let mut timestamp_file = fs::File::create(&timestamp_path).unwrap();
        write!(timestamp_file, "{}", header).unwrap();

        let aast_manifest_path = format!("{}/manifest", cache_dir);
        let mut manifest_file = fs::File::create(&aast_manifest_path).unwrap();
        let mapped = file_statuses
            .iter()
            .filter(|(_, v)| match v {
                FileStatus::Deleted => false,
                _ => true,
            })
            .map(|(k, v)| {
                (
                    k.clone(),
                    match v {
                        FileStatus::Unchanged(a, b)
                        | FileStatus::Added(a, b)
                        | FileStatus::Modified(a, b) => (a, b),
                        FileStatus::Deleted => panic!(),
                    },
                )
            })
            .collect::<FxHashMap<_, _>>();
        let serialized_hashes = bincode::serialize(&mapped).unwrap();
        manifest_file
            .write_all(&serialized_hashes)
            .unwrap_or_else(|_| panic!("Could not write aast manifest {}", &aast_manifest_path));
    }

    let references_path = if let Some(cache_dir) = cache_dir {
        Some(format!("{}/references", cache_dir))
    } else {
        None
    };

    let issues_path = if let Some(cache_dir) = cache_dir {
        Some(format!("{}/issues", cache_dir))
    } else {
        None
    };

    let mut safe_symbols = FxHashSet::default();
    let mut safe_symbol_members = FxHashSet::default();
    let mut existing_issues = BTreeMap::new();
    let mut symbol_references = SymbolReferences::new();

    if config.ast_diff {
        if let Some(cached_analysis) = mark_safe_symbols_from_diff(
            &references_path,
            verbosity,
            codebase_diff,
            &codebase,
            &mut interner,
            &mut files_to_analyze,
            &config,
            &issues_path,
        ) {
            safe_symbols = cached_analysis.safe_symbols;
            safe_symbol_members = cached_analysis.safe_symbol_members;
            existing_issues = cached_analysis.existing_issues;
            symbol_references = cached_analysis.symbol_references;
        }
    }

    let elapsed = now.elapsed();

    if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
        println!("File discovery & scanning took {:.2?}", elapsed);
    }

    if !matches!(verbosity, Verbosity::Quiet) {
        println!("Calculating symbol inheritance");
    }

    populate_codebase(&mut codebase, &interner, &mut symbol_references);

    codebase.interner = interner;
    codebase.safe_symbols = safe_symbols;
    codebase.safe_symbol_members = safe_symbol_members;

    let now = Instant::now();

    let mut analysis_result = AnalysisResult::new(config.graph_kind, symbol_references);

    analysis_result.emitted_issues = existing_issues;

    let analysis_result = Arc::new(Mutex::new(analysis_result));

    let arc_codebase = Arc::new(codebase);

    analyze_files(
        files_to_analyze,
        arc_codebase.clone(),
        &resolved_names,
        config.clone(),
        &analysis_result,
        filter,
        &ignored_paths,
        None,
        &file_statuses,
        threads,
        verbosity,
    )?;

    let elapsed = now.elapsed();

    if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
        println!("File analysis took {:.2?}", elapsed);
    }

    let mut analysis_result = (*analysis_result.lock().unwrap()).clone();

    if let Some(references_path) = references_path {
        let mut symbols_file = fs::File::create(&references_path).unwrap();
        let serialized_symbol_references =
            bincode::serialize(&analysis_result.symbol_references).unwrap();
        symbols_file.write_all(&serialized_symbol_references)?;
    }

    if let Some(issues_path) = issues_path {
        let mut issues_file = fs::File::create(&issues_path).unwrap();
        let serialized_issues = bincode::serialize(&analysis_result.emitted_issues).unwrap();
        issues_file.write_all(&serialized_issues)?;
    }

    let mut codebase = Arc::try_unwrap(arc_codebase).unwrap();

    if config.find_unused_definitions {
        find_unused_definitions(&mut analysis_result, &config, &codebase, &ignored_paths);
    }

    let interner = codebase.interner;

    std::thread::spawn(move || {
        codebase.classlike_infos.clear();
        codebase.functionlike_infos.clear();
        codebase.constant_infos.clear();
        codebase.type_definitions.clear();
    });

    if let GraphKind::WholeProgram(whole_program_kind) = config.graph_kind {
        let issues = match whole_program_kind {
            WholeProgramKind::Taint => find_tainted_data(
                &analysis_result.program_dataflow_graph,
                &config,
                verbosity,
                &interner,
            ),
            WholeProgramKind::Query => find_connections(
                &analysis_result.program_dataflow_graph,
                &config,
                verbosity,
                &interner,
            ),
        };

        for issue in issues {
            analysis_result
                .emitted_issues
                .entry(interner.lookup(issue.pos.file_path).to_string())
                .or_insert_with(Vec::new)
                .push(issue);
        }
    }

    Ok(analysis_result)
}

fn find_files_in_dir(
    scan_dir: &String,
    config: &Config,
    files_to_analyze: &mut Vec<String>,
) -> IndexMap<String, u64> {
    let mut files_to_scan = IndexMap::new();

    let ignore_dirs = config
        .ignore_files
        .iter()
        .filter(|file| file.ends_with("/**"))
        .map(|file| file[0..(file.len() - 3)].to_string())
        .collect::<FxHashSet<_>>();

    let mut walker_builder = ignore::WalkBuilder::new(scan_dir);

    walker_builder
        .sort_by_file_path(|a, b| a.file_name().cmp(&b.file_name()))
        .follow_links(true);
    walker_builder.git_ignore(false);
    walker_builder.filter_entry(move |f| !ignore_dirs.contains(f.path().to_str().unwrap()));
    walker_builder.add_ignore(Path::new(".git"));

    let walker = walker_builder.build().into_iter().filter_map(|e| e.ok());

    for entry in walker {
        let path = entry.path();

        let metadata = if let Ok(metadata) = fs::metadata(&path) {
            metadata
        } else {
            println!("Could not get metadata");
            panic!();
        };

        if metadata.is_file() {
            if let Some(extension) = path.extension() {
                if extension.eq("hack") || extension.eq("php") || extension.eq("hhi") {
                    let path = path.to_str().unwrap().to_string();

                    files_to_scan.insert(
                        path.clone(),
                        metadata
                            .modified()
                            .unwrap()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_micros() as u64,
                    );

                    if !extension.eq("hhi") {
                        if matches!(config.graph_kind, GraphKind::WholeProgram(_)) {
                            if config.allow_taints_in_file(&path) {
                                files_to_analyze.push(path.clone());
                            }
                        } else {
                            files_to_analyze.push(path.clone());
                        }
                    }
                }
            }
        }
    }

    files_to_scan
}

pub fn get_aast_for_path(
    path: &String,
    root_dir: &String,
    cache_dir: Option<&String>,
) -> Result<(aast::Program<(), ()>, ScouredComments, String), String> {
    let file_contents = if path.starts_with("hsl_embedded_") {
        std::str::from_utf8(
            &HslAsset::get(path)
                .unwrap_or_else(|| panic!("Could not read HSL file {}", path))
                .data,
        )
        .unwrap_or_else(|_| panic!("Could not convert HSL file {}", path))
        .to_string()
    } else if path.starts_with("hhi_embedded_") {
        std::str::from_utf8(
            &HhiAsset::get(path)
                .unwrap_or_else(|| panic!("Could not read HSL file {}", path))
                .data,
        )
        .unwrap_or_else(|_| panic!("Could not convert HHI file {}", path))
        .to_string()
    } else if path.ends_with("tests/stubs/stubs.hack") {
        "function hakana_expect_type<T>(T $id): void {}".to_string()
    } else {
        match fs::read_to_string(path) {
            Ok(str_file) => str_file,
            Err(err) => return Err(err.to_string()),
        }
    };

    let mut local_path = path.clone();

    if local_path.starts_with(root_dir) {
        local_path = local_path.replace(root_dir, "");
        local_path = local_path[1..].to_string();
    }

    let aast_cache_dir = if let Some(cache_dir) = cache_dir {
        Some(format!("{}/ast", cache_dir))
    } else {
        None
    };

    get_aast_for_path_and_contents(local_path, file_contents, aast_cache_dir)
}

fn update_progressbar(percentage: u64, bar: Option<Arc<ProgressBar>>) {
    if let Some(bar) = bar {
        bar.set_position(percentage);
    }
}

fn get_relative_path(str_path: &String, config: &Config) -> String {
    if str_path.contains(&config.root_dir) {
        str_path[(config.root_dir.len() + 1)..].to_string()
    } else {
        str_path.clone()
    }
}
