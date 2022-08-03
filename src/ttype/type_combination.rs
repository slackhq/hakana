use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use hakana_reflection_info::{t_atomic::TAtomic, t_union::TUnion};

pub(crate) struct TypeCombination {
    pub value_types: HashMap<String, TAtomic>,

    pub named_object_types: HashMap<String, TAtomic>,
    pub has_object_top_type: bool,

    pub enum_types: HashSet<String>,
    pub enum_value_types: HashMap<String, HashSet<String>>,

    pub object_type_params: HashMap<String, (String, Vec<TUnion>)>,

    pub object_static: HashMap<String, bool>,

    pub vec_counts: Option<HashSet<usize>>,

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

    pub literal_strings: HashMap<String, TAtomic>,
    pub literal_ints: HashMap<String, TAtomic>,

    pub class_string_types: HashMap<String, TAtomic>,

    pub extra_types: Option<HashMap<String, TAtomic>>,
}

impl TypeCombination {
    pub(crate) fn new() -> Self {
        Self {
            value_types: HashMap::new(),
            named_object_types: HashMap::new(),
            has_object_top_type: false,
            object_type_params: HashMap::new(),
            object_static: HashMap::new(),
            vec_counts: Some(HashSet::new()),
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
            literal_strings: HashMap::new(),
            literal_ints: HashMap::new(),
            class_string_types: HashMap::new(),
            extra_types: None,
            enum_types: HashSet::new(),
            enum_value_types: HashMap::new(),
        }
    }
}
