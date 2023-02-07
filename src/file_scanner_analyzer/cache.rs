use hakana_analyzer::config::Verbosity;
use hakana_reflection_info::codebase_info::symbols::SymbolKind;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::issue::Issue;
use hakana_reflection_info::symbol_references::SymbolReferences;
use hakana_reflection_info::Interner;
use hakana_reflection_info::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) fn load_cached_codebase(
    codebase_path: &String,
    use_codebase_cache: bool,
    codebase: &mut CodebaseInfo,
    interner: &Interner,
    changed_files: &FxHashSet<String>,
    verbosity: Verbosity,
) {
    if Path::new(codebase_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing stored codebase cache");
        }
        let serialized = fs::read(&codebase_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &codebase_path));
        if let Ok(d) = bincode::deserialize::<CodebaseInfo>(&serialized) {
            *codebase = d;

            for (_, file_storage) in codebase
                .files
                .iter()
                .filter(|f| changed_files.contains(interner.lookup(f.0)))
            {
                for ast_node in &file_storage.ast_nodes {
                    match codebase.symbols.all.get(&ast_node.name) {
                        Some(kind) => match kind {
                            SymbolKind::TypeDefinition => {
                                codebase.type_definitions.remove(&ast_node.name);
                            }
                            SymbolKind::Function => {
                                codebase.functionlike_infos.remove(&ast_node.name);
                            }
                            SymbolKind::Constant => {
                                codebase.constant_infos.remove(&ast_node.name);
                            }
                            _ => {
                                codebase.classlike_infos.remove(&ast_node.name);
                            }
                        },
                        None => {}
                    }

                    codebase.symbols.all.remove(&ast_node.name);
                }
            }

            // we need to check for anonymous functions here
            let closures_to_remove = codebase
                .closures_in_files
                .iter()
                .filter(|(k, _)| changed_files.contains(*k))
                .map(|(_, v)| v.clone().into_iter().collect::<Vec<_>>())
                .flatten()
                .collect::<FxHashSet<_>>();

            codebase
                .functionlike_infos
                .retain(|k, _| !closures_to_remove.contains(k));
        }
    }
}

pub(crate) fn load_cached_symbols(
    symbols_path: &String,
    use_codebase_cache: bool,
    interner: &mut Interner,
    verbosity: Verbosity,
) {
    if Path::new(symbols_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing stored symbol cache");
        }
        let serialized = fs::read(&symbols_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &symbols_path));
        if let Ok(d) = bincode::deserialize::<Interner>(&serialized) {
            *interner = d;
        }
    }
}

pub(crate) fn load_cached_aast_names(
    aast_names_path: &String,
    use_codebase_cache: bool,
    resolved_names: &mut FxHashMap<String, FxHashMap<usize, StrId>>,
    verbosity: Verbosity,
) {
    if Path::new(aast_names_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing aast names cache");
        }
        let serialized = fs::read(&aast_names_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &aast_names_path));
        if let Ok(d) =
            bincode::deserialize::<FxHashMap<String, FxHashMap<usize, StrId>>>(&serialized)
        {
            *resolved_names = d;
        }
    }
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
) -> Option<BTreeMap<String, Vec<Issue>>> {
    if Path::new(existing_issues_path).exists() && use_codebase_cache {
        if !matches!(verbosity, Verbosity::Quiet) {
            println!("Deserializing existing issues cache");
        }
        let serialized = fs::read(&existing_issues_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &existing_issues_path));
        if let Ok(d) = bincode::deserialize::<BTreeMap<String, Vec<Issue>>>(&serialized) {
            return Some(d);
        }
    }

    None
}
