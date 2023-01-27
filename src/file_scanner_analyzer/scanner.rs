use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use super::find_files_in_dir;
use super::update_progressbar;
use super::HhiAsset;
use super::HslAsset;
use crate::ast_differ;
use crate::cache::load_cached_aast_names;
use crate::cache::load_cached_codebase;
use crate::cache::load_cached_symbols;
use crate::file_cache_provider;
use crate::file_cache_provider::FileStatus;
use crate::get_aast_for_path;
use crate::get_relative_path;
use ast_differ::get_diff;
use hakana_aast_helper::name_context::NameContext;
use hakana_aast_helper::ParserError;
use hakana_analyzer::config::Config;
use hakana_analyzer::config::Verbosity;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::diff::CodebaseDiff;
use hakana_reflection_info::FileSource;
use hakana_reflection_info::Interner;
use hakana_reflection_info::StrId;
use hakana_reflection_info::ThreadedInterner;
use indexmap::IndexMap;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub struct ScanFilesResult {
    pub codebase: CodebaseInfo,
    pub interner: Interner,
    pub file_statuses: IndexMap<String, FileStatus>,
    pub resolved_names: FxHashMap<String, FxHashMap<usize, StrId>>,
    pub codebase_diff: CodebaseDiff,
}

pub(crate) fn scan_files(
    scan_dirs: &Vec<String>,
    include_core_libs: bool,
    cache_dir: Option<&String>,
    files_to_analyze: &mut Vec<String>,
    config: &Arc<Config>,
    threads: u8,
    verbosity: Verbosity,
    build_checksum: &str,
    starter_data: Option<(CodebaseInfo, Interner)>,
) -> io::Result<ScanFilesResult> {
    if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
        println!("{:#?}", scan_dirs);
    }

    let mut files_to_scan = IndexMap::new();

    let codebase_path = if let Some(cache_dir) = cache_dir {
        Some(format!("{}/codebase", cache_dir))
    } else {
        None
    };

    let symbols_path = if let Some(cache_dir) = cache_dir {
        Some(format!("{}/symbols", cache_dir))
    } else {
        None
    };

    let aast_names_path = if let Some(cache_dir) = cache_dir {
        Some(format!("{}/aast_names", cache_dir))
    } else {
        None
    };

    let (mut codebase, mut interner) =
        starter_data.unwrap_or((CodebaseInfo::new(), Interner::new()));

    if include_core_libs {
        // add HHVM libs
        for file in HhiAsset::iter() {
            files_to_scan.insert(file.to_string(), 0);
        }

        // add HSL
        for file in HslAsset::iter() {
            files_to_scan.insert(file.to_string(), 0);
        }
    }

    if !matches!(verbosity, Verbosity::Quiet) {
        println!("Looking for Hack files");
    }

    let now = Instant::now();

    for scan_dir in scan_dirs {
        if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
            println!(" - in {}", scan_dir);
        }

        files_to_scan.extend(find_files_in_dir(scan_dir, config, files_to_analyze));
    }

    let elapsed = now.elapsed();

    if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
        println!("File discovery took {:.2?}", elapsed);
    }

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

        if !use_codebase_cache {
            if Path::new(&codebase_path_unwrapped).exists() {
                fs::remove_file(&codebase_path_unwrapped).unwrap();
            }
        }
    }

    let file_update_hashes = if let Some(cache_dir) = cache_dir {
        if use_codebase_cache {
            file_cache_provider::get_file_manifest(cache_dir).unwrap_or(FxHashMap::default())
        } else {
            FxHashMap::default()
        }
    } else {
        FxHashMap::default()
    };

    let file_statuses = file_cache_provider::get_file_diff(&files_to_scan, file_update_hashes);

    let now = Instant::now();

    if let Some(symbols_path) = &symbols_path {
        load_cached_symbols(symbols_path, use_codebase_cache, &mut interner, verbosity);
    }

    let changed_files = file_statuses
        .iter()
        .filter(|(_, v)| !matches!(v, FileStatus::Unchanged(..)))
        .map(|(k, _)| {
            if k.contains(&config.root_dir) {
                k[(config.root_dir.len() + 1)..].to_string()
            } else {
                k.clone()
            }
        })
        .collect::<FxHashSet<_>>();

    // this needs to come after we've loaded interned strings
    if let Some(codebase_path) = &codebase_path {
        load_cached_codebase(
            codebase_path,
            use_codebase_cache,
            &mut codebase,
            &interner,
            &changed_files,
            verbosity,
        );
    }

    let mut resolved_names = FxHashMap::default();

    if let Some(aast_names_path) = &aast_names_path {
        load_cached_aast_names(
            aast_names_path,
            use_codebase_cache,
            &mut resolved_names,
            verbosity,
        );
    }

    let elapsed = now.elapsed();

    if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
        println!(
            "Loading serialised codebase from cache took {:.2?}",
            elapsed
        );
    }

    let mut files_to_scan = vec![];

    for (target_file, status) in &file_statuses {
        if matches!(status, FileStatus::Added(..) | FileStatus::Modified(..)) {
            files_to_scan.push(target_file);
            interner.intern(if target_file.contains(&config.root_dir) {
                target_file[(&config.root_dir.len() + 1)..].to_string()
            } else {
                target_file.clone()
            });
        }
    }

    let mut updated_files = FxHashMap::default();

    let files = codebase.files;

    let mut unchanged_files = FxHashMap::default();

    for (file_id, file_info) in files {
        if changed_files.contains(interner.lookup(&file_id)) {
            updated_files.insert(file_id, file_info);
        } else {
            unchanged_files.insert(file_id, file_info);
        }
    }

    codebase.files = unchanged_files;

    // get the full list of unchanged symbols
    let mut codebase_diff = if config.ast_diff {
        get_diff(&codebase.files, &codebase.files)
    } else {
        CodebaseDiff::default()
    };

    let interner = Arc::new(Mutex::new(interner));
    let resolved_names = Arc::new(Mutex::new(resolved_names));

    let has_new_files = files_to_scan.len() > 0;

    if files_to_scan.len() > 0 {
        let now = Instant::now();

        let bar = if matches!(verbosity, Verbosity::Simple) {
            let pb = ProgressBar::new(files_to_scan.len() as u64);
            let sty =
                ProgressStyle::with_template("{bar:40.green/yellow} {pos:>7}/{len:7}").unwrap();
            pb.set_style(sty);
            Some(Arc::new(pb))
        } else {
            None
        };

        let files_processed: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Scanning {} files", files_to_scan.len());
        }

        let mut group_size = threads as usize;

        let mut path_groups = FxHashMap::default();

        if (files_to_scan.len() / group_size) < 4 {
            group_size = 1;
        }

        let test_patterns = config
            .test_files
            .iter()
            .map(|ignore_file| glob::Pattern::new(ignore_file).unwrap())
            .collect::<Vec<_>>();

        for (i, str_path) in files_to_scan.into_iter().enumerate() {
            let group = i % group_size;
            path_groups
                .entry(group)
                .or_insert_with(Vec::new)
                .push(str_path);
        }

        if path_groups.len() == 1 {
            let mut new_codebase = CodebaseInfo::new();
            let mut new_interner = ThreadedInterner::new(interner.clone());
            let empty_name_context = NameContext::new(&mut new_interner);

            let analyze_map = files_to_analyze
                .clone()
                .into_iter()
                .collect::<FxHashSet<_>>();

            for (i, str_path) in path_groups[&0].iter().enumerate() {
                let file_resolved_names = if let Ok(file_resolved_names) = scan_file(
                    str_path,
                    &config.root_dir,
                    &config.all_custom_issues,
                    &mut new_codebase,
                    &mut new_interner,
                    empty_name_context.clone(),
                    analyze_map.contains(*str_path),
                    !test_patterns.iter().any(|p| p.matches(&str_path)),
                    verbosity,
                ) {
                    file_resolved_names
                } else {
                    let str_path = get_relative_path(str_path, &config.root_dir);
                    new_interner.intern(str_path.clone());
                    FxHashMap::default()
                };

                resolved_names
                    .lock()
                    .unwrap()
                    .insert((**str_path).clone(), file_resolved_names);

                update_progressbar(i as u64, bar.clone());
            }

            if config.ast_diff {
                codebase_diff.extend(get_diff(&updated_files, &new_codebase.files));
            }

            codebase.extend(new_codebase);
        } else {
            let mut handles = vec![];

            let thread_codebases = Arc::new(Mutex::new(vec![]));

            for (_, path_group) in path_groups {
                let pgc = path_group
                    .iter()
                    .map(|c| c.clone().clone())
                    .collect::<Vec<_>>();

                let root_dir_c = config.root_dir.clone();

                let codebases = thread_codebases.clone();

                let bar = bar.clone();
                let files_processed = files_processed.clone();

                let analyze_map = files_to_analyze
                    .clone()
                    .into_iter()
                    .collect::<FxHashSet<_>>();

                let interner = interner.clone();

                let resolved_names = resolved_names.clone();

                let config = config.clone();
                let test_patterns = test_patterns.clone();

                let handle = std::thread::spawn(move || {
                    let mut new_codebase = CodebaseInfo::new();
                    let mut new_interner = ThreadedInterner::new(interner);
                    let empty_name_context = NameContext::new(&mut new_interner);
                    let mut local_resolved_names = FxHashMap::default();

                    for str_path in &pgc {
                        if let Ok(file_resolved_names) = scan_file(
                            str_path,
                            &root_dir_c,
                            &config.all_custom_issues,
                            &mut new_codebase,
                            &mut new_interner,
                            empty_name_context.clone(),
                            analyze_map.contains(str_path),
                            !test_patterns.iter().any(|p| p.matches(&str_path)),
                            verbosity,
                        ) {
                            local_resolved_names.insert((*str_path).clone(), file_resolved_names);
                        } else {
                            local_resolved_names.insert((*str_path).clone(), FxHashMap::default());
                            let str_path = get_relative_path(str_path, &root_dir_c);
                            new_interner.intern(str_path.clone());
                        };

                        let mut tally = files_processed.lock().unwrap();
                        *tally += 1;

                        update_progressbar(*tally, bar.clone());
                    }

                    resolved_names.lock().unwrap().extend(local_resolved_names);

                    let mut codebases = codebases.lock().unwrap();
                    codebases.push(new_codebase);
                });

                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }

            if let Ok(thread_codebases) = Arc::try_unwrap(thread_codebases) {
                for thread_codebase in thread_codebases.into_inner().unwrap().into_iter() {
                    if config.ast_diff {
                        codebase_diff.extend(get_diff(&updated_files, &thread_codebase.files));
                    }

                    codebase.extend(thread_codebase.clone());
                }
            }
        }

        if let Some(bar) = &bar {
            bar.finish_and_clear();
        }

        let elapsed = now.elapsed();

        if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
            println!("Scanning files took {:.2?}", elapsed);
        }
    }

    let interner = Arc::try_unwrap(interner).unwrap().into_inner().unwrap();

    let resolved_names = Arc::try_unwrap(resolved_names)
        .unwrap()
        .into_inner()
        .unwrap();

    if has_new_files {
        if let Some(codebase_path) = codebase_path {
            let mut codebase_file = fs::File::create(&codebase_path).unwrap();
            let serialized_codebase = bincode::serialize(&codebase).unwrap();
            codebase_file.write_all(&serialized_codebase)?;
        }

        if let Some(symbols_path) = symbols_path {
            let mut symbols_file = fs::File::create(&symbols_path).unwrap();
            let serialized_symbols = bincode::serialize(&interner).unwrap();
            symbols_file.write_all(&serialized_symbols)?;
        }

        if let Some(aast_names_path) = aast_names_path {
            let mut symbols_file = fs::File::create(&aast_names_path).unwrap();
            let serialized_symbols = bincode::serialize(&resolved_names).unwrap();
            symbols_file.write_all(&serialized_symbols)?;
        }
    }

    Ok(ScanFilesResult {
        codebase,
        interner,
        file_statuses,
        resolved_names,
        codebase_diff,
    })
}

pub(crate) fn scan_file(
    target_file: &String,
    root_dir: &String,
    all_custom_issues: &FxHashSet<String>,
    codebase: &mut CodebaseInfo,
    interner: &mut ThreadedInterner,
    empty_name_context: NameContext,
    user_defined: bool,
    is_production_code: bool,
    verbosity: Verbosity,
) -> Result<FxHashMap<usize, StrId>, ParserError> {
    if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
        println!("scanning {}", &target_file);
    }

    let aast = get_aast_for_path(&target_file, root_dir, None);

    let aast = match aast {
        Ok(aast) => aast,
        Err(err) => {
            return Err(err);
        }
    };

    let target_name = get_relative_path(target_file, root_dir);

    let interned_file_path = interner.intern(target_name.clone());

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
            file_path_actual: target_name.clone(),
            file_path: interned_file_path,
            hh_fixmes: aast.1.fixmes,
            comments: aast.1.comments,
            file_contents: aast.2,
        },
        user_defined,
        uses,
    );

    Ok(resolved_names)
}
