use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::time::Instant;

use super::HhiAsset;
use super::HslAsset;
use crate::SuccessfulScanData;
use crate::ast_differ;
use crate::cache::get_file_manifest;
use crate::cache::load_cached_aast_names;
use crate::cache::load_cached_codebase;
use crate::cache::load_cached_interner;
use crate::file::FileStatus;
use crate::file::VirtualFileSystem;
use crate::get_aast_for_path;
use ast_differ::get_diff;
use hakana_aast_helper::name_context::NameContext;
use hakana_analyzer::config::Config;
use hakana_code_info::FileSource;
use hakana_code_info::code_location::FilePath;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::codebase_info::symbols::SymbolKind;
use hakana_code_info::diff::CodebaseDiff;
use hakana_code_info::file_info::FileInfo;
use hakana_code_info::file_info::ParserError;
use hakana_logger::Logger;
use hakana_str::Interner;
use hakana_str::StrId;
use hakana_str::ThreadedInterner;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

#[derive(Debug)]
pub struct ScanFilesResult {
    pub codebase: CodebaseInfo,
    pub interner: Interner,
    pub file_system: VirtualFileSystem,
    pub resolved_names: FxHashMap<FilePath, FxHashMap<u32, StrId>>,
    pub codebase_diff: CodebaseDiff,
    pub files_to_analyze: Vec<String>,
    pub invalid_files: FxHashSet<FilePath>,
}

struct FileScanResult {
    codebase: CodebaseInfo,
    file_path: FilePath,
    resolved_names: Option<FxHashMap<u32, StrId>>,
    is_invalid: bool,
}

pub fn scan_files(
    scan_dirs: &Vec<String>,
    cache_dir: Option<&String>,
    config: &Arc<Config>,
    threads: u8,
    logger: Arc<Logger>,
    build_checksum: &str,
    starter_interner: &Arc<Interner>,
    starter_data: Option<SuccessfulScanData>,
    language_server_changes: Option<FxHashMap<String, FileStatus>>,
    files_scanned: Option<Arc<AtomicU32>>,
    total_files_to_scan: Option<Arc<AtomicU32>>,
) -> io::Result<ScanFilesResult> {
    logger.log_debug_sync(&format!("{:#?}", scan_dirs));

    let mut files_to_scan = vec![];

    let mut files_to_analyze = vec![];

    let codebase_path = cache_dir.map(|cache_dir| format!("{}/codebase", cache_dir));

    let symbols_path = cache_dir.map(|cache_dir| format!("{}/interned_names", cache_dir));

    let aast_names_path = cache_dir.map(|cache_dir| format!("{}/aast_strids", cache_dir));

    let mut use_codebase_cache = true;

    if let (Some(cache_dir), Some(codebase_path_unwrapped)) = (cache_dir, codebase_path.clone()) {
        let build_checksum_path = format!("{}/buildinfo", cache_dir);
        let build_checksum_path = Path::new(&build_checksum_path);

        if build_checksum_path.exists() {
            if let Ok(contents) = fs::read_to_string(build_checksum_path) {
                if contents != build_checksum {
                    use_codebase_cache = false;
                }
            } else {
                use_codebase_cache = false;
            }
        } else {
            use_codebase_cache = false;
        }

        if !use_codebase_cache && Path::new(&codebase_path_unwrapped).exists() {
            fs::remove_file(&codebase_path_unwrapped).unwrap();
        }
    }

    let has_starter = starter_data.is_some();

    let mut existing_file_system = None;

    let mut interner;
    let mut codebase;
    let mut resolved_names;

    if let Some(starter_data) = starter_data {
        existing_file_system = Some(starter_data.file_system);
        interner = starter_data.interner;
        codebase = starter_data.codebase;
        resolved_names = starter_data.resolved_names;
    } else {
        interner = (**starter_interner).clone();
        codebase = CodebaseInfo::new();
        resolved_names = FxHashMap::default();
    }

    if existing_file_system.is_none() && use_codebase_cache {
        if let Some(cache_dir) = cache_dir {
            existing_file_system = get_file_manifest(cache_dir);
        };
    }

    let file_discovery_now = Instant::now();
    let load_from_cache_now = Instant::now();

    if let Some(symbols_path) = &symbols_path {
        if let Some(cached_interner) =
            load_cached_interner(symbols_path, use_codebase_cache, &logger)
        {
            interner = cached_interner;
        }
    }

    let file_system = if let Some(language_server_changes) = language_server_changes {
        let mut file_system = existing_file_system.clone().unwrap();

        file_system.apply_language_server_changes(
            language_server_changes,
            &mut files_to_scan,
            &mut interner,
            config,
            &mut files_to_analyze,
        );

        file_system
    } else {
        get_filesystem(
            &mut files_to_scan,
            &mut interner,
            &logger,
            scan_dirs,
            &existing_file_system,
            config,
            cache_dir,
            &mut files_to_analyze,
            None,
        )
    };

    let file_discovery_elapsed = file_discovery_now.elapsed();

    if logger.can_log_timing() {
        logger.log_sync(&format!(
            "File discovery took {:.2?}",
            file_discovery_elapsed
        ));
    }

    let file_statuses =
        file_system.get_file_statuses(&files_to_scan, &interner, &existing_file_system);

    let changed_files = file_statuses
        .iter()
        .filter(|(_, v)| !matches!(v, FileStatus::Unchanged(..)))
        .map(|(k, _)| *k)
        .collect::<FxHashSet<_>>();

    // this needs to come after we've loaded interned strings
    if !has_starter {
        if let Some(codebase_path) = &codebase_path {
            if let Some(cache_codebase) =
                load_cached_codebase(codebase_path, use_codebase_cache, &logger)
            {
                codebase = cache_codebase;
            }
        }
    }

    if let Some(aast_names_path) = &aast_names_path {
        if let Some(cached_resolved_names) =
            load_cached_aast_names(aast_names_path, use_codebase_cache, &logger)
        {
            resolved_names = cached_resolved_names
        };
    }

    let load_from_cache_elapsed = load_from_cache_now.elapsed();

    if logger.can_log_timing() {
        logger.log_sync(&format!(
            "Loading serialised codebase information from cache took {:.2?}",
            load_from_cache_elapsed
        ));
    }

    invalidate_changed_codebase_elements(&mut codebase, &changed_files);

    let mut files_to_scan = vec![];

    for (target_file, status) in &file_statuses {
        if matches!(status, FileStatus::Added(..) | FileStatus::Modified(..)) {
            files_to_scan.push(target_file);
        }
    }

    let mut existing_changed_files = FxHashMap::default();

    let files = codebase.files;

    let mut existing_unchanged_files = FxHashMap::default();

    for (file_id, file_info) in files {
        if changed_files.contains(&file_id) {
            existing_changed_files.insert(file_id, file_info);
        } else {
            existing_unchanged_files.insert(file_id, file_info);
        }
    }

    codebase.files = existing_unchanged_files;

    // get the full list of unchanged symbols
    let mut codebase_diff = if config.ast_diff {
        let mut codebase_diff = get_diff(&codebase.files, &codebase.files);

        for (target_file, status) in &file_statuses {
            if let FileStatus::Deleted = status {
                if let Some(deleted_file_info) = existing_changed_files.get(target_file) {
                    for node in &deleted_file_info.ast_nodes {
                        codebase_diff
                            .add_or_delete
                            .insert((node.name, StrId::EMPTY));
                    }
                }
            }
        }

        codebase_diff
    } else {
        CodebaseDiff::default()
    };

    let interner = Arc::new(Mutex::new(interner));
    let resolved_names = Arc::new(Mutex::new(resolved_names));

    let has_new_files = !files_to_scan.is_empty() || !changed_files.is_empty();

    let invalid_files = Arc::new(Mutex::new(vec![]));

    if !files_to_scan.is_empty() {
        let file_scanning_now = Instant::now();

        // Convert file references to owned FilePaths
        let file_paths: Vec<FilePath> = files_to_scan.into_iter().map(|fp| *fp).collect();

        let analyze_map: FxHashSet<String> = files_to_analyze.clone().into_iter().collect();

        // Create the process function
        let process_fn = {
            let interner = interner.clone();
            let config = config.clone();
            let logger = logger.clone();

            move |file_path: FilePath| -> FileScanResult {
                let mut new_codebase = CodebaseInfo::new();
                let mut new_interner = ThreadedInterner::new(interner.clone());
                let empty_name_context = NameContext::new(&mut new_interner);

                let str_path = new_interner
                    .parent
                    .lock()
                    .unwrap()
                    .lookup(&file_path.0)
                    .to_string();

                match scan_file(
                    &str_path,
                    file_path,
                    &config.all_custom_issues,
                    &mut new_codebase,
                    &mut new_interner,
                    empty_name_context.clone(),
                    analyze_map.contains(&str_path),
                    !config.test_files.iter().any(|p| p.matches(&str_path)),
                    &logger,
                ) {
                    Ok(scanner_result) => FileScanResult {
                        codebase: new_codebase,
                        file_path,
                        resolved_names: Some(scanner_result),
                        is_invalid: false,
                    },
                    Err(parser_error) => {
                        new_codebase.files.insert(
                            file_path,
                            FileInfo {
                                parser_errors: vec![parser_error],
                                ..FileInfo::default()
                            },
                        );
                        FileScanResult {
                            codebase: new_codebase,
                            file_path,
                            resolved_names: None,
                            is_invalid: true,
                        }
                    }
                }
            }
        };

        // Execute in parallel
        let results = hakana_executor::parallel_execute(
            file_paths,
            threads,
            logger.clone(),
            process_fn,
            files_scanned,
            total_files_to_scan,
        );

        // Aggregate results
        let mut local_resolved_names = FxHashMap::default();
        let mut local_invalid_files = vec![];

        for result in results {
            if config.ast_diff {
                codebase_diff.extend(get_diff(&existing_changed_files, &result.codebase.files));
            }

            codebase.extend(result.codebase);

            if let Some(names) = result.resolved_names {
                local_resolved_names.insert(result.file_path, names);
            }

            if result.is_invalid {
                local_invalid_files.push(result.file_path);
            }
        }

        resolved_names.lock().unwrap().extend(local_resolved_names);
        invalid_files.lock().unwrap().extend(local_invalid_files);

        let file_scanning_elapsed = file_scanning_now.elapsed();

        if logger.can_log_timing() {
            logger.log_sync(&format!(
                "Scanning files took {:.2?}",
                file_scanning_elapsed
            ));
        }
    }

    let interner = Arc::try_unwrap(interner).unwrap().into_inner().unwrap();
    let invalid_files = Arc::try_unwrap(invalid_files)
        .unwrap()
        .into_inner()
        .unwrap();

    let resolved_names = Arc::try_unwrap(resolved_names)
        .unwrap()
        .into_inner()
        .unwrap();

    if has_new_files {
        if let Some(codebase_path) = codebase_path {
            let mut codebase_file = fs::File::create(codebase_path).unwrap();
            let serialized_codebase = bincode::serialize(&codebase).unwrap();
            codebase_file.write_all(&serialized_codebase)?;
        }

        if let Some(symbols_path) = symbols_path {
            let mut symbols_file = fs::File::create(symbols_path).unwrap();
            let serialized_symbols = bincode::serialize(&interner).unwrap();
            symbols_file.write_all(&serialized_symbols)?;
        }

        if let Some(aast_names_path) = aast_names_path {
            let mut aast_names_file = fs::File::create(aast_names_path).unwrap();
            let serialized_aast_names = bincode::serialize(&resolved_names).unwrap();
            aast_names_file.write_all(&serialized_aast_names)?;
        }

        codebase.functionlike_infos.shrink_to_fit();
        codebase.classlike_infos.shrink_to_fit();
        codebase.type_definitions.shrink_to_fit();
        codebase.constant_infos.shrink_to_fit();
        codebase.files.shrink_to_fit();
        codebase.symbols.all.shrink_to_fit();
    }

    Ok(ScanFilesResult {
        codebase,
        interner,
        resolved_names,
        codebase_diff,
        files_to_analyze,
        file_system,
        invalid_files: invalid_files.into_iter().collect(),
    })
}

pub fn get_filesystem(
    files_to_scan: &mut Vec<String>,
    interner: &mut Interner,
    logger: &Logger,
    scan_dirs: &Vec<String>,
    existing_file_system: &Option<VirtualFileSystem>,
    config: &Arc<Config>,
    cache_dir: Option<&String>,
    files_to_analyze: &mut Vec<String>,
    include_builtins: Option<bool>,
) -> VirtualFileSystem {
    let mut file_system = VirtualFileSystem::default();

    if include_builtins.unwrap_or(true) {
        add_builtins_to_scan(files_to_scan, interner, &mut file_system);
    }

    logger.log_sync("Looking for Hack files");

    for scan_dir in scan_dirs {
        logger.log_debug_sync(&format!(" - in {}", scan_dir));

        files_to_scan.extend(file_system.find_files_in_dir(
            scan_dir,
            interner,
            existing_file_system,
            config,
            cache_dir.is_some() || config.ast_diff,
            files_to_analyze,
        ));
    }

    file_system
}

pub fn add_builtins_to_scan(
    files_to_scan: &mut Vec<String>,
    interner: &mut Interner,
    file_system: &mut VirtualFileSystem,
) {
    // add HHVM libs
    for file in HhiAsset::iter() {
        files_to_scan.push(file.to_string());
        let interned_file_path = FilePath(interner.intern(file.to_string()));
        file_system
            .file_hashes_and_times
            .insert(interned_file_path, (0, 0));
    }

    // add HSL
    for file in HslAsset::iter() {
        files_to_scan.push(file.to_string());
        let interned_file_path = FilePath(interner.intern(file.to_string()));
        file_system
            .file_hashes_and_times
            .insert(interned_file_path, (0, 0));
    }
}

pub(crate) fn scan_file(
    str_path: &str,
    file_path: FilePath,
    all_custom_issues: &FxHashSet<String>,
    codebase: &mut CodebaseInfo,
    interner: &mut ThreadedInterner,
    empty_name_context: NameContext<'_>,
    user_defined: bool,
    is_production_code: bool,
    logger: &Logger,
) -> Result<FxHashMap<u32, StrId>, ParserError> {
    logger.log_debug_sync(&format!("scanning {}", str_path));

    let aast = get_aast_for_path(file_path, str_path);

    let aast = match aast {
        Ok(aast) => aast,
        Err(err) => {
            return Err(err);
        }
    };

    let (resolved_names, uses) =
        hakana_aast_helper::scope_names(&aast.0, interner, empty_name_context);

    hakana_reflector::collect_info_for_aast(
        &aast.0,
        &resolved_names,
        interner,
        codebase,
        all_custom_issues,
        FileSource {
            is_production_code,
            file_path_actual: str_path.to_string(),
            file_path,
            hh_fixmes: &aast.1.fixmes,
            comments: &aast.1.comments,
            file_contents: aast.2,
        },
        user_defined,
        uses,
        aast.3,
    );

    Ok(resolved_names)
}

fn invalidate_changed_codebase_elements(
    codebase: &mut CodebaseInfo,
    changed_files: &FxHashSet<FilePath>,
) {
    for (file_path, file_storage) in codebase
        .files
        .iter()
        .filter(|f| changed_files.contains(f.0))
    {
        for ast_node in &file_storage.ast_nodes {
            match codebase.symbols.all.remove(&ast_node.name) {
                Some(kind) => {
                    if let SymbolKind::TypeDefinition | SymbolKind::NewtypeDefinition = kind {
                        codebase.type_definitions.remove(&ast_node.name);
                        // Also remove from defs tracking
                        codebase.type_definitions_defs.remove(&ast_node.name);
                    } else if let Some(classlike_info) =
                        codebase.classlike_infos.remove(&ast_node.name)
                    {
                        // Also remove from defs tracking
                        codebase.classlike_infos_defs.remove(&ast_node.name);
                        for method_name in classlike_info.methods {
                            codebase
                                .functionlike_infos
                                .remove(&(ast_node.name, method_name));
                        }
                    }
                }
                None => {
                    if ast_node.is_function {
                        codebase
                            .functionlike_infos
                            .remove(&(ast_node.name, StrId::EMPTY));
                        // Also remove from defs tracking
                        codebase
                            .functionlike_infos_defs
                            .remove(&(ast_node.name, StrId::EMPTY));
                    } else if ast_node.is_constant {
                        codebase.constant_infos.remove(&ast_node.name);
                        // Also remove from defs tracking
                        codebase.constant_infos_defs.remove(&ast_node.name);
                    }
                }
            }
        }

        for closure_ref in &file_storage.closure_refs {
            codebase
                .functionlike_infos
                .remove(&(file_path.0, StrId(*closure_ref)));
        }
    }

    // we need to check for anonymous functions here
    let closures_to_remove = codebase
        .closures_in_files
        .iter()
        .filter(|(k, _)| changed_files.contains(*k))
        .flat_map(|(_, v)| v.clone().into_iter().collect::<Vec<_>>())
        .collect::<FxHashSet<_>>();

    codebase
        .functionlike_infos
        .retain(|k, _| !closures_to_remove.contains(&k.0));
}
