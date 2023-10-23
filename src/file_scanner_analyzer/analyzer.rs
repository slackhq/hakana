use crate::{get_aast_for_path, update_progressbar, SuccessfulScanData};
use hakana_aast_helper::ParserError;
use hakana_analyzer::config::Config;
use hakana_analyzer::file_analyzer;
use hakana_logger::Logger;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::code_location::{FilePath, HPos};
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::{FileSource, Interner, StrId};
use indicatif::{ProgressBar, ProgressStyle};
use oxidized::aast;
use oxidized::scoured_comments::ScouredComments;
use rustc_hash::{FxHashMap, FxHashSet};

use std::sync::{Arc, Mutex};
use std::{fs, io};

pub fn analyze_files(
    mut paths: Vec<String>,
    scan_data: Arc<SuccessfulScanData>,
    asts: FxHashMap<FilePath, (aast::Program<(), ()>, ScouredComments)>,
    config: Arc<Config>,
    analysis_result: &Arc<Mutex<AnalysisResult>>,
    filter: Option<String>,
    ignored_paths: &Option<FxHashSet<String>>,
    threads: u8,
    logger: Arc<Logger>,
) -> io::Result<()> {
    let mut group_size = threads as usize;

    let mut path_groups = FxHashMap::default();

    if let Some(filter) = filter {
        paths.retain(|str_path| str_path.matches(filter.as_str()).count() > 0);
    }

    paths.retain(|str_path| config.allow_issues_in_file(&str_path));

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

        let asts = Arc::new(asts);

        for (i, str_path) in path_groups[&0].iter().enumerate() {
            let file_path = FilePath(interner.get(str_path).unwrap());
            if let Some(resolved_names) = resolved_names.get(&file_path) {
                analyze_file(
                    file_path,
                    str_path,
                    &codebase,
                    &interner,
                    &config,
                    &mut new_analysis_result,
                    resolved_names,
                    &logger,
                    &asts,
                );
            } else {
                new_analysis_result.emitted_issues.insert(
                    file_path,
                    vec![Issue::new(
                        IssueKind::InvalidHackFile,
                        "Invalid Hack file".to_string(),
                        HPos {
                            file_path,
                            start_offset: 1,
                            end_offset: 1,
                            start_line: 1,
                            end_line: 1,
                            start_column: 1,
                            end_column: 1,
                            insertion_start: None,
                        },
                        &None,
                    )],
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
            let scan_data = scan_data.clone();

            let pgc = path_group.iter().map(|c| (*c).clone()).collect::<Vec<_>>();

            let analysis_result = analysis_result.clone();

            let analysis_config = config.clone();

            let files_processed = files_processed.clone();
            let bar = bar.clone();

            let asts = asts.clone();

            let logger = logger.clone();

            let handle = std::thread::spawn(move || {
                let codebase = &scan_data.codebase;
                let interner = &scan_data.interner;
                let resolved_names = &scan_data.resolved_names;

                let mut new_analysis_result =
                    AnalysisResult::new(analysis_config.graph_kind, SymbolReferences::new());

                for str_path in &pgc {
                    let file_path = FilePath(interner.get(&str_path).unwrap());

                    if let Some(resolved_names) = resolved_names.get(&file_path) {
                        analyze_file(
                            file_path,
                            str_path,
                            &codebase,
                            &interner,
                            &analysis_config,
                            &mut new_analysis_result,
                            resolved_names,
                            &logger,
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

    if let Some(bar) = &bar {
        bar.finish_and_clear();
    }

    Ok(())
}

fn analyze_file(
    file_path: FilePath,
    str_path: &String,
    codebase: &CodebaseInfo,
    interner: &Interner,
    config: &Arc<Config>,
    analysis_result: &mut AnalysisResult,
    resolved_names: &FxHashMap<usize, StrId>,
    logger: &Logger,
    asts: &Arc<FxHashMap<FilePath, (aast::Program<(), ()>, ScouredComments)>>,
) {
    logger.log_debug_sync(&format!("Analyzing {}", &str_path));

    if let Some(aast) = asts.get(&file_path) {
        analyze_loaded_ast(
            str_path,
            file_path,
            aast,
            resolved_names,
            codebase,
            interner,
            config,
            analysis_result,
        );
    } else {
        let aast = match get_aast_for_path(file_path, str_path) {
            Ok(aast) => (aast.0, aast.1),
            Err(err) => {
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
                        ParserError::SyntaxError { message, pos } => {
                            Issue::new(IssueKind::InvalidHackFile, message, pos, &None)
                        }
                    }],
                );

                return;
            }
        };

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
    };
}

fn analyze_loaded_ast(
    str_path: &String,
    file_path: FilePath,
    aast: &(aast::Program<(), ()>, ScouredComments),
    resolved_names: &FxHashMap<usize, StrId>,
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
        file_contents: if config.migration_symbols.len() > 1 {
            match fs::read_to_string(str_path) {
                Ok(str_file) => str_file,
                Err(_) => panic!("Could not read {}", str_path),
            }
        } else {
            "".to_string()
        },
    };
    let mut file_analyzer =
        file_analyzer::FileAnalyzer::new(file_source, &resolved_names, codebase, interner, config);

    match file_analyzer.analyze(&aast.0, analysis_result) {
        Ok(()) => {}
        Err(err) => {
            analysis_result.emitted_issues.insert(
                file_path,
                vec![Issue::new(IssueKind::InternalError, err.0, err.1, &None)],
            );
        }
    };
}
