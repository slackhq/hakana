//! Container visitors for retained-size estimates. Unlike allocative's generic
//! HashMap visitor, these include spare capacity and estimated hash-table layout.
//! IndexMap 1.x needs a visitor of its own (allocative supports 2.x).
use std::{
    alloc::Layout,
    collections::{BTreeMap, HashMap, HashSet},
    mem::{align_of, size_of, size_of_val},
    sync::Arc,
};

use allocative::{Allocative, Key, Visitor};
use indexmap::{IndexMap, IndexSet};

/// Estimate hashbrown storage from public capacity. Control-group width and
/// private table layout can differ between Rust versions/platforms.
fn table_bytes<T>(capacity: usize) -> usize {
    if capacity == 0 {
        return 0;
    }
    let buckets = capacity.next_power_of_two();
    let alignment = align_of::<T>().max(16);
    let payload = (buckets * size_of::<T>()).div_ceil(alignment) * alignment;
    payload + buckets + 16
}

fn field_with<T>(visitor: &mut Visitor<'_>, value: &T, visit: impl Fn(&T, &mut Visitor<'_>)) {
    visitor.visit_field_with(Key::new("value"), size_of_val(value), |visitor| {
        visit(value, visitor);
    });
}

pub fn hash_map<K: Allocative, V: Allocative, S>(
    map: &HashMap<K, V, S>,
    visitor: &mut Visitor<'_>,
) {
    hash_map_with(map, visitor, |value, visitor| value.visit(visitor));
}

fn hash_map_with<K: Allocative, V, S>(
    map: &HashMap<K, V, S>,
    visitor: &mut Visitor<'_>,
    visit_value: impl Fn(&V, &mut Visitor<'_>),
) {
    let mut visitor = visitor.enter_self(map);
    if map.capacity() != 0 {
        let mut allocation = visitor.enter_unique(Key::new("table"), size_of::<usize>());
        let mut entries =
            allocation.enter(Key::new("capacity"), table_bytes::<(K, V)>(map.capacity()));
        for (key, value) in map {
            entries.visit_field(Key::new("key"), key);
            field_with(&mut entries, value, &visit_value);
        }
    }
}

pub fn hash_set<T: Allocative, S>(set: &HashSet<T, S>, visitor: &mut Visitor<'_>) {
    let mut visitor = visitor.enter_self(set);
    if set.capacity() != 0 {
        let mut allocation = visitor.enter_unique(Key::new("table"), size_of::<usize>());
        let mut entries = allocation.enter(Key::new("capacity"), table_bytes::<T>(set.capacity()));
        for value in set {
            entries.visit_field(Key::new("value"), value);
        }
    }
}

pub fn hash_map_of_sets<K: Allocative, V: Allocative, S1, S2>(
    map: &HashMap<K, HashSet<V, S2>, S1>,
    visitor: &mut Visitor<'_>,
) {
    hash_map_with(map, visitor, hash_set);
}

pub fn hash_map_of_maps<K1: Allocative, K2: Allocative, V: Allocative, S1, S2>(
    map: &HashMap<K1, HashMap<K2, V, S2>, S1>,
    visitor: &mut Visitor<'_>,
) {
    hash_map_with(map, visitor, hash_map);
}

pub fn hash_map_of_index_maps<K1: Allocative, K2: Allocative, V: Allocative, S1, S2>(
    map: &HashMap<K1, IndexMap<K2, V, S2>, S1>,
    visitor: &mut Visitor<'_>,
) {
    hash_map_with(map, visitor, index_map);
}

pub fn option_hash_map<K: Allocative, V: Allocative, S>(
    map: &Option<HashMap<K, V, S>>,
    visitor: &mut Visitor<'_>,
) {
    let mut visitor = visitor.enter_self(map);
    if let Some(map) = map {
        field_with(&mut visitor, map, hash_map);
    }
}

pub fn option_hash_set<T: Allocative, S>(set: &Option<HashSet<T, S>>, visitor: &mut Visitor<'_>) {
    let mut visitor = visitor.enter_self(set);
    if let Some(set) = set {
        field_with(&mut visitor, set, hash_set);
    }
}

pub fn index_map<K: Allocative, V: Allocative, S>(
    map: &IndexMap<K, V, S>,
    visitor: &mut Visitor<'_>,
) {
    let mut visitor = visitor.enter_self(map);
    if map.capacity() != 0 {
        let mut allocation = visitor.enter_unique(Key::new("entries"), size_of::<usize>());
        let mut entries = allocation.enter(
            Key::new("capacity"),
            map.capacity() * size_of::<(usize, K, V)>(),
        );
        for (key, value) in map {
            entries.visit_field(Key::new("key"), key);
            entries.visit_field(Key::new("value"), value);
        }
    }
    if map.capacity() != 0 {
        let mut table = visitor.enter_unique(Key::new("index"), size_of::<usize>());
        table.visit_simple(Key::new("capacity"), table_bytes::<usize>(map.capacity()));
    }
}

pub fn index_set<T: Allocative, S>(set: &IndexSet<T, S>, visitor: &mut Visitor<'_>) {
    let mut visitor = visitor.enter_self(set);
    if set.capacity() != 0 {
        let mut allocation = visitor.enter_unique(Key::new("entries"), size_of::<usize>());
        let mut entries = allocation.enter(
            Key::new("capacity"),
            set.capacity() * size_of::<(usize, T)>(),
        );
        for value in set {
            entries.visit_field(Key::new("value"), value);
        }
    }
    if set.capacity() != 0 {
        let mut table = visitor.enter_unique(Key::new("index"), size_of::<usize>());
        table.visit_simple(Key::new("capacity"), table_bytes::<usize>(set.capacity()));
    }
}

/// BTreeMap does not expose capacity. Count live entry payload and descendants;
/// node headers and unused slots are explicitly excluded from the estimates.
pub fn btree_map<K: Allocative, V: Allocative>(map: &BTreeMap<K, V>, visitor: &mut Visitor<'_>) {
    let mut visitor = visitor.enter_self(map);
    if !map.is_empty() {
        let mut allocation = visitor.enter_unique(Key::new("entries"), size_of::<usize>());
        let mut entries =
            allocation.enter(Key::new("live_payload"), map.len() * size_of::<(K, V)>());
        for (key, value) in map {
            entries.visit_field(Key::new("key"), key);
            entries.visit_field(Key::new("value"), value);
        }
    }
}

pub fn option_btree_map<K: Allocative, V: Allocative>(
    map: &Option<BTreeMap<K, V>>,
    visitor: &mut Visitor<'_>,
) {
    let mut visitor = visitor.enter_self(map);
    if let Some(map) = map {
        field_with(&mut visitor, map, btree_map);
    }
}

pub fn option_arc_btree_map<K: Allocative, V: Allocative>(
    map: &Option<Arc<BTreeMap<K, V>>>,
    visitor: &mut Visitor<'_>,
) {
    let mut visitor = visitor.enter_self(map);
    if let Some(map) = map {
        let mut arc = visitor.enter(Key::new("Arc"), size_of_val(map));
        if let Some(mut shared) =
            arc.enter_shared(Key::new("ptr"), size_of::<usize>(), Arc::as_ptr(map).cast())
        {
            let layout = Layout::new::<[usize; 2]>()
                .extend(Layout::new::<BTreeMap<K, V>>())
                .unwrap()
                .0
                .pad_to_align();
            let mut inner = shared.enter(Key::new("ArcInner"), layout.size());
            btree_map(map, &mut inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocative::FlameGraphBuilder;

    fn measured(visit: impl Fn(&mut Visitor<'_>)) -> usize {
        struct Root<F>(F);
        impl<F: Fn(&mut Visitor<'_>)> Allocative for Root<F> {
            fn visit<'a, 'b: 'a>(&self, visitor: &'a mut Visitor<'b>) {
                (self.0)(visitor);
            }
        }
        let mut builder = FlameGraphBuilder::default();
        builder.visit_root(&Root(visit));
        let output = builder.finish();
        assert!(output.warnings().is_empty(), "{}", output.warnings());
        output.flamegraph().total_size()
    }

    #[test]
    fn empty_hash_maps_still_charge_reserved_storage() {
        let map = HashMap::<u64, u64>::with_capacity(100);
        assert_eq!(
            measured(|visitor| hash_map(&map, visitor)),
            size_of_val(&map) + table_bytes::<(u64, u64)>(map.capacity()),
        );
    }

    #[test]
    fn nested_maps_include_inner_capacity_without_counting_inline_values_twice() {
        let inner = HashMap::<u64, u64>::with_capacity(100);
        let inner_bytes = table_bytes::<(u64, u64)>(inner.capacity());
        let map = HashMap::from([(1u64, inner)]);
        let expected = size_of_val(&map)
            + table_bytes::<(u64, HashMap<u64, u64>)>(map.capacity())
            + inner_bytes;
        assert_eq!(
            measured(|visitor| hash_map_of_maps(&map, visitor)),
            expected,
        );
    }

    #[test]
    fn shared_trees_are_deduplicated_across_roots() {
        let map = Arc::new(BTreeMap::from([(1u64, "value".to_owned())]));
        let left = Some(map.clone());
        let shared = Some(map.clone());
        let distinct = Some(Arc::new((*map).clone()));
        let once = measured(|visitor| option_arc_btree_map(&left, visitor));
        let twice_shared = measured(|visitor| {
            option_arc_btree_map(&left, visitor);
            option_arc_btree_map(&shared, visitor);
        });
        let twice_distinct = measured(|visitor| {
            option_arc_btree_map(&left, visitor);
            option_arc_btree_map(&distinct, visitor);
        });
        assert_eq!(twice_shared, once + size_of_val(&shared));
        assert_eq!(twice_distinct, once * 2);
    }

    #[test]
    fn interned_strings_include_spare_capacity_and_heap_contents() {
        let mut set = IndexSet::<String>::with_capacity(100);
        let string = String::with_capacity(200);
        let string_capacity = string.capacity();
        set.insert(string);
        assert_eq!(
            measured(|visitor| index_set(&set, visitor)),
            size_of_val(&set)
                + set.capacity() * size_of::<(usize, String)>()
                + table_bytes::<usize>(set.capacity())
                + string_capacity,
        );
    }
}
