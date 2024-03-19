use std::{
    collections::BTreeMap,
    hash::BuildHasherDefault,
    sync::{Arc, Mutex},
};

use indexmap::{IndexMap, IndexSet};
use rustc_hash::{FxHashMap, FxHasher};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct StrId(pub u32);

include!(concat!(env!("OUT_DIR"), "/interned_strings.rs"));

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Interner {
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
    map: IndexMap<String, StrId>,
    reverse_map: BTreeMap<StrId, usize>,
    pub parent: Arc<Mutex<Interner>>,
}

impl ThreadedInterner {
    pub fn new(interner: Arc<Mutex<Interner>>) -> Self {
        ThreadedInterner {
            map: IndexMap::default(),
            reverse_map: BTreeMap::new(),
            parent: interner.clone(),
        }
    }

    pub fn intern_str(&mut self, path: &str) -> StrId {
        if let Some(id) = self.map.get(path) {
            return *id;
        }

        let id;
        {
            id = self.parent.lock().unwrap().intern(path.to_string());
        }
        let index = self.map.insert_full(path.to_string(), id).0;
        self.reverse_map.insert(id, index);

        id
    }

    pub fn intern(&mut self, path: String) -> StrId {
        if let Some(id) = self.map.get(&path) {
            return *id;
        }

        let id;
        {
            id = self.parent.lock().unwrap().intern(path.clone());
        }
        let index = self.map.insert_full(path, id).0;
        self.reverse_map.insert(id, index);

        id
    }

    pub fn lookup(&self, id: StrId) -> &str {
        if let Some(entry) = self.map.get_index(*self.reverse_map.get(&id).unwrap()) {
            entry.0
        } else {
            panic!()
        }
    }
}
