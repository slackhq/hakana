use std::{
    hash::BuildHasherDefault,
    sync::{Arc, RwLock},
};

use indexmap::{IndexMap, IndexSet};
use rustc_hash::{FxHashMap, FxHasher};
use serde::{Deserialize, Serialize};

#[cfg(feature = "memory-profile")]
pub mod memory_profile;

#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct StrId(pub u32);

include!(concat!(env!("OUT_DIR"), "/interned_strings.rs"));

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct Interner {
    #[cfg_attr(feature = "memory-profile", allocative(visit = memory_profile::index_set))]
    map: IndexSet<String, BuildHasherDefault<FxHasher>>,
}

impl StrId {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl Interner {
    /// Get the id corresponding to `path`.
    ///
    /// If `path` does not exists in `self`, returns [`None`].
    pub fn get(&self, path: &str) -> Option<StrId> {
        self.map.get_index_of(path).map(|i| StrId(i as u32))
    }

    /// Insert `path` in `self`.
    ///
    /// - If `path` already exists in `self`, returns its associated id;
    /// - Else, returns a newly allocated id.
    pub fn intern(&mut self, path: String) -> StrId {
        let (id, _added) = self.map.insert_full(path);
        assert!(id < u32::MAX as usize);
        StrId(id as u32)
    }

    /// Returns the path corresponding to `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` does not exists in `self`.
    pub fn lookup(&self, id: &StrId) -> &str {
        self.map.get_index(id.0 as usize).unwrap()
    }

    pub fn get_map(&self) -> FxHashMap<String, StrId> {
        self.map
            .iter()
            .enumerate()
            .map(|(k, v)| (v.clone(), StrId(k as u32)))
            .collect()
    }
}

#[derive(Debug)]
pub struct ThreadedInterner {
    map: IndexMap<String, StrId, BuildHasherDefault<FxHasher>>,
    reverse_map: FxHashMap<StrId, usize>,
    pub parent: Arc<RwLock<Interner>>,
}

impl ThreadedInterner {
    pub fn new(interner: Arc<RwLock<Interner>>) -> Self {
        ThreadedInterner {
            map: IndexMap::default(),
            reverse_map: FxHashMap::default(),
            parent: interner.clone(),
        }
    }

    pub fn intern_str(&mut self, path: &str) -> StrId {
        if let Some(id) = self.map.get(path) {
            return *id;
        }

        let id = self.intern_shared(path);
        let index = self.map.insert_full(path.to_string(), id).0;
        self.reverse_map.insert(id, index);

        id
    }

    pub fn intern(&mut self, path: String) -> StrId {
        if let Some(id) = self.map.get(&path) {
            return *id;
        }

        let id = self.intern_shared(&path);
        let index = self.map.insert_full(path, id).0;
        self.reverse_map.insert(id, index);

        id
    }

    /// The id of a string this thread has not seen before. Most such strings
    /// are already known to another thread, so look under the shared lock
    /// first: threads only queue up to add strings that are new to everyone.
    fn intern_shared(&self, path: &str) -> StrId {
        if let Some(id) = self.parent.read().unwrap().get(path) {
            return id;
        }
        let owned = path.to_string();
        self.parent.write().unwrap().intern(owned)
    }

    pub fn lookup(&self, id: StrId) -> &str {
        if let Some(entry) = self.map.get_index(*self.reverse_map.get(&id).unwrap()) {
            entry.0
        } else {
            panic!()
        }
    }
}
