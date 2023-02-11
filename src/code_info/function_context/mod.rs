pub use crate::functionlike_identifier::FunctionLikeIdentifier;
use crate::{symbol_references::ReferenceSource, StrId};

#[derive(Clone, Debug, Copy)]
pub struct FunctionContext {
    pub calling_class: Option<StrId>,

    pub is_static: bool,

    pub calling_functionlike_id: Option<FunctionLikeIdentifier>,
}

impl FunctionContext {
    pub fn new() -> Self {
        Self {
            calling_class: None,
            is_static: false,
            calling_functionlike_id: None,
        }
    }

    pub fn get_reference_source(&self, file_path: &StrId) -> ReferenceSource {
        if let Some(calling_functionlike_id) = &self.calling_functionlike_id {
            match calling_functionlike_id {
                FunctionLikeIdentifier::Function(name) => ReferenceSource::Symbol(false, *name),
                FunctionLikeIdentifier::Method(a, b) => {
                    ReferenceSource::ClasslikeMember(false, *a, *b)
                }
            }
        } else {
            ReferenceSource::Symbol(false, *file_path)
        }
    }
}
