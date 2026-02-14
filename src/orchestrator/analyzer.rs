use crate::file::get_file_contents_hash;
use crate::{SuccessfulScanData, get_aast_for_path};
use hakana_analyzer::config::Config;
use hakana_analyzer::file_analyzer;
use hakana_code_info::FileSource;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::code_location::{FilePath, HPos};
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::file_info::ParserError;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::symbol_references::SymbolReferences;
use hakana_logger::Logger;
use hakana_str::{Interner, StrId};
use oxidized::aast;
use oxidized::scoured_comments::ScouredComments;
use rustc_hash::{FxHashMap, FxHashSet};

use std::sync::atomic::AtomicU32;
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
    files_processed: Option<Arc<AtomicU32>>,
) -> io::Result<()> {
    // Filter paths
    if let Some(filter) = filter {
        paths.retain(|str_path| str_path.matches(filter.as_str()).count() > 0);
    }

    paths.retain(|str_path| config.allow_issues_in_file(str_path));

    if let Some(ignored_paths) = &ignored_paths {
        for ignored_path in ignored_paths {
            paths.retain(|str_path| str_path.matches(ignored_path.as_str()).count() == 0);
        }
    }

    let process_fn = {
        let config = config.clone();
        let logger = logger.clone();

        move |str_path: String| -> (AnalysisResult, Duration) {
            let codebase = &scan_data.codebase;
            let interner = &scan_data.interner;
            let resolved_names = &scan_data.resolved_names;

            let mut new_analysis_result =
                AnalysisResult::new(config.graph_kind, SymbolReferences::new());

            let file_path = FilePath(interner.get(&str_path).unwrap());

            let duration = if let Some(resolved_names) = resolved_names.get(&file_path) {
                analyze_file(
                    file_path,
                    &str_path,
                    scan_data.file_system.file_hashes_and_times.get(&file_path),
                    codebase,
                    interner,
                    &config,
                    &mut new_analysis_result,
                    resolved_names,
                    &logger,
                )
            } else {
                Duration::default()
            };

            (new_analysis_result, duration)
        }
    };

    // Execute in parallel
    let results = hakana_executor::parallel_execute(
        paths,
        threads,
        logger,
        process_fn,
        files_processed,
        None,
    );

    // Aggregate results
    let mut total_duration = Duration::default();
    for (result, duration) in results {
        analysis_result.lock().unwrap().extend(result);
        total_duration += duration;
    }

    *file_analysis_time = total_duration;

    Ok(())
}

fn analyze_file(
    file_path: FilePath,
    str_path: &String,
    last_hash_and_time: Option<&(u64, u64)>,
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

        if let Some((file_hash, last_updated_time)) = last_hash_and_time {
            if updated_time != *last_updated_time
                && get_file_contents_hash(&str_path).unwrap_or(0) != *file_hash
            {
                analysis_result.has_invalid_hack_files = true;
                analysis_result
                    .changed_during_analysis_files
                    .insert(file_path);
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
