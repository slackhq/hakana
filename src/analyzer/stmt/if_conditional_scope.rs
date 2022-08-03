use crate::scope_context::ScopeContext;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(crate) struct IfConditionalScope {
    pub if_body_context: ScopeContext,

    pub outer_context: ScopeContext,

    pub post_if_context: ScopeContext,

    pub cond_referenced_var_ids: HashSet<String>,

    pub assigned_in_conditional_var_ids: HashMap<String, usize>,
}
