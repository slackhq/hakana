use hakana_str::StrId;
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
    NewtypeDefinition,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Symbols {
    pub all: FxHashMap<StrId, SymbolKind>,
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
        }
    }

    pub fn add_class_name(&mut self, fq_class_name: &StrId) {
        self.all.insert(*fq_class_name, SymbolKind::Class);
    }

    pub fn add_enum_class_name(&mut self, fq_class_name: &StrId) {
        self.all.insert(*fq_class_name, SymbolKind::EnumClass);
    }

    pub fn add_interface_name(&mut self, fq_class_name: &StrId) {
        self.all.insert(*fq_class_name, SymbolKind::Interface);
    }

    pub fn add_trait_name(&mut self, fq_class_name: &StrId) {
        self.all.insert(*fq_class_name, SymbolKind::Trait);
    }

    pub fn add_enum_name(&mut self, fq_class_name: &StrId) {
        self.all.insert(*fq_class_name, SymbolKind::Enum);
    }

    pub fn add_typedef_name(&mut self, fq_class_name: StrId, is_newtype: bool) {
        if is_newtype {
            self.all
                .insert(fq_class_name, SymbolKind::NewtypeDefinition);
        } else {
            self.all.insert(fq_class_name, SymbolKind::TypeDefinition);
        }
    }
}
