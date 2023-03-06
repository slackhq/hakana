use crate::file_cache_provider::FileStatus;
use crate::{get_aast_for_path, get_relative_path, update_progressbar};
use hakana_aast_helper::ParserError;
use hakana_analyzer::config::{Config, Verbosity};
use hakana_analyzer::file_analyzer;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::{FileSource, StrId};
use indexmap::IndexMap;
use indicatif::{ProgressBar, ProgressStyle};
use oxidized::aast;
use oxidized::scoured_comments::ScouredComments;
use rustc_hash::{FxHashMap, FxHashSet};

use std::io;
use std::sync::{Arc, Mutex};

pub fn analyze_files(
    mut paths: Vec<String>,
    codebase: Arc<CodebaseInfo>,
    resolved_names: &FxHashMap<String, FxHashMap<usize, StrId>>,
    asts: FxHashMap<StrId, Vec<u8>>,
    config: Arc<Config>,
    analysis_result: &Arc<Mutex<AnalysisResult>>,
    filter: Option<String>,
    ignored_paths: &Option<FxHashSet<String>>,
    cache_dir: Option<&String>,
    _file_statuses: &IndexMap<String, FileStatus>,
    threads: u8,
    verbosity: Verbosity,
) -> io::Result<()> {
    let mut group_size = threads as usize;

    let mut path_groups = FxHashMap::default();

    if let Some(filter) = filter {
        paths.retain(|str_path| str_path.matches(filter.as_str()).count() > 0);
    }

    paths.retain(|str_path| {
        config.allow_issues_in_file(&get_relative_path(str_path, &config.root_dir))
    });

    if let Some(ignored_paths) = &ignored_paths {
        for ignored_path in ignored_paths {
            paths.retain(|str_path| str_path.matches(ignored_path.as_str()).count() == 0);
        }
    }

    let total_file_count = paths.len() as u64;

    if !matches!(verbosity, Verbosity::Quiet) {
        println!("Analyzing {} files", total_file_count);
    }

    if (paths.len() / group_size) < 4 {
        group_size = 1;
    }

    for (i, str_path) in paths.iter().enumerate() {
        let group = i % group_size;
        path_groups
            .entry(group)
            .or_insert_with(Vec::new)
            .push(str_path);
    }

    let bar = if matches!(verbosity, Verbosity::Simple) {
        let pb = ProgressBar::new(total_file_count);
        let sty = ProgressStyle::with_template("{bar:40.green/yellow} {pos:>7}/{len:7}").unwrap();
        pb.set_style(sty);
        Some(Arc::new(pb))
    } else {
        None
    };

    if path_groups.len() == 1 {
        let mut new_analysis_result =
            AnalysisResult::new(config.graph_kind, SymbolReferences::new());

        let asts = Arc::new(asts);

        for (i, str_path) in path_groups[&0].iter().enumerate() {
            if let Some(resolved_names) = resolved_names.get(*str_path) {
                analyze_file(
                    str_path,
                    cache_dir,
                    &codebase,
                    &config,
                    &mut new_analysis_result,
                    resolved_names,
                    verbosity,
                    &asts,
                );
            }

            update_progressbar(i as u64, bar.clone());
        }

        analysis_result.lock().unwrap().extend(new_analysis_result);
    } else {
        let mut handles = vec![];

        let files_processed = Arc::new(Mutex::new(0));

        let asts = Arc::new(asts);

        for (_, path_group) in path_groups {
            let codebase = codebase.clone();

            let pgc = path_group
                .iter()
                .map(|c| c.clone().clone())
                .collect::<Vec<_>>();

            let cache_dir_c = cache_dir.cloned();

            let analysis_result = analysis_result.clone();

            let analysis_config = config.clone();

            let files_processed = files_processed.clone();
            let bar = bar.clone();

            let resolved_names = resolved_names.clone();

            let asts = asts.clone();

            let handle = std::thread::spawn(move || {
                let mut new_analysis_result =
                    AnalysisResult::new(analysis_config.graph_kind, SymbolReferences::new());

                for str_path in &pgc {
                    if let Some(resolved_names) = resolved_names.get(str_path) {
                        analyze_file(
                            str_path,
                            cache_dir_c.as_ref(),
                            &codebase,
                            &analysis_config,
                            &mut new_analysis_result,
                            resolved_names,
                            verbosity,
                            &asts,
                        );
                    }

                    let mut tally = files_processed.lock().unwrap();
                    *tally += 1;

                    update_progressbar(*tally, bar.clone());
                }

                analysis_result.lock().unwrap().extend(new_analysis_result);
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    std::fs::remove_dir_all(format!("{}/.hakana_ast_cache", config.root_dir)).ok();

    if let Some(bar) = &bar {
        bar.finish_and_clear();
    }

    Ok(())
}

fn analyze_file(
    str_path: &String,
    cache_dir: Option<&String>,
    codebase: &Arc<CodebaseInfo>,
    config: &Arc<Config>,
    analysis_result: &mut AnalysisResult,
    resolved_names: &FxHashMap<usize, StrId>,
    verbosity: Verbosity,
    asts: &Arc<FxHashMap<StrId, Vec<u8>>>,
) {
    if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
        println!("Analyzing {}", &str_path);
    }

    let target_name = get_relative_path(str_path, &config.root_dir);

    let file_path = codebase.interner.get(target_name.as_str()).unwrap();

    let aast = if let Some(aast_result) = get_deserialized_ast(asts, file_path) {
        aast_result
    } else {
        match get_aast_for_path(str_path, &config.root_dir, cache_dir) {
            Ok(aast) => (aast.0, aast.1),
            Err(err) => {
                analysis_result.emitted_issues.insert(
                    target_name.clone(),
                    vec![match err {
                        ParserError::NotAHackFile => Issue::new(
                            IssueKind::InvalidHackFile,
                            "Invalid Hack file".to_string(),
                            HPos {
                                file_path,
                                start_offset: 0,
                                end_offset: 0,
                                start_line: 0,
                                end_line: 0,
                                start_column: 0,
                                end_column: 0,
                                insertion_start: None,
                            },
                            &None,
                        ),
                        ParserError::SyntaxError { message, mut pos } => {
                            pos.file_path = file_path;
                            Issue::new(IssueKind::InvalidHackFile, message, pos, &None)
                        }
                    }],
                );

                return;
            }
        }
    };

    let file_source = FileSource {
        is_production_code: true,
        file_path_actual: target_name,
        file_path,
        hh_fixmes: &aast.1.fixmes,
        comments: &aast.1.comments,
        file_contents: "".to_string(),
    };
    let mut file_analyzer =
        file_analyzer::FileAnalyzer::new(file_source, &resolved_names, codebase, config);
    file_analyzer.analyze(&aast.0, analysis_result);
}

fn get_deserialized_ast(
    asts: &Arc<FxHashMap<StrId, Vec<u8>>>,
    file_path: StrId,
) -> Option<(aast::Program<(), ()>, ScouredComments)> {
    if let Some(serialized_ast) = &asts.get(&file_path) {
        if let Ok(d) =
            bincode::deserialize::<(aast::Program<(), ()>, ScouredComments)>(&serialized_ast)
        {
            Some(d)
        } else {
            None
        }
    } else {
        None
    }
}
