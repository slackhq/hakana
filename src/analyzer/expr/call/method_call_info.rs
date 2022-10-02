use std::sync::Arc;

use function_context::method_identifier::MethodIdentifier;
use hakana_reflection_info::classlike_info::ClassLikeInfo;

pub(crate) struct MethodCallInfo<'a> {
    pub self_fq_classlike_name: Arc<String>,
    pub declaring_method_id: Option<MethodIdentifier>,
    pub classlike_storage: &'a ClassLikeInfo,
}
