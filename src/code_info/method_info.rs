use serde::{Deserialize, Serialize};

use crate::{
    codebase_info::CodebaseInfo,
    function_context::{FunctionContext, FunctionLikeIdentifier},
    member_visibility::MemberVisibility,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    pub is_static: bool,

    pub visibility: MemberVisibility,

    pub is_final: bool,

    pub is_abstract: bool,
}

impl Default for MethodInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl MethodInfo {
    pub fn new() -> Self {
        Self {
            is_static: false,
            visibility: MemberVisibility::Public,
            is_final: false,
            is_abstract: false,
        }
    }

    pub fn is_final_and_unextended(
        &self,
        function_context: &FunctionContext,
        codebase: &CodebaseInfo,
    ) -> bool {
        if self.is_final {
            if let Some(FunctionLikeIdentifier::Method(calling_class, calling_method_name)) =
                function_context.calling_functionlike_id
            {
                if let Some(classlike_info) = codebase.classlike_infos.get(&calling_class) {
                    if !classlike_info
                        .overridden_method_ids
                        .contains_key(&calling_method_name)
                    {
                        return true;
                    }
                }
            }
        }

        return false;
    }
}
