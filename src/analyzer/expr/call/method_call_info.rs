use hakana_code_info::classlike_info::ClassLikeInfo;
use hakana_code_info::method_identifier::MethodIdentifier;
use hakana_str::StrId;

pub(crate) struct MethodCallInfo<'a> {
    pub self_fq_classlike_name: StrId,
    pub declaring_method_id: Option<MethodIdentifier>,
    pub classlike_storage: &'a ClassLikeInfo,
}
