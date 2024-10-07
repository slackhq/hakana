use std::sync::Arc;

use crate::data_flow::node::DataFlowNode;
use crate::{t_atomic::TAtomic, t_union::TUnion};
use crate::ttype::template::TemplateBound;

#[derive(Debug)]
pub struct TypeComparisonResult {
    pub type_coerced: Option<bool>,

    /* type is coerced from a nested param with explicit mixed e.g. dict<string, mixed> into dict<string, string> */
    pub type_coerced_from_nested_mixed: Option<bool>,

    /* type is coerced from a nested param with untyped any e.g. dict<string, any> into dict<string, string> */
    pub type_coerced_from_nested_any: Option<bool>,

    /* type is coerced from a generic `as mixed` param e.g. dict<string, T> into dict<string, string> */
    pub type_coerced_from_as_mixed: Option<bool>,

    pub upcasted_awaitable: bool,

    /**
     * This is used for array access. In the function below
     * we know that there are only two possible keys, 0 and 1,
     * but we allow the array to be addressed by an arbitrary
     * integer $i.
     *
     * function takesAnInt(int $i): string {
     *     $arr = vec["foo", "bar"];
     *     return $arr[$i];
     * }
     */
    pub type_coerced_to_literal: Option<bool>,

    pub replacement_union_type: Option<TUnion>,
    pub replacement_atomic_type: Option<TAtomic>,

    pub type_variable_lower_bounds: Vec<(String, TemplateBound)>,
    pub type_variable_upper_bounds: Vec<(String, TemplateBound)>,

    pub type_mismatch_parents: Option<(Vec<DataFlowNode>, Arc<TUnion>)>,
}

impl Default for TypeComparisonResult {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeComparisonResult {
    pub fn new() -> Self {
        Self {
            type_coerced: None,
            type_coerced_from_nested_mixed: None,
            type_coerced_from_nested_any: None,
            type_coerced_from_as_mixed: None,
            type_coerced_to_literal: None,
            replacement_union_type: None,
            replacement_atomic_type: None,
            type_variable_lower_bounds: vec![],
            type_variable_upper_bounds: vec![],
            upcasted_awaitable: false,
            type_mismatch_parents: None,
        }
    }
}
