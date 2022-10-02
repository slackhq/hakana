use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodIdentifier(pub Arc<String>, pub String);

impl MethodIdentifier {
    pub fn to_string(&self) -> String {
        format!("{}::{}", self.0, self.1)
    }
}
