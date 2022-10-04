pub mod aliases;
pub mod analysis_result;
pub mod assertion;
pub mod attribute_info;
pub mod class_constant_info;
pub mod class_type_alias;
pub mod classlike_info;
pub mod code_location;
pub mod codebase_info;
pub mod data_flow;
pub mod enum_case_info;
pub mod file_info;
pub mod function_context;
pub mod functionlike_identifier;
pub mod functionlike_info;
pub mod functionlike_parameter;
pub mod issue;
pub mod member_visibility;
pub mod method_identifier;
pub mod method_info;
pub mod property_info;
pub mod symbol_references;
pub mod t_atomic;
pub mod t_union;
pub mod taint;
pub mod type_definition_info;
pub mod type_resolution;

use std::{collections::BTreeMap, hash::BuildHasherDefault};

use indexmap::IndexSet;
use oxidized::{prim_defs::Comment, tast::Pos};
use rustc_hash::{self, FxHasher};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct FileSource {
    pub file_path: StrId,
    pub file_path_actual: String,
    pub hh_fixmes: BTreeMap<isize, BTreeMap<isize, Pos>>,
    pub comments: Vec<(Pos, Comment)>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct StrId(pub u32);

impl StrId {
    pub fn anonymous_fn() -> Self {
        StrId(0)
    }
    pub fn member_of() -> Self {
        StrId(4)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Interner {
    map: IndexSet<String, BuildHasherDefault<FxHasher>>,
}

impl Default for Interner {
    fn default() -> Self {
        Self {
            map: IndexSet::default(),
        }
    }
}

impl Interner {
    pub fn new() -> Self {
        let mut interner = Interner::default();
        interner.intern("<anonymous function>".to_string());
        interner.intern("echo".to_string());
        interner.intern("isset".to_string());
        interner.intern("unset".to_string());
        interner.intern("HH\\MemberOf".to_string());
        interner
    }

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
    pub fn lookup(&self, id: StrId) -> &str {
        self.map.get_index(id.0 as usize).unwrap()
    }
}
