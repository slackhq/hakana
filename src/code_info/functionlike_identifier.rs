use hakana_str::{Interner, StrId};
use serde::{Deserialize, Serialize};

use crate::{code_location::FilePath, method_identifier::MethodIdentifier};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy, PartialOrd, Ord)]
pub enum FunctionLikeIdentifier {
    Function(StrId),
    Method(StrId, StrId),
    Closure(FilePath, u32),
}

impl FunctionLikeIdentifier {
    pub fn as_method_identifier(&self) -> Option<MethodIdentifier> {
        if let FunctionLikeIdentifier::Method(fq_classlike_name, method_name) = &self {
            Some(MethodIdentifier(*fq_classlike_name, *method_name))
        } else {
            None
        }
    }

    pub fn to_string(&self, interner: &Interner) -> String {
        match self {
            FunctionLikeIdentifier::Function(fn_name) => interner.lookup(fn_name).to_string(),
            FunctionLikeIdentifier::Method(fq_classlike_name, method_name) => {
                format!(
                    "{}::{}",
                    interner.lookup(fq_classlike_name),
                    interner.lookup(method_name)
                )
            }
            FunctionLikeIdentifier::Closure(file_name, offset) => {
                format!("{}:{}", file_name.0 .0, offset)
            }
        }
    }

    pub fn to_hash(&self) -> String {
        match self {
            FunctionLikeIdentifier::Function(fn_name) => fn_name.0.to_string(),
            FunctionLikeIdentifier::Method(fq_classlike_name, method_name) => {
                format!("{}::{}", fq_classlike_name.0, method_name.0)
            }
            FunctionLikeIdentifier::Closure(file_name, offset) => {
                format!("{}::{}", file_name.0 .0, offset)
            }
        }
    }
}
