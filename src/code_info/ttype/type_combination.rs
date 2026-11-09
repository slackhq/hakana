use std::{collections::BTreeMap, sync::Arc};

use crate::{
    classlike_info::ClassLikeInfo,
    codebase_info::{CodebaseInfo, symbols::SymbolKind},
    t_atomic::{DictKey, TAtomic, TNamedObject},
    t_union::TUnion,
};
use hakana_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

/// What makes two types the same entry in a combination: two types with the
/// same key are one type in the result. It also fixes the order of the result,
/// which is by key, so that a union comes out the same whatever order its
/// types went in. `null` comes last: a nullable type reads `T, null`, and
/// some checks only look at the first type they can judge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CombinerKey {
    Bool,
    False,
    True,
    Int,
    Float,
    String,
    Arraykey,
    Num,
    Scalar,
    Void,
    Nothing,
    Mixed,
    /// A class, interface or enum, by name
    Symbol(StrId),
    EnumCase(StrId, StrId),
    /// Everything else, by `TAtomic::get_key`
    Other(String),
    Null,
}

impl CombinerKey {
    pub(crate) fn of(atomic: &TAtomic) -> Self {
        match atomic {
            TAtomic::TNull => Self::Null,
            TAtomic::TBool => Self::Bool,
            TAtomic::TFalse => Self::False,
            TAtomic::TTrue => Self::True,
            TAtomic::TInt => Self::Int,
            TAtomic::TFloat => Self::Float,
            TAtomic::TString | TAtomic::TStringWithFlags(..) => Self::String,
            TAtomic::TArraykey { .. } => Self::Arraykey,
            TAtomic::TNum => Self::Num,
            TAtomic::TScalar => Self::Scalar,
            TAtomic::TVoid => Self::Void,
            TAtomic::TNothing => Self::Nothing,
            TAtomic::TNamedObject(TNamedObject {
                name,
                type_params: None,
                ..
            }) => Self::Symbol(*name),
            _ => Self::from_key(atomic.get_key()),
        }
    }

    /// The key for a type whose `TAtomic::get_key` is this string. Whichever
    /// way a key is arrived at, types that share a `get_key` share a key.
    fn from_key(key: String) -> Self {
        match key.as_str() {
            "null" => Self::Null,
            "bool" => Self::Bool,
            "false" => Self::False,
            "true" => Self::True,
            "int" => Self::Int,
            "float" => Self::Float,
            "string" => Self::String,
            "arraykey" => Self::Arraykey,
            "num" => Self::Num,
            "scalar" => Self::Scalar,
            "void" => Self::Void,
            "nothing" => Self::Nothing,
            "mixed" => Self::Mixed,
            _ => {
                // names are keyed by the number of their interned string
                let as_id = |digits: &str| {
                    digits
                        .parse::<u32>()
                        .ok()
                        .filter(|id| id.to_string() == digits)
                        .map(StrId)
                };

                if let Some(name) = as_id(&key) {
                    Self::Symbol(name)
                } else if let Some((enum_name, member_name)) = key.split_once("::")
                    && let (Some(enum_name), Some(member_name)) =
                        (as_id(enum_name), as_id(member_name))
                {
                    Self::EnumCase(enum_name, member_name)
                } else {
                    Self::Other(key)
                }
            }
        }
    }
}

/// The types of a combination that are kept whole, by key and in key order.
/// A union rarely has more than a few of them, so a sorted list does better
/// than a map.
#[derive(Debug, Default)]
pub(crate) struct ValueTypes {
    entries: Vec<(CombinerKey, TAtomic)>,
}

impl ValueTypes {
    fn position(&self, key: &CombinerKey) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|(existing_key, _)| existing_key.cmp(key))
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn contains_key(&self, key: &CombinerKey) -> bool {
        self.position(key).is_ok()
    }

    pub(crate) fn get(&self, key: &CombinerKey) -> Option<&TAtomic> {
        self.position(key).ok().map(|i| &self.entries[i].1)
    }

    pub(crate) fn get_mut(&mut self, key: &CombinerKey) -> Option<&mut TAtomic> {
        self.position(key).ok().map(|i| &mut self.entries[i].1)
    }

    /// Adds a type, in place of any that has the same key.
    pub(crate) fn insert(&mut self, key: CombinerKey, atomic: TAtomic) {
        match self.position(&key) {
            Ok(i) => self.entries[i].1 = atomic,
            Err(i) => self.entries.insert(i, (key, atomic)),
        }
    }

    pub(crate) fn remove(&mut self, key: &CombinerKey) {
        if let Ok(i) = self.position(key) {
            self.entries.remove(i);
        }
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&CombinerKey, &TAtomic) -> bool) {
        self.entries.retain(|(key, atomic)| keep(key, atomic));
    }

    pub(crate) fn into_values(self) -> impl Iterator<Item = TAtomic> {
        self.entries.into_iter().map(|(_, atomic)| atomic)
    }
}

/// A class type in a combination, with what is needed to tell whether
/// another class type is its parent or its child.
#[derive(Debug)]
pub(crate) struct CombinedObject<'a> {
    pub name: StrId,
    pub kind: Option<&'a SymbolKind>,
    /// Looked up when first needed, which is when a second class turns up
    info: Option<Option<&'a ClassLikeInfo>>,
}

impl<'a> CombinedObject<'a> {
    pub(crate) fn new(name: StrId, kind: Option<&'a SymbolKind>) -> Self {
        Self {
            name,
            kind,
            info: None,
        }
    }

    pub(crate) fn info(&mut self, codebase: &'a CodebaseInfo) -> Option<&'a ClassLikeInfo> {
        *self
            .info
            .get_or_insert_with(|| codebase.classlike_infos.get(&self.name))
    }
}

#[derive(Debug)]
pub(crate) struct TypeCombination<'a> {
    pub value_types: ValueTypes,

    /// The class types among `value_types`
    pub objects: Vec<CombinedObject<'a>>,

    pub has_object_top_type: bool,

    pub enum_types: FxHashMap<StrId, (Option<Arc<TAtomic>>, Option<Arc<TAtomic>>)>,
    pub enum_value_types: FxHashMap<
        StrId,
        (
            usize,
            FxHashMap<StrId, (Option<Arc<TAtomic>>, Option<Arc<TAtomic>>)>,
        ),
    >,

    pub object_type_params: BTreeMap<String, (StrId, Vec<TUnion>)>,

    pub object_static: FxHashMap<StrId, bool>,

    pub vec_counts: Option<FxHashSet<usize>>,

    pub vec_always_filled: bool,
    pub dict_always_filled: bool,
    pub keyset_always_filled: bool,

    pub has_dict: bool,
    pub dict_entries: BTreeMap<DictKey, (bool, Arc<TUnion>)>,
    /// The fields of the first shape seen, still in the map they came in.
    /// Most unions hold at most one shape, and then this map is the result:
    /// copying it field by field, only to wrap the copy up again, would also
    /// cut it off from every other holder of the same shape. It moves into
    /// `dict_entries` (see `own_dict_entries`) once something has to change.
    pub lone_dict_entries: Option<Arc<BTreeMap<DictKey, (bool, Arc<TUnion>)>>>,
    pub dict_always_shape: bool,
    pub vec_entries: BTreeMap<usize, (bool, TUnion)>,

    pub dict_type_params: Option<(TUnion, TUnion)>,
    pub vec_type_param: Option<TUnion>,
    pub keyset_type_param: Option<TUnion>,
    pub awaitable_param: Option<TUnion>,

    pub dict_alias_name: Option<Option<(StrId, Option<StrId>)>>,

    pub nonnull_mixed: Option<bool>,
    pub any_mixed: bool,
    pub vanilla_mixed: bool,
    pub has_mixed: bool,

    pub mixed_from_loop_isset: Option<bool>,

    pub literal_strings: FxHashSet<String>,
    pub literal_ints: FxHashSet<i64>,
}

impl TypeCombination<'_> {
    pub(crate) fn new() -> Self {
        Self {
            value_types: ValueTypes::default(),
            objects: Vec::new(),
            has_object_top_type: false,
            object_type_params: BTreeMap::new(),
            object_static: FxHashMap::default(),
            vec_counts: Some(FxHashSet::default()),
            vec_always_filled: true,
            dict_always_filled: true,
            keyset_always_filled: true,
            has_dict: false,
            dict_entries: BTreeMap::new(),
            lone_dict_entries: None,
            vec_entries: BTreeMap::new(),
            dict_type_params: None,
            vec_type_param: None,
            keyset_type_param: None,
            awaitable_param: None,
            dict_alias_name: None,
            dict_always_shape: true,
            nonnull_mixed: None,
            vanilla_mixed: false,
            has_mixed: false,
            any_mixed: false,
            mixed_from_loop_isset: None,
            literal_strings: FxHashSet::default(),
            literal_ints: FxHashSet::default(),
            enum_types: FxHashMap::default(),
            enum_value_types: FxHashMap::default(),
        }
    }

    /// Takes a private copy of the first shape's fields, for changing.
    pub(crate) fn own_dict_entries(&mut self) {
        if let Some(entries) = self.lone_dict_entries.take() {
            self.dict_entries = Arc::unwrap_or_clone(entries);
        }
    }

    #[inline]
    pub(crate) fn is_simple(&self) -> bool {
        if self.value_types.len() == 1
            && !self.has_dict
            && let (None, None, None, None) = (
                &self.dict_type_params,
                &self.vec_type_param,
                &self.keyset_type_param,
                &self.awaitable_param,
            )
        {
            return self.dict_entries.is_empty()
                && self
                    .lone_dict_entries
                    .as_ref()
                    .is_none_or(|entries| entries.is_empty())
                && self.vec_entries.is_empty()
                && self.object_type_params.is_empty()
                && self.enum_types.is_empty()
                && self.enum_value_types.is_empty()
                && self.literal_strings.is_empty()
                && self.literal_ints.is_empty();
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::t_atomic::TDict;

    #[test]
    fn typed_keys_agree_with_string_keys() {
        // A type that is given its key directly has to get the key its
        // `get_key` string stands for, or it would no longer be merged with
        // the types that share that string.
        for atomic in [
            TAtomic::TNull,
            TAtomic::TBool,
            TAtomic::TFalse,
            TAtomic::TTrue,
            TAtomic::TInt,
            TAtomic::TFloat,
            TAtomic::TString,
            TAtomic::TStringWithFlags(true, false),
            TAtomic::TArraykey { from_any: false },
            TAtomic::TNum,
            TAtomic::TScalar,
            TAtomic::TVoid,
            TAtomic::TNothing,
            TAtomic::TNamedObject(TNamedObject {
                name: StrId(1234),
                type_params: None,
                is_this: false,
                remapped_params: false,
            }),
        ] {
            assert_eq!(
                CombinerKey::of(&atomic),
                CombinerKey::from_key(atomic.get_key()),
                "{:?}",
                atomic
            );
        }

        assert_eq!(
            CombinerKey::of(&TAtomic::TMixedFromLoopIsset),
            CombinerKey::Mixed
        );
        assert_eq!(
            CombinerKey::from_key("12::34".to_string()),
            CombinerKey::EnumCase(StrId(12), StrId(34))
        );
        assert_eq!(
            CombinerKey::of(&TAtomic::TDict(TDict::default())),
            CombinerKey::Other("dict".to_string())
        );
    }
}
