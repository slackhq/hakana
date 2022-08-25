use std::{collections::BTreeMap, sync::Arc};

use hakana_reflection_info::{t_atomic::TAtomic, t_union::TUnion};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) struct TypeCombination {
    pub value_types: FxHashMap<String, TAtomic>,

    pub named_object_types: FxHashMap<String, TAtomic>,
    pub has_object_top_type: bool,

    pub enum_types: FxHashSet<String>,
    pub enum_value_types: FxHashMap<String, FxHashMap<String, Option<Box<TAtomic>>>>,

    pub object_type_params: FxHashMap<String, (String, Vec<TUnion>)>,

    pub object_static: FxHashMap<String, bool>,

    pub vec_counts: Option<FxHashSet<usize>>,

    pub vec_sometimes_filled: bool,
    pub vec_always_filled: bool,

    pub dict_sometimes_filled: bool,
    pub dict_always_filled: bool,

    // we only care about string dict entries, since
    // those are the ones allowed by shapes
    pub dict_entries: BTreeMap<String, (bool, Arc<TUnion>)>,
    pub vec_entries: BTreeMap<usize, (bool, TUnion)>,

    pub dict_type_params: Option<(TUnion, TUnion)>,
    pub vec_type_param: Option<TUnion>,
    pub keyset_type_param: Option<TUnion>,

    pub dict_name: Option<String>,

    pub falsy_mixed: bool,
    pub truthy_mixed: bool,
    pub nonnull_mixed: bool,
    pub vanilla_mixed: bool,
    pub any_mixed: bool,

    pub mixed_from_loop_isset: Option<bool>,

    pub literal_strings: FxHashMap<String, TAtomic>,
    pub literal_ints: FxHashMap<String, TAtomic>,

    pub class_string_types: FxHashMap<String, TAtomic>,

    pub extra_types: Option<FxHashMap<String, TAtomic>>,
}

impl TypeCombination {
    pub(crate) fn new() -> Self {
        Self {
            value_types: FxHashMap::default(),
            named_object_types: FxHashMap::default(),
            has_object_top_type: false,
            object_type_params: FxHashMap::default(),
            object_static: FxHashMap::default(),
            vec_counts: Some(FxHashSet::default()),
            vec_sometimes_filled: false,
            vec_always_filled: true,
            dict_sometimes_filled: false,
            dict_always_filled: true,
            dict_entries: BTreeMap::new(),
            vec_entries: BTreeMap::new(),
            dict_type_params: None,
            vec_type_param: None,
            keyset_type_param: None,
            dict_name: None,
            falsy_mixed: false,
            truthy_mixed: false,
            nonnull_mixed: false,
            vanilla_mixed: false,
            any_mixed: false,
            mixed_from_loop_isset: None,
            literal_strings: FxHashMap::default(),
            literal_ints: FxHashMap::default(),
            class_string_types: FxHashMap::default(),
            extra_types: None,
            enum_types: FxHashSet::default(),
            enum_value_types: FxHashMap::default(),
        }
    }
}
