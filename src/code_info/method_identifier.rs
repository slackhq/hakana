use crate::{codebase_info::symbols::Symbol, Interner};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodIdentifier(pub Symbol, pub Symbol);

impl MethodIdentifier {
    pub fn to_string(&self, interner: &Interner) -> String {
        format!("{}::{}", interner.lookup(self.0), interner.lookup(self.1))
    }
}
