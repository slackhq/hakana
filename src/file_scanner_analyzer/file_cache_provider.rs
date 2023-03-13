use std::{fs, path::Path};

use hakana_reflection_info::{code_location::FilePath, Interner};
use indexmap::IndexMap;
use rustc_hash::FxHashMap;

#[derive(Debug)]
pub enum FileStatus {
    Unchanged(u64, u64),
    Added(u64, u64),
    Deleted,
    Modified(u64, u64),
}

pub(crate) fn get_file_manifest(cache_dir: &String) -> Option<FxHashMap<FilePath, (u64, u64)>> {
    let aast_manifest_path = format!("{}/manifest", cache_dir);

    if Path::new(&aast_manifest_path).exists() {
        let serialized = fs::read(&aast_manifest_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &aast_manifest_path));
        if let Ok(d) = bincode::deserialize::<FxHashMap<FilePath, (u64, u64)>>(&serialized) {
            return Some(d);
        }
    }

    None
}

fn get_contents_hash(file_path: &String) -> Result<u64, std::io::Error> {
    match fs::read_to_string(&file_path) {
        Ok(file_contents) => Ok(xxhash_rust::xxh3::xxh3_64(file_contents.as_bytes())),
        Err(error) => Err(error),
    }
}

pub(crate) fn get_file_diff(
    target_files: &IndexMap<String, u64>,
    file_update_hashes: FxHashMap<FilePath, (u64, u64)>,
    interner: &mut Interner,
) -> IndexMap<FilePath, FileStatus> {
    let mut file_statuses = IndexMap::new();

    for (file_path, new_update_time) in target_files {
        let interned_file_path = FilePath(interner.intern(file_path.clone()));
        if let Some((old_contents_hash, old_update_time)) =
            file_update_hashes.get(&interned_file_path)
        {
            if file_path.starts_with("hhi_embedded_") || file_path.starts_with("hsl_embedded_") {
                file_statuses.insert(
                    interned_file_path,
                    FileStatus::Unchanged(0, *new_update_time),
                );
                continue;
            }

            if new_update_time != old_update_time {
                if let Ok(new_contents_hash) = get_contents_hash(&file_path) {
                    if new_contents_hash != *old_contents_hash {
                        file_statuses.insert(
                            interned_file_path,
                            FileStatus::Modified(new_contents_hash, *new_update_time),
                        );
                    } else {
                        file_statuses.insert(
                            interned_file_path,
                            FileStatus::Unchanged(new_contents_hash, *new_update_time),
                        );
                    }
                } else {
                    continue;
                }
            } else {
                file_statuses.insert(
                    interned_file_path,
                    FileStatus::Unchanged(*old_contents_hash, *new_update_time),
                );
            }
        } else {
            if file_path.starts_with("hhi_embedded_") || file_path.starts_with("hsl_embedded_") {
                file_statuses.insert(interned_file_path, FileStatus::Added(0, *new_update_time));
                continue;
            }

            if let Ok(contents_hash) = get_contents_hash(&file_path) {
                file_statuses.insert(
                    interned_file_path,
                    FileStatus::Added(contents_hash, *new_update_time),
                );
            } else {
                continue;
            }
        }
    }

    for (file_path, _) in &file_update_hashes {
        if !file_statuses.contains_key(file_path) {
            file_statuses.insert(file_path.clone(), FileStatus::Deleted);
        }
    }

    file_statuses
}
