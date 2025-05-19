use std::borrow::Borrow;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::ops::Deref;

#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct VarName(String);

impl Debug for VarName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "VarName({:?})", self.0)
    }
}

impl VarName {
    pub fn new(name: String) -> Self {
        VarName(name)
    }

    pub fn to_string(&self) -> String {
        self.0.clone()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for VarName {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for VarName {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Borrow<str> for VarName {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<'a> From<&'a str> for VarName {
    #[inline]
    fn from(s: &'a str) -> Self {
        VarName(s.into())
    }
}

impl From<String> for VarName {
    #[inline]
    fn from(s: String) -> Self {
        VarName(s.into())
    }
}

impl<'a> From<&'a String> for VarName {
    #[inline]
    fn from(s: &'a String) -> Self {
        VarName(s.into())
    }
}
