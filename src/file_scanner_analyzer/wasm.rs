use hakana_aast_helper::name_context::NameContext;
use hakana_aast_helper::{get_aast_for_path_and_contents, ParserError};
use hakana_analyzer::config::{Config, Verbosity};
use hakana_analyzer::dataflow::program_analyzer::find_tainted_data;
use hakana_analyzer::file_analyzer;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::{FileSource, Interner, StrId, ThreadedInterner};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, Mutex};

use crate::populator::populate_codebase;
use crate::scanner::scan_file;
use crate::{HhiAsset, HslAsset};

pub fn scan_and_analyze_single_file(
    codebase: &mut CodebaseInfo,
    file_name: String,
    file_contents: String,
    find_unused_expressions: bool,
) -> std::result::Result<AnalysisResult, String> {
    let mut analysis_config = Config::new("".to_string(), FxHashSet::default());
    analysis_config.find_unused_expressions = find_unused_expressions;
    analysis_config.graph_kind = if file_contents.starts_with("// security-check")
        || file_contents.starts_with("//security-check")
    {
        GraphKind::WholeProgram(WholeProgramKind::Taint)
    } else {
        GraphKind::FunctionBody
    };

    let mut interner = ThreadedInterner::new(Arc::new(Mutex::new(codebase.interner.clone())));

    let resolved_names = if let Ok(resolved_names) = scan_single_file(
        codebase,
        &mut interner,
        file_name.clone(),
        file_contents.clone(),
    ) {
        resolved_names
    } else {
        FxHashMap::default()
    };

    let interner = Arc::try_unwrap(interner.parent)
        .unwrap()
        .into_inner()
        .unwrap();

    let mut symbol_references = SymbolReferences::new();

    populate_codebase(codebase, &interner, &mut symbol_references);

    codebase.interner = interner;

    let mut analysis_result = analyze_single_file(
        file_name.clone(),
        file_contents.clone(),
        &codebase,
        &resolved_names,
        &analysis_config,
    )?;

    if matches!(analysis_config.graph_kind, GraphKind::WholeProgram(_)) {
        let issues = find_tainted_data(
            &analysis_result.program_dataflow_graph,
            &analysis_config,
            Verbosity::Quiet,
            &codebase.interner,
        );

        for issue in issues {
            let file_path = codebase.interner.lookup(issue.pos.file_path);
            analysis_result
                .emitted_issues
                .entry(file_path.to_string())
                .or_insert_with(Vec::new)
                .push(issue);
        }
    }

    Ok(analysis_result)
}

pub fn get_single_file_codebase(additional_files: Vec<&str>) -> (CodebaseInfo, Interner) {
    let mut codebase = CodebaseInfo::new();
    let interner = Arc::new(Mutex::new(Interner::new()));

    let mut threaded_interner = ThreadedInterner::new(interner.clone());
    let empty_name_context = NameContext::new(&mut threaded_interner);

    // add HHVM libs
    for file in HhiAsset::iter() {
        scan_file(
            &file.to_string(),
            &"".to_string(),
            &FxHashSet::default(),
            &mut codebase,
            &mut threaded_interner,
            empty_name_context.clone(),
            false,
            Verbosity::Quiet,
        )
        .unwrap();
    }

    // add HHVM libs
    for file in HslAsset::iter() {
        scan_file(
            &file.to_string(),
            &"".to_string(),
            &FxHashSet::default(),
            &mut codebase,
            &mut threaded_interner,
            empty_name_context.clone(),
            false,
            Verbosity::Quiet,
        )
        .unwrap();
    }

    for str_path in additional_files {
        scan_file(
            &str_path.to_string(),
            &"".to_string(),
            &FxHashSet::default(),
            &mut codebase,
            &mut threaded_interner,
            empty_name_context.clone(),
            false,
            Verbosity::Quiet,
        )
        .unwrap();
    }

    drop(threaded_interner);

    let mutex = match Arc::try_unwrap(interner) {
        Ok(mutex) => mutex,
        Err(_) => {
            panic!("There's a lock somewhere")
        }
    };

    let interner = mutex.into_inner();

    let interner = match interner {
        Ok(interner) => interner,
        Err(err) => {
            println!("{}", err.to_string());
            panic!()
        }
    };

    let mut symbol_references = SymbolReferences::new();

    populate_codebase(&mut codebase, &interner, &mut symbol_references);

    (codebase, interner)
}

pub fn scan_single_file(
    codebase: &mut CodebaseInfo,
    interner: &mut ThreadedInterner,
    path: String,
    file_contents: String,
) -> std::result::Result<FxHashMap<usize, StrId>, ParserError> {
    let aast = match get_aast_for_path_and_contents(path.clone(), file_contents, None) {
        Ok(aast) => aast,
        Err(err) => return Err(err),
    };

    let file_path = interner.intern(path.clone());

    let name_context = NameContext::new(interner);

    let (resolved_names, uses) = hakana_aast_helper::scope_names(&aast.0, interner, name_context);

    hakana_reflector::collect_info_for_aast(
        &aast.0,
        &resolved_names,
        interner,
        codebase,
        &FxHashSet::default(),
        FileSource {
            file_path_actual: path.clone(),
            file_path,
            hh_fixmes: aast.1.fixmes,
            comments: aast.1.comments,
            file_contents: aast.2,
        },
        true,
        uses,
    );

    Ok(resolved_names)
}

pub fn analyze_single_file(
    path: String,
    file_contents: String,
    codebase: &CodebaseInfo,
    resolved_names: &FxHashMap<usize, StrId>,
    analysis_config: &Config,
) -> std::result::Result<AnalysisResult, String> {
    let aast_result = get_aast_for_path_and_contents(path.clone(), file_contents, None);

    let mut analysis_result =
        AnalysisResult::new(analysis_config.graph_kind, SymbolReferences::new());

    let file_path = codebase.interner.get(path.as_str()).unwrap();

    let aast = match aast_result {
        Ok(aast) => aast,
        Err(error) => match error {
            ParserError::NotAHackFile => return Err("Not a Hack file".to_string()),
            ParserError::SyntaxError { message, mut pos } => {
                pos.file_path = file_path;
                analysis_result.emitted_issues.insert(
                    path.clone(),
                    vec![Issue::new(IssueKind::InvalidHackFile, message, pos)],
                );

                return Ok(analysis_result);
            }
        },
    };

    let file_source = FileSource {
        file_path_actual: path.clone(),
        file_path,
        hh_fixmes: aast.1.fixmes,
        comments: aast.1.comments,
        file_contents: "".to_string(),
    };

    let mut file_analyzer =
        file_analyzer::FileAnalyzer::new(file_source, &resolved_names, codebase, analysis_config);

    file_analyzer.analyze(&aast.0, &mut analysis_result);

    Ok(analysis_result)
}
