use std::sync::Arc;

use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{
    assertion::Assertion,
    attribute_info::AttributeInfo,
    code_location::HPos,
    functionlike_parameter::FunctionLikeParameter,
    issue::IssueKind,
    method_info::MethodInfo,
    t_union::TUnion,
    taint::{SinkType, SourceType},
    type_resolution::TypeResolutionContext,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionLikeInfo {
    pub def_location: Option<HPos>,

    pub name_location: Option<HPos>,

    pub params: Vec<FunctionLikeParameter>,

    pub return_type: Option<TUnion>,

    pub return_type_location: Option<HPos>,

    pub name: String,

    pub suppressed_issues: Option<FxHashMap<IssueKind, HPos>>,

    pub deprecated: bool,

    pub internal_to: Option<String>,

    /**
     * An array holding the class template "as" types.
     *
     * It's the de-facto list of all templates on a given class.
     *
     * The name of the template is the first key. The nested array is keyed by a unique
     * function identifier. This allows operations with the same-named template defined
     * across multiple classes and/or functions to not run into trouble.
     */
    pub template_types: IndexMap<String, FxHashMap<String, Arc<TUnion>>>,

    pub template_covariants: FxHashMap<u32, bool>,

    pub assertions: Option<FxHashMap<usize, Assertion>>,

    pub if_true_assertions: Option<FxHashMap<usize, Assertion>>,

    pub if_false_assertions: Option<FxHashMap<usize, Assertion>>,

    pub has_visitor_issues: bool,

    pub has_yield: bool,

    pub is_async: bool,

    pub mutation_free: bool,

    pub pure: bool,

    /**
     * Whether or not the function output is dependent solely on input - a function can be
     * impure but still have this property (e.g. var_export). Useful for taint analysis.
     */
    pub specialize_call: bool,

    /**
     * If this is given we don't allow anything to be tainted via this function/method.
     * Useful for things that are only executed in tests
     */
    pub ignore_taint_path: bool,

    /**
     * If this function returns true, block taints while this holds
     */
    pub ignore_taints_if_true: bool,

    pub taint_source_types: FxHashSet<SourceType>,

    pub added_taints: Option<FxHashSet<SinkType>>,

    pub removed_taints: Option<FxHashSet<SinkType>>,

    pub return_source_params: FxHashMap<usize, String>,

    pub attributes: Vec<AttributeInfo>,

    pub method_info: Option<MethodInfo>,

    // used for dead-code analysis
    pub user_defined: bool,

    // used for dead-code analysis — this is true for all __EntryPoint and __DynamicallyCallable functions
    pub dynamically_callable: bool,

    // generated functions also get a pass
    pub generated: bool,

    pub type_resolution_context: Option<TypeResolutionContext>,
}

impl FunctionLikeInfo {
    pub fn new(name: String) -> Self {
        Self {
            def_location: None,
            name_location: None,
            params: Vec::new(),
            return_type: None,
            return_type_location: None,
            name,
            suppressed_issues: None,
            deprecated: false,
            internal_to: None,
            template_types: IndexMap::new(),
            template_covariants: FxHashMap::default(),
            assertions: None,
            if_true_assertions: None,
            if_false_assertions: None,
            has_visitor_issues: false,
            has_yield: false,
            mutation_free: false,
            pure: false,
            specialize_call: false,
            taint_source_types: FxHashSet::default(),
            added_taints: None,
            removed_taints: None,
            return_source_params: FxHashMap::default(),
            attributes: Vec::new(),
            method_info: None,
            is_async: false,
            ignore_taint_path: false,
            user_defined: false,
            dynamically_callable: false,
            generated: false,
            ignore_taints_if_true: false,
            type_resolution_context: None,
        }
    }
}
