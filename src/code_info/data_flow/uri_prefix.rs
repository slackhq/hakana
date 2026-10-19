//! A prefix whose authority cannot be changed by appending more text.
//!
//! This is a property of the whole value, not a sanitizer. Existing taint in
//! the prefix still flows; only a later suffix can discharge URL-destination
//! obligations. Dependencies require every return of the named helpers to
//! establish the same property, independent of their arguments.

use crate::functionlike_identifier::FunctionLikeIdentifier;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UriPrefix {
    pub dependencies: Vec<FunctionLikeIdentifier>,
}

impl UriPrefix {
    pub fn returning(function: FunctionLikeIdentifier) -> Self {
        Self {
            dependencies: vec![function],
        }
    }

    pub fn join(mut self, other: Self) -> Self {
        for dependency in other.dependencies {
            if !self.dependencies.contains(&dependency) {
                self.dependencies.push(dependency);
            }
        }
        self.dependencies.sort();
        self
    }

    pub fn is_proven(&self, functions: &FxHashSet<FunctionLikeIdentifier>) -> bool {
        self.dependencies.iter().all(|id| functions.contains(id))
    }
}

pub fn has_fixed_authority(prefix: &str) -> bool {
    if prefix.bytes().any(|b| b <= b' ' || b == b'\\' || b == 127) {
        return false;
    }
    if prefix.starts_with('/') && !prefix.starts_with("//") {
        return prefix.len() > 1;
    }
    if prefix.starts_with('?') || prefix.starts_with('#') {
        return true;
    }
    prefix
        .strip_prefix("https://")
        .or_else(|| prefix.strip_prefix("http://"))
        .is_some_and(|rest| {
            rest.find(['/', '?', '#'])
                .is_some_and(|end| end > 0 && !rest[..end].ends_with('@'))
        })
}

pub fn resolve(
    returns: &FxHashMap<FunctionLikeIdentifier, Option<UriPrefix>>,
) -> FxHashSet<FunctionLikeIdentifier> {
    // Least fixed point: recursion alone cannot establish a proof.
    let mut proven = FxHashSet::default();
    loop {
        let before = proven.len();
        for (function, prefix) in returns {
            if prefix
                .as_ref()
                .is_some_and(|prefix| prefix.is_proven(&proven))
            {
                proven.insert(*function);
            }
        }
        if before == proven.len() {
            return proven;
        }
    }
}
