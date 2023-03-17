use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::num::Wrapping;

use hakana_reflection_info::assertion::Assertion;
use hakana_reflection_info::Interner;
use indexmap::IndexMap;

#[derive(Clone, Debug, Eq)]
pub struct Clause {
    pub creating_conditional_id: (usize, usize),
    pub creating_object_id: (usize, usize),

    pub hash: u64,

    // An array of strings of the form
    // [
    //     '$a' => ['falsy'],
    //     '$b' => ['!falsy'],
    //     '$c' => ['!null'],
    //     '$d' => ['string', 'int']
    // ]
    //
    // represents the formula
    // !$a || $b || $c !== null || is_string($d) || is_int($d)
    pub possibilities: BTreeMap<String, IndexMap<u64, Assertion>>,

    pub wedge: bool,
    pub reconcilable: bool,
    pub generated: bool,
}

impl PartialEq for Clause {
    fn eq(&self, other: &Self) -> bool {
        return self.hash == other.hash;
    }
}

impl Hash for Clause {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state)
    }
}

impl Clause {
    pub fn new(
        possibilities: BTreeMap<String, IndexMap<u64, Assertion>>,
        creating_conditional_id: (usize, usize),
        creating_object_id: (usize, usize),
        wedge: Option<bool>,
        reconcilable: Option<bool>,
        generated: Option<bool>,
    ) -> Clause {
        let reconcilable = match reconcilable {
            None => true,
            Some(x) => x,
        };

        let generated = match generated {
            None => false,
            Some(x) => x,
        };

        let wedge = match wedge {
            None => false,
            Some(x) => x,
        };

        let hash = get_hash(&possibilities, creating_object_id, wedge, reconcilable);

        return Clause {
            possibilities,
            creating_conditional_id,
            creating_object_id,
            wedge,
            reconcilable,
            generated,
            hash,
        };
    }

    pub fn remove_possibilities(&self, var_id: &String) -> Option<Clause> {
        let mut possibilities = self.possibilities.clone();

        possibilities.remove(var_id);

        if possibilities.len() == 0 {
            return None;
        }

        return Some(Clause {
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
        });
    }

    pub fn add_possibility(
        &self,
        var_id: String,
        new_possibility: IndexMap<u64, Assertion>,
    ) -> Clause {
        let mut possibilities = self.possibilities.clone();

        possibilities.insert(var_id, new_possibility);

        return Clause {
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
        };
    }

    pub fn contains(&self, other_clause: &Self) -> bool {
        if other_clause.possibilities.len() > self.possibilities.len() {
            return false;
        }

        for (var, possible_types) in &other_clause.possibilities {
            let local_possibilities = self.possibilities.get(var);

            if let Some(local_possibilities) = local_possibilities {
                for (k, _) in possible_types {
                    if !local_possibilities.contains_key(k) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        return true;
    }

    pub fn get_impossibilities(&self) -> BTreeMap<String, Vec<Assertion>> {
        let mut impossibilities = BTreeMap::new();

        for (var_id, possiblity) in &self.possibilities {
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

            if impossibility.len() > 0 {
                impossibilities.insert(var_id.clone(), impossibility);
            }
        }
        impossibilities
    }

    pub fn to_string(&self, interner: &Interner) -> String {
        let mut clause_strings = vec![];

        if self.possibilities.len() == 0 {
            return "<empty>".to_string();
        }

        for (var_id, values) in self.possibilities.iter() {
            let mut var_id = var_id.clone();

            if var_id[0..1] == "*".to_string() {
                var_id = "<expr>".to_string()
            }

            let mut clause_string_parts = vec![];

            for (_, value) in values {
                match value {
                    Assertion::Any => {
                        clause_string_parts.push(var_id.to_string() + " is any");
                    }
                    Assertion::Falsy => {
                        clause_string_parts.push("!".to_string() + &var_id);
                        continue;
                    }
                    Assertion::Truthy => {
                        clause_string_parts.push(var_id.clone());
                        continue;
                    }
                    Assertion::IsType(value) | Assertion::IsEqual(value) => {
                        clause_string_parts.push(
                            var_id.to_string() + " is " + value.get_id(Some(interner)).as_str(),
                        );
                    }
                    Assertion::IsNotType(value) | Assertion::IsNotEqual(value) => {
                        clause_string_parts.push(
                            var_id.to_string() + " is not " + value.get_id(Some(interner)).as_str(),
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

        if clause_strings.len() > 1 {
            return "(".to_string() + &clause_strings.join(") || (") + ")";
        }

        return clause_strings[0].clone();
    }
}

#[inline]
fn get_hash(
    possibilities: &BTreeMap<String, IndexMap<u64, Assertion>>,
    creating_object_id: (usize, usize),
    wedge: bool,
    reconcilable: bool,
) -> u64 {
    if wedge || !reconcilable {
        (Wrapping(creating_object_id.0)
            + Wrapping(creating_object_id.1)
            + Wrapping(if wedge { 100000 } else { 0 }))
        .0
        .try_into()
        .unwrap()
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

        hasher.finish()
    }
}
