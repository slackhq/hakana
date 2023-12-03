use std::hash::Hasher;

use derivative::Derivative;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

use crate::{
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
    taint::SinkType,
    Interner,
};
use core::hash::Hash;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Derivative)]
#[derivative(Hash)]
pub enum Assertion {
    Any,
    IsType(TAtomic),
    IsNotType(TAtomic),
    Falsy,
    Truthy,
    IsEqual(TAtomic),
    IsNotEqual(TAtomic),
    IsEqualIsset,
    IsIsset,
    IsNotIsset,
    HasStringArrayAccess,
    HasIntOrStringArrayAccess,
    ArrayKeyExists,
    ArrayKeyDoesNotExist,
    InArray(TUnion),
    NotInArray(TUnion),
    HasArrayKey(DictKey),
    DoesNotHaveArrayKey(DictKey),
    HasNonnullEntryForKey(DictKey),
    DoesNotHaveNonnullEntryForKey(DictKey),
    NonEmptyCountable(bool),
    EmptyCountable,
    HasExactCount(usize),
    DoesNotHaveExactCount(usize),
    IgnoreTaints,
    DontIgnoreTaints,
    RemoveTaints(String, #[derivative(Hash = "ignore")] FxHashSet<SinkType>),
    DontRemoveTaints(String, #[derivative(Hash = "ignore")] FxHashSet<SinkType>),
}

impl Assertion {
    pub fn to_string(&self, interner: Option<&Interner>) -> String {
        match self {
            Assertion::Any => "any".to_string(),
            Assertion::Falsy => "falsy".to_string(),
            Assertion::Truthy => "truthy".to_string(),
            Assertion::IsType(atomic) => atomic.get_id(interner),
            Assertion::IsNotType(atomic) => "!".to_string() + &atomic.get_id(interner),
            Assertion::IsEqual(atomic) => "=".to_string() + &atomic.get_id(interner),
            Assertion::IsNotEqual(atomic) => "!=".to_string() + &atomic.get_id(interner),
            Assertion::IsEqualIsset => "=isset".to_string(),
            Assertion::IsIsset => "isset".to_string(),
            Assertion::IsNotIsset => "!isset".to_string(),
            Assertion::HasStringArrayAccess => "=string-array-access".to_string(),
            Assertion::HasIntOrStringArrayAccess => "=int-or-string-array-access".to_string(),
            Assertion::ArrayKeyExists => "array-key-exists".to_string(),
            Assertion::ArrayKeyDoesNotExist => "!array-key-exists".to_string(),
            Assertion::HasArrayKey(key) => {
                "=has-array-key-".to_string() + key.to_string(interner).as_str()
            }
            Assertion::DoesNotHaveArrayKey(key) => {
                "!=has-array-key-".to_string() + key.to_string(interner).as_str()
            }
            Assertion::HasNonnullEntryForKey(key) => {
                "=has-nonnull-entry-for-".to_string() + key.to_string(interner).as_str()
            }
            Assertion::DoesNotHaveNonnullEntryForKey(key) => {
                "!=has-nonnull-entry-for-".to_string() + key.to_string(interner).as_str()
            }
            Assertion::InArray(union) => "=in-array-".to_string() + &union.get_id(interner),
            Assertion::NotInArray(union) => "!=in-array-".to_string() + &union.get_id(interner),
            Assertion::NonEmptyCountable(negatable) => {
                if *negatable {
                    "non-empty-countable".to_string()
                } else {
                    "=non-empty-countable".to_string()
                }
            }
            Assertion::EmptyCountable => "empty-countable".to_string(),
            Assertion::HasExactCount(number) => "has-exactly-".to_string() + &number.to_string(),
            Assertion::DoesNotHaveExactCount(number) => {
                "!has-exactly-".to_string() + &number.to_string()
            }
            Assertion::IgnoreTaints => "ignore-taints".to_string(),
            Assertion::DontIgnoreTaints => "dont-ignore-taints".to_string(),
            Assertion::RemoveTaints(key, _) => "remove-some-taints-".to_string() + key,
            Assertion::DontRemoveTaints(key, _) => "!remove-some-taints-".to_string() + key,
        }
    }

    pub fn to_hash(&self) -> u64 {
        let mut state = rustc_hash::FxHasher::default();
        self.to_string(None).hash(&mut state);
        state.finish()
    }

    pub fn has_negation(&self) -> bool {
        matches!(
            self,
            Assertion::Falsy
                | Assertion::IsNotType(_)
                | Assertion::IsNotEqual(_)
                | Assertion::IsNotIsset
                | Assertion::NotInArray(..)
                | Assertion::ArrayKeyDoesNotExist
                | Assertion::DoesNotHaveArrayKey(_)
                | Assertion::DoesNotHaveExactCount(_)
                | Assertion::DoesNotHaveNonnullEntryForKey(_)
                | Assertion::EmptyCountable
        )
    }

    pub fn has_isset(&self) -> bool {
        matches!(
            self,
            Assertion::IsIsset
                | Assertion::ArrayKeyExists
                | Assertion::HasStringArrayAccess
                | Assertion::IsEqualIsset
        )
    }

    pub fn has_non_isset_equality(&self) -> bool {
        matches!(
            self,
            Assertion::InArray(_)
                | Assertion::HasIntOrStringArrayAccess
                | Assertion::HasStringArrayAccess
                | Assertion::IsEqual(_)
        )
    }

    pub fn has_equality(&self) -> bool {
        matches!(
            self,
            Assertion::InArray(_)
                | Assertion::HasIntOrStringArrayAccess
                | Assertion::HasStringArrayAccess
                | Assertion::IsEqualIsset
                | Assertion::IsEqual(_)
                | Assertion::IsNotEqual(_)
                | Assertion::RemoveTaints(_, _)
                | Assertion::DontRemoveTaints(_, _)
        )
    }

    pub fn has_literal_string_or_int(&self) -> bool {
        match self {
            Assertion::IsEqual(atomic)
            | Assertion::IsNotEqual(atomic)
            | Assertion::IsType(atomic)
            | Assertion::IsNotType(atomic) => matches!(
                atomic,
                TAtomic::TLiteralInt { .. }
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TEnumLiteralCase { .. }
            ),

            _ => false,
        }
    }

    pub fn get_type(&self) -> Option<&TAtomic> {
        match self {
            Assertion::IsEqual(atomic)
            | Assertion::IsNotEqual(atomic)
            | Assertion::IsType(atomic)
            | Assertion::IsNotType(atomic) => Some(atomic),

            _ => None,
        }
    }

    pub fn is_negation_of(&self, other: &Assertion) -> bool {
        match self {
            Assertion::Any => false,
            Assertion::Falsy => matches!(other, Assertion::Truthy),
            Assertion::Truthy => matches!(other, Assertion::Falsy),
            Assertion::IsType(atomic) => match other {
                Assertion::IsNotType(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsNotType(atomic) => match other {
                Assertion::IsType(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsEqual(atomic) => match other {
                Assertion::IsNotEqual(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsNotEqual(atomic) => match other {
                Assertion::IsEqual(other_atomic) => other_atomic == atomic,
                _ => false,
            },
            Assertion::IsEqualIsset => false,
            Assertion::IsIsset => matches!(other, Assertion::IsNotIsset),
            Assertion::IsNotIsset => matches!(other, Assertion::IsIsset),
            Assertion::HasStringArrayAccess => false,
            Assertion::HasIntOrStringArrayAccess => false,
            Assertion::ArrayKeyExists => matches!(other, Assertion::ArrayKeyDoesNotExist),
            Assertion::ArrayKeyDoesNotExist => matches!(other, Assertion::ArrayKeyExists),
            Assertion::HasArrayKey(str) => match other {
                Assertion::DoesNotHaveArrayKey(other_str) => other_str == str,
                _ => false,
            },
            Assertion::DoesNotHaveArrayKey(str) => match other {
                Assertion::HasArrayKey(other_str) => other_str == str,
                _ => false,
            },
            Assertion::HasNonnullEntryForKey(str) => match other {
                Assertion::DoesNotHaveNonnullEntryForKey(other_str) => other_str == str,
                _ => false,
            },
            Assertion::DoesNotHaveNonnullEntryForKey(str) => match other {
                Assertion::HasNonnullEntryForKey(other_str) => other_str == str,
                _ => false,
            },
            Assertion::InArray(union) => match other {
                Assertion::NotInArray(other_union) => other_union == union,
                _ => false,
            },
            Assertion::NotInArray(union) => match other {
                Assertion::InArray(other_union) => other_union == union,
                _ => false,
            },
            Assertion::NonEmptyCountable(negatable) => {
                if *negatable {
                    matches!(other, Assertion::EmptyCountable)
                } else {
                    false
                }
            }
            Assertion::EmptyCountable => matches!(other, Assertion::NonEmptyCountable(true)),
            Assertion::HasExactCount(number) => match other {
                Assertion::DoesNotHaveExactCount(other_number) => other_number == number,
                _ => false,
            },
            Assertion::DoesNotHaveExactCount(number) => match other {
                Assertion::HasExactCount(other_number) => other_number == number,
                _ => false,
            },
            Assertion::IgnoreTaints => matches!(other, Assertion::DontIgnoreTaints),
            Assertion::DontIgnoreTaints => matches!(other, Assertion::IgnoreTaints),
            Assertion::RemoveTaints(key, taints) => match other {
                Assertion::DontRemoveTaints(other_key, other_taints) => {
                    other_key == key && other_taints == taints
                }
                _ => false,
            },
            Assertion::DontRemoveTaints(key, taints) => match other {
                Assertion::RemoveTaints(other_key, other_taints) => {
                    other_key == key && other_taints == taints
                }
                _ => false,
            },
        }
    }

    pub fn get_negation(&self) -> Self {
        match self {
            Assertion::Any => Assertion::Any,
            Assertion::Falsy => Assertion::Truthy,
            Assertion::IsType(atomic) => Assertion::IsNotType(atomic.clone()),
            Assertion::IsNotType(atomic) => Assertion::IsType(atomic.clone()),
            Assertion::Truthy => Assertion::Falsy,
            Assertion::IsEqual(atomic) => Assertion::IsNotEqual(atomic.clone()),
            Assertion::IsNotEqual(atomic) => Assertion::IsEqual(atomic.clone()),
            Assertion::IsIsset => Assertion::IsNotIsset,
            Assertion::IsNotIsset => Assertion::IsIsset,
            Assertion::NonEmptyCountable(negatable) => {
                if *negatable {
                    Assertion::EmptyCountable
                } else {
                    Assertion::Any
                }
            }
            Assertion::EmptyCountable => Assertion::NonEmptyCountable(true),
            Assertion::ArrayKeyExists => Assertion::ArrayKeyDoesNotExist,
            Assertion::ArrayKeyDoesNotExist => Assertion::ArrayKeyExists,
            Assertion::InArray(union) => Assertion::NotInArray(union.clone()),
            Assertion::NotInArray(union) => Assertion::InArray(union.clone()),
            Assertion::HasExactCount(size) => Assertion::DoesNotHaveExactCount(*size),
            Assertion::DoesNotHaveExactCount(size) => Assertion::HasExactCount(*size),
            Assertion::HasArrayKey(str) => Assertion::DoesNotHaveArrayKey(str.clone()),
            Assertion::DoesNotHaveArrayKey(str) => Assertion::HasArrayKey(str.clone()),
            Assertion::HasNonnullEntryForKey(str) => {
                Assertion::DoesNotHaveNonnullEntryForKey(str.clone())
            }
            Assertion::DoesNotHaveNonnullEntryForKey(str) => {
                Assertion::HasNonnullEntryForKey(str.clone())
            }

            // these are just generated within the reconciler,
            // so their negations are meaningless
            Assertion::HasStringArrayAccess => Assertion::Any,
            Assertion::HasIntOrStringArrayAccess => Assertion::Any,
            Assertion::IsEqualIsset => Assertion::Any,
            Assertion::IgnoreTaints => Assertion::DontIgnoreTaints,
            Assertion::DontIgnoreTaints => Assertion::IgnoreTaints,
            Assertion::RemoveTaints(key, taints) => {
                Assertion::DontRemoveTaints(key.clone(), taints.clone())
            }
            Assertion::DontRemoveTaints(key, taints) => {
                Assertion::RemoveTaints(key.clone(), taints.clone())
            }
        }
    }
}
