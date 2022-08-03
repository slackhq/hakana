use std::collections::{HashMap, HashSet};

use crate::{
    code_location::HPos, codebase_info::symbols::SymbolKind, functionlike_info::FunctionLikeInfo,
    t_atomic::TAtomic, t_union::TUnion,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    aliases::Aliases, attribute_info::AttributeInfo, class_constant_info::ConstantInfo,
    class_type_alias::ClassTypeAlias, enum_case_info::EnumCaseInfo, property_info::PropertyInfo,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassLikeInfo {
    pub constants: HashMap<String, ConstantInfo>,

    /**
     * Aliases to help Hakana understand constant refs
     */
    pub aliases: Option<Aliases>,

    pub is_populated: bool,

    pub is_stubbed: bool,

    pub is_deprecated: bool,

    pub internal_to: Option<String>,

    pub suppressed_issues: Option<HashMap<u32, String>>,

    pub name: String,

    pub is_user_defined: bool,

    /**
     * Interfaces this class implements directly
     */
    pub direct_class_interfaces: HashSet<String>,

    /**
     * Interfaces this class implements explicitly and implicitly
     */
    pub all_class_interfaces: HashSet<String>,

    /**
     * Parent interfaces listed explicitly
     */
    pub direct_parent_interfaces: HashSet<String>,

    /**
     * All parent interfaces
     */
    pub all_parent_interfaces: HashSet<String>,

    /**
     * There can only be one parent class
     */
    pub direct_parent_class: Option<String>,

    /**
     * Parent classes
     */
    pub all_parent_classes: HashSet<String>,

    pub def_location: Option<HPos>,

    pub name_location: Option<HPos>,

    pub is_abstract: bool,

    pub is_final: bool,

    pub kind: SymbolKind,

    pub used_traits: HashSet<String>,

    pub trait_alias_map: HashMap<String, String>,

    pub trait_final_map: HashMap<String, String>,

    pub trait_visibility_map: HashMap<String, String>,

    pub immutable: bool,

    pub specialize_instance: bool,

    pub methods: HashMap<String, FunctionLikeInfo>,

    pub declaring_method_ids: HashMap<String, String>,

    pub appearing_method_ids: HashMap<String, String>,

    /**
     * Map from lowercase method name to list of declarations in order from parent, to grandparent, to
     * great-grandparent, etc **including traits and interfaces**. Ancestors that don't have their own declaration are
     * skipped.
     */
    pub overridden_method_ids: HashMap<String, HashSet<String>>,

    pub inheritable_method_ids: HashMap<String, String>,

    pub potential_declaring_method_ids: HashMap<String, HashSet<String>>,

    pub properties: HashMap<String, PropertyInfo>,

    pub appearing_property_ids: HashMap<String, String>,

    pub declaring_property_ids: HashMap<String, String>,

    pub inheritable_property_ids: HashMap<String, String>,

    pub overridden_property_ids: HashMap<String, Vec<String>>,

    /**
     * An array holding the class template "as" types.
     *
     * It's the de-facto list of all templates on a given class.
     *
     * The name of the template is the first key. The nested array is keyed by the defining class
     * (i.e. the same as the class name). This allows operations with the same-named template defined
     * across multiple classes to not run into trouble.
     */
    pub template_types: IndexMap<String, HashMap<String, TUnion>>,

    pub template_covariants: HashSet<usize>,

    /**
     * A map of which generic classlikes are extended or implemented by this class or interface.
     *
     * This is only used in the populator, which poulates the $template_extended_params property below.
     *
     * @internal
     */
    pub template_extended_offsets: HashMap<String, Vec<TUnion>>,

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
    pub template_extended_params: HashMap<String, IndexMap<String, TUnion>>,

    pub template_extended_count: u32,

    pub template_type_implements_count: HashMap<String, u32>,

    pub template_type_uses_count: HashMap<String, u32>,

    pub initialized_properties: HashSet<String>,

    pub invalid_dependencies: Vec<String>,

    /**
     * A hash of the source file's name, contents, and this file's modified on date
     */
    pub hash: Option<String>,

    pub has_visitor_issues: bool,

    pub type_aliases: HashMap<String, ClassTypeAlias>,

    pub preserve_constructor_signature: bool,

    pub enforce_template_inheritance: bool,

    pub extension_requirement: Option<String>,

    pub implementation_requirements: Vec<String>,

    pub attributes: Vec<AttributeInfo>,

    pub enum_cases: Option<HashMap<String, EnumCaseInfo>>,

    pub enum_type: Option<TAtomic>,

    pub description: Option<String>,

    pub type_constants: HashMap<String, TUnion>,

    pub user_defined: bool,

    pub generated: bool,

    pub child_classlikes: Option<HashSet<String>>,
}

impl Default for ClassLikeInfo {
    fn default() -> ClassLikeInfo {
        ClassLikeInfo {
            constants: HashMap::new(),
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

            direct_class_interfaces: HashSet::new(),
            aliases: None,
            all_parent_classes: HashSet::new(),
            appearing_method_ids: HashMap::new(),
            attributes: Vec::new(),
            all_class_interfaces: HashSet::new(),
            all_parent_interfaces: HashSet::new(),
            declaring_method_ids: HashMap::new(),
            appearing_property_ids: HashMap::new(),
            declaring_property_ids: HashMap::new(),
            direct_parent_class: None,
            description: None,
            direct_parent_interfaces: HashSet::new(),
            inheritable_method_ids: HashMap::new(),
            enum_cases: None,
            enum_type: None,
            extension_requirement: None,
            hash: None,
            implementation_requirements: Vec::new(),
            inheritable_property_ids: HashMap::new(),
            initialized_properties: HashSet::new(),
            internal_to: None,
            invalid_dependencies: Vec::new(),
            def_location: None,
            name_location: None,
            methods: HashMap::new(),
            overridden_method_ids: HashMap::new(),
            overridden_property_ids: HashMap::new(),
            potential_declaring_method_ids: HashMap::new(),
            properties: HashMap::new(),
            suppressed_issues: None,
            template_covariants: HashSet::new(),
            template_extended_count: 0,
            template_extended_params: HashMap::new(),
            template_extended_offsets: HashMap::new(),
            template_type_implements_count: HashMap::new(),
            template_type_uses_count: HashMap::new(),
            template_types: IndexMap::new(),
            trait_alias_map: HashMap::new(),
            trait_final_map: HashMap::new(),
            trait_visibility_map: HashMap::new(),
            type_aliases: HashMap::new(),
            used_traits: HashSet::new(),
            name: "".to_string(),

            type_constants: HashMap::new(),
            user_defined: false,
            generated: false,
            child_classlikes: None,
        }
    }
}
