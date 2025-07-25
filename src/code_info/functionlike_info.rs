use std::sync::Arc;

use hakana_str::StrId;
use serde::{Deserialize, Serialize};

use crate::{
    attribute_info::AttributeInfo,
    code_location::HPos,
    codebase_info::CodebaseInfo,
    function_context::{FunctionContext, FunctionLikeIdentifier},
    functionlike_parameter::FunctionLikeParameter,
    issue::IssueKind,
    member_visibility::MemberVisibility,
    method_info::MethodInfo,
    t_union::TUnion,
    taint::{SinkType, SourceType},
    type_resolution::TypeResolutionContext,
    GenericParent, EFFECT_PURE,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FnEffect {
    Unknown,
    Pure,
    Arg(u8),
    Some(u8),
}

impl FnEffect {
    pub fn to_u8(&self) -> Option<u8> {
        match self {
            FnEffect::Unknown => None,
            FnEffect::Pure => Some(EFFECT_PURE),
            FnEffect::Arg(_) => None,
            FnEffect::Some(effects) => Some(*effects),
        }
    }

    pub fn from_u8(effects: &Option<u8>) -> Self {
        if let Some(effects) = effects {
            if effects == &0 {
                FnEffect::Pure
            } else {
                FnEffect::Some(*effects)
            }
        } else {
            FnEffect::Unknown
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MetaStart {
    pub start_offset: u32,
    pub start_line: u32,
    pub start_column: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionLikeInfo {
    /// HPos for the whole definition
    pub def_location: HPos,

    /**
    The start of the function's docblock(s)
    This allows us to remove that data when removing the function itself.
    */
    pub meta_start: MetaStart,

    /// The location of the function name. This is None for Closures
    pub name_location: Option<HPos>,

    pub params: Vec<FunctionLikeParameter>,

    pub return_type: Option<TUnion>,

    /// The location of the return type (which can be empty)
    pub return_type_location: Option<HPos>,

    /// Whether or not the populator has calculated inheritance for this function.
    pub is_populated: bool,

    /// All the issues we're suppressing at the function level
    pub suppressed_issues: Vec<(IssueKind, HPos)>,

    /// Whether this function is deprecated
    pub deprecated: bool,

    /**
    An array holding the class template "as" types.

    It's the de-facto list of all templates on a given class.

    The name of the template is the first key. The nested array is keyed by a unique
    function identifier. This allows operations with the same-named template defined
    across multiple classes and/or functions to not run into trouble.
    */
    pub template_types: Vec<(StrId, Vec<(GenericParent, Arc<TUnion>)>)>,

    pub has_visitor_issues: bool,

    pub has_yield: bool,

    pub is_async: bool,

    pub has_asio_join: bool,

    pub has_db_asio_join: bool,

    pub calls_db_asio_join: bool,

    pub is_request_handler: bool,

    pub must_use: bool,

    pub mutation_free: bool,

    pub effects: FnEffect,

    pub has_throw: bool,

    /**
    Whether or not the function output is dependent solely on input - a function can be
    impure but still have this property (e.g. var_export). Useful for taint analysis.
    */
    pub specialize_call: bool,

    /**
    If this is given we don't allow anything to be tainted via this function/method.
    Useful for things that are only executed in tests
    */
    pub ignore_taint_path: bool,

    /**
    If this function returns true, block taints while this holds
    */
    pub ignore_taints_if_true: bool,

    pub taint_source_types: Vec<SourceType>,

    pub added_taints: Vec<SinkType>,

    pub removed_taints: Vec<SinkType>,

    pub attributes: Vec<AttributeInfo>,

    pub method_info: Option<Box<MethodInfo>>,

    /// Identifiers matching <<Hakana\Calls(<id>, <id2>)>> attributes
    pub service_calls: Vec<String>,

    pub transitive_service_calls: Vec<String>,

    /// used for dead-code analysis
    pub user_defined: bool,

    /// used for dead-code analysis — this is true for all __EntryPoint and __DynamicallyCallable functions
    pub dynamically_callable: bool,

    /// generated functions also get a pass
    pub generated: bool,

    pub type_resolution_context: Option<TypeResolutionContext>,

    pub where_constraints: Vec<(StrId, TUnion)>,

    /*
    Stores a reference to the Asynchronous version of this function.
    If a function body is just a one-line
    return HH\Asio\join(some_other_function(...));
    then the id for the other function is stored here
    */
    pub async_version: Option<FunctionLikeIdentifier>,

    pub is_production_code: bool,

    pub ignore_noreturn_calls: bool,

    pub banned_function_message: Option<StrId>,

    pub is_closure: bool,

    pub overriding: bool,
}

impl FunctionLikeInfo {
    pub fn new(def_location: HPos, meta_start: MetaStart) -> Self {
        Self {
            def_location,
            meta_start,
            name_location: None,
            params: Vec::new(),
            return_type: None,
            return_type_location: None,
            is_populated: false,
            user_defined: false,
            suppressed_issues: vec![],
            deprecated: false,
            template_types: vec![],
            has_visitor_issues: false,
            has_yield: false,
            mutation_free: false,
            effects: FnEffect::Unknown,
            specialize_call: false,
            taint_source_types: vec![],
            added_taints: vec![],
            removed_taints: vec![],
            attributes: Vec::new(),
            method_info: None,
            is_async: false,
            has_asio_join: false,
            has_db_asio_join: false,
            calls_db_asio_join: false,
            is_request_handler: false,
            must_use: false,
            ignore_taint_path: false,
            dynamically_callable: false,
            generated: false,
            ignore_taints_if_true: false,
            type_resolution_context: None,
            where_constraints: vec![],
            async_version: None,
            is_production_code: true,
            has_throw: false,
            is_closure: false,
            overriding: false,
            banned_function_message: None,
            ignore_noreturn_calls: false,
            transitive_service_calls: vec![],
            service_calls: vec![],
        }
    }

    pub fn has_multi_line_params(&self) -> bool {
        let first_line = if let Some(name_location) = &self.name_location {
            name_location.start_line
        } else {
            self.def_location.start_line
        };

        if let Some(last_param) = self.params.last() {
            return last_param.location.start_line != first_line;
        }

        true
    }

    pub fn is_simple_fn(
        &self,
        function_context: &FunctionContext,
        codebase: &CodebaseInfo,
    ) -> bool {
        if let Some(method_storage) = &self.method_info {
            match &method_storage.visibility {
                MemberVisibility::Public | MemberVisibility::Protected => {
                    method_storage.is_final_and_unextended(function_context, codebase)
                }
                MemberVisibility::Private => true,
            }
        } else {
            true
        }
    }
}
