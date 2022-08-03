use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    assertion::Assertion, attribute_info::AttributeInfo, code_location::HPos,
    functionlike_parameter::FunctionLikeParameter, issue::IssueKind, method_info::MethodInfo,
    t_union::TUnion, taint::TaintType, type_resolution::TypeResolutionContext,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionLikeInfo {
    pub def_location: Option<HPos>,

    pub name_location: Option<HPos>,

    pub params: Vec<FunctionLikeParameter>,

    pub return_type: Option<TUnion>,

    pub return_type_location: Option<HPos>,

    pub name: String,

    pub suppressed_issues: Option<HashMap<IssueKind, HPos>>,

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
    pub template_types: IndexMap<String, HashMap<String, TUnion>>,

    pub template_covariants: HashMap<u32, bool>,

    pub assertions: Option<HashMap<usize, Assertion>>,

    pub if_true_assertions: Option<HashMap<usize, Assertion>>,

    pub if_false_assertions: Option<HashMap<usize, Assertion>>,

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

    pub taint_source_types: HashSet<TaintType>,

    pub added_taints: HashSet<TaintType>,

    pub removed_taints: HashSet<TaintType>,

    pub return_source_params: HashMap<usize, String>,

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
            template_covariants: HashMap::new(),
            assertions: None,
            if_true_assertions: None,
            if_false_assertions: None,
            has_visitor_issues: false,
            has_yield: false,
            mutation_free: false,
            pure: false,
            specialize_call: false,
            taint_source_types: HashSet::new(),
            added_taints: HashSet::new(),
            removed_taints: HashSet::new(),
            return_source_params: HashMap::new(),
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
    // pub fn to_string(&mut self) -> String
    // {
    //     self.get_signature(false)
    // }

    // pub fn getSignature(allow_newlines: bool) -> String {
    //     let allow_newlines = allow_newlines && self.params.count() > 0;

    //     $symbol_text = 'function ' . $this->cased_name . '(' . ($newlines ? "\n" : '') . implode(
    //         ',' . ($newlines ? "\n" : ' '),
    //         array_map(
    //             function (FunctionLikeParameter $param) use ($newlines) : string {
    //                 return ($newlines ? '    ' : '') . ($param->type ?: 'mixed') . ' $' . $param->name;
    //             },
    //             $this->params
    //         )
    //     ) . ($newlines ? "\n" : '') . ') : ' . ($this->return_type ?: 'mixed');

    //     if (!$this instanceof MethodStorage) {
    //         return $symbol_text;
    //     }

    //     switch ($this->visibility) {
    //         case ClassLikeAnalyzer::VISIBILITY_PRIVATE:
    //             $visibility_text = 'private';
    //             break;

    //         case ClassLikeAnalyzer::VISIBILITY_PROTECTED:
    //             $visibility_text = 'protected';
    //             break;

    //         default:
    //             $visibility_text = 'public';
    //     }

    //     return $visibility_text . ' ' . $symbol_text;
    // }

    // /**
    //  * @internal
    //  *
    //  * @param list<FunctionLikeParameter> $params
    //  */
    // pub fn setParams(array $params): void
    // {
    //     $this->params = $params;
    //     $param_names = array_column($params, 'name');
    //     $this->param_lookup = array_fill_keys($param_names, true);
    // }

    // /**
    //  * @internal
    //  */
    // pub fn addParam(FunctionLikeParameter $param, bool $lookup_value = null): void
    // {
    //     $this->params[] = $param;
    //     $this->param_lookup[$param->name] = $lookup_value ?? true;
    // }
}
