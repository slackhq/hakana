pub mod symbols;

use std::sync::Arc;

use self::symbols::SymbolKind;
pub use self::symbols::Symbols;
use crate::classlike_info::ClassLikeInfo;
use crate::code_location::HPos;
use crate::file_info::FileInfo;
use crate::functionlike_info::FunctionLikeInfo;
use crate::method_identifier::MethodIdentifier;
use crate::property_info::PropertyInfo;
use crate::t_atomic::TAtomic;
use crate::t_union::TUnion;
use crate::type_definition_info::TypeDefinitionInfo;
use crate::{class_constant_info::ConstantInfo, code_location::FilePath};
use hakana_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodebaseInfo {
    pub classlike_infos: FxHashMap<StrId, ClassLikeInfo>,
    pub functionlike_infos: FxHashMap<(StrId, StrId), FunctionLikeInfo>,
    pub type_definitions: FxHashMap<StrId, TypeDefinitionInfo>,
    pub symbols: Symbols,
    pub infer_types_from_usage: bool,
    pub constant_infos: FxHashMap<StrId, ConstantInfo>,
    pub closures_in_files: FxHashMap<FilePath, FxHashSet<StrId>>,
    pub const_files: FxHashMap<String, FxHashSet<StrId>>,
    pub all_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>>,
    pub direct_classlike_descendants: FxHashMap<StrId, FxHashSet<StrId>>,
    pub files: FxHashMap<FilePath, FileInfo>,

    /* Symbols that have already been checked on a previous Hakana run */
    pub safe_symbols: FxHashSet<StrId>,
    /* Symbol members that have already been checked on a previous Hakana run */
    pub safe_symbol_members: FxHashSet<(StrId, StrId)>,
}

impl Default for CodebaseInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl CodebaseInfo {
    pub fn new() -> Self {
        Self {
            classlike_infos: FxHashMap::default(),
            functionlike_infos: FxHashMap::default(),
            symbols: Symbols::new(),
            type_definitions: FxHashMap::default(),
            infer_types_from_usage: false,
            constant_infos: FxHashMap::default(),
            closures_in_files: FxHashMap::default(),
            const_files: FxHashMap::default(),
            all_classlike_descendants: FxHashMap::default(),
            direct_classlike_descendants: FxHashMap::default(),
            files: FxHashMap::default(),
            safe_symbols: FxHashSet::default(),
            safe_symbol_members: FxHashSet::default(),
        }
    }

    #[inline]
    pub fn class_or_interface_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(
            self.symbols.all.get(fq_class_name),
            Some(SymbolKind::Class | SymbolKind::EnumClass | SymbolKind::Interface)
        )
    }

    #[inline]
    pub fn class_or_interface_or_enum_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(
            self.symbols.all.get(fq_class_name),
            Some(
                SymbolKind::Class
                    | SymbolKind::EnumClass
                    | SymbolKind::Interface
                    | SymbolKind::Enum,
            )
        )
    }

    #[inline]
    pub fn class_or_interface_or_enum_or_trait_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(
            self.symbols.all.get(fq_class_name),
            Some(
                SymbolKind::Class
                    | SymbolKind::EnumClass
                    | SymbolKind::Interface
                    | SymbolKind::Enum
                    | SymbolKind::Trait,
            )
        )
    }

    #[inline]
    pub fn class_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(
            self.symbols.all.get(fq_class_name),
            Some(SymbolKind::Class | SymbolKind::EnumClass)
        )
    }

    #[inline]
    pub fn trait_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(self.symbols.all.get(fq_class_name), Some(SymbolKind::Trait))
    }

    #[inline]
    pub fn class_or_trait_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(
            self.symbols.all.get(fq_class_name),
            Some(SymbolKind::Class | SymbolKind::EnumClass | SymbolKind::Trait)
        )
    }

    #[inline]
    pub fn interface_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(
            self.symbols.all.get(fq_class_name),
            Some(SymbolKind::Interface)
        )
    }

    #[inline]
    pub fn enum_exists(&self, fq_class_name: &StrId) -> bool {
        matches!(self.symbols.all.get(fq_class_name), Some(SymbolKind::Enum))
    }

    #[inline]
    pub fn typedef_exists(&self, fq_alias_name: &StrId) -> bool {
        matches!(
            self.symbols.all.get(fq_alias_name),
            Some(SymbolKind::TypeDefinition | SymbolKind::NewtypeDefinition)
        )
    }

    pub fn class_or_trait_extends(&self, child_class: &StrId, parent_class: &StrId) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            return classlike_storage.all_parent_classes.contains(parent_class);
        }
        false
    }

    pub fn class_extends_or_implements(&self, child_class: &StrId, parent_class: &StrId) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            return classlike_storage.all_parent_classes.contains(parent_class)
                || classlike_storage
                    .all_parent_interfaces
                    .contains(parent_class);
        }
        false
    }

    pub fn can_intersect_interface(&self, fq_class_name: &StrId) -> bool {
        match self.symbols.all.get(fq_class_name) {
            Some(SymbolKind::Class) => {
                if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
                    !classlike_storage.is_final
                } else {
                    false
                }
            }
            _ => true,
        }
    }

    pub fn class_or_interface_can_use_trait(
        &self,
        child_class: &StrId,
        parent_trait: &StrId,
    ) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            if classlike_storage.used_traits.contains(parent_trait) {
                return true;
            }

            if let Some(parent_trait_storage) = self.classlike_infos.get(parent_trait) {
                for trait_parent_interface in &parent_trait_storage.direct_parent_interfaces {
                    if self.interface_extends(child_class, trait_parent_interface) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn interface_extends(&self, child_class: &StrId, parent_class: &StrId) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            return classlike_storage
                .all_parent_interfaces
                .contains(parent_class)
                || classlike_storage.all_parent_classes.contains(parent_class);
        }
        false
    }

    pub fn class_or_trait_implements(&self, child_class: &StrId, parent_class: &StrId) -> bool {
        if let Some(classlike_storage) = self.classlike_infos.get(child_class) {
            return classlike_storage
                .all_parent_interfaces
                .contains(parent_class);
        }
        false
    }

    pub fn get_class_constant_type(
        &self,
        fq_class_name: &StrId,
        is_this: bool,
        const_name: &StrId,
        _visited_constant_ids: FxHashSet<String>,
    ) -> Option<TUnion> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            if matches!(classlike_storage.kind, SymbolKind::Enum) {
                Some(TUnion::new(vec![TAtomic::TEnumLiteralCase {
                    enum_name: classlike_storage.name,
                    member_name: *const_name,
                    as_type: classlike_storage
                        .enum_as_type
                        .as_ref()
                        .map(|t| Arc::new(t.clone())),
                    underlying_type: classlike_storage
                        .enum_underlying_type
                        .as_ref()
                        .map(|t| Arc::new(t.clone())),
                }]))
            } else if let Some(constant_storage) = classlike_storage.constants.get(const_name) {
                if matches!(classlike_storage.kind, SymbolKind::EnumClass) {
                    return constant_storage.provided_type.clone();
                } else if let Some(provided_type) = &constant_storage.provided_type {
                    if provided_type.types.iter().all(|v| v.is_boring_scalar()) && !is_this {
                        if let Some(inferred_type) = &constant_storage.inferred_type {
                            Some(TUnion::new(vec![inferred_type.clone()]))
                        } else {
                            Some(provided_type.clone())
                        }
                    } else {
                        Some(provided_type.clone())
                    }
                } else if let Some(inferred_type) = &constant_storage.inferred_type {
                    if !is_this {
                        Some(TUnion::new(vec![inferred_type.clone()]))
                    } else {
                        None
                    }
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

    pub fn get_classconst_literal_value(
        &self,
        fq_class_name: &StrId,
        const_name: &StrId,
    ) -> Option<&TAtomic> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            if let Some(constant_storage) = classlike_storage.constants.get(const_name) {
                constant_storage.inferred_type.as_ref()
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn property_exists(&self, classlike_name: &StrId, property_name: &StrId) -> bool {
        if let Some(classlike_info) = self.classlike_infos.get(classlike_name) {
            classlike_info
                .declaring_property_ids
                .contains_key(property_name)
        } else {
            false
        }
    }

    pub fn method_exists(&self, classlike_name: &StrId, method_name: &StrId) -> bool {
        if let Some(classlike_info) = self.classlike_infos.get(classlike_name) {
            classlike_info
                .declaring_method_ids
                .contains_key(method_name)
        } else {
            false
        }
    }

    pub fn declaring_method_exists(&self, classlike_name: &StrId, method_name: &StrId) -> bool {
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
        fq_class_name: &StrId,
        property_name: &StrId,
    ) -> Option<StrId> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            return classlike_storage
                .declaring_property_ids
                .get(property_name)
                .copied();
        }

        None
    }

    pub fn get_property_storage(
        &self,
        fq_class_name: &StrId,
        property_name: &StrId,
    ) -> Option<&PropertyInfo> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            classlike_storage.properties.get(property_name)
        } else {
            None
        }
    }

    pub fn get_property_type(
        &self,
        fq_class_name: &StrId,
        property_name: &StrId,
    ) -> Option<TUnion> {
        if let Some(classlike_storage) = self.classlike_infos.get(fq_class_name) {
            let declaring_property_class =
                classlike_storage.declaring_property_ids.get(property_name);

            let storage = if let Some(declaring_property_class) = declaring_property_class {
                let declaring_classlike_storage =
                    self.classlike_infos.get(declaring_property_class).unwrap();
                declaring_classlike_storage.properties.get(property_name)
            } else {
                None
            };

            if let Some(storage) = storage {
                return Some(storage.type_.clone());
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
                .unwrap_or(method_id.0);
            return MethodIdentifier(classlike_name, method_id.1);
        }

        *method_id
    }

    pub fn get_appearing_method_id(&self, method_id: &MethodIdentifier) -> MethodIdentifier {
        if let Some(classlike_storage) = self.classlike_infos.get(&method_id.0) {
            let classlike_name = classlike_storage
                .appearing_method_ids
                .get(&method_id.1)
                .cloned()
                .unwrap_or(method_id.0);
            return MethodIdentifier(classlike_name, method_id.1);
        }

        *method_id
    }

    pub fn get_symbol_pos(&self, classlike_name: &StrId, member_name: &StrId) -> Option<HPos> {
        let classlike_info = self.classlike_infos.get(classlike_name);

        if *member_name == StrId::EMPTY {
            if let Some(classlike_info) = classlike_info {
                return Some(classlike_info.name_location);
            }
        }

        if let Some(classlike_info) = classlike_info {
            if let Some(property_info) = classlike_info.properties.get(member_name) {
                if let Some(property_pos) = property_info.pos {
                    return Some(property_pos);
                }
            }

            if let Some(constant_info) = classlike_info.constants.get(member_name) {
                return Some(constant_info.pos);
            }
        } else if let Some(type_info) = self.type_definitions.get(classlike_name) {
            return Some(type_info.location);
        }

        let functionlike_info = self
            .functionlike_infos
            .get(&(*classlike_name, *member_name));

        if let Some(functionlike_info) = functionlike_info {
            if let Some(name_pos) = functionlike_info.name_location {
                return Some(name_pos);
            }
        }

        return None;
    }

    #[inline]
    pub fn get_method(&self, method_id: &MethodIdentifier) -> Option<&FunctionLikeInfo> {
        self.functionlike_infos.get(&(method_id.0, method_id.1))
    }

    pub fn get_all_descendants(&self, classlike_name: &StrId) -> FxHashSet<StrId> {
        let mut base_set = FxHashSet::default();

        if let Some(classlike_descendants) = self.all_classlike_descendants.get(classlike_name) {
            base_set.extend(classlike_descendants);
            for classlike_descendant in classlike_descendants {
                base_set.extend(self.get_all_descendants(classlike_descendant));
            }
        }

        base_set
    }

    #[inline]
    pub fn get_declaring_method(&self, method_id: &MethodIdentifier) -> Option<&FunctionLikeInfo> {
        self.get_method(&self.get_declaring_method_id(method_id))
    }

    pub fn extend(&mut self, other: CodebaseInfo) {
        self.classlike_infos.extend(other.classlike_infos);
        self.functionlike_infos.extend(other.functionlike_infos);
        self.symbols.all.extend(other.symbols.all);
        self.type_definitions.extend(other.type_definitions);
        self.constant_infos.extend(other.constant_infos);
        self.closures_in_files.extend(other.closures_in_files);
        self.const_files.extend(other.const_files);
        self.files.extend(other.files);
    }
}
