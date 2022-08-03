use std::hash::Hasher;

use serde::{Deserialize, Serialize};

use crate::{t_atomic::TAtomic, t_union::TUnion};
use core::hash::Hash;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    HasArrayKey(String),
    DoesNotHaveArrayKey(String),
    NonEmptyCountable(bool),
    EmptyCountable,
    HasExactCount(usize),
    DoesNotHaveExactCount(usize),
    IgnoreTaints,
    DontIgnoreTaints,
}

impl Hash for Assertion {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_string().hash(state);
    }
}

impl Assertion {
    pub fn to_string(&self) -> String {
        match self {
            Assertion::Any => "any".to_string(),
            Assertion::Falsy => "falsy".to_string(),
            Assertion::Truthy => "!falsy".to_string(),
            Assertion::IsType(atomic) => (&atomic).get_id(),
            Assertion::IsNotType(atomic) => "!".to_string() + &atomic.get_id(),
            Assertion::IsEqual(atomic) => "=".to_string() + &atomic.get_id(),
            Assertion::IsNotEqual(atomic) => "!=".to_string() + &atomic.get_id(),
            Assertion::IsEqualIsset => "=isset".to_string(),
            Assertion::IsIsset => "isset".to_string(),
            Assertion::IsNotIsset => "!isset".to_string(),
            Assertion::HasStringArrayAccess => "=string-array-access".to_string(),
            Assertion::HasIntOrStringArrayAccess => "=int-or-string-array-access".to_string(),
            Assertion::ArrayKeyExists => "array-key-exists".to_string(),
            Assertion::ArrayKeyDoesNotExist => "!array-key-exists".to_string(),
            Assertion::HasArrayKey(str) => "=has-array-key-".to_string() + str,
            Assertion::DoesNotHaveArrayKey(str) => "!=has-array-key-".to_string() + str,
            Assertion::InArray(union) => "=in-array-".to_string() + &union.get_id(),
            Assertion::NotInArray(union) => "!=in-array-".to_string() + &union.get_id(),
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
        }
    }

    pub fn has_negation(&self) -> bool {
        match self {
            Assertion::Falsy
            | Assertion::IsNotType(_)
            | Assertion::IsNotEqual(_)
            | Assertion::IsNotIsset
            | Assertion::NotInArray(..)
            | Assertion::ArrayKeyDoesNotExist
            | Assertion::DoesNotHaveArrayKey(_)
            | Assertion::DoesNotHaveExactCount(_)
            | Assertion::EmptyCountable => true,

            _ => false,
        }
    }

    pub fn has_isset(&self) -> bool {
        match self {
            Assertion::IsIsset
            | Assertion::ArrayKeyExists
            | Assertion::HasStringArrayAccess
            | Assertion::IsEqualIsset => true,

            _ => false,
        }
    }

    pub fn has_non_isset_equality(&self) -> bool {
        match self {
            Assertion::InArray(_)
            | Assertion::HasIntOrStringArrayAccess
            | Assertion::HasStringArrayAccess
            | Assertion::IsEqual(_) => true,

            _ => false,
        }
    }

    pub fn has_equality(&self) -> bool {
        match self {
            Assertion::InArray(_)
            | Assertion::HasIntOrStringArrayAccess
            | Assertion::HasStringArrayAccess
            | Assertion::IsEqualIsset
            | Assertion::IsEqual(_)
            | Assertion::IsNotEqual(_) => true,

            _ => false,
        }
    }

    pub fn has_literal_string_or_int(&self) -> bool {
        match self {
            Assertion::IsEqual(atomic)
            | Assertion::IsNotEqual(atomic)
            | Assertion::IsType(atomic)
            | Assertion::IsNotType(atomic) => match atomic {
                TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralString { .. }
                | TAtomic::TEnumLiteralCase { .. } => true,
                _ => false,
            },

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

            // these are just generated within the reconciler,
            // so their negations are meaningless
            Assertion::HasStringArrayAccess => Assertion::Any,
            Assertion::HasIntOrStringArrayAccess => Assertion::Any,
            Assertion::IsEqualIsset => Assertion::Any,
            Assertion::IgnoreTaints => Assertion::DontIgnoreTaints,
            Assertion::DontIgnoreTaints => Assertion::IgnoreTaints,
        }
    }
}
