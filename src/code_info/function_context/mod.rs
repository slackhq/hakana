use hakana_str::StrId;

pub use crate::functionlike_identifier::FunctionLikeIdentifier;
use crate::{
    codebase_info::CodebaseInfo, functionlike_info::FunctionLikeInfo,
    symbol_references::ReferenceSource,
};

#[derive(Clone, Debug, Copy)]
pub struct FunctionContext {
    pub calling_class: Option<StrId>,

    pub calling_class_final: bool,

    pub is_static: bool,

    pub calling_functionlike_id: Option<FunctionLikeIdentifier>,

    pub ignore_noreturn_calls: bool,
}

impl Default for FunctionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionContext {
    pub fn new() -> Self {
        Self {
            calling_class: None,
            is_static: false,
            calling_functionlike_id: None,
            calling_class_final: false,
            ignore_noreturn_calls: false,
        }
    }

    pub fn get_reference_source(&self, file_path: &StrId) -> ReferenceSource {
        if let Some(calling_functionlike_id) = &self.calling_functionlike_id {
            match calling_functionlike_id {
                FunctionLikeIdentifier::Function(name) => ReferenceSource::Symbol(false, *name),
                FunctionLikeIdentifier::Method(a, b) => {
                    ReferenceSource::ClasslikeMember(false, *a, *b)
                }
                _ => {
                    panic!()
                }
            }
        } else {
            ReferenceSource::Symbol(false, *file_path)
        }
    }

    pub fn get_functionlike_info<'a>(
        &self,
        codebase: &'a CodebaseInfo,
    ) -> Option<&'a FunctionLikeInfo> {
        match self.calling_functionlike_id {
            Some(FunctionLikeIdentifier::Function(function_id)) => codebase
                .functionlike_infos
                .get(&(function_id, StrId::EMPTY)),
            Some(FunctionLikeIdentifier::Method(classlike_name, method_name)) => codebase
                .functionlike_infos
                .get(&(classlike_name, method_name)),
            _ => None,
        }
    }

    pub fn is_production(&self, codebase: &CodebaseInfo) -> bool {
        let functionlike_info = self.get_functionlike_info(codebase);

        match functionlike_info {
            Some(functionlike_info) => functionlike_info.is_production_code,
            _ => {
                if let Some(calling_class) = self.calling_class {
                    codebase
                        .classlike_infos
                        .get(&calling_class)
                        .unwrap()
                        .is_production_code
                } else {
                    false
                }
            }
        }
    }

    pub fn can_call_db_asio_join(&self, codebase: &CodebaseInfo) -> bool {
        let functionlike_info = self.get_functionlike_info(codebase);

        match functionlike_info {
            Some(functionlike_info) => {
                functionlike_info.calls_db_asio_join || functionlike_info.is_request_handler
            }
            _ => false,
        }
    }

    pub fn is_request_handler(&self, codebase: &CodebaseInfo) -> bool {
        let functionlike_info = self.get_functionlike_info(codebase);

        match functionlike_info {
            Some(functionlike_info) => functionlike_info.is_request_handler,
            _ => false,
        }
    }

    pub fn can_call_service(&self, codebase: &CodebaseInfo, service_name: &str) -> bool {
        let functionlike_info = self.get_functionlike_info(codebase);

        match functionlike_info {
            Some(functionlike_info) => {
                if !functionlike_info.is_production_code {
                    // Non-production code can call any service
                    return true;
                }
                functionlike_info.service_calls.iter().any(|s| s == service_name) 
                    || functionlike_info.is_request_handler
            }
            _ => false,
        }
    }

    pub fn can_transitively_call_service(&self, codebase: &CodebaseInfo, service_name: &str) -> bool {
        let functionlike_info = self.get_functionlike_info(codebase);

        match functionlike_info {
            Some(functionlike_info) => {
                if !functionlike_info.is_production_code {
                    // Non-production code can call any service
                    return true;
                }
                functionlike_info.transitive_service_calls.iter().any(|s| s == service_name)
                    || functionlike_info.service_calls.iter().any(|s| s == service_name)
                    || functionlike_info.is_request_handler
            }
            _ => false,
        }
    }
}
