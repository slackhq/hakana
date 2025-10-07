use std::{collections::BTreeMap, sync::Arc};

use crate::{
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug)]
pub(crate) struct TypeCombination {
    pub value_types: FxHashMap<String, TAtomic>,

    pub has_object_top_type: bool,

    pub enum_types: FxHashMap<StrId, (Option<Arc<TAtomic>>, Option<Arc<TAtomic>>)>,
    pub enum_value_types: FxHashMap<
        StrId,
        (
            usize,
            FxHashMap<StrId, (Option<Arc<TAtomic>>, Option<Arc<TAtomic>>)>,
        ),
    >,

    pub object_type_params: FxHashMap<String, (StrId, Vec<TUnion>)>,

    pub object_static: FxHashMap<StrId, bool>,

    pub vec_counts: Option<FxHashSet<usize>>,

    pub vec_always_filled: bool,
    pub dict_always_filled: bool,
    pub keyset_always_filled: bool,

    pub has_dict: bool,
    pub dict_entries: BTreeMap<DictKey, (bool, Arc<TUnion>)>,
    pub vec_entries: BTreeMap<usize, (bool, TUnion)>,

    pub dict_type_params: Option<(TUnion, TUnion)>,
    pub vec_type_param: Option<TUnion>,
    pub keyset_type_param: Option<TUnion>,
    pub awaitable_param: Option<TUnion>,

    pub dict_alias_name: Option<Option<(StrId, Option<StrId>)>>,

    pub falsy_mixed: Option<bool>,
    pub truthy_mixed: Option<bool>,
    pub nonnull_mixed: Option<bool>,
    pub any_mixed: bool,
    pub vanilla_mixed: bool,
    pub has_mixed: bool,

    pub mixed_from_loop_isset: Option<bool>,

    pub literal_strings: FxHashSet<String>,
    pub literal_ints: FxHashSet<i64>,

    pub class_string_types: FxHashMap<String, TAtomic>,
}

impl TypeCombination {
    pub(crate) fn new() -> Self {
        Self {
            value_types: FxHashMap::default(),
            has_object_top_type: false,
            object_type_params: FxHashMap::default(),
            object_static: FxHashMap::default(),
            vec_counts: Some(FxHashSet::default()),
            vec_always_filled: true,
            dict_always_filled: true,
            keyset_always_filled: true,
            has_dict: false,
            dict_entries: BTreeMap::new(),
            vec_entries: BTreeMap::new(),
            dict_type_params: None,
            vec_type_param: None,
            keyset_type_param: None,
            awaitable_param: None,
            dict_alias_name: None,
            falsy_mixed: None,
            truthy_mixed: None,
            nonnull_mixed: None,
            vanilla_mixed: false,
            has_mixed: false,
            any_mixed: false,
            mixed_from_loop_isset: None,
            literal_strings: FxHashSet::default(),
            literal_ints: FxHashSet::default(),
            class_string_types: FxHashMap::default(),
            enum_types: FxHashMap::default(),
            enum_value_types: FxHashMap::default(),
        }
    }

    #[inline]
    pub(crate) fn is_simple(&self) -> bool {
        if self.value_types.len() == 1 && !self.has_dict {
            if let (None, None, None, None) = (
                &self.dict_type_params,
                &self.vec_type_param,
                &self.keyset_type_param,
                &self.awaitable_param,
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
