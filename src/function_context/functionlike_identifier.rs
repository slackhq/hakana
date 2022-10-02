use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::method_identifier::MethodIdentifier;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionLikeIdentifier {
    Function(Arc<String>),
    Method(Arc<String>, String),
}

impl FunctionLikeIdentifier {
    pub fn as_method_identifier(&self) -> Option<MethodIdentifier> {
        if let FunctionLikeIdentifier::Method(fq_classlike_name, method_name) = &self {
            Some(MethodIdentifier(
                fq_classlike_name.clone(),
                method_name.to_string(),
            ))
        } else {
            None
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            FunctionLikeIdentifier::Function(fn_name) => fn_name.to_string(),
            FunctionLikeIdentifier::Method(fq_classlike_name, method_name) => {
                format!("{}::{}", fq_classlike_name, method_name)
            }
        }
    }
}
