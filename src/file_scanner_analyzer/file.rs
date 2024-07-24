use std::{fs, path::Path, time::SystemTime};

use hakana_analyzer::config::Config;
use hakana_reflection_info::{code_location::FilePath, data_flow::graph::GraphKind};
use hakana_str::Interner;
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum FileStatus {
    Unchanged(u64, u64),
    Added(u64, u64),
    Deleted,
    DeletedDir,
    Modified(u64, u64),
}

#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct VirtualFileSystem {
    pub file_hashes_and_times: FxHashMap<FilePath, (u64, u64)>,
}

impl VirtualFileSystem {
    pub(crate) fn apply_language_server_changes(
        &mut self,
        language_server_changes: FxHashMap<String, FileStatus>,
        files_to_scan: &mut Vec<String>,
        interner: &mut Interner,
        config: &Config,
        files_to_analyze: &mut Vec<String>,
    ) {
        let deleted_folders = language_server_changes
            .iter()
            .filter(|(_, v)| matches!(v, FileStatus::DeletedDir))
            .map(|(k, _)| k)
            .collect::<Vec<_>>();

        let mut deleted_files = vec![];

        for file in self.file_hashes_and_times.keys() {
            let str_path = interner.lookup(&file.0).to_string();

            let in_deleted_folder = deleted_folders
                .iter()
                .any(|f| str_path.starts_with(&(f.to_string() + "/")));

            if in_deleted_folder {
                deleted_files.push(*file);
            } else if let Some(file_status) = language_server_changes.get(&str_path) {
                if let FileStatus::Deleted = file_status {
                    deleted_files.push(*file);
                }
            } else {
                files_to_scan.push(str_path.clone());

                if !str_path.starts_with("hsl_embedded") && !str_path.ends_with(".hhi") {
                    if matches!(config.graph_kind, GraphKind::WholeProgram(_)) {
                        if config.allow_taints_in_file(&str_path) {
                            files_to_analyze.push(str_path.clone());
                        }
                    } else {
                        files_to_analyze.push(str_path.clone());
                    }
                }
            }
        }

        for deleted_file in deleted_files {
            self.file_hashes_and_times.remove(&deleted_file);
        }

        for (file_path, status) in language_server_changes {
            let path = Path::new(&file_path);

            match status {
                FileStatus::Unchanged(_, _) => panic!(),
                FileStatus::Added(_, _) | FileStatus::Modified(_, _) => {
                    self.add_path(
                        path,
                        &vec![],
                        interner,
                        &None,
                        true,
                        files_to_scan,
                        config,
                        files_to_analyze,
                    );
                }
                FileStatus::Deleted | FileStatus::DeletedDir => {
                    // handled above
                }
            }
        }
    }

    pub(crate) fn get_file_statuses(
        &self,
        target_files: &Vec<String>,
        interner: &Interner,
        existing_file_system: &Option<VirtualFileSystem>,
    ) -> IndexMap<FilePath, FileStatus> {
        let mut file_statuses = IndexMap::new();

        for file_path in target_files {
            let interned_file_path = FilePath(interner.get(file_path).unwrap());

            file_statuses.insert(
                interned_file_path,
                self.get_file_status(existing_file_system, interned_file_path, file_path),
            );
        }

        if let Some(existing_file_system) = existing_file_system {
            for file_path in existing_file_system.file_hashes_and_times.keys() {
                if !file_statuses.contains_key(file_path) {
                    file_statuses.insert(*file_path, FileStatus::Deleted);
                }
            }
        }

        file_statuses
    }

    fn get_file_status(
        &self,
        existing_file_system: &Option<VirtualFileSystem>,
        interned_file_path: FilePath,
        file_path: &str,
    ) -> FileStatus {
        if let Some((old_contents_hash, _)) =
            if let Some(existing_file_system) = existing_file_system {
                existing_file_system
                    .file_hashes_and_times
                    .get(&interned_file_path)
            } else {
                None
            }
        {
            if file_path.starts_with("hhi_embedded_") || file_path.starts_with("hsl_embedded_") {
                FileStatus::Unchanged(0, 0)
            } else {
                let (new_contents_hash, new_update_time) =
                    self.file_hashes_and_times.get(&interned_file_path).unwrap();

                if new_contents_hash != old_contents_hash {
                    FileStatus::Modified(*new_contents_hash, *new_update_time)
                } else {
                    FileStatus::Unchanged(*new_contents_hash, *new_update_time)
                }
            }
        } else {
            let (new_contents_hash, new_update_time) =
                self.file_hashes_and_times.get(&interned_file_path).unwrap();

            FileStatus::Added(*new_contents_hash, *new_update_time)
        }
    }

    pub fn find_files_in_dir(
        &mut self,
        scan_dir: &String,
        interner: &mut Interner,
        existing_file_system: &Option<VirtualFileSystem>,
        config: &Config,
        calculate_file_hashes: bool,
        files_to_analyze: &mut Vec<String>,
    ) -> Vec<String> {
        let mut files_to_scan = vec![];

        let ignore_dirs = config
            .ignore_files
            .iter()
            .filter(|file| file.ends_with("/**"))
            .map(|file| file[0..(file.len() - 3)].to_string())
            .collect::<FxHashSet<_>>();

        let mut walker_builder = ignore::WalkBuilder::new(scan_dir);

        walker_builder
            .sort_by_file_path(|a, b| a.file_name().cmp(&b.file_name()))
            .follow_links(true);
        walker_builder.git_ignore(false);
        walker_builder.filter_entry(move |f| {
            let p = f.path().to_str().unwrap();
            !ignore_dirs.contains(p) && !p.contains("/.")
        });

        let walker = walker_builder.build().filter_map(|e| e.ok());

        let ignore_patterns = config
            .ignore_files
            .iter()
            .filter(|file| !file.ends_with("/**"))
            .map(|ignore_file| glob::Pattern::new(ignore_file).unwrap())
            .collect::<Vec<_>>();

        for entry in walker {
            let path = entry.path();

            self.add_path(
                path,
                &ignore_patterns,
                interner,
                existing_file_system,
                calculate_file_hashes,
                &mut files_to_scan,
                config,
                files_to_analyze,
            );
        }

        files_to_scan
    }

    fn add_path(
        &mut self,
        path: &std::path::Path,
        ignore_patterns: &Vec<glob::Pattern>,
        interner: &mut Interner,
        existing_file_system: &Option<VirtualFileSystem>,
        calculate_file_hashes: bool,
        files_to_scan: &mut Vec<String>,
        config: &Config,
        files_to_analyze: &mut Vec<String>,
    ) {
        let metadata = if let Ok(metadata) = fs::metadata(path) {
            metadata
        } else {
            return;
        };

        if metadata.is_file() {
            if let Some(extension) = path.extension() {
                if extension.eq("hack") || extension.eq("php") || extension.eq("hhi") {
                    let str_path = path.to_str().unwrap().to_string();

                    for ignore_pattern in ignore_patterns {
                        if ignore_pattern.matches(&str_path) {
                            return;
                        }
                    }

                    let interned_file_path = FilePath(interner.intern(str_path.clone()));

                    let updated_time = metadata
                        .modified()
                        .unwrap()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64;

                    let file_hash = if let Some(existing_file_system) = existing_file_system {
                        if let Some((old_contents_hash, old_update_time)) = existing_file_system
                            .file_hashes_and_times
                            .get(&interned_file_path)
                        {
                            if old_update_time == &updated_time {
                                *old_contents_hash
                            } else if calculate_file_hashes {
                                get_file_contents_hash(&str_path).unwrap_or(0)
                            } else {
                                0
                            }
                        } else {
                            0
                        }
                    } else if calculate_file_hashes {
                        get_file_contents_hash(&str_path).unwrap_or(0)
                    } else {
                        0
                    };

                    self.file_hashes_and_times
                        .insert(interned_file_path, (file_hash, updated_time));

                    files_to_scan.push(str_path.clone());

                    if !extension.eq("hhi") {
                        if matches!(config.graph_kind, GraphKind::WholeProgram(_)) {
                            if config.allow_taints_in_file(&str_path) {
                                files_to_analyze.push(str_path.clone());
                            }
                        } else {
                            files_to_analyze.push(str_path.clone());
                        }
                    }
                }
            }
        }
    }
}

pub fn get_file_contents_hash(file_path: &String) -> Result<u64, std::io::Error> {
    match fs::read_to_string(file_path) {
        Ok(file_contents) => Ok(xxhash_rust::xxh3::xxh3_64(file_contents.as_bytes())),
        Err(error) => Err(error),
    }
}
