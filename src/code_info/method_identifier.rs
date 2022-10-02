use crate::codebase_info::symbols::Symbol;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodIdentifier(pub Symbol, pub String);

impl MethodIdentifier {
    pub fn to_string(&self) -> String {
        format!("{}::{}", self.0, self.1)
    }
}
