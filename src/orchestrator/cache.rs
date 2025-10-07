use hakana_code_info::code_location::FilePath;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::issue::Issue;
use hakana_code_info::symbol_references::SymbolReferences;
use hakana_logger::Logger;
use hakana_str::Interner;
use hakana_str::StrId;
use rustc_hash::FxHashMap;
use std::fs;
use std::path::Path;

use crate::file::VirtualFileSystem;

pub(crate) fn load_cached_codebase(
    codebase_path: &String,
    use_codebase_cache: bool,
    logger: &Logger,
) -> Option<CodebaseInfo> {
    if Path::new(codebase_path).exists() && use_codebase_cache {
        logger.log_sync("Deserializing stored codebase cache");
        let serialized = fs::read(codebase_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &codebase_path));
        if let Ok(d) = bincode::deserialize::<CodebaseInfo>(&serialized) {
            return Some(d);
        }
    }

    None
}

pub(crate) fn load_cached_interner(
    symbols_path: &String,
    use_codebase_cache: bool,
    logger: &Logger,
) -> Option<Interner> {
    if Path::new(symbols_path).exists() && use_codebase_cache {
        logger.log_sync("Deserializing stored symbol cache");
        let serialized = fs::read(symbols_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &symbols_path));
        if let Ok(d) = bincode::deserialize::<Interner>(&serialized) {
            return Some(d);
        }
    }

    None
}

pub(crate) fn load_cached_aast_names(
    aast_names_path: &String,
    use_codebase_cache: bool,
    logger: &Logger,
) -> Option<FxHashMap<FilePath, FxHashMap<u32, StrId>>> {
    if Path::new(aast_names_path).exists() && use_codebase_cache {
        logger.log_sync("Deserializing aast names cache");
        let serialized = fs::read(aast_names_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &aast_names_path));
        if let Ok(d) =
            bincode::deserialize::<FxHashMap<FilePath, FxHashMap<u32, StrId>>>(&serialized)
        {
            return Some(d);
        }
    }

    None
}

pub(crate) fn load_cached_existing_references(
    existing_references_path: &String,
    use_codebase_cache: bool,
    logger: &Logger,
) -> Option<SymbolReferences> {
    if Path::new(existing_references_path).exists() && use_codebase_cache {
        logger.log_sync("Deserializing existing references cache");
        let serialized = fs::read(existing_references_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &existing_references_path));
        if let Ok(d) = bincode::deserialize::<SymbolReferences>(&serialized) {
            return Some(d);
        }
    }

    None
}

pub(crate) fn load_cached_existing_issues(
    existing_issues_path: &String,
    use_codebase_cache: bool,
    logger: &Logger,
) -> Option<FxHashMap<FilePath, Vec<Issue>>> {
    if Path::new(existing_issues_path).exists() && use_codebase_cache {
        logger.log_sync("Deserializing existing issues cache");
        let serialized = fs::read(existing_issues_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &existing_issues_path));
        if let Ok(d) = bincode::deserialize::<FxHashMap<FilePath, Vec<Issue>>>(&serialized) {
            return Some(d);
        }
    }

    None
}

pub(crate) fn get_file_manifest(cache_dir: &String) -> Option<VirtualFileSystem> {
    let aast_manifest_path = format!("{}/manifest", cache_dir);

    if Path::new(&aast_manifest_path).exists() {
        let serialized = fs::read(&aast_manifest_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &aast_manifest_path));
        if let Ok(d) = bincode::deserialize::<VirtualFileSystem>(&serialized) {
            return Some(d);
        }
    }

    None
}
