pub mod symbols;

use self::symbols::SymbolKind;
pub use self::symbols::Symbols;
use crate::class_constant_info::ConstantInfo;
use crate::classlike_info::ClassLikeInfo;
use crate::functionlike_info::FunctionLikeInfo;
use crate::t_atomic::TAtomic;
use crate::t_union::TUnion;
use crate::type_definition_info::TypeDefinitionInfo;
use function_context::method_identifier::MethodIdentifier;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct CodebaseInfo {
    pub classlike_infos: FxHashMap<String, ClassLikeInfo>,
    pub functionlike_infos: FxHashMap<String, FunctionLikeInfo>,
    pub type_definitions: FxHashMap<String, TypeDefinitionInfo>,
    pub symbols: Symbols,
    pub infer_types_from_usage: bool,
    pub register_stub_files: bool,
    pub constant_infos: FxHashMap<String, ConstantInfo>,
    pub classlikes_in_files: FxHashMap<String, FxHashSet<String>>,
    pub typedefs_in_files: FxHashMap<String, FxHashSet<String>>,
    pub functions_in_files: FxHashMap<String, FxHashSet<String>>,
    pub const_files: FxHashMap<String, FxHashSet<String>>,
    pub classlike_descendents: FxHashMap<String, FxHashSet<String>>,
}

impl CodebaseInfo {
    pub fn new() -> Self {
        Self {
            classlike_infos: FxHashMap::default(),
            functionlike_infos: FxHashMap::default(),
            symbols: Symbols::new(),
            type_definitions: FxHashMap::default(),
            infer_types_from_usage: false,
            register_stub_files: false,
            constant_infos: FxHashMap::default(),
            classlikes_in_files: FxHashMap::default(),
            typedefs_in_files: FxHashMap::default(),
            functions_in_files: FxHashMap::default(),
            const_files: FxHashMap::default(),
            classlike_descendents: FxHashMap::default(),
        }
    }

    pub fn class_or_interface_exists(&self, fq_class_name: &String) -> bool {
        match self.symbols.all.get(fq_class_name) {
            Some(SymbolKind::Class | SymbolKind::EnumClass | SymbolKind::Interface) => true,
            _ => false,
        }
    }

    pub fn class_or_interface_or_enum_exists(&self, fq_class_name: &String) -> bool {
        match self.symbols.all.get(fq_class_name) {
            Some(
                SymbolKind::Class
                | SymbolKind::EnumClass
                | SymbolKind::Interface
                | SymbolKind::Enum,
            ) => true,
            _ => false,
        }
    }

    pub fn class_or_interface_or_enum_or_trait_exists(&self, fq_class_name: &String) -> bool {
        match self.symbols.all.get(fq_class_name) {
            Some(
                SymbolKind::Class
                | SymbolKind::EnumClass
                | SymbolKind::Interface
                | SymbolKind::Enum
                | SymbolKind::Trait,
            ) => true,
            _ => false,
        }
    }

    pub fn class_exists(&self, fq_class_name: &String) -> bool {
        match self.symbols.all.get(fq_class_name) {
            Some(SymbolKind::Class | SymbolKind::EnumClass) => true,
            _ => false,
        }
    }

    pub fn interface_exists(&self, fq_class_name: &String) -> bool {
        match self.symbols.all.get(fq_class_name) {
            Some(SymbolKind::Interface) => true,
            _ => false,
        }
    }

    pub fn typedef_exists(&self, fq_alias_name: &String) -> bool {
        match self.symbols.all.get(fq_alias_name) {
            Some(SymbolKind::TypeDefinition) => true,
            _ => false,
        }
    }

    pub fn class_extends(&self, child_class: &String, parent_class: &String) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            return classlike_storage.all_parent_classes.contains(parent_class);
        }
        false
    }

    pub fn class_extends_or_implements(&self, child_class: &String, parent_class: &String) -> bool {
        self.class_extends(child_class, parent_class)
            || self.class_implements(child_class, parent_class)
    }

    pub fn interface_extends(&self, child_class: &String, parent_class: &String) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            return classlike_storage
                .all_parent_interfaces
                .contains(parent_class);
        }
        false
    }

    pub fn class_implements(&self, child_class: &String, parent_class: &String) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            return classlike_storage
                .all_class_interfaces
                .contains(parent_class);
        }
        false
    }

    pub fn get_class_constant_type(
        &self,
        fq_class_name: &String,
        const_name: &String,
        _visited_constant_ids: FxHashSet<String>,
    ) -> Option<TUnion> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            if matches!(classlike_storage.kind, SymbolKind::Enum) {
                return Some(TUnion::new(vec![TAtomic::TEnumLiteralCase {
                    enum_name: classlike_storage.name.clone(),
                    member_name: const_name.clone(),
                    constraint_type: classlike_storage.enum_constraint.clone(),
                }]));
            } else {
                if let Some(constant_storage) = classlike_storage.constants.get(const_name) {
                    if matches!(classlike_storage.kind, SymbolKind::EnumClass) {
                        return if let Some(provided_type) = &constant_storage.provided_type {
                            Some(provided_type.clone())
                        } else {
                            None
                        };
                    } else {
                        return if let Some(provided_type) = &constant_storage.provided_type {
                            if provided_type
                                .types
                                .iter()
                                .all(|(_, v)| v.is_boring_scalar())
                            {
                                if let Some(inferred_type) = &constant_storage.inferred_type {
                                    Some(inferred_type.clone())
                                } else {
                                    Some(provided_type.clone())
                                }
                            } else {
                                Some(provided_type.clone())
                            }
                        } else if let Some(inferred_type) = &constant_storage.inferred_type {
                            Some(inferred_type.clone())
                        } else {
                            None
                        };
                    }
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_classconst_literal_value(
        &self,
        fq_class_name: &String,
        const_name: &String,
    ) -> Option<TUnion> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            if let Some(constant_storage) = classlike_storage.constants.get(const_name) {
                if let Some(inferred_type) = &constant_storage.inferred_type {
                    Some(inferred_type.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn property_exists(&self, classlike_name: &String, property_name: &String) -> bool {
        if let Some(classlike_info) = self.classlike_infos.get(classlike_name) {
            classlike_info
                .declaring_property_ids
                .contains_key(property_name)
        } else {
            false
        }
    }

    pub fn method_exists(&self, classlike_name: &String, method_name: &String) -> bool {
        if let Some(classlike_info) = self.classlike_infos.get(classlike_name) {
            classlike_info
                .declaring_method_ids
                .contains_key(method_name)
        } else {
            false
        }
    }

    pub fn declaring_method_exists(&self, classlike_name: &String, method_name: &String) -> bool {
        if let Some(classlike_info) = self.classlike_infos.get(classlike_name) {
            if let Some(declaring_class) = classlike_info.declaring_method_ids.get(method_name) {
                declaring_class == classlike_name
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn get_declaring_class_for_property(
        &self,
        fq_class_name: &String,
        property_name: &String,
    ) -> Option<&String> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            return classlike_storage.declaring_property_ids.get(property_name);
        }

        return None;
    }

    pub fn get_property_type(
        &self,
        fq_class_name: &String,
        property_name: &String,
    ) -> Option<TUnion> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            let declaring_property_class =
                classlike_storage.declaring_property_ids.get(property_name);

            let storage = if let Some(declaring_property_class) = declaring_property_class {
                let declaring_classlike_storage =
                    self.classlike_infos.get(declaring_property_class).unwrap();
                if let Some(val) = declaring_classlike_storage.properties.get(property_name) {
                    Some(val)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(storage) = storage {
                return Some(storage.type_.clone());
            }

            if let Some(overriden_properties) =
                classlike_storage.overridden_property_ids.get(property_name)
            {
                for overriden_property in overriden_properties {
                    if let Some(_overridden_storage) = self.classlike_infos.get(overriden_property)
                    {
                        // TODO handle overriden property types
                    }
                }
            }
        }

        None
    }

    pub fn get_declaring_method_id(&self, method_id: &MethodIdentifier) -> MethodIdentifier {
        if let Some(classlike_storage) = self.classlike_infos.get(&method_id.0) {
            let classlike_name = classlike_storage
                .declaring_method_ids
                .get(&method_id.1)
                .cloned()
                .unwrap_or(method_id.0.clone());
            return MethodIdentifier(classlike_name, method_id.1.clone());
        }

        method_id.clone()
    }

    pub fn get_appearing_method_id(&self, method_id: &MethodIdentifier) -> MethodIdentifier {
        if let Some(classlike_storage) = self.classlike_infos.get(&method_id.0) {
            let classlike_name = classlike_storage
                .appearing_method_ids
                .get(&method_id.1)
                .cloned()
                .unwrap_or(method_id.0.clone());
            return MethodIdentifier(classlike_name, method_id.1.clone());
        }

        method_id.clone()
    }

    pub fn get_method(&self, method_id: &MethodIdentifier) -> Option<&FunctionLikeInfo> {
        if let Some(classlike_storage) = self.classlike_infos.get(&method_id.0) {
            return classlike_storage.methods.get(&method_id.1);
        }

        None
    }

    pub fn extend(&mut self, other: CodebaseInfo) {
        self.classlike_infos.extend(other.classlike_infos);
        self.functionlike_infos.extend(other.functionlike_infos);
        self.symbols.all.extend(other.symbols.all);
        self.symbols
            .classlike_files
            .extend(other.symbols.classlike_files);
        self.type_definitions.extend(other.type_definitions);
        self.constant_infos.extend(other.constant_infos);
        self.classlikes_in_files.extend(other.classlikes_in_files);
        self.typedefs_in_files.extend(other.typedefs_in_files);
        self.functions_in_files.extend(other.functions_in_files);
        self.const_files.extend(other.const_files);
    }
}
