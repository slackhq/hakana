use crate::scope_context::ScopeContext;
use rustc_hash::FxHashSet;

#[derive(Clone)]
pub(crate) struct IfConditionalScope {
    pub if_body_context: ScopeContext,

    pub outer_context: ScopeContext,

    pub post_if_context: ScopeContext,

    pub cond_referenced_var_ids: FxHashSet<String>,
}
