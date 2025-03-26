use std::{collections::BTreeMap, sync::Arc};

use crate::{
    classlike_info::Variance,
    codebase_info::{symbols::SymbolKind, CodebaseInfo},
    t_atomic::{DictKey, TAtomic, TDict},
    t_union::TUnion,
};
use hakana_str::StrId;
use indexmap::IndexMap;
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ttype::{
    combine_union_types,
    comparison::{object_type_comparator, type_comparison_result::TypeComparisonResult},
    get_int,
    type_combination::{self, TypeCombination},
    wrap_atomic,
};

pub fn combine(
    types: Vec<TAtomic>,
    codebase: &CodebaseInfo,
    overwrite_empty_array: bool,
) -> Vec<TAtomic> {
    if types.len() == 1 {
        return types;
    }

    let mut combination = type_combination::TypeCombination::new();

    for atomic in types {
        scrape_type_properties(atomic, &mut combination, codebase, overwrite_empty_array);
    }

    if combination.falsy_mixed.unwrap_or(false)
        || combination.nonnull_mixed.unwrap_or(false)
        || combination.any_mixed
        || combination.truthy_mixed.unwrap_or(false)
    {
        return vec![TAtomic::TMixedWithFlags(
            combination.any_mixed,
            combination.truthy_mixed.unwrap_or(false),
            combination.falsy_mixed.unwrap_or(false),
            combination.nonnull_mixed.unwrap_or(false),
        )];
    } else if combination.has_mixed {
        return vec![TAtomic::TMixed];
    }

    if combination.is_simple() {
        if combination.value_types.contains_key("false") {
            return vec![TAtomic::TFalse];
        }

        if combination.value_types.contains_key("true") {
            return vec![TAtomic::TTrue];
        }

        return combination.value_types.into_values().collect();
    }

    if combination.value_types.contains_key("void") {
        combination.value_types.remove("void");

        if combination.value_types.contains_key("null") {
            combination
                .value_types
                .insert("null".to_string(), TAtomic::TNull);
        }
    }

    if combination.value_types.contains_key("false") && combination.value_types.contains_key("true")
    {
        combination.value_types.remove("false");
        combination.value_types.remove("true");
        combination
            .value_types
            .insert("bool".to_string(), TAtomic::TBool);
    }

    let mut new_types = Vec::new();

    if combination.has_dict {
        new_types.push(TAtomic::TDict(TDict {
            known_items: if combination.dict_entries.is_empty() {
                None
            } else {
                Some(combination.dict_entries)
            },
            params: if let Some((k, v)) = combination.dict_type_params {
                Some((Box::new(k), Box::new(v)))
            } else {
                None
            },
            non_empty: combination.dict_always_filled,
            shape_name: combination.dict_alias_name.unwrap_or(None),
        }));
    }

    if let Some(vec_type_param) = combination.vec_type_param {
        new_types.push(TAtomic::TVec {
            known_items: if combination.vec_entries.is_empty() {
                None
            } else {
                Some(combination.vec_entries)
            },
            type_param: Box::new(vec_type_param),
            non_empty: combination.vec_always_filled,
            known_count: None,
        });
    }

    if let Some(keyset_type_param) = combination.keyset_type_param {
        new_types.push(TAtomic::TKeyset {
            type_param: Box::new(keyset_type_param),
        });
    }

    if let Some(type_param) = combination.awaitable_param {
        new_types.push(TAtomic::TAwaitable {
            value: Box::new(type_param),
        });
    }

    for (_, (generic_type, generic_type_params)) in combination.object_type_params {
        let generic_object = TAtomic::TNamedObject {
            is_this: *combination
                .object_static
                .get(&generic_type)
                .unwrap_or(&false),
            name: generic_type,
            type_params: Some(generic_type_params),
            extra_types: None,
            remapped_params: false,
        };

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

    if combination.value_types.contains_key("string")
        && combination.value_types.contains_key("float")
        && combination.value_types.contains_key("int")
        && combination.value_types.contains_key("bool")
    {
        combination.value_types.remove("string");
        combination.value_types.remove("float");
        combination.value_types.remove("int");
        combination.value_types.remove("bool");
        new_types.push(TAtomic::TScalar {});
    }

    for (enum_name, (as_type, underlying_type)) in combination.enum_types {
        combination.value_types.insert(
            enum_name.0.to_string(),
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
                format!("{}::{}", enum_name.0, member_name.0),
                TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                    as_type,
                    underlying_type,
                },
            );
        }
    }

    let mut has_nothing = combination.value_types.contains_key("nothing");

    let combination_value_type_count = combination.value_types.len();

    for (_, atomic) in combination.value_types {
        let tc = if has_nothing { 1 } else { 0 };
        if atomic.is_mixed()
            && combination.mixed_from_loop_isset.unwrap_or(false)
            && (combination_value_type_count > (tc + 1) || new_types.len() > tc)
        {
            continue;
        }

        if let TAtomic::TNothing = atomic {
            if combination_value_type_count > 1 || !new_types.is_empty() {
                has_nothing = true;
                continue;
            }
        }

        new_types.push(atomic);
    }

    if new_types.is_empty() {
        if !has_nothing {
            panic!();
        }

        return vec![TAtomic::TNothing];
    }

    new_types
}

fn scrape_type_properties(
    atomic: TAtomic,
    combination: &mut TypeCombination,
    codebase: &CodebaseInfo,
    overwrite_empty_array: bool,
) {
    match atomic {
        TAtomic::TMixed => {
            combination.falsy_mixed = Some(false);
            combination.truthy_mixed = Some(false);
            combination.mixed_from_loop_isset = Some(false);
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

            combination.value_types.insert("mixed".to_string(), atomic);
            return;
        }
        TAtomic::TMixedWithFlags(any, truthy_mixed, falsy_mixed, nonnull_mixed) => {
            combination.has_mixed = true;

            if any {
                combination.any_mixed = true;
            }

            if truthy_mixed {
                if combination.vanilla_mixed {
                    return;
                }

                combination.mixed_from_loop_isset = Some(false);

                if combination.falsy_mixed.unwrap_or(false) {
                    combination.vanilla_mixed = true;
                    combination.falsy_mixed = Some(false);
                    return;
                }

                if combination.truthy_mixed.is_some() {
                    return;
                }

                for existing_value_type in combination.value_types.values() {
                    if !existing_value_type.is_truthy() {
                        combination.vanilla_mixed = true;
                        return;
                    }
                }

                combination.truthy_mixed = Some(true);
            } else {
                combination.truthy_mixed = Some(false);
            }

            if falsy_mixed {
                if combination.vanilla_mixed {
                    return;
                }

                combination.mixed_from_loop_isset = Some(false);

                if combination.truthy_mixed.unwrap_or(false) {
                    combination.vanilla_mixed = true;
                    combination.truthy_mixed = Some(false);
                    return;
                }

                if combination.falsy_mixed.is_some() {
                    return;
                }

                for existing_value_type in combination.value_types.values() {
                    if !existing_value_type.is_falsy() {
                        combination.vanilla_mixed = true;
                        return;
                    }
                }

                combination.falsy_mixed = Some(true);
            } else {
                combination.falsy_mixed = Some(false);
            }

            if nonnull_mixed {
                if combination.vanilla_mixed {
                    return;
                }

                combination.mixed_from_loop_isset = Some(false);

                if combination.value_types.contains_key("null") {
                    combination.vanilla_mixed = true;
                    return;
                }

                if combination.falsy_mixed.unwrap_or(false) {
                    combination.falsy_mixed = Some(false);
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

    if combination.falsy_mixed.unwrap_or(false) {
        if !atomic.is_falsy() {
            combination.falsy_mixed = Some(false);
            combination.vanilla_mixed = true;
        }

        return;
    } else if combination.truthy_mixed.unwrap_or(false) {
        if !atomic.is_truthy() {
            combination.truthy_mixed = Some(false);
            combination.vanilla_mixed = true;
        }

        return;
    } else if combination.nonnull_mixed.unwrap_or(false) {
        if let TAtomic::TNull = atomic {
            combination.nonnull_mixed = Some(false);
            combination.vanilla_mixed = true;
        }

        return;
    } else if combination.has_mixed {
        return;
    }

    // bool|false = bool
    if let TAtomic::TFalse { .. } | TAtomic::TTrue { .. } = atomic {
        if combination.value_types.contains_key("bool") {
            return;
        }
    }

    // false|bool = bool
    if let TAtomic::TBool { .. } = atomic {
        combination.value_types.remove("false");
        combination.value_types.remove("true");
    }

    if let TAtomic::TVec {
        ref type_param,
        non_empty,
        known_count,
        ref known_items,
        ..
    } = atomic
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

            combination.vec_sometimes_filled = true;
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

    if let TAtomic::TKeyset { ref type_param, .. } = atomic {
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
        ..
    }) = atomic
    {
        let had_previous_dict = combination.has_dict;
        combination.has_dict = true;

        if non_empty {
            combination.dict_sometimes_filled = true;
        } else {
            combination.dict_always_filled = false;
        }

        if let Some(shape_name) = &shape_name {
            if let Some(ref mut existing_name) = combination.dict_alias_name {
                if let Some(existing_name_inner) = existing_name {
                    if existing_name_inner != shape_name {
                        *existing_name = None;
                    }
                }
            } else {
                combination.dict_alias_name = Some(Some(*shape_name));
            }
        } else {
            combination.dict_alias_name = Some(None);
        }

        if let Some(known_items) = known_items {
            let has_existing_entries = !combination.dict_entries.is_empty() || had_previous_dict;
            let mut possibly_undefined_entries = combination
                .dict_entries
                .keys()
                .cloned()
                .collect::<FxHashSet<_>>();

            let mut has_defined_keys = false;

            for (candidate_item_name, (cu, candidate_item_type)) in known_items {
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
        combination.value_types.insert(atomic.get_key(), atomic);

        return;
    }

    // TODO (maybe) add support for Vector, Map etc.
    if let TAtomic::TNamedObject {
        ref name, is_this, ..
    } = atomic
    {
        if let Some(object_static) = combination.object_static.get(name) {
            if *object_static && !is_this {
                combination.object_static.insert(*name, false);
            }
        } else {
            combination.object_static.insert(*name, is_this);
        }
    }

    if let TAtomic::TNamedObject {
        name: ref fq_class_name,
        type_params: Some(type_params),
        ..
    } = atomic
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

        if let Some((_, ref existing_type_params)) =
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
        } else {
            if let Some(enum_storage) = codebase.classlike_infos.get(&enum_name) {
                combination.enum_value_types.insert(
                    enum_name,
                    (
                        enum_storage.constants.len(),
                        FxHashMap::from_iter([(member_name, (as_type, underlying_type))]),
                    ),
                );
            }
        }

        if let Some((as_type, underlying_type)) = matched_len_constraint {
            combination.enum_value_types.remove(&enum_name);
            combination.value_types.insert(
                enum_name.0.to_string(),
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

    if let TAtomic::TNamedObject {
        name: ref fq_class_name,
        type_params: None,
        ref extra_types,
        ..
    } = atomic
    {
        if !combination.has_object_top_type {
            if combination.value_types.contains_key(&atomic.get_key()) {
                return;
            }
        } else {
            return;
        }

        let symbol_type = if let Some(symbol_type) = codebase.symbols.all.get(fq_class_name) {
            symbol_type
        } else {
            combination.value_types.insert(atomic.get_key(), atomic);
            return;
        };

        if !matches!(
            symbol_type,
            SymbolKind::EnumClass | SymbolKind::Class | SymbolKind::Enum | SymbolKind::Interface
        ) {
            combination.value_types.insert(atomic.get_key(), atomic);
            return;
        }

        let is_class = matches!(symbol_type, SymbolKind::EnumClass | SymbolKind::Class);
        let is_interface = matches!(symbol_type, SymbolKind::Interface);

        let mut types_to_remove = Vec::new();

        for (key, existing_type) in &combination.value_types {
            if let TAtomic::TNamedObject {
                name: existing_name,
                extra_types: existing_extra_types,
                ..
            } = &existing_type
            {
                if extra_types.is_some() || existing_extra_types.is_some() {
                    if object_type_comparator::is_shallowly_contained_by(
                        codebase,
                        existing_type,
                        &atomic,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) {
                        types_to_remove.push(existing_name.0.to_string());
                        continue;
                    }

                    if object_type_comparator::is_shallowly_contained_by(
                        codebase,
                        &atomic,
                        existing_type,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) {
                        return;
                    }

                    continue;
                }

                let existing_symbol_type =
                    if let Some(symbol_type) = codebase.symbols.all.get(existing_name) {
                        symbol_type
                    } else {
                        continue;
                    };

                if matches!(
                    existing_symbol_type,
                    SymbolKind::EnumClass | SymbolKind::Class
                ) {
                    // remove subclasses
                    if codebase.class_extends_or_implements(existing_name, fq_class_name) {
                        types_to_remove.push(key.clone());
                        continue;
                    }

                    if is_class {
                        // if covered by a parent class
                        if codebase.class_or_trait_extends(fq_class_name, existing_name) {
                            return;
                        }
                    } else if is_interface {
                        // if covered by a parent class
                        if codebase.interface_extends(fq_class_name, existing_name) {
                            return;
                        }
                    }
                } else if matches!(existing_symbol_type, SymbolKind::Interface) {
                    if codebase.interface_extends(existing_name, fq_class_name) {
                        types_to_remove.push(existing_name.0.to_string());
                        continue;
                    }

                    if is_class {
                        // skip if interface is implemented by fq_class_name
                        if codebase.class_or_trait_implements(fq_class_name, existing_name) {
                            return;
                        }
                    } else if is_interface
                        && codebase.interface_extends(fq_class_name, existing_name)
                    {
                        return;
                    }
                }
            }
        }

        combination.value_types.insert(atomic.get_key(), atomic);

        for type_key in types_to_remove {
            combination.value_types.remove(&type_key);
        }

        return;
    }

    if let TAtomic::TScalar { .. } = atomic {
        combination.literal_strings = FxHashSet::default();
        combination.literal_ints = FxHashSet::default();
        combination.value_types.retain(|k, _| {
            k != "string"
                && k != "int"
                && k != "bool"
                && k != "false"
                && k != "true"
                && k != "float"
                && k != "arraykey"
                && k != "num"
        });

        combination.value_types.insert(atomic.get_key(), atomic);
        return;
    }

    if let TAtomic::TArraykey { .. } = atomic {
        if combination.value_types.contains_key("scalar") {
            return;
        }

        combination.literal_strings = FxHashSet::default();
        combination.literal_ints = FxHashSet::default();
        combination
            .value_types
            .retain(|k, _| k != "string" && k != "int");

        combination.value_types.insert(atomic.get_key(), atomic);
        return;
    }

    if let TAtomic::TNum { .. } = atomic {
        if combination.value_types.contains_key("scalar") {
            return;
        }

        combination.literal_ints = FxHashSet::default();
        combination
            .value_types
            .retain(|k, _| k != "float" && k != "int");

        combination.value_types.insert(atomic.get_key(), atomic);
        return;
    }

    if let TAtomic::TString { .. }
    | TAtomic::TLiteralString { .. }
    | TAtomic::TStringWithFlags(..)
    | TAtomic::TInt
    | TAtomic::TLiteralInt { .. } = atomic
    {
        if combination.value_types.contains_key("arraykey")
            || combination.value_types.contains_key("scalar")
        {
            return;
        }
    }

    if let TAtomic::TFloat | TAtomic::TInt | TAtomic::TLiteralInt { .. } = atomic {
        if combination.value_types.contains_key("num")
            || combination.value_types.contains_key("scalar")
        {
            return;
        }
    }

    if let TAtomic::TString { .. } = atomic {
        combination.literal_strings = FxHashSet::default();
        combination.value_types.insert(atomic.get_key(), atomic);
        return;
    }

    if let TAtomic::TStringWithFlags(mut is_truthy, mut is_nonempty, is_nonspecific_literal) =
        atomic
    {
        if let Some(existing_string_type) = combination.value_types.get_mut("string") {
            if let TAtomic::TString = existing_string_type {
                return;
            }

            if let TAtomic::TStringWithFlags(
                existing_is_truthy,
                existing_is_non_empty,
                existing_is_nonspecific,
            ) = existing_string_type
            {
                if *existing_is_truthy == is_truthy
                    && *existing_is_non_empty == is_nonempty
                    && *existing_is_nonspecific == is_nonspecific_literal
                {
                    return;
                }

                *existing_string_type = TAtomic::TStringWithFlags(
                    *existing_is_truthy && is_truthy,
                    *existing_is_non_empty && is_nonempty,
                    *existing_is_nonspecific && is_nonspecific_literal,
                );
            }
            return;
        }

        if is_truthy || is_nonempty {
            for value in &combination.literal_strings {
                if value.is_empty() {
                    is_nonempty = false;
                    is_truthy = false;
                    break;
                } else if value == "0" {
                    is_truthy = false;
                }
            }
        }

        combination.value_types.insert(
            "string".to_string(),
            if !is_truthy && !is_nonempty && !is_nonspecific_literal {
                TAtomic::TString
            } else {
                TAtomic::TStringWithFlags(is_truthy, is_nonempty, is_nonspecific_literal)
            },
        );

        combination.literal_strings = FxHashSet::default();

        return;
    }

    if let TAtomic::TLiteralString { value, .. } = &atomic {
        if let Some(existing_string_type) = combination.value_types.get_mut("string") {
            match existing_string_type {
                TAtomic::TString => return,
                TAtomic::TStringWithFlags(is_truthy, is_nonempty, is_nonspecific_literal) => {
                    if value == "" {
                        *is_truthy = false;
                        *is_nonempty = false;
                    } else if value == "0" {
                        *is_truthy = false;
                    }

                    if !*is_truthy && !*is_nonempty && !*is_nonspecific_literal {
                        *existing_string_type = TAtomic::TString;
                    }

                    return;
                }

                _ => (),
            }
        } else if combination.literal_strings.len() > 20 {
            combination.value_types.insert(
                "string".to_string(),
                TAtomic::TStringWithFlags(
                    combination
                        .literal_strings
                        .iter()
                        .all(|s| s != "" && s != "0"),
                    combination.literal_strings.iter().all(|s| s != ""),
                    true,
                ),
            );
            combination.literal_strings = FxHashSet::default();
        } else {
            combination.literal_strings.insert(value.clone());
        }

        return;
    }

    if let TAtomic::TInt = atomic {
        combination.literal_ints = FxHashSet::default();
        combination.value_types.insert(atomic.get_key(), atomic);
        return;
    }

    if let TAtomic::TLiteralInt { value } = atomic {
        if let Some(existing_int_type) = combination.value_types.get("int") {
            if let TAtomic::TInt = existing_int_type {
                return;
            }
        } else if combination.literal_ints.len() > 20 {
            combination.literal_ints = FxHashSet::default();
            combination
                .value_types
                .insert("int".to_string(), TAtomic::TInt);
        } else {
            combination.literal_ints.insert(value);
        }

        return;
    }

    combination.value_types.insert(atomic.get_key(), atomic);
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
