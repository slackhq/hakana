use std::{collections::BTreeSet, ops::Deref};

use hakana_code_info::code_location::HPos;
use hakana_code_info::ttype::type_variable::SubtypeConstraint;
use rustc_hash::{FxHashMap, FxHashSet};

/// Append-only deferred comparisons, with retries indexed by the bounds read
/// during their last resolution. Indices stay stable across closure inheritance.
#[derive(Clone, Default)]
pub struct TypeVariableConstraints {
    constraints: Vec<SubtypeConstraint>,
    by_position: FxHashMap<HPos, Vec<usize>>,
    dependencies: Vec<FxHashSet<String>>,
    subscribers: FxHashMap<String, FxHashSet<usize>>,
    pending: BTreeSet<usize>,
}

impl TypeVariableConstraints {
    /// Only comparisons from the same source position can be duplicates.
    /// Do not hash their types: union equality is insensitive to type ordering.
    pub fn push(&mut self, constraint: SubtypeConstraint) -> bool {
        let candidates = self.by_position.entry(constraint.pos).or_default();
        if candidates
            .iter()
            .any(|&index| self.constraints[index] == constraint)
        {
            return false;
        }
        let index = self.constraints.len();
        candidates.push(index);
        self.constraints.push(constraint);
        self.dependencies.push(FxHashSet::default());
        self.pending.insert(index);
        true
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn take_pending(&mut self) -> BTreeSet<usize> {
        // Preserve insertion order, as in the original full-list traversal.
        std::mem::take(&mut self.pending)
    }

    pub(crate) fn notify_changed(&mut self, names: &FxHashSet<String>) {
        for name in names {
            if let Some(subscribers) = self.subscribers.get(name) {
                self.pending.extend(subscribers);
            }
        }
    }

    pub(crate) fn set_dependencies(&mut self, index: usize, dependencies: FxHashSet<String>) {
        let previous = &self.dependencies[index];
        for name in previous.difference(&dependencies) {
            let subscribers = self.subscribers.get_mut(name).unwrap();
            subscribers.remove(&index);
            if subscribers.is_empty() {
                self.subscribers.remove(name);
            }
        }
        for name in dependencies.difference(previous) {
            self.subscribers
                .entry(name.clone())
                .or_default()
                .insert(index);
        }
        self.dependencies[index] = dependencies;
    }
}

impl Deref for TypeVariableConstraints {
    type Target = [SubtypeConstraint];

    fn deref(&self) -> &Self::Target {
        &self.constraints
    }
}

impl IntoIterator for TypeVariableConstraints {
    type Item = SubtypeConstraint;
    type IntoIter = std::vec::IntoIter<SubtypeConstraint>;

    fn into_iter(self) -> Self::IntoIter {
        self.constraints.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakana_code_info::{
        code_location::{FilePath, HPos},
        t_atomic::TAtomic,
        ttype::{get_int, wrap_atomic},
    };
    use hakana_str::StrId;

    fn constraint(offset: u32) -> SubtypeConstraint {
        SubtypeConstraint {
            lower: wrap_atomic(TAtomic::TTypeVariable {
                name: format!("T{offset}"),
            }),
            upper: get_int(),
            pos: HPos {
                file_path: FilePath(StrId::EMPTY),
                start_offset: offset,
                end_offset: offset + 1,
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
        }
    }

    fn names(names: &[&str]) -> FxHashSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn independent_constraints_only_retry_when_their_dependencies_change() {
        let mut constraints = TypeVariableConstraints::default();
        for index in 0..1_000 {
            assert!(constraints.push(constraint(index)));
            constraints
                .set_dependencies(index as usize, FxHashSet::from_iter([format!("T{index}")]));
        }
        assert_eq!(constraints.take_pending().len(), 1_000);
        constraints.notify_changed(&names(&["unrelated"]));
        assert!(!constraints.has_pending());
        constraints.notify_changed(&names(&["T999", "T17"]));
        constraints.notify_changed(&names(&["T17"]));
        assert_eq!(
            constraints.take_pending().into_iter().collect::<Vec<_>>(),
            vec![17, 999]
        );
        assert!(!constraints.push(constraint(17)));
        assert!(!constraints.has_pending());
    }

    #[test]
    fn refreshed_dependencies_replace_subscriptions_and_survive_cloning() {
        let mut parent = TypeVariableConstraints::default();
        parent.push(constraint(0));
        parent.set_dependencies(0, names(&["root", "old"]));
        parent.take_pending();

        let mut child = parent.clone();
        child.set_dependencies(0, names(&["root", "new"]));
        child.notify_changed(&names(&["old"]));
        assert!(!child.has_pending());
        child.notify_changed(&names(&["new"]));
        assert_eq!(child.take_pending(), BTreeSet::from([0]));
        parent.notify_changed(&names(&["new"]));
        assert!(!parent.has_pending());
        parent.notify_changed(&names(&["old"]));
        assert_eq!(parent.take_pending(), BTreeSet::from([0]));

        child.push(constraint(1));
        let inherited = child.clone();
        assert_eq!(inherited.pending, BTreeSet::from([1]));
        for constraint in child {
            parent.push(constraint);
        }
        assert_eq!(parent.len(), 2);
        assert_eq!(parent.take_pending(), BTreeSet::from([1]));
    }

    #[test]
    fn deduplication_preserves_distinct_metadata_and_positions() {
        let mut constraints = TypeVariableConstraints::default();
        let original = constraint(0);
        let mut different_metadata = original.clone();
        different_metadata.lower.uri_prefix = Some(Default::default());
        let mut different_position = original.clone();
        different_position.pos.start_offset = 1;
        assert!(constraints.push(original.clone()));
        assert!(constraints.push(different_metadata.clone()));
        assert!(constraints.push(different_position.clone()));
        assert!(!constraints.push(original));
        assert!(!constraints.push(different_metadata));
        assert!(!constraints.push(different_position));
        assert_eq!(constraints.len(), 3);
    }

    #[test]
    fn deduplication_ignores_union_type_order() {
        let mut constraints = TypeVariableConstraints::default();
        let mut original = constraint(0);
        original.lower.types = vec![TAtomic::TInt, TAtomic::TString];
        let mut reordered = original.clone();
        reordered.lower.types.reverse();
        assert!(constraints.push(original));
        assert!(!constraints.push(reordered));
        assert_eq!(constraints.len(), 1);
    }
}
