use crate::scope::BlockContext;
use hakana_code_info::var_name::VarName;
use rustc_hash::FxHashSet;

#[derive(Clone)]
pub(crate) struct IfConditionalScope {
    pub if_body_context: BlockContext,

    pub outer_context: BlockContext,

    pub post_if_context: BlockContext,

    pub cond_referenced_var_ids: FxHashSet<VarName>,
}
