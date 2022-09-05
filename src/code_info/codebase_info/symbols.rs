use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SymbolKind {
    Class,
    Enum,
    EnumClass,
    Trait,
    Interface,
    TypeDefinition,
    Function,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Symbols {
    pub all: FxHashMap<String, SymbolKind>,
    pub classlike_files: FxHashMap<String, String>,
}

impl Symbols {
    pub fn new() -> Symbols {
        Symbols {
            all: FxHashMap::default(),
            classlike_files: FxHashMap::default(),
        }
    }

    pub fn add_class_name(&mut self, fq_class_name: &String, file_path: Option<&String>) {
        self.all.insert(fq_class_name.clone(), SymbolKind::Class);

        if let Some(file_path) = file_path {
            self.classlike_files
                .insert(fq_class_name.clone(), file_path.clone());
        }
    }

    pub fn add_enum_class_name(&mut self, fq_class_name: &String, file_path: Option<&String>) {
        self.all
            .insert(fq_class_name.clone(), SymbolKind::EnumClass);

        if let Some(file_path) = file_path {
            self.classlike_files
                .insert(fq_class_name.clone(), file_path.clone());
        }
    }

    pub fn add_interface_name(&mut self, fq_class_name: &String, file_path: Option<&String>) {
        self.all
            .insert(fq_class_name.clone(), SymbolKind::Interface);

        if let Some(file_path) = file_path {
            self.classlike_files
                .insert(fq_class_name.clone(), file_path.clone());
        }
    }

    pub fn add_trait_name(&mut self, fq_class_name: &String, file_path: Option<&String>) {
        self.all.insert(fq_class_name.clone(), SymbolKind::Trait);

        if let Some(file_path) = file_path {
            self.classlike_files
                .insert(fq_class_name.clone(), file_path.clone());
        }
    }

    pub fn add_enum_name(&mut self, fq_class_name: &String, file_path: Option<&String>) {
        self.all.insert(fq_class_name.clone(), SymbolKind::Enum);

        if let Some(file_path) = file_path {
            self.classlike_files
                .insert(fq_class_name.clone(), file_path.clone());
        }
    }

    pub fn add_typedef_name(&mut self, fq_class_name: String) {
        self.all.insert(fq_class_name, SymbolKind::TypeDefinition);
    }

    pub fn add_function_name(&mut self, function_name: String) {
        self.all.insert(function_name, SymbolKind::Function);
    }
}
