use std::{collections::HashMap, fs, path::Path};

use indexmap::IndexMap;

#[derive(Debug)]
pub enum FileStatus {
    Unchanged(u64, u64),
    Added(u64, u64),
    Deleted,
    Modified(u64, u64),
}

pub(crate) fn get_file_manifest(cache_dir: &String) -> Option<HashMap<String, (u64, u64)>> {
    let aast_manifest_path = format!("{}/manifest", cache_dir);

    if Path::new(&aast_manifest_path).exists() {
        let serialized = fs::read(&aast_manifest_path)
            .unwrap_or_else(|_| panic!("Could not read file {}", &aast_manifest_path));
        if let Ok(d) = bincode::deserialize::<HashMap<String, (u64, u64)>>(&serialized) {
            return Some(d);
        }
    }

    None
}

fn get_contents_hash(file_path: &String) -> u64 {
    let file_contents = fs::read_to_string(&file_path)
        .unwrap_or_else(|_| panic!("Could not read file {}", &file_path));
    xxhash_rust::xxh3::xxh3_64(file_contents.as_bytes())
}

pub(crate) fn get_file_diff(
    target_files: &IndexMap<String, u64>,
    file_update_hashes: HashMap<String, (u64, u64)>,
) -> IndexMap<String, FileStatus> {
    let mut file_statuses = IndexMap::new();

    for (file_path, new_update_time) in target_files {
        if let Some((old_contents_hash, old_update_time)) = file_update_hashes.get(file_path) {
            if file_path.starts_with("hhi_embedded_") || file_path.starts_with("hsl_embedded_") {
                file_statuses.insert(
                    file_path.clone(),
                    FileStatus::Unchanged(0, *new_update_time),
                );
                continue;
            }

            if new_update_time != old_update_time {
                let new_contents_hash = get_contents_hash(&file_path);

                if new_contents_hash != *old_contents_hash {
                    file_statuses.insert(
                        file_path.clone(),
                        FileStatus::Modified(new_contents_hash, *new_update_time),
                    );
                } else {
                    file_statuses.insert(
                        file_path.clone(),
                        FileStatus::Unchanged(new_contents_hash, *new_update_time),
                    );
                }
            } else {
                file_statuses.insert(
                    file_path.clone(),
                    FileStatus::Unchanged(*old_contents_hash, *new_update_time),
                );
            }
        } else {
            if file_path.starts_with("hhi_embedded_") || file_path.starts_with("hsl_embedded_") {
                file_statuses.insert(file_path.clone(), FileStatus::Added(0, *new_update_time));
                continue;
            }

            file_statuses.insert(
                file_path.clone(),
                FileStatus::Added(get_contents_hash(&file_path), *new_update_time),
            );
        }
    }

    for (file_path, _) in &file_update_hashes {
        if !file_statuses.contains_key(file_path) {
            file_statuses.insert(file_path.clone(), FileStatus::Deleted);
        }
    }

    file_statuses
}
