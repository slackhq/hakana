use hakana_aast_helper::name_context::NameContext;
use hakana_aast_helper::{get_aast_for_path_and_contents, ParserError};
use hakana_analyzer::config::{Config, Verbosity};
use hakana_analyzer::dataflow::program_analyzer::find_tainted_data;
use hakana_analyzer::file_analyzer;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::code_location::FilePath;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::{FileSource, Interner, StrId, ThreadedInterner};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, Mutex};

use crate::file::VirtualFileSystem;
use crate::populator::populate_codebase;
use crate::scanner::scan_file;
use crate::{HhiAsset, HslAsset};

pub fn scan_and_analyze_single_file(
    codebase: &mut CodebaseInfo,
    interner: &Interner,
    file_name: String,
    file_contents: String,
    find_unused_expressions: bool,
) -> std::result::Result<(AnalysisResult, Interner), String> {
    let mut analysis_config = Config::new("".to_string(), FxHashSet::default());
    analysis_config.find_unused_expressions = find_unused_expressions;
    analysis_config.graph_kind = if file_contents.starts_with("// security-check")
        || file_contents.starts_with("//security-check")
    {
        GraphKind::WholeProgram(WholeProgramKind::Taint)
    } else {
        GraphKind::FunctionBody
    };

    let mut interner = ThreadedInterner::new(Arc::new(Mutex::new(interner.clone())));

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

    let mut analysis_result = analyze_single_file(
        file_name.clone(),
        file_contents.clone(),
        &codebase,
        &interner,
        &resolved_names,
        &analysis_config,
    )?;

    if matches!(analysis_config.graph_kind, GraphKind::WholeProgram(_)) {
        let issues = find_tainted_data(
            &analysis_result.program_dataflow_graph,
            &analysis_config,
            Verbosity::Quiet,
            &interner,
        );

        for issue in issues {
            analysis_result
                .emitted_issues
                .entry(issue.pos.file_path)
                .or_insert_with(Vec::new)
                .push(issue);
        }
    }

    Ok((analysis_result, interner))
}

pub fn get_single_file_codebase(
    additional_files: Vec<&str>,
) -> (CodebaseInfo, Interner, VirtualFileSystem) {
    let mut codebase = CodebaseInfo::new();
    let interner = Arc::new(Mutex::new(Interner::default()));

    let mut threaded_interner = ThreadedInterner::new(interner.clone());
    let empty_name_context = NameContext::new(&mut threaded_interner);

    let mut file_system = VirtualFileSystem::default();

    // add HHVM libs
    for file in HhiAsset::iter() {
        let interned_file_path = FilePath(threaded_interner.intern(file.to_string()));
        file_system
            .file_hashes_and_times
            .insert(interned_file_path, (0, 0));

        scan_file(
            &file.to_string(),
            interned_file_path,
            &FxHashSet::default(),
            &mut codebase,
            &mut threaded_interner,
            empty_name_context.clone(),
            false,
            false,
            Verbosity::Quiet,
        )
        .unwrap();
    }

    // add HHVM libs
    for file in HslAsset::iter() {
        let interned_file_path = FilePath(threaded_interner.intern(file.to_string()));
        file_system
            .file_hashes_and_times
            .insert(interned_file_path, (0, 0));

        scan_file(
            &file.to_string(),
            interned_file_path,
            &FxHashSet::default(),
            &mut codebase,
            &mut threaded_interner,
            empty_name_context.clone(),
            false,
            false,
            Verbosity::Quiet,
        )
        .unwrap();
    }

    for str_path in additional_files {
        let interned_file_path = FilePath(threaded_interner.intern(str_path.to_string()));
        file_system
            .file_hashes_and_times
            .insert(interned_file_path, (0, 0));

        scan_file(
            str_path,
            interned_file_path,
            &FxHashSet::default(),
            &mut codebase,
            &mut threaded_interner,
            empty_name_context.clone(),
            false,
            false,
            Verbosity::Quiet,
        )
        .unwrap();
    }

    drop(threaded_interner);

    let interner = Arc::try_unwrap(interner).unwrap().into_inner().unwrap();

    let mut symbol_references = SymbolReferences::new();

    populate_codebase(&mut codebase, &interner, &mut symbol_references);

    (codebase, interner, file_system)
}

pub fn scan_single_file(
    codebase: &mut CodebaseInfo,
    interner: &mut ThreadedInterner,
    path: String,
    file_contents: String,
) -> std::result::Result<FxHashMap<usize, StrId>, ParserError> {
    let aast = match get_aast_for_path_and_contents(&path, file_contents) {
        Ok(aast) => aast,
        Err(err) => return Err(err),
    };

    let file_path = FilePath(interner.intern(path.clone()));

    let name_context = NameContext::new(interner);

    let (resolved_names, uses) = hakana_aast_helper::scope_names(&aast.0, interner, name_context);

    hakana_reflector::collect_info_for_aast(
        &aast.0,
        &resolved_names,
        interner,
        codebase,
        &FxHashSet::default(),
        FileSource {
            is_production_code: true,
            file_path_actual: path.clone(),
            file_path,
            hh_fixmes: &aast.1.fixmes,
            comments: &aast.1.comments,
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
    interner: &Interner,
    resolved_names: &FxHashMap<usize, StrId>,
    analysis_config: &Config,
) -> std::result::Result<AnalysisResult, String> {
    let aast_result = get_aast_for_path_and_contents(&path, file_contents);

    let mut analysis_result =
        AnalysisResult::new(analysis_config.graph_kind, SymbolReferences::new());

    let file_path = FilePath(interner.get(path.as_str()).unwrap());

    let aast = match aast_result {
        Ok(aast) => aast,
        Err(error) => match error {
            ParserError::NotAHackFile => return Err("Not a Hack file".to_string()),
            ParserError::SyntaxError { message, mut pos } => {
                pos.file_path = file_path;
                analysis_result.emitted_issues.insert(
                    file_path,
                    vec![Issue::new(IssueKind::InvalidHackFile, message, pos, &None)],
                );

                return Ok(analysis_result);
            }
        },
    };

    let file_source = FileSource {
        is_production_code: true,
        file_path_actual: path.clone(),
        file_path,
        hh_fixmes: &aast.1.fixmes,
        comments: &aast.1.comments,
        file_contents: "".to_string(),
    };

    let mut file_analyzer = file_analyzer::FileAnalyzer::new(
        file_source,
        &resolved_names,
        codebase,
        interner,
        analysis_config,
    );

    file_analyzer.analyze(&aast.0, &mut analysis_result);

    Ok(analysis_result)
}
