use hakana_analyzer::config::Verbosity;
use hakana_reflection_info::code_location::FilePath;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::issue::Issue;
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::Interner;
use hakana_reflection_info::StrId;
use rustc_hash::FxHashMap;
use std::fs;
use std::path::Path;

pub(crate) fn load_cached_codebase(
    codebase_path: &String,
    use_codebase_cache: bool,
    verbosity: Verbosity,
) -> Option<CodebaseInfo> {
    if Path::new(codebase_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing stored codebase cache");
        }
        let serialized = fs::read(&codebase_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &codebase_path));
        if let Ok(d) = bincode::deserialize::<CodebaseInfo>(&serialized) {
            return Some(d);
        }
    }

    None
}

pub(crate) fn load_cached_symbols(
    symbols_path: &String,
    use_codebase_cache: bool,
    verbosity: Verbosity,
) -> Option<Interner> {
    if Path::new(symbols_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing stored symbol cache");
        }
        let serialized = fs::read(&symbols_path)
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
    verbosity: Verbosity,
) -> Option<FxHashMap<FilePath, FxHashMap<usize, StrId>>> {
    if Path::new(aast_names_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing aast names cache");
        }
        let serialized = fs::read(&aast_names_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &aast_names_path));
        if let Ok(d) =
            bincode::deserialize::<FxHashMap<FilePath, FxHashMap<usize, StrId>>>(&serialized)
        {
            return Some(d);
        }
    }

    return None;
}

pub(crate) fn load_cached_existing_references(
    existing_references_path: &String,
    use_codebase_cache: bool,
    verbosity: Verbosity,
) -> Option<SymbolReferences> {
    if Path::new(existing_references_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing existing references cache");
        }
        let serialized = fs::read(&existing_references_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &existing_references_path));
        if let Ok(d) = bincode::deserialize::<SymbolReferences>(&serialized) {
            return Some(d);
        }
    }

    return None;
}

pub(crate) fn load_cached_existing_issues(
    existing_issues_path: &String,
    use_codebase_cache: bool,
    verbosity: Verbosity,
) -> Option<FxHashMap<FilePath, Vec<Issue>>> {
    if Path::new(existing_issues_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing existing issues cache");
        }
        let serialized = fs::read(&existing_issues_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &existing_issues_path));
        if let Ok(d) = bincode::deserialize::<FxHashMap<FilePath, Vec<Issue>>>(&serialized) {
            return Some(d);
        }
    }

    None
}
