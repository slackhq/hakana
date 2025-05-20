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
pub struct ReflectionInterner {
    map: IndexSet<String, BuildHasherDefault<FxHasher>>,
}

impl StrId {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl ReflectionInterner {
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

    pub fn get_size(&self) -> usize {
        self.map.len()
    }
}

#[derive(Debug)]
pub struct ThreadedInterner {
    map: IndexMap<String, StrId>,
    reverse_map: BTreeMap<StrId, usize>,
    pub parent: Arc<Mutex<ReflectionInterner>>,
}

impl ThreadedInterner {
    pub fn new(interner: Arc<Mutex<ReflectionInterner>>) -> Self {
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

#[derive(Debug)]
pub struct Interner {
    parent: Arc<ReflectionInterner>,
    local_strings: Vec<String>,
    local_map: FxHashMap<String, StrId>,
    next_local_id: u32,
}

impl Interner {
    pub fn new(parent: Arc<ReflectionInterner>) -> Self {
        Self {
            next_local_id: parent.get_size() as u32,
            parent,
            local_strings: Vec::new(),
            local_map: FxHashMap::default(),
        }
    }

    pub fn intern(&mut self, string: String) -> StrId {
        if let Some(id) = self.local_map.get(&string) {
            return *id;
        }
        if let Some(id) = self.parent.get(&string) {
            return id;
        }

        let id = StrId(self.next_local_id);
        self.next_local_id += 1;
        self.local_map.insert(string.clone(), id);
        self.local_strings.push(string);
        id
    }

    pub fn intern_str(&mut self, string: &str) -> StrId {
        if let Some(id) = self.local_map.get(string) {
            return *id;
        }
        if let Some(id) = self.parent.get(string) {
            return id;
        }

        let id = StrId(self.next_local_id);
        self.next_local_id += 1;
        self.local_map.insert(string.to_string(), id);
        self.local_strings.push(string.to_string());
        id
    }

    pub fn lookup(&self, id: &StrId) -> &str {
        // Check if it's a local ID
        if id.0 >= self.next_local_id {
            let local_index = (id.0 - self.next_local_id) as usize;
            if local_index < self.local_strings.len() {
                return &self.local_strings[local_index];
            }
        }
        // Otherwise, assume it's a parent ID
        self.parent.lookup(&id)
    }

    /// Get the id corresponding to `path`.
    ///
    /// If `path` does not exists in `self`, returns [`None`].
    pub fn get(&self, path: &str) -> Option<StrId> {
        if let Some(id) = self.local_map.get(path) {
            return Some(*id);
        }
        self.parent.get(path)
    }

    pub fn parent(&self) -> &ReflectionInterner {
        &self.parent
    }
}
