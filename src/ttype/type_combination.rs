use std::{collections::BTreeMap, sync::Arc};

use hakana_reflection_info::{
    codebase_info::symbols::Symbol,
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug)]
pub(crate) struct TypeCombination {
    pub value_types: FxHashMap<String, TAtomic>,

    pub has_object_top_type: bool,

    pub enum_types: FxHashMap<Symbol, Option<Box<TAtomic>>>,
    pub enum_value_types: FxHashMap<Symbol, FxHashMap<Symbol, Option<Box<TAtomic>>>>,

    pub object_type_params: FxHashMap<String, (Symbol, Vec<TUnion>)>,

    pub object_static: FxHashMap<Symbol, bool>,

    pub vec_counts: Option<FxHashSet<usize>>,

    pub vec_sometimes_filled: bool,
    pub vec_always_filled: bool,

    pub dict_sometimes_filled: bool,
    pub dict_always_filled: bool,

    pub has_dict: bool,
    pub dict_entries: BTreeMap<DictKey, (bool, Arc<TUnion>)>,
    pub vec_entries: BTreeMap<usize, (bool, TUnion)>,

    pub dict_type_params: Option<(TUnion, TUnion)>,
    pub vec_type_param: Option<TUnion>,
    pub keyset_type_param: Option<TUnion>,

    pub dict_alias_name: Option<Option<String>>,

    pub falsy_mixed: Option<bool>,
    pub truthy_mixed: Option<bool>,
    pub nonnull_mixed: Option<bool>,
    pub any_mixed: bool,
    pub vanilla_mixed: bool,
    pub has_mixed: bool,

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
            has_object_top_type: false,
            object_type_params: FxHashMap::default(),
            object_static: FxHashMap::default(),
            vec_counts: Some(FxHashSet::default()),
            vec_sometimes_filled: false,
            vec_always_filled: true,
            dict_sometimes_filled: false,
            dict_always_filled: true,
            has_dict: false,
            dict_entries: BTreeMap::new(),
            vec_entries: BTreeMap::new(),
            dict_type_params: None,
            vec_type_param: None,
            keyset_type_param: None,
            dict_alias_name: None,
            falsy_mixed: None,
            truthy_mixed: None,
            nonnull_mixed: None,
            vanilla_mixed: false,
            has_mixed: false,
            any_mixed: false,
            mixed_from_loop_isset: None,
            literal_strings: FxHashMap::default(),
            literal_ints: FxHashMap::default(),
            class_string_types: FxHashMap::default(),
            extra_types: None,
            enum_types: FxHashMap::default(),
            enum_value_types: FxHashMap::default(),
        }
    }

    #[inline]
    pub(crate) fn is_simple(&self) -> bool {
        if self.value_types.len() == 1 && !self.has_dict {
            if let (None, None, None) = (
                &self.dict_type_params,
                &self.vec_type_param,
                &self.keyset_type_param,
            ) {
                return self.dict_entries.is_empty()
                    && self.vec_entries.is_empty()
                    && self.object_type_params.is_empty()
                    && self.enum_types.is_empty()
                    && self.enum_value_types.is_empty()
                    && self.literal_strings.is_empty()
                    && self.literal_ints.is_empty()
                    && self.class_string_types.is_empty();
            }
        }

        false
    }
}
