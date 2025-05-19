use std::borrow::{Borrow, Cow};
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::ops::Deref;

#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct VarName(compact_str::CompactString);

impl VarName {
    #[inline]
    pub fn empty() -> Self {
        Self(compact_str::CompactString::default())
    }

    #[inline]
    pub fn new(name: impl AsRef<str>) -> Self {
        Self(compact_str::CompactString::new(name))
    }

    #[inline]
    pub const fn new_static(name: &'static str) -> Self {
        Self(compact_str::CompactString::const_new(name))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Debug for VarName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Name({:?})", self.as_str())
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

impl<'a> From<Cow<'a, str>> for VarName {
    #[inline]
    fn from(cow: Cow<'a, str>) -> Self {
        VarName(cow.into())
    }
}

impl From<Box<str>> for VarName {
    #[inline]
    fn from(b: Box<str>) -> Self {
        VarName(b.into())
    }
}

impl From<compact_str::CompactString> for VarName {
    #[inline]
    fn from(value: compact_str::CompactString) -> Self {
        Self(value)
    }
}

impl From<VarName> for compact_str::CompactString {
    #[inline]
    fn from(name: VarName) -> Self {
        name.0
    }
}

impl FromIterator<char> for VarName {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl std::fmt::Display for VarName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for VarName {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<VarName> for str {
    #[inline]
    fn eq(&self, other: &VarName) -> bool {
        other == self
    }
}

impl PartialEq<&str> for VarName {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<VarName> for &str {
    #[inline]
    fn eq(&self, other: &VarName) -> bool {
        other == self
    }
}

impl PartialEq<String> for VarName {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<VarName> for String {
    #[inline]
    fn eq(&self, other: &VarName) -> bool {
        other == self
    }
}

impl PartialEq<&String> for VarName {
    #[inline]
    fn eq(&self, other: &&String) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<VarName> for &String {
    #[inline]
    fn eq(&self, other: &VarName) -> bool {
        other == self
    }
}
