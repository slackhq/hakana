use crate::{file, get_aast_for_path, update_progressbar, SuccessfulScanData};
use hakana_analyzer::config::Config;
use hakana_analyzer::file_analyzer;
use hakana_logger::Logger;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::code_location::{FilePath, HPos};
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::file_info::ParserError;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::FileSource;
use hakana_str::{Interner, StrId};
use indicatif::{ProgressBar, ProgressStyle};
use oxidized::aast;
use oxidized::scoured_comments::ScouredComments;
use rustc_hash::{FxHashMap, FxHashSet};

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use std::{fs, io};

pub fn analyze_files(
    mut paths: Vec<String>,
    scan_data: Arc<SuccessfulScanData>,
    config: Arc<Config>,
    analysis_result: &Arc<Mutex<AnalysisResult>>,
    filter: Option<String>,
    ignored_paths: &Option<FxHashSet<String>>,
    threads: u8,
    logger: Arc<Logger>,
    file_analysis_time: &mut Duration,
) -> io::Result<()> {
    let mut group_size = threads as usize;

    let mut path_groups = FxHashMap::default();

    if let Some(filter) = filter {
        paths.retain(|str_path| str_path.matches(filter.as_str()).count() > 0);
    }

    paths.retain(|str_path| config.allow_issues_in_file(str_path));

    if let Some(ignored_paths) = &ignored_paths {
        for ignored_path in ignored_paths {
            paths.retain(|str_path| str_path.matches(ignored_path.as_str()).count() == 0);
        }
    }

    let total_file_count = paths.len() as u64;

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

    let bar = if logger.show_progress() {
        let pb = ProgressBar::new(total_file_count);
        let sty = ProgressStyle::with_template("{bar:40.green/yellow} {pos:>7}/{len:7}").unwrap();
        pb.set_style(sty);
        Some(Arc::new(pb))
    } else {
        None
    };

    if path_groups.len() == 1 {
        let codebase = &scan_data.codebase;
        let interner = &scan_data.interner;
        let resolved_names = &scan_data.resolved_names;

        let mut new_analysis_result =
            AnalysisResult::new(config.graph_kind, SymbolReferences::new());

        for (i, str_path) in path_groups[&0].iter().enumerate() {
            let file_path = FilePath(interner.get(str_path).unwrap());
            if let Some(resolved_names) = resolved_names.get(&file_path) {
                *file_analysis_time += analyze_file(
                    file_path,
                    str_path,
                    scan_data
                        .file_system
                        .file_hashes_and_times
                        .get(&file_path)
                        .map(|k| k.1),
                    codebase,
                    interner,
                    &config,
                    &mut new_analysis_result,
                    resolved_names,
                    &logger,
                );
            }

            update_progressbar(i as u64, bar.clone());
        }

        analysis_result.lock().unwrap().extend(new_analysis_result);
    } else {
        let mut handles = vec![];

        let files_processed = Arc::new(Mutex::new(0));

        let arc_file_analysis_time = Arc::new(Mutex::new(Duration::default()));

        for (_, path_group) in path_groups {
            let scan_data = scan_data.clone();

            let pgc = path_group.iter().map(|c| (*c).clone()).collect::<Vec<_>>();

            let analysis_result = analysis_result.clone();

            let analysis_config = config.clone();

            let files_processed = files_processed.clone();
            let bar = bar.clone();

            let logger = logger.clone();

            let arc_file_analysis_time = arc_file_analysis_time.clone();

            let handle = std::thread::spawn(move || {
                let codebase = &scan_data.codebase;
                let interner = &scan_data.interner;
                let resolved_names = &scan_data.resolved_names;

                let mut file_analysis_time = Duration::default();

                let mut new_analysis_result =
                    AnalysisResult::new(analysis_config.graph_kind, SymbolReferences::new());

                for str_path in &pgc {
                    let file_path = FilePath(interner.get(str_path).unwrap());

                    if let Some(resolved_names) = resolved_names.get(&file_path) {
                        file_analysis_time += analyze_file(
                            file_path,
                            str_path,
                            scan_data
                                .file_system
                                .file_hashes_and_times
                                .get(&file_path)
                                .map(|k| k.1),
                            codebase,
                            interner,
                            &analysis_config,
                            &mut new_analysis_result,
                            resolved_names,
                            &logger,
                        );
                    }

                    let mut tally = files_processed.lock().unwrap();
                    *tally += 1;

                    update_progressbar(*tally, bar.clone());
                }

                let mut t = arc_file_analysis_time.lock().unwrap();
                *t += file_analysis_time;
                analysis_result.lock().unwrap().extend(new_analysis_result);
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        *file_analysis_time = Arc::try_unwrap(arc_file_analysis_time)
            .unwrap()
            .into_inner()
            .unwrap();
    }

    if let Some(bar) = &bar {
        bar.finish_and_clear();
    }

    Ok(())
}

fn analyze_file(
    file_path: FilePath,
    str_path: &String,
    last_updated_time: Option<u64>,
    codebase: &CodebaseInfo,
    interner: &Interner,
    config: &Arc<Config>,
    analysis_result: &mut AnalysisResult,
    resolved_names: &FxHashMap<u32, StrId>,
    logger: &Logger,
) -> Duration {
    logger.log_debug_sync(&format!("Analyzing {}", &str_path));

    if let Ok(metadata) = fs::metadata(str_path) {
        let updated_time = metadata
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        if let Some(last_updated_time) = last_updated_time {
            if updated_time != last_updated_time {
                analysis_result.has_invalid_hack_files = true;
                analysis_result.emitted_issues.insert(
                    file_path,
                    vec![Issue::new(
                        IssueKind::InvalidHackFile,
                        "File changed during analysis".to_string(),
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
                    )],
                );

                return Duration::default();
            }
        }
    }

    let aast = match get_aast_for_path(file_path, str_path) {
        Ok(aast) => (aast.0, aast.1),
        Err(err) => {
            analysis_result.has_invalid_hack_files = true;
            analysis_result.emitted_issues.insert(
                file_path,
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
                    ParserError::CannotReadFile => Issue::new(
                        IssueKind::InvalidHackFile,
                        "Cannot read file".to_string(),
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
                    ParserError::SyntaxError { message, pos } => {
                        Issue::new(IssueKind::InvalidHackFile, message, pos, &None)
                    }
                }],
            );

            return Duration::default();
        }
    };

    let analyzed_files_now = Instant::now();

    analyze_loaded_ast(
        str_path,
        file_path,
        &aast,
        resolved_names,
        codebase,
        interner,
        config,
        analysis_result,
    );

    analyzed_files_now.elapsed()
}

fn analyze_loaded_ast(
    str_path: &String,
    file_path: FilePath,
    aast: &(aast::Program<(), ()>, ScouredComments),
    resolved_names: &FxHashMap<u32, StrId>,
    codebase: &CodebaseInfo,
    interner: &Interner,
    config: &Arc<Config>,
    analysis_result: &mut AnalysisResult,
) {
    let file_source = FileSource {
        is_production_code: true,
        file_path_actual: str_path.clone(),
        file_path,
        hh_fixmes: &aast.1.fixmes,
        comments: &aast.1.comments,
        file_contents: if !config.migration_symbols.is_empty() {
            match fs::read_to_string(str_path) {
                Ok(str_file) => str_file,
                Err(_) => panic!("Could not read {}", str_path),
            }
        } else {
            "".to_string()
        },
    };
    let mut file_analyzer =
        file_analyzer::FileAnalyzer::new(file_source, resolved_names, codebase, interner, config);

    match file_analyzer.analyze(&aast.0, analysis_result) {
        Ok(()) => {}
        Err(err) => {
            analysis_result.has_invalid_hack_files = true;
            analysis_result.emitted_issues.insert(
                file_path,
                vec![Issue::new(IssueKind::InternalError, err.0, err.1, &None)],
            );
        }
    };
}
