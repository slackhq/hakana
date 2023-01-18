use crate::{Interner, StrId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodIdentifier(pub StrId, pub StrId);

impl MethodIdentifier {
    pub fn to_string(&self, interner: &Interner) -> String {
        format!("{}::{}", interner.lookup(&self.0), interner.lookup(&self.1))
    }
}
