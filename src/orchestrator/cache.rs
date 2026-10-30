use hakana_code_info::code_location::FilePath;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::issue::Issue;
use hakana_code_info::symbol_references::SymbolReferences;
use hakana_str::Interner;
use hakana_str::StrId;
use log::info;
use rustc_hash::FxHashMap;
use std::fs;
use std::path::Path;

use crate::file::VirtualFileSystem;

pub(crate) fn load_cached_codebase(
    codebase_path: &String,
    use_codebase_cache: bool,
) -> Option<CodebaseInfo> {
    if Path::new(codebase_path).exists() && use_codebase_cache {
        info!("Deserializing stored codebase cache");
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
) -> Option<Interner> {
    if Path::new(symbols_path).exists() && use_codebase_cache {
        info!("Deserializing stored symbol cache");
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
) -> Option<FxHashMap<FilePath, FxHashMap<u32, StrId>>> {
    if Path::new(aast_names_path).exists() && use_codebase_cache {
        info!("Deserializing aast names cache");
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
) -> Option<SymbolReferences> {
    if Path::new(existing_references_path).exists() && use_codebase_cache {
        info!("Deserializing existing references cache");
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
) -> Option<FxHashMap<FilePath, Vec<Issue>>> {
    if Path::new(existing_issues_path).exists() && use_codebase_cache {
        info!("Deserializing existing issues cache");
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

#[cfg(test)]
mod tests {
    use super::*;
    use hakana_code_info::{
        VarId,
        classlike_info::ClassLikeInfo,
        code_location::HPos,
        codebase_info::FxIndexMap,
        functionlike_info::{FunctionLikeInfo, MetaStart},
        functionlike_parameter::{DefaultType, FunctionLikeParameter},
        t_atomic::TAtomic,
        t_union::TUnion,
        type_resolution::TypeResolutionContext,
    };
    use std::sync::Arc;

    fn metadata() -> CodebaseInfo {
        let pos = HPos {
            file_path: FilePath(StrId(100)),
            start_offset: 0,
            end_offset: 1,
            start_line: 1,
            end_line: 1,
            start_column: 1,
            end_column: 2,
        };
        let meta = MetaStart {
            start_offset: 0,
            start_line: 1,
            start_column: 1,
        };
        let mut codebase = CodebaseInfo::new();
        let mut class = ClassLikeInfo::new(StrId(101), pos, meta, pos);
        class.enum_underlying_type = Some(Box::new(TAtomic::TInt));
        class.enum_as_type = Some(Box::new(TAtomic::TArraykey { from_any: false }));
        codebase.classlike_infos.insert(class.name, class);
        let mut function = FunctionLikeInfo::new(pos, meta);
        function.type_resolution_context = Some(TypeResolutionContext::new().into_shared());
        let mut param = FunctionLikeParameter::new(VarId(StrId(102)), pos, pos);
        param.default_type = Some(Box::new(DefaultType::NormalData(TAtomic::TLiteralInt {
            value: 42,
        })));
        function.params.push(param);
        codebase
            .functionlike_infos
            .insert((StrId(103), StrId::EMPTY), function.clone());
        codebase
            .functionlike_infos
            .insert((StrId(104), StrId::EMPTY), function);
        codebase
    }

    #[test]
    fn dense_metadata_maps_read_hashmap_cache_encoding() {
        let codebase = metadata();
        let old_classes: FxHashMap<_, _> = codebase.classlike_infos.into_iter().collect();
        let old_functions: FxHashMap<_, _> = codebase.functionlike_infos.into_iter().collect();
        let bytes = bincode::serialize(&(old_classes, old_functions)).unwrap();
        let (classes, functions): (
            FxIndexMap<StrId, ClassLikeInfo>,
            FxIndexMap<(StrId, StrId), FunctionLikeInfo>,
        ) = bincode::deserialize(&bytes).unwrap();
        assert_eq!(
            classes[&StrId(101)].enum_underlying_type.as_deref(),
            Some(&TAtomic::TInt)
        );
        assert_eq!(
            classes[&StrId(101)].enum_as_type.as_deref(),
            Some(&TAtomic::TArraykey { from_any: false })
        );
        assert_eq!(functions.len(), 2);
        assert_eq!(
            functions[&(StrId(103), StrId::EMPTY)].params[0]
                .default_type
                .as_deref(),
            Some(&DefaultType::NormalData(TAtomic::TLiteralInt { value: 42 })),
        );
    }

    #[test]
    fn cached_empty_contexts_share_storage_and_clone_mutations_are_isolated() {
        let bytes = bincode::serialize(&metadata()).unwrap();
        let restored: CodebaseInfo = bincode::deserialize(&bytes).unwrap();
        let first = restored.functionlike_infos[&(StrId(103), StrId::EMPTY)]
            .type_resolution_context
            .as_ref()
            .unwrap();
        let second = restored.functionlike_infos[&(StrId(104), StrId::EMPTY)]
            .type_resolution_context
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(first, second));
        assert!(Arc::ptr_eq(
            first,
            &TypeResolutionContext::new().into_shared()
        ));

        let mut cloned = restored.clone();
        let context = cloned
            .functionlike_infos
            .get_mut(&(StrId(103), StrId::EMPTY))
            .unwrap()
            .type_resolution_context
            .as_mut()
            .unwrap();
        Arc::make_mut(context)
            .template_supers
            .push((StrId(105), TUnion::new(vec![TAtomic::TInt])));
        assert!(first.template_supers.is_empty());
        assert!(second.template_supers.is_empty());
        assert_eq!(context.template_supers.len(), 1);
    }
}
