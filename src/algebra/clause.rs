use std::collections::BTreeMap;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::num::Wrapping;

use hakana_code_info::assertion::Assertion;
use hakana_code_info::var_name::VarName;
use hakana_str::Interner;
use indexmap::IndexMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClauseKey {
    Name(VarName),
    Range(u32, u32),
}

impl Display for ClauseKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClauseKey::Name(var_name) => write!(f, "{}", var_name.to_string()),
            ClauseKey::Range(start, end) => write!(f, "{}-{}", start, end),
        }
    }
}

#[derive(Clone, Debug, Eq)]
pub struct Clause {
    pub creating_conditional_id: (u32, u32),
    pub creating_object_id: (u32, u32),

    pub hash: u32,

    // An array of VarName strings of the form
    // [
    //     '$a' => ['falsy'],
    //     '$b' => ['!falsy'],
    //     '$c' => ['!null'],
    //     '$d' => ['string', 'int']
    // ]
    //
    // represents the formula
    // !$a || $b || $c !== null || is_string($d) || is_int($d)
    pub possibilities: BTreeMap<ClauseKey, IndexMap<u64, Assertion>>,

    pub wedge: bool,
    pub reconcilable: bool,
    pub generated: bool,
}

impl PartialEq for Clause {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Hash for Clause {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state)
    }
}

impl Clause {
    pub fn new(
        possibilities: BTreeMap<ClauseKey, IndexMap<u64, Assertion>>,
        creating_conditional_id: (u32, u32),
        creating_object_id: (u32, u32),
        wedge: Option<bool>,
        reconcilable: Option<bool>,
        generated: Option<bool>,
    ) -> Clause {
        Clause {
            creating_conditional_id,
            creating_object_id,
            wedge: wedge.unwrap_or(false),
            reconcilable: reconcilable.unwrap_or(true),
            generated: generated.unwrap_or(false),
            hash: get_hash(
                &possibilities,
                creating_object_id,
                wedge.unwrap_or(false),
                reconcilable.unwrap_or(true),
            ),
            possibilities,
        }
    }

    pub fn remove_possibilities(&self, var_id: &ClauseKey) -> Option<Clause> {
        let mut possibilities = self.possibilities.clone();

        possibilities.remove(var_id);

        if possibilities.is_empty() {
            return None;
        }

        Some(Clause {
            hash: get_hash(
                &possibilities,
                self.creating_object_id,
                self.wedge,
                self.reconcilable,
            ),
            possibilities,
            creating_conditional_id: self.creating_conditional_id,
            creating_object_id: self.creating_object_id,
            wedge: self.wedge,
            reconcilable: self.reconcilable,
            generated: self.generated,
        })
    }

    pub fn add_possibility(
        &self,
        var_id: ClauseKey,
        new_possibility: IndexMap<u64, Assertion>,
    ) -> Clause {
        let mut possibilities = self.possibilities.clone();

        possibilities.insert(var_id, new_possibility);

        Clause {
            hash: get_hash(
                &possibilities,
                self.creating_object_id,
                self.wedge,
                self.reconcilable,
            ),
            possibilities,
            creating_conditional_id: self.creating_conditional_id,
            creating_object_id: self.creating_object_id,
            wedge: self.wedge,
            reconcilable: self.reconcilable,
            generated: self.generated,
        }
    }

    pub fn contains(&self, other_clause: &Self) -> bool {
        if other_clause.possibilities.len() > self.possibilities.len() {
            return false;
        }

        other_clause
            .possibilities
            .iter()
            .all(|(var, possible_types)| {
                self.possibilities
                    .get(var)
                    .map(|local_possibilities| {
                        possible_types
                            .keys()
                            .all(|k| local_possibilities.contains_key(k))
                    })
                    .unwrap_or(false)
            })
    }

    pub fn get_impossibilities(&self) -> BTreeMap<ClauseKey, Vec<Assertion>> {
        let mut impossibilities = BTreeMap::new();

        for (var_key, possiblity) in &self.possibilities {
            let mut impossibility = vec![];

            for (_, assertion) in possiblity {
                match assertion {
                    Assertion::IsEqual(atomic) | Assertion::IsNotEqual(atomic) => {
                        if atomic.is_literal() {
                            impossibility.push(assertion.get_negation());
                        }
                    }
                    _ => {
                        impossibility.push(assertion.get_negation());
                    }
                }
            }

            if !impossibility.is_empty() {
                impossibilities.insert(var_key.clone(), impossibility);
            }
        }
        impossibilities
    }

    pub fn to_string(&self, interner: &Interner) -> String {
        let mut clause_strings = vec![];

        if self.possibilities.is_empty() {
            return "<empty>".to_string();
        }

        for (var_id, values) in self.possibilities.iter() {
            let var_id_str = match var_id {
                ClauseKey::Name(var_id) => var_id.to_string(),
                ClauseKey::Range(_, _) => "<expr>".to_string(),
            };

            let mut clause_string_parts = vec![];

            for (_, value) in values {
                match value {
                    Assertion::Any => {
                        clause_string_parts.push(var_id_str.to_string() + " is any");
                    }
                    Assertion::Falsy => {
                        clause_string_parts.push("!".to_string() + &var_id_str);
                        continue;
                    }
                    Assertion::Truthy => {
                        clause_string_parts.push(var_id_str.clone());
                        continue;
                    }
                    Assertion::IsType(value) | Assertion::IsEqual(value) => {
                        clause_string_parts.push(
                            var_id_str.to_string() + " is " + value.get_id(Some(interner)).as_str(),
                        );
                    }
                    Assertion::IsNotType(value) | Assertion::IsNotEqual(value) => {
                        clause_string_parts.push(
                            var_id_str.to_string()
                                + " is not "
                                + value.get_id(Some(interner)).as_str(),
                        );
                    }
                    _ => {
                        clause_string_parts.push(value.to_string(Some(interner)));
                    }
                }
            }

            if clause_string_parts.len() > 1 {
                let bracketed = "(".to_string() + &clause_string_parts.join(") || (") + ")";
                clause_strings.push(bracketed)
            } else {
                clause_strings.push(clause_string_parts[0].clone());
            }
        }

        let joined_clause = clause_strings.join(") || (");

        if clause_strings.len() > 1 {
            format!("({})", joined_clause)
        } else {
            joined_clause
        }
    }
}

#[inline]
fn get_hash(
    possibilities: &BTreeMap<ClauseKey, IndexMap<u64, Assertion>>,
    creating_object_id: (u32, u32),
    wedge: bool,
    reconcilable: bool,
) -> u32 {
    if wedge || !reconcilable {
        (Wrapping(creating_object_id.0)
            + Wrapping(creating_object_id.1)
            + Wrapping(if wedge { 100000 } else { 0 }))
        .0
    } else {
        let mut hasher = rustc_hash::FxHasher::default();

        for possibility in possibilities {
            possibility.0.hash(&mut hasher);
            0.hash(&mut hasher);

            for i in possibility.1.keys() {
                i.hash(&mut hasher);
                1.hash(&mut hasher);
            }
        }

        hasher.finish() as u32
    }
}
