use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{code_location::FilePath, StrId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SymbolKind {
    Class,
    Enum,
    EnumClass,
    Trait,
    Interface,
    TypeDefinition,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Symbols {
    pub all: FxHashMap<StrId, SymbolKind>,
    pub classlike_files: FxHashMap<StrId, FilePath>,
}

impl Default for Symbols {
    fn default() -> Self {
        Self::new()
    }
}

impl Symbols {
    pub fn new() -> Symbols {
        Symbols {
            all: FxHashMap::default(),
            classlike_files: FxHashMap::default(),
        }
    }

    pub fn add_class_name(&mut self, fq_class_name: &StrId, file_path: Option<FilePath>) {
        self.all.insert(*fq_class_name, SymbolKind::Class);

        if let Some(file_path) = file_path {
            self.classlike_files.insert(*fq_class_name, file_path);
        }
    }

    pub fn add_enum_class_name(&mut self, fq_class_name: &StrId, file_path: Option<FilePath>) {
        self.all.insert(*fq_class_name, SymbolKind::EnumClass);

        if let Some(file_path) = file_path {
            self.classlike_files.insert(*fq_class_name, file_path);
        }
    }

    pub fn add_interface_name(&mut self, fq_class_name: &StrId, file_path: Option<FilePath>) {
        self.all.insert(*fq_class_name, SymbolKind::Interface);

        if let Some(file_path) = file_path {
            self.classlike_files.insert(*fq_class_name, file_path);
        }
    }

    pub fn add_trait_name(&mut self, fq_class_name: &StrId, file_path: Option<FilePath>) {
        self.all.insert(*fq_class_name, SymbolKind::Trait);

        if let Some(file_path) = file_path {
            self.classlike_files.insert(*fq_class_name, file_path);
        }
    }

    pub fn add_enum_name(&mut self, fq_class_name: &StrId, file_path: Option<FilePath>) {
        self.all.insert(*fq_class_name, SymbolKind::Enum);

        if let Some(file_path) = file_path {
            self.classlike_files.insert(*fq_class_name, file_path);
        }
    }

    pub fn add_typedef_name(&mut self, fq_class_name: StrId) {
        self.all.insert(fq_class_name, SymbolKind::TypeDefinition);
    }
}
