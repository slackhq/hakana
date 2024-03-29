use hakana_str::{Interner, StrId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Copy, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub struct MethodIdentifier(pub StrId, pub StrId);

impl MethodIdentifier {
    pub fn to_string(&self, interner: &Interner) -> String {
        format!("{}::{}", interner.lookup(&self.0), interner.lookup(&self.1))
    }
}
