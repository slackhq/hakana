use hakana_str::StrId;

pub use crate::functionlike_identifier::FunctionLikeIdentifier;
use crate::{codebase_info::CodebaseInfo, symbol_references::ReferenceSource};

#[derive(Clone, Debug, Copy)]
pub struct FunctionContext {
    pub calling_class: Option<StrId>,

    pub calling_class_final: bool,

    pub is_static: bool,

    pub calling_functionlike_id: Option<FunctionLikeIdentifier>,
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

    pub fn is_production(&self, codebase: &CodebaseInfo) -> bool {
        match self.calling_functionlike_id {
            Some(FunctionLikeIdentifier::Function(function_id)) => {
                codebase
                    .functionlike_infos
                    .get(&(function_id, StrId::EMPTY))
                    .unwrap()
                    .is_production_code
            }
            Some(FunctionLikeIdentifier::Method(classlike_name, method_name)) => {
                codebase
                    .functionlike_infos
                    .get(&(classlike_name, method_name))
                    .unwrap()
                    .is_production_code
            }
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
}
