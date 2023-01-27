use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    code_location::HPos, codebase_info::symbols::SymbolKind, functionlike_info::FunctionLikeInfo,
    t_atomic::TAtomic, t_union::TUnion, StrId,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    aliases::Aliases, attribute_info::AttributeInfo, class_constant_info::ConstantInfo,
    enum_case_info::EnumCaseInfo, property_info::PropertyInfo,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Variance {
    Invariant,
    Covariant,
    Contravariant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassLikeInfo {
    pub constants: IndexMap<StrId, ConstantInfo>,

    /**
     * Aliases to help Hakana understand constant refs
     */
    pub aliases: Option<Aliases>,

    pub is_populated: bool,

    pub is_stubbed: bool,

    pub is_deprecated: bool,

    pub internal_to: Option<String>,

    pub suppressed_issues: Option<FxHashMap<u32, String>>,

    pub name: StrId,

    pub is_user_defined: bool,

    /**
     * Interfaces this class implements directly
     */
    pub direct_class_interfaces: FxHashSet<StrId>,

    /**
     * Interfaces this class implements explicitly and implicitly
     */
    pub all_class_interfaces: FxHashSet<StrId>,

    /**
     * Parent interfaces listed explicitly
     */
    pub direct_parent_interfaces: FxHashSet<StrId>,

    /**
     * All parent interfaces
     */
    pub all_parent_interfaces: FxHashSet<StrId>,

    /**
     * There can only be one parent class
     */
    pub direct_parent_class: Option<StrId>,

    /**
     * Parent classes
     */
    pub all_parent_classes: FxHashSet<StrId>,

    pub def_location: HPos,

    pub name_location: HPos,

    pub is_abstract: bool,

    pub is_final: bool,

    pub kind: SymbolKind,

    pub used_traits: FxHashSet<StrId>,

    pub immutable: bool,

    pub specialize_instance: bool,

    pub methods: FxHashMap<StrId, FunctionLikeInfo>,

    pub declaring_method_ids: FxHashMap<StrId, StrId>,

    pub appearing_method_ids: FxHashMap<StrId, StrId>,

    /**
     * Map from lowercase method name to list of declarations in order from parent, to grandparent, to
     * great-grandparent, etc **including traits and interfaces**. Ancestors that don't have their own declaration are
     * skipped.
     */
    pub overridden_method_ids: FxHashMap<StrId, FxHashSet<StrId>>,

    pub inheritable_method_ids: FxHashMap<StrId, StrId>,

    pub potential_declaring_method_ids: FxHashMap<StrId, FxHashSet<StrId>>,

    pub properties: FxHashMap<StrId, PropertyInfo>,

    pub appearing_property_ids: FxHashMap<StrId, StrId>,

    pub declaring_property_ids: FxHashMap<StrId, StrId>,

    pub inheritable_property_ids: FxHashMap<StrId, StrId>,

    pub overridden_property_ids: FxHashMap<StrId, Vec<StrId>>,

    /**
     * An array holding the class template "as" types.
     *
     * It's the de-facto list of all templates on a given class.
     *
     * The name of the template is the first key. The nested array is keyed by the defining class
     * (i.e. the same as the class name). This allows operations with the same-named template defined
     * across multiple classes to not run into trouble.
     */
    pub template_types: IndexMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,

    pub generic_variance: FxHashMap<usize, Variance>,

    /**
     * A map of which generic classlikes are extended or implemented by this class or interface.
     *
     * This is only used in the populator, which poulates the $template_extended_params property below.
     *
     * @internal
     */
    pub template_extended_offsets: FxHashMap<StrId, Vec<Arc<TUnion>>>,

    /**
     * A map of which generic classlikes are extended or implemented by this class or interface.
     *
     * The annotation "@extends Traversable<SomeClass, SomeOtherClass>" would generate an entry of
     *
     * [
     *     "Traversable" => [
     *         "TKey" => new Union([new TNamedObject("SomeClass")]),
     *         "TValue" => new Union([new TNamedObject("SomeOtherClass")])
     *     ]
     * ]
     */
    pub template_extended_params: FxHashMap<StrId, IndexMap<StrId, Arc<TUnion>>>,

    pub template_extended_count: u32,

    pub template_type_implements_count: FxHashMap<String, u32>,

    pub template_type_uses_count: FxHashMap<String, u32>,

    pub initialized_properties: FxHashSet<StrId>,

    pub invalid_dependencies: Vec<StrId>,

    /**
     * A hash of the source file's name, contents, and this file's modified on date
     */
    pub hash: Option<String>,

    pub has_visitor_issues: bool,

    pub preserve_constructor_signature: bool,

    pub enforce_template_inheritance: bool,

    pub attributes: Vec<AttributeInfo>,

    pub enum_cases: Option<FxHashMap<String, EnumCaseInfo>>,

    pub enum_type: Option<TAtomic>,
    pub enum_constraint: Option<Box<TAtomic>>,

    pub type_constants: FxHashMap<StrId, Option<TUnion>>,

    pub user_defined: bool,

    pub generated: bool,

    pub child_classlikes: Option<FxHashSet<StrId>>,

    pub uses_position: Option<(usize, usize)>,
    pub namespace_position: Option<(usize, usize)>,

    pub is_production_code: bool,
}

impl ClassLikeInfo {
    pub fn new(name: StrId, def_location: HPos, name_location: HPos) -> ClassLikeInfo {
        ClassLikeInfo {
            constants: IndexMap::default(),
            is_populated: false,
            is_stubbed: false,
            is_deprecated: false,
            is_user_defined: false,
            is_abstract: false,
            is_final: false,
            kind: SymbolKind::Class,
            immutable: false,
            specialize_instance: false,
            has_visitor_issues: false,
            preserve_constructor_signature: false,
            enforce_template_inheritance: false,

            direct_class_interfaces: FxHashSet::default(),
            aliases: None,
            all_parent_classes: FxHashSet::default(),
            appearing_method_ids: FxHashMap::default(),
            attributes: Vec::new(),
            all_class_interfaces: FxHashSet::default(),
            all_parent_interfaces: FxHashSet::default(),
            declaring_method_ids: FxHashMap::default(),
            appearing_property_ids: FxHashMap::default(),
            declaring_property_ids: FxHashMap::default(),
            direct_parent_class: None,
            direct_parent_interfaces: FxHashSet::default(),
            inheritable_method_ids: FxHashMap::default(),
            enum_cases: None,
            enum_type: None,
            enum_constraint: None,
            hash: None,
            inheritable_property_ids: FxHashMap::default(),
            initialized_properties: FxHashSet::default(),
            internal_to: None,
            invalid_dependencies: Vec::new(),
            def_location,
            name_location,
            methods: FxHashMap::default(),
            overridden_method_ids: FxHashMap::default(),
            overridden_property_ids: FxHashMap::default(),
            potential_declaring_method_ids: FxHashMap::default(),
            properties: FxHashMap::default(),
            suppressed_issues: None,
            generic_variance: FxHashMap::default(),
            template_extended_count: 0,
            template_extended_params: FxHashMap::default(),
            template_extended_offsets: FxHashMap::default(),
            template_type_implements_count: FxHashMap::default(),
            template_type_uses_count: FxHashMap::default(),
            template_types: IndexMap::new(),
            used_traits: FxHashSet::default(),
            name,
            type_constants: FxHashMap::default(),
            user_defined: false,
            generated: false,
            child_classlikes: None,
            uses_position: None,
            namespace_position: None,
            is_production_code: true,
        }
    }
}
