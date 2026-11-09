use std::{collections::BTreeMap, sync::Arc};

use crate::{
    classlike_info::{ClassLikeInfo, Variance},
    codebase_info::{CodebaseInfo, symbols::SymbolKind},
    t_atomic::{DictKey, TAtomic, TDict, TNamedObject, TVec},
    t_union::TUnion,
};
use hakana_str::StrId;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ttype::{
    combine_union_types, get_int,
    type_combination::{self, CombinedObject, CombinerKey, TypeCombination},
    wrap_atomic,
};

pub fn combine(
    mut types: Vec<TAtomic>,
    codebase: &CodebaseInfo,
    overwrite_empty_array: bool,
) -> Vec<TAtomic> {
    if types.len() == 1 {
        return types;
    }

    if combine_unrelated(&mut types) {
        return types;
    }

    let mut combination = type_combination::TypeCombination::new();

    for atomic in types {
        scrape_type_properties(atomic, &mut combination, codebase, overwrite_empty_array);
    }

    if combination.nonnull_mixed.unwrap_or(false) || combination.any_mixed {
        return vec![TAtomic::TMixedWithFlags(
            combination.any_mixed,
            combination.nonnull_mixed.unwrap_or(false),
        )];
    } else if combination.has_mixed {
        return vec![TAtomic::TMixed];
    }

    if combination.is_simple() {
        if combination.value_types.contains_key(&CombinerKey::False) {
            return vec![TAtomic::TFalse];
        }

        if combination.value_types.contains_key(&CombinerKey::True) {
            return vec![TAtomic::TTrue];
        }

        return combination.value_types.into_values().collect();
    }

    if combination.value_types.contains_key(&CombinerKey::Void) {
        combination.value_types.remove(&CombinerKey::Void);

        if combination.value_types.contains_key(&CombinerKey::Null) {
            combination
                .value_types
                .insert(CombinerKey::Null, TAtomic::TNull);
        }
    }

    if combination.value_types.contains_key(&CombinerKey::False)
        && combination.value_types.contains_key(&CombinerKey::True)
    {
        combination.value_types.remove(&CombinerKey::False);
        combination.value_types.remove(&CombinerKey::True);
        combination
            .value_types
            .insert(CombinerKey::Bool, TAtomic::TBool);
    }

    let mut new_types = Vec::new();

    if combination.has_dict {
        new_types.push(TAtomic::TDict(TDict {
            known_items: if let Some(entries) = combination.lone_dict_entries {
                if entries.is_empty() {
                    None
                } else {
                    Some(entries)
                }
            } else if combination.dict_entries.is_empty() {
                None
            } else {
                Some(Arc::new(combination.dict_entries))
            },
            params: if let Some((k, v)) = combination.dict_type_params {
                Some((Box::new(k), Box::new(v)))
            } else {
                None
            },
            non_empty: combination.dict_always_filled,
            shape_name: combination.dict_alias_name.unwrap_or(None),
            is_shape: combination.dict_always_shape,
        }));
    }

    if let Some(vec_type_param) = combination.vec_type_param {
        new_types.push(TAtomic::TVec(TVec {
            known_items: if combination.vec_entries.is_empty() {
                None
            } else {
                Some(combination.vec_entries)
            },
            type_param: Box::new(vec_type_param),
            non_empty: combination.vec_always_filled,
            known_count: None,
            variadic_type: None,
        }));
    }

    if let Some(keyset_type_param) = combination.keyset_type_param {
        new_types.push(TAtomic::TKeyset {
            type_param: Box::new(keyset_type_param),
            non_empty: combination.keyset_always_filled,
        });
    }

    if let Some(type_param) = combination.awaitable_param {
        new_types.push(TAtomic::TAwaitable {
            value: Box::new(type_param),
        });
    }

    for (_, (generic_type, generic_type_params)) in combination.object_type_params {
        let generic_object = TAtomic::TNamedObject(TNamedObject {
            is_this: *combination
                .object_static
                .get(&generic_type)
                .unwrap_or(&false),
            name: generic_type,
            type_params: Some(generic_type_params),
            remapped_params: false,
        });

        new_types.push(generic_object);
    }

    new_types.extend(
        combination
            .literal_strings
            .into_iter()
            .map(|s| TAtomic::TLiteralString { value: s })
            .collect::<Vec<_>>(),
    );
    new_types.extend(
        combination
            .literal_ints
            .into_iter()
            .map(|i| TAtomic::TLiteralInt { value: i })
            .collect::<Vec<_>>(),
    );

    if combination.value_types.contains_key(&CombinerKey::String)
        && combination.value_types.contains_key(&CombinerKey::Float)
        && combination.value_types.contains_key(&CombinerKey::Int)
        && combination.value_types.contains_key(&CombinerKey::Bool)
    {
        combination.value_types.remove(&CombinerKey::String);
        combination.value_types.remove(&CombinerKey::Float);
        combination.value_types.remove(&CombinerKey::Int);
        combination.value_types.remove(&CombinerKey::Bool);
        new_types.push(TAtomic::TScalar {});
    }

    for (enum_name, (as_type, underlying_type)) in combination.enum_types {
        combination.value_types.insert(
            CombinerKey::Symbol(enum_name),
            TAtomic::TEnum {
                name: enum_name,
                as_type,
                underlying_type,
            },
        );
    }

    for (enum_name, values) in combination.enum_value_types {
        for (member_name, (as_type, underlying_type)) in values.1 {
            combination.value_types.insert(
                CombinerKey::EnumCase(enum_name, member_name),
                TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                    as_type,
                    underlying_type,
                },
            );
        }
    }

    let mut has_nothing = combination.value_types.contains_key(&CombinerKey::Nothing);

    let combination_value_type_count = combination.value_types.len();

    for atomic in combination.value_types.into_values() {
        let tc = if has_nothing { 1 } else { 0 };
        if atomic.is_mixed()
            && combination.mixed_from_loop_isset.unwrap_or(false)
            && (combination_value_type_count > (tc + 1) || new_types.len() > tc)
        {
            continue;
        }

        if let TAtomic::TNothing = atomic
            && (combination_value_type_count > 1 || !new_types.is_empty())
        {
            has_nothing = true;
            continue;
        }

        new_types.push(atomic);
    }

    if new_types.is_empty() {
        return vec![TAtomic::TNothing];
    }

    new_types
}

/// Where a type that has no effect on the others stands in a result (the
/// order of its `CombinerKey`), or `None` for a type that may merge with,
/// absorb or be absorbed by another.
fn unrelated_rank(atomic: &TAtomic) -> Option<u8> {
    match atomic {
        TAtomic::TBool => Some(0),
        TAtomic::TInt => Some(1),
        TAtomic::TFloat => Some(2),
        TAtomic::TString => Some(3),
        TAtomic::TNamedObject(TNamedObject {
            type_params: None, ..
        }) => Some(UNRELATED_CLASS),
        TAtomic::TNull => Some(5),
        _ => None,
    }
}

const UNRELATED_CLASS: u8 = 4;

/// Most unions that get combined are a couple of plain types, such as `?int`
/// or `?Foo`, which have nothing to say to each other: the result is those
/// types, once each, in order. Does that in place, or returns false for
/// anything that needs the full treatment.
fn combine_unrelated(types: &mut Vec<TAtomic>) -> bool {
    let mut seen = 0u8;

    for atomic in types.iter() {
        let Some(rank) = unrelated_rank(atomic) else {
            return false;
        };

        // two classes may be parent and child
        if rank == UNRELATED_CLASS && seen & (1 << rank) != 0 {
            return false;
        }

        seen |= 1 << rank;
    }

    // nothing to combine, or bool|int|float|string, which is `scalar`
    if seen == 0 || seen & 0b1111 == 0b1111 {
        return false;
    }

    types.sort_by_key(unrelated_rank);
    types.dedup_by_key(|atomic| unrelated_rank(atomic));

    true
}

fn scrape_type_properties<'a>(
    atomic: TAtomic,
    combination: &mut TypeCombination<'a>,
    codebase: &'a CodebaseInfo,
    overwrite_empty_array: bool,
) {
    match atomic {
        TAtomic::TMixed => {
            combination.mixed_from_loop_isset = Some(false);
            combination.nonnull_mixed = Some(false);
            combination.vanilla_mixed = true;
            combination.has_mixed = true;

            return;
        }
        TAtomic::TMixedFromLoopIsset => {
            if combination.vanilla_mixed || combination.any_mixed {
                return;
            }

            if combination.mixed_from_loop_isset.is_none() {
                combination.mixed_from_loop_isset = Some(true);
            }

            combination.value_types.insert(CombinerKey::Mixed, atomic);
            return;
        }
        TAtomic::TMixedWithFlags(any, nonnull_mixed) => {
            combination.has_mixed = true;

            if any {
                combination.any_mixed = true;
            }

            if nonnull_mixed {
                if combination.vanilla_mixed {
                    return;
                }

                combination.mixed_from_loop_isset = Some(false);

                if combination.value_types.contains_key(&CombinerKey::Null) {
                    combination.vanilla_mixed = true;
                    return;
                }

                if combination.nonnull_mixed.is_some() {
                    return;
                }

                combination.mixed_from_loop_isset = Some(false);
                combination.nonnull_mixed = Some(true);
            } else {
                combination.nonnull_mixed = Some(false);
            }

            return;
        }
        _ => (),
    }

    if combination.nonnull_mixed.unwrap_or(false) {
        if let TAtomic::TNull = atomic {
            combination.nonnull_mixed = Some(false);
            combination.vanilla_mixed = true;
        }

        return;
    } else if combination.has_mixed {
        return;
    }

    // bool|false = bool
    if let TAtomic::TFalse | TAtomic::TTrue = atomic
        && combination.value_types.contains_key(&CombinerKey::Bool)
    {
        return;
    }

    // false|bool = bool
    if let TAtomic::TBool = atomic {
        combination.value_types.remove(&CombinerKey::False);
        combination.value_types.remove(&CombinerKey::True);
    }

    if let TAtomic::TVec(TVec {
        ref type_param,
        non_empty,
        known_count,
        ref known_items,
        ..
    }) = atomic
    {
        let had_previous_param = combination.vec_type_param.is_some();

        if non_empty {
            if let Some(ref mut existing_counts) = combination.vec_counts {
                if let Some(known_count) = known_count {
                    existing_counts.insert(known_count);
                } else {
                    combination.vec_counts = None;
                }
            }
        } else {
            combination.vec_always_filled = false;
        }

        if let Some(known_items) = known_items {
            let has_existing_entries = !combination.vec_entries.is_empty() || had_previous_param;
            let mut possibly_undefined_entries: FxHashSet<usize> =
                combination.vec_entries.keys().cloned().collect();

            let mut has_defined_keys = false;

            for (candidate_item_offset, (cu, candidate_item_type)) in known_items {
                combination.vec_entries.insert(
                    *candidate_item_offset,
                    if let Some((eu, existing_type)) =
                        combination.vec_entries.get(candidate_item_offset)
                    {
                        (
                            *eu || *cu,
                            combine_union_types(
                                existing_type,
                                candidate_item_type,
                                codebase,
                                overwrite_empty_array,
                            ),
                        )
                    } else {
                        (
                            has_existing_entries || *cu,
                            if let Some(ref mut existing_value_param) = combination.vec_type_param {
                                if !existing_value_param.is_nothing() {
                                    *existing_value_param = combine_union_types(
                                        existing_value_param,
                                        candidate_item_type,
                                        codebase,
                                        overwrite_empty_array,
                                    );
                                    continue;
                                }

                                candidate_item_type.clone()
                            } else {
                                candidate_item_type.clone()
                            },
                        )
                    },
                );

                possibly_undefined_entries.remove(candidate_item_offset);

                if !cu {
                    has_defined_keys = true;
                }
            }

            if !has_defined_keys {
                combination.vec_always_filled = false;
            }

            for possibly_undefined_type_key in possibly_undefined_entries {
                let possibly_undefined_type = combination
                    .vec_entries
                    .get_mut(&possibly_undefined_type_key);
                if let Some((pu, _)) = possibly_undefined_type {
                    *pu = true;
                }
            }
        } else if !overwrite_empty_array {
            if type_param.is_nothing() {
                for (_, (tu, _)) in combination.vec_entries.iter_mut() {
                    *tu = true;
                }
            } else {
                for (_, entry_type) in combination.vec_entries.values() {
                    if let Some(ref mut existing_value_param) = combination.vec_type_param {
                        *existing_value_param = combine_union_types(
                            existing_value_param,
                            entry_type,
                            codebase,
                            overwrite_empty_array,
                        );
                    }
                }

                combination.vec_entries = BTreeMap::new();
            }
        }

        combination.vec_type_param = if let Some(ref existing_type) = combination.vec_type_param {
            Some(combine_union_types(
                existing_type,
                type_param,
                codebase,
                overwrite_empty_array,
            ))
        } else {
            Some((**type_param).clone())
        };

        return;
    }

    if let TAtomic::TKeyset {
        ref type_param,
        non_empty,
    } = atomic
    {
        if !non_empty {
            combination.keyset_always_filled = false;
        }

        combination.keyset_type_param =
            if let Some(ref existing_type) = combination.keyset_type_param {
                Some(combine_union_types(
                    existing_type,
                    type_param,
                    codebase,
                    overwrite_empty_array,
                ))
            } else {
                Some((**type_param).clone())
            };

        return;
    }

    if let TAtomic::TDict(TDict {
        ref params,
        ref known_items,
        non_empty,
        shape_name,
        is_shape,
        ..
    }) = atomic
    {
        let had_previous_dict = combination.has_dict;
        combination.has_dict = true;

        if !non_empty {
            combination.dict_always_filled = false;
        }
        if !is_shape {
            combination.dict_always_shape = false;
        }

        if let Some(shape_name) = &shape_name {
            if let Some(ref mut existing_name) = combination.dict_alias_name {
                if let Some(existing_name_inner) = existing_name
                    && existing_name_inner != shape_name
                {
                    *existing_name = None;
                }
            } else {
                combination.dict_alias_name = Some(Some(*shape_name));
            }
        } else {
            combination.dict_alias_name = Some(None);
        }

        if let Some(known_items) = known_items
            && !had_previous_dict
            && combination.dict_entries.is_empty()
            && combination.lone_dict_entries.is_none()
            && combination.dict_type_params.is_none()
        {
            // The first shape: its fields are, as they stand, what merging
            // them into nothing would give.
            if known_items
                .values()
                .all(|(possibly_undefined, _)| *possibly_undefined)
            {
                combination.dict_always_filled = false;
            }

            combination.lone_dict_entries = Some(known_items.clone());
        } else if let Some(known_items) = known_items {
            combination.own_dict_entries();

            let has_existing_entries = !combination.dict_entries.is_empty() || had_previous_dict;
            let mut possibly_undefined_entries = combination
                .dict_entries
                .keys()
                .cloned()
                .collect::<FxHashSet<_>>();

            let mut has_defined_keys = false;

            for (candidate_item_name, (cu, candidate_item_type)) in known_items.iter() {
                if let Some((eu, existing_type)) =
                    combination.dict_entries.get_mut(candidate_item_name)
                {
                    if *cu {
                        *eu = true;
                    }
                    if candidate_item_type != existing_type {
                        *existing_type = Arc::new(combine_union_types(
                            existing_type,
                            candidate_item_type,
                            codebase,
                            overwrite_empty_array,
                        ));
                    }
                } else {
                    let new_item_value_type =
                        if let Some((ref mut existing_key_param, ref mut existing_value_param)) =
                            combination.dict_type_params
                        {
                            adjust_key_value_dict_params(
                                existing_value_param,
                                candidate_item_type,
                                codebase,
                                overwrite_empty_array,
                                candidate_item_name,
                                existing_key_param,
                            );

                            continue;
                        } else {
                            let new_type = candidate_item_type.clone();
                            (has_existing_entries || *cu, new_type)
                        };

                    combination
                        .dict_entries
                        .insert(candidate_item_name.clone(), new_item_value_type);
                };

                possibly_undefined_entries.remove(candidate_item_name);

                if !cu {
                    has_defined_keys = true;
                }
            }

            if !has_defined_keys {
                combination.dict_always_filled = false;
            }

            for possibly_undefined_type_key in possibly_undefined_entries {
                let possibly_undefined_type = combination
                    .dict_entries
                    .get_mut(&possibly_undefined_type_key);
                if let Some((pu, _)) = possibly_undefined_type {
                    *pu = true;
                }
            }
        } else if !overwrite_empty_array {
            combination.own_dict_entries();

            if match &params {
                Some((_, value_param)) => value_param.is_nothing(),
                None => true,
            } {
                for (_, (tu, _)) in combination.dict_entries.iter_mut() {
                    *tu = true;
                }
            } else {
                for (key, (_, entry_type)) in &combination.dict_entries {
                    if let Some((ref mut existing_key_param, ref mut existing_value_param)) =
                        combination.dict_type_params
                    {
                        adjust_key_value_dict_params(
                            existing_value_param,
                            entry_type,
                            codebase,
                            overwrite_empty_array,
                            key,
                            existing_key_param,
                        );
                    }
                }

                combination.dict_entries = BTreeMap::new();
            }
        }

        combination.dict_type_params = match (&combination.dict_type_params, params) {
            (None, None) => None,
            (Some(existing_types), None) => Some(existing_types.clone()),
            (None, Some(params)) => Some(((*params.0).clone(), (*params.1).clone())),
            (Some(existing_types), Some(params)) => Some((
                combine_union_types(
                    &existing_types.0,
                    &params.0,
                    codebase,
                    overwrite_empty_array,
                ),
                combine_union_types(
                    &existing_types.1,
                    &params.1,
                    codebase,
                    overwrite_empty_array,
                ),
            )),
        };

        return;
    }

    if let TAtomic::TAwaitable { ref value } = atomic {
        combination.awaitable_param = Some(
            if let Some(ref existing_info) = combination.awaitable_param {
                combine_union_types(existing_info, value, codebase, overwrite_empty_array)
            } else {
                (**value).clone()
            },
        );

        return;
    }

    // this probably won't ever happen, but the object top type
    // can eliminate variants
    if let TAtomic::TObject = atomic {
        combination.has_object_top_type = true;
        combination
            .value_types
            .retain(|_, t| !matches!(t, TAtomic::TNamedObject { .. }));
        combination.objects.clear();
        combination
            .value_types
            .insert(CombinerKey::of(&atomic), atomic);

        return;
    }

    // TODO (maybe) add support for Vector, Map etc.
    if let TAtomic::TNamedObject(TNamedObject {
        ref name, is_this, ..
    }) = atomic
    {
        if let Some(object_static) = combination.object_static.get(name) {
            if *object_static && !is_this {
                combination.object_static.insert(*name, false);
            }
        } else {
            combination.object_static.insert(*name, is_this);
        }
    }

    if let TAtomic::TNamedObject(TNamedObject {
        name: ref fq_class_name,
        type_params: Some(type_params),
        ..
    }) = atomic
    {
        match fq_class_name {
            &StrId::CONTAINER => {
                // dict<string, Foo>|Container<Bar> => Container<Foo|Bar>
                if let Some(ref dict_types) = combination.dict_type_params {
                    let container_value_type = if let Some((_, container_types)) = combination
                        .object_type_params
                        .get(&StrId::CONTAINER.0.to_string())
                    {
                        combine_union_types(
                            container_types.first().unwrap(),
                            &dict_types.1,
                            codebase,
                            false,
                        )
                    } else {
                        dict_types.1.clone()
                    };
                    combination.object_type_params.insert(
                        StrId::CONTAINER.0.to_string(),
                        (*fq_class_name, vec![container_value_type]),
                    );

                    combination.dict_type_params = None;
                    combination.has_dict = false;
                }

                // vec<Foo>|Container<Bar> => Container<Foo|Bar>
                if let Some(ref value_param) = combination.vec_type_param {
                    let container_value_type = if let Some((_, container_types)) = combination
                        .object_type_params
                        .get(&StrId::CONTAINER.0.to_string())
                    {
                        combine_union_types(
                            container_types.first().unwrap(),
                            value_param,
                            codebase,
                            false,
                        )
                    } else {
                        value_param.clone()
                    };
                    combination.object_type_params.insert(
                        StrId::CONTAINER.0.to_string(),
                        (*fq_class_name, vec![container_value_type]),
                    );

                    combination.vec_type_param = None;
                }

                // KeyedContainer<string, Foo>|Container<Bar> = Container<Foo|Bar>
                if let Some((_, keyed_container_types)) = combination
                    .object_type_params
                    .get(&StrId::KEYED_CONTAINER.0.to_string())
                {
                    let container_value_type = if let Some((_, container_types)) = combination
                        .object_type_params
                        .get(&StrId::KEYED_CONTAINER.0.to_string())
                    {
                        combine_union_types(
                            container_types.first().unwrap(),
                            keyed_container_types.get(1).unwrap(),
                            codebase,
                            false,
                        )
                    } else {
                        keyed_container_types.get(1).unwrap().clone()
                    };
                    combination.object_type_params.insert(
                        StrId::CONTAINER.0.to_string(),
                        (*fq_class_name, vec![container_value_type]),
                    );

                    combination
                        .object_type_params
                        .remove(&StrId::KEYED_CONTAINER.0.to_string());
                }
            }
            &StrId::KEYED_CONTAINER | &StrId::ANY_ARRAY => {
                merge_array_subtype(combination, fq_class_name, codebase);
            }
            _ => {}
        };

        let object_type_key = get_combiner_key(fq_class_name, &type_params, codebase);

        if let Some((_, existing_type_params)) =
            combination.object_type_params.get(&object_type_key)
        {
            let mut new_type_params = Vec::new();
            for (i, type_param) in type_params.into_iter().enumerate() {
                if let Some(existing_type_param) = existing_type_params.get(i) {
                    new_type_params.insert(
                        i,
                        combine_union_types(
                            existing_type_param,
                            &type_param,
                            codebase,
                            overwrite_empty_array,
                        ),
                    );
                }
            }

            combination
                .object_type_params
                .insert(object_type_key, (*fq_class_name, new_type_params));
        } else {
            combination
                .object_type_params
                .insert(object_type_key, (*fq_class_name, type_params));
        }

        return;
    }

    if let TAtomic::TEnumLiteralCase {
        enum_name,
        member_name,
        as_type,
        underlying_type,
    } = atomic
    {
        if combination.enum_types.contains_key(&enum_name) {
            return;
        }

        let mut matched_len_constraint = None;

        if let Some((expected_count, existing_enum_values)) =
            combination.enum_value_types.get_mut(&enum_name)
        {
            if *expected_count == existing_enum_values.len() + 1
                && !existing_enum_values.contains_key(&member_name)
            {
                matched_len_constraint = Some((as_type, underlying_type));
            } else {
                existing_enum_values.insert(member_name, (as_type, underlying_type));
            }
        } else if let Some(enum_storage) = codebase.classlike_infos.get(&enum_name) {
            combination.enum_value_types.insert(
                enum_name,
                (
                    enum_storage.constants.len(),
                    FxHashMap::from_iter([(member_name, (as_type, underlying_type))]),
                ),
            );
        }

        if let Some((as_type, underlying_type)) = matched_len_constraint {
            combination.enum_value_types.remove(&enum_name);
            combination.value_types.insert(
                CombinerKey::Symbol(enum_name),
                TAtomic::TEnum {
                    name: enum_name,
                    as_type,
                    underlying_type,
                },
            );
        }

        return;
    }

    if let TAtomic::TEnum {
        name,
        as_type,
        underlying_type,
        ..
    } = atomic
    {
        combination.enum_value_types.remove(&name);
        combination
            .enum_types
            .insert(name, (as_type, underlying_type));

        return;
    }

    if let TAtomic::TNamedObject(TNamedObject {
        name: fq_class_name,
        type_params: None,
        ..
    }) = atomic
    {
        let key = CombinerKey::Symbol(fq_class_name);

        if combination.has_object_top_type || combination.value_types.contains_key(&key) {
            return;
        }

        let symbol_type = codebase.symbols.all.get(&fq_class_name);

        let (is_class, is_interface) = match symbol_type {
            Some(SymbolKind::EnumClass | SymbolKind::Class) => (true, false),
            Some(SymbolKind::Interface) => (false, true),
            Some(SymbolKind::Enum) => (false, false),
            // not something that has parents or children
            _ => {
                combination.value_types.insert(key, atomic);
                combination
                    .objects
                    .push(CombinedObject::new(fq_class_name, symbol_type));
                return;
            }
        };

        // The ancestors of the new class are looked up once, and those of
        // each class already here once in the life of the combination: a
        // union of many classes compares every pair of them.
        let mut new_class_info = None;
        let mut objects_to_remove = Vec::new();

        for existing in combination.objects.iter_mut() {
            let existing_is_interface = match existing.kind {
                Some(SymbolKind::EnumClass | SymbolKind::Class) => false,
                Some(SymbolKind::Interface) => true,
                _ => continue,
            };

            // remove subclasses, and interfaces that extend this one
            if existing.info(codebase).is_some_and(|existing_info| {
                existing_info.all_parent_classes.contains(&fq_class_name)
                    || existing_info.all_parent_interfaces.contains(&fq_class_name)
            }) {
                objects_to_remove.push(existing.name);
                continue;
            }

            if !is_class && !is_interface {
                continue;
            }

            let new_class_info: &ClassLikeInfo = match new_class_info
                .get_or_insert_with(|| codebase.classlike_infos.get(&fq_class_name))
            {
                Some(new_class_info) => new_class_info,
                None => continue,
            };

            // skip if covered by a parent class, or by an interface it has
            let is_covered = if is_interface {
                new_class_info
                    .all_parent_interfaces
                    .contains(&existing.name)
                    || new_class_info.all_parent_classes.contains(&existing.name)
            } else if existing_is_interface {
                new_class_info
                    .all_parent_interfaces
                    .contains(&existing.name)
            } else {
                new_class_info.all_parent_classes.contains(&existing.name)
            };

            if is_covered {
                return;
            }
        }

        combination.value_types.insert(key, atomic);

        if !objects_to_remove.is_empty() {
            for name in &objects_to_remove {
                combination.value_types.remove(&CombinerKey::Symbol(*name));
            }
            combination
                .objects
                .retain(|existing| !objects_to_remove.contains(&existing.name));
        }

        combination
            .objects
            .push(CombinedObject::new(fq_class_name, symbol_type));

        return;
    }

    if let TAtomic::TScalar = atomic {
        combination.literal_strings.clear();
        combination.literal_ints.clear();
        combination.value_types.retain(|k, _| {
            !matches!(
                k,
                CombinerKey::String
                    | CombinerKey::Int
                    | CombinerKey::Bool
                    | CombinerKey::False
                    | CombinerKey::True
                    | CombinerKey::Float
                    | CombinerKey::Arraykey
                    | CombinerKey::Num
            )
        });

        combination
            .value_types
            .insert(CombinerKey::of(&atomic), atomic);
        return;
    }

    if let TAtomic::TArraykey { .. } = atomic {
        if combination.value_types.contains_key(&CombinerKey::Scalar) {
            return;
        }

        combination.literal_strings.clear();
        combination.literal_ints.clear();
        combination
            .value_types
            .retain(|k, _| !matches!(k, CombinerKey::String | CombinerKey::Int));

        combination
            .value_types
            .insert(CombinerKey::of(&atomic), atomic);
        return;
    }

    if let TAtomic::TNum = atomic {
        if combination.value_types.contains_key(&CombinerKey::Scalar) {
            return;
        }

        combination.literal_ints.clear();
        combination
            .value_types
            .retain(|k, _| !matches!(k, CombinerKey::Float | CombinerKey::Int));

        combination
            .value_types
            .insert(CombinerKey::of(&atomic), atomic);
        return;
    }

    if let TAtomic::TString
    | TAtomic::TLiteralString { .. }
    | TAtomic::TStringWithFlags(..)
    | TAtomic::TInt
    | TAtomic::TLiteralInt { .. } = atomic
        && (combination.value_types.contains_key(&CombinerKey::Arraykey)
            || combination.value_types.contains_key(&CombinerKey::Scalar))
    {
        return;
    }

    if let TAtomic::TFloat | TAtomic::TInt | TAtomic::TLiteralInt { .. } = atomic
        && (combination.value_types.contains_key(&CombinerKey::Num)
            || combination.value_types.contains_key(&CombinerKey::Scalar))
    {
        return;
    }

    if let TAtomic::TString = atomic {
        combination.literal_strings.clear();
        combination
            .value_types
            .insert(CombinerKey::of(&atomic), atomic);
        return;
    }

    if let TAtomic::TStringWithFlags(mut is_nonempty, is_nonspecific_literal) = atomic {
        if let Some(existing_string_type) = combination.value_types.get_mut(&CombinerKey::String) {
            if let TAtomic::TString = existing_string_type {
                return;
            }

            if let TAtomic::TStringWithFlags(existing_is_non_empty, existing_is_nonspecific) =
                existing_string_type
            {
                if *existing_is_non_empty == is_nonempty
                    && *existing_is_nonspecific == is_nonspecific_literal
                {
                    return;
                }

                *existing_string_type = TAtomic::TStringWithFlags(
                    *existing_is_non_empty && is_nonempty,
                    *existing_is_nonspecific && is_nonspecific_literal,
                );
            }
            return;
        }

        if is_nonempty && combination.literal_strings.iter().any(|v| v.is_empty()) {
            is_nonempty = false;
        }

        combination.value_types.insert(
            CombinerKey::String,
            if !is_nonempty && !is_nonspecific_literal {
                TAtomic::TString
            } else {
                TAtomic::TStringWithFlags(is_nonempty, is_nonspecific_literal)
            },
        );

        combination.literal_strings.clear();

        return;
    }

    if let TAtomic::TLiteralString { value, .. } = &atomic {
        if let Some(existing_string_type) = combination.value_types.get_mut(&CombinerKey::String) {
            match existing_string_type {
                TAtomic::TString => return,
                TAtomic::TStringWithFlags(is_nonempty, is_nonspecific_literal) => {
                    if value.is_empty() {
                        *is_nonempty = false;
                    }

                    if !*is_nonempty && !*is_nonspecific_literal {
                        *existing_string_type = TAtomic::TString;
                    }

                    return;
                }

                _ => (),
            }
        } else if combination.literal_strings.len() > 20 {
            combination.value_types.insert(
                CombinerKey::String,
                TAtomic::TStringWithFlags(
                    combination.literal_strings.iter().all(|s| !s.is_empty()),
                    true,
                ),
            );
            combination.literal_strings.clear();
        } else {
            combination.literal_strings.insert(value.clone());
        }

        return;
    }

    if let TAtomic::TInt = atomic {
        combination.literal_ints.clear();
        combination
            .value_types
            .insert(CombinerKey::of(&atomic), atomic);
        return;
    }

    if let TAtomic::TLiteralInt { value } = atomic {
        if let Some(existing_int_type) = combination.value_types.get(&CombinerKey::Int) {
            if let TAtomic::TInt = existing_int_type {
                return;
            }
        } else if combination.literal_ints.len() > 20 {
            combination.literal_ints.clear();
            combination
                .value_types
                .insert(CombinerKey::Int, TAtomic::TInt);
        } else {
            combination.literal_ints.insert(value);
        }

        return;
    }

    combination
        .value_types
        .insert(CombinerKey::of(&atomic), atomic);
}

fn adjust_key_value_dict_params(
    existing_value_param: &mut TUnion,
    entry_type: &Arc<TUnion>,
    codebase: &CodebaseInfo,
    overwrite_empty_array: bool,
    key: &DictKey,
    existing_key_param: &mut TUnion,
) {
    *existing_value_param = combine_union_types(
        existing_value_param,
        entry_type,
        codebase,
        overwrite_empty_array,
    );

    let new_key_type = wrap_atomic(match key {
        DictKey::Int(value) => TAtomic::TLiteralInt {
            value: *value as i64,
        },
        DictKey::String(value) => TAtomic::TLiteralString {
            value: value.clone(),
        },
        DictKey::Enum(a, b) => TAtomic::TEnumLiteralCase {
            enum_name: *a,
            member_name: *b,
            as_type: None,
            underlying_type: None,
        },
    });

    *existing_key_param = combine_union_types(
        existing_key_param,
        &new_key_type,
        codebase,
        overwrite_empty_array,
    );
}

fn get_combiner_key(name: &StrId, type_params: &[TUnion], codebase: &CodebaseInfo) -> String {
    let covariants = if let Some(classlike_storage) = codebase.classlike_infos.get(name) {
        &classlike_storage.generic_variance
    } else {
        return name.0.to_string();
    };

    let mut str = String::new();
    str += &name.0.to_string();
    str += "<";
    str += type_params
        .iter()
        .enumerate()
        .map(|(i, tunion)| {
            if let Some(Variance::Covariant) = covariants.get(&i) {
                "*".to_string()
            } else {
                tunion.get_key()
            }
        })
        .join(", ")
        .as_str();
    str += ">";
    str
}

fn merge_array_subtype(
    combination: &mut TypeCombination,
    fq_class_name: &StrId,
    codebase: &CodebaseInfo,
) {
    let fq_class_name_key = fq_class_name.0.to_string();
    let keyed_container_types = combination.object_type_params.get(&fq_class_name_key);
    // dict<string, Foo>|KeyedContainer<int, Bar> => KeyedContainer<string|int, Foo|Bar>
    if let Some(ref dict_types) = combination.dict_type_params {
        let container_key_type = if let Some((_, keyed_container_types)) = keyed_container_types {
            combine_union_types(
                keyed_container_types.first().unwrap(),
                &dict_types.0,
                codebase,
                false,
            )
        } else {
            dict_types.1.clone()
        };
        let container_value_type = if let Some((_, keyed_container_types)) = keyed_container_types {
            combine_union_types(
                keyed_container_types.get(1).unwrap(),
                &dict_types.1,
                codebase,
                false,
            )
        } else {
            dict_types.1.clone()
        };
        combination.object_type_params.insert(
            fq_class_name_key.clone(),
            (
                *fq_class_name,
                vec![container_key_type, container_value_type],
            ),
        );

        combination.dict_type_params = None;
        combination.has_dict = false;
    }
    // vec<Foo>|KeyedContainer<string, Bar> => Container<int|string, Foo|Bar>
    if let Some(ref value_param) = combination.vec_type_param {
        let keyed_container_types = combination.object_type_params.get(&fq_class_name_key);
        let container_key_type = if let Some((_, keyed_container_types)) = keyed_container_types {
            combine_union_types(
                keyed_container_types.first().unwrap(),
                &get_int(),
                codebase,
                false,
            )
        } else {
            get_int()
        };

        let container_value_type = if let Some((_, keyed_container_types)) = keyed_container_types {
            combine_union_types(
                keyed_container_types.get(1).unwrap(),
                value_param,
                codebase,
                false,
            )
        } else {
            value_param.clone()
        };
        combination.object_type_params.insert(
            fq_class_name_key.clone(),
            (
                *fq_class_name,
                vec![container_key_type, container_value_type],
            ),
        );

        combination.vec_type_param = None;
    }
}
