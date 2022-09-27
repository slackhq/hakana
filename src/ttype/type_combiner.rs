use std::sync::Arc;

use hakana_reflection_info::{
    codebase_info::CodebaseInfo,
    t_atomic::{DictKey, TAtomic},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    combine_union_types, get_int,
    type_combination::{self, TypeCombination},
};

pub fn combine(
    types: Vec<TAtomic>,
    codebase: Option<&CodebaseInfo>,
    overwrite_empty_array: bool,
) -> Vec<TAtomic> {
    if types.len() == 1 {
        return types;
    }

    let mut combination = type_combination::TypeCombination::new();

    for atomic in types {
        let result =
            scrape_type_properties(atomic, &mut combination, codebase, overwrite_empty_array);

        if let Some(result) = result {
            return result;
        }
    }

    if combination.nonnull_mixed && combination.value_types.contains_key("null") {
        return vec![TAtomic::TMixed];
    }

    if combination.falsy_mixed {
        if !combination.value_types.is_empty() {
            return vec![TAtomic::TMixed];
        }
        return vec![TAtomic::TFalsyMixed];
    } else if combination.truthy_mixed {
        if !combination.value_types.is_empty() {
            return vec![TAtomic::TMixed];
        }
        return vec![TAtomic::TTruthyMixed];
    } else if combination.nonnull_mixed {
        return vec![TAtomic::TNonnullMixed];
    } else if combination.any_mixed {
        return vec![TAtomic::TMixedAny];
    } else if combination.vanilla_mixed {
        return vec![TAtomic::TMixed];
    }

    if combination.is_simple() {
        if combination.value_types.contains_key("false") {
            return vec![TAtomic::TFalse];
        }

        if combination.value_types.contains_key("true") {
            return vec![TAtomic::TTrue];
        }

        return combination
            .value_types
            .into_iter()
            .map(|(_, a)| a)
            .collect();
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
            .insert("null".to_string(), TAtomic::TBool);
    }

    let mut new_types = Vec::new();

    if combination.has_dict {
        new_types.push(TAtomic::TDict {
            known_items: if combination.dict_entries.is_empty() {
                None
            } else {
                Some(combination.dict_entries)
            },
            params: combination.dict_type_params,
            non_empty: combination.dict_always_filled,
            shape_name: if combination
                .dict_name
                .clone()
                .unwrap_or("".to_string())
                .is_empty()
            {
                None
            } else {
                combination.dict_name.clone()
            },
        });
    }

    if let Some(vec_type_param) = combination.vec_type_param {
        new_types.push(TAtomic::TVec {
            known_items: if combination.vec_entries.is_empty() {
                None
            } else {
                Some(combination.vec_entries)
            },
            type_param: vec_type_param,
            non_empty: combination.vec_always_filled,
            known_count: None,
        });
    }

    if let Some(keyset_type_param) = combination.keyset_type_param {
        new_types.push(TAtomic::TKeyset {
            type_param: keyset_type_param,
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
            .map(|(_, a)| a)
            .collect::<Vec<TAtomic>>(),
    );
    new_types.extend(
        combination
            .literal_ints
            .into_iter()
            .map(|(_, a)| a)
            .collect::<Vec<TAtomic>>(),
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

    for enum_name in combination.enum_types {
        combination
            .value_types
            .insert(enum_name.clone(), TAtomic::TEnum { name: enum_name });
    }

    for (enum_name, values) in combination.enum_value_types {
        for value in values {
            combination.value_types.insert(
                format!("{}::{}", enum_name, value.0),
                TAtomic::TEnumLiteralCase {
                    enum_name: enum_name.clone(),
                    member_name: value.0,
                    constraint_type: value.1,
                },
            );
        }
    }

    let mut has_nothing = combination.value_types.contains_key("nothing");

    let combination_value_type_count = combination.value_types.len();

    for (_, atomic) in combination.value_types {
        let tc = if has_nothing { 1 } else { 0 };
        if atomic.is_mixed() {
            if combination.mixed_from_loop_isset.unwrap_or(false)
                && (combination_value_type_count > (tc + 1) || new_types.len() > tc)
            {
                continue;
            }
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

    return new_types;
}

fn scrape_type_properties(
    atomic: TAtomic,
    combination: &mut TypeCombination,
    codebase: Option<&CodebaseInfo>,
    overwrite_empty_array: bool,
) -> Option<Vec<TAtomic>> {
    match atomic {
        TAtomic::TMixed | TAtomic::TMixedAny => {
            combination.falsy_mixed = false;
            combination.truthy_mixed = false;
            combination.mixed_from_loop_isset = Some(false);
            combination.vanilla_mixed = true;

            if let TAtomic::TMixedAny = atomic {
                combination.any_mixed = true;
            }

            return None;
        }
        TAtomic::TMixedFromLoopIsset => {
            if combination.vanilla_mixed || combination.any_mixed {
                return None;
            }

            if let None = combination.mixed_from_loop_isset {
                combination.mixed_from_loop_isset = Some(true);
            }

            combination.value_types.insert("mixed".to_string(), atomic);
            return None;
        }
        TAtomic::TTruthyMixed | TAtomic::TFalsyMixed => {
            if combination.vanilla_mixed || combination.any_mixed {
                return None;
            }

            combination.mixed_from_loop_isset = Some(false);

            if matches!(atomic, TAtomic::TTruthyMixed) {
                combination.truthy_mixed = true;

                if combination.falsy_mixed {
                    return Some(vec![TAtomic::TMixed]);
                }
            } else if matches!(atomic, TAtomic::TFalsyMixed) {
                combination.falsy_mixed = true;

                if combination.truthy_mixed {
                    return Some(vec![TAtomic::TMixed]);
                }
            }

            return None;
        }
        TAtomic::TNonnullMixed => {
            if combination.vanilla_mixed || combination.any_mixed {
                return None;
            }

            if combination.falsy_mixed {
                return Some(vec![TAtomic::TMixed]);
            }

            combination.mixed_from_loop_isset = Some(false);
            combination.nonnull_mixed = true;

            return None;
        }
        _ => (),
    }

    // bool|false = bool
    if let TAtomic::TFalse { .. } | TAtomic::TTrue { .. } = atomic {
        if combination.value_types.contains_key("bool") {
            return None;
        }
    }

    // false|bool = bool
    if let TAtomic::TBool { .. } = atomic {
        combination.value_types.remove("false");
        combination.value_types.remove("true");
    }

    let type_key = if (matches!(atomic, TAtomic::TVec { .. })
        || matches!(atomic, TAtomic::TDict { .. }))
        && (combination.object_type_params.contains_key("HH\\Container")
            || combination
                .object_type_params
                .contains_key("HH\\KeyedContainer"))
    {
        if combination.object_type_params.contains_key("HH\\Container") {
            "HH\\Container".to_string()
        } else {
            "HH\\KeyedContainer".to_string()
        }
    } else {
        if let Some(codebase) = codebase {
            atomic.get_combiner_key(codebase)
        } else {
            atomic.get_key()
        }
    };

    if let TAtomic::TVec {
        ref type_param,
        non_empty,
        known_count,
        ref known_items,
        ..
    } = atomic
    {
        let mut had_previous_param = false;
        combination.vec_type_param = if let Some(ref existing_type) = combination.vec_type_param {
            had_previous_param = true;
            Some(combine_union_types(
                &existing_type,
                &type_param,
                codebase,
                overwrite_empty_array,
            ))
        } else {
            Some(type_param.clone())
        };

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
                let existing_type = combination.vec_entries.get(&candidate_item_offset);

                let new_type_possibly_undefined;
                let new_type = if let Some((eu, existing_type)) = existing_type {
                    new_type_possibly_undefined = *eu || *cu;
                    combine_union_types(
                        existing_type,
                        &candidate_item_type,
                        codebase,
                        overwrite_empty_array,
                    )
                } else {
                    let new_type = candidate_item_type.clone();
                    new_type_possibly_undefined = has_existing_entries || *cu;

                    new_type
                };

                combination.vec_entries.insert(
                    *candidate_item_offset,
                    (new_type_possibly_undefined, new_type),
                );

                possibly_undefined_entries.remove(&candidate_item_offset);

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
            for (_, (tu, _)) in combination.vec_entries.iter_mut() {
                *tu = true;
            }
        }

        return None;
    }

    if let TAtomic::TKeyset { ref type_param, .. } = atomic {
        combination.keyset_type_param =
            if let Some(ref existing_type) = combination.keyset_type_param {
                Some(combine_union_types(
                    &existing_type,
                    &type_param,
                    codebase,
                    overwrite_empty_array,
                ))
            } else {
                Some(type_param.clone())
            };

        return None;
    }

    if let TAtomic::TDict {
        ref params,
        ref known_items,
        non_empty,
        shape_name,
        ..
    } = atomic
    {
        let had_previous_dict = combination.has_dict;
        combination.has_dict = true;

        combination.dict_type_params = match (&combination.dict_type_params, params) {
            (None, None) => None,
            (Some(existing_types), None) => Some(existing_types.clone()),
            (None, Some(params)) => Some(params.clone()),
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

        if non_empty {
            combination.dict_sometimes_filled = true;
        } else {
            combination.dict_always_filled = false;
        }

        if let Some(shape_name) = &shape_name {
            if let Some(ref mut existing_name) = combination.dict_name {
                if existing_name != shape_name {
                    *existing_name = "".to_string();
                }
            } else {
                combination.dict_name = Some(shape_name.clone());
            }
        } else {
            combination.dict_name = Some("".to_string());
        }

        if let Some(known_items) = known_items {
            let has_existing_entries = !combination.dict_entries.is_empty() || had_previous_dict;
            let mut possibly_undefined_entries: FxHashSet<DictKey> =
                combination.dict_entries.keys().cloned().collect();

            let mut has_defined_keys = false;

            for (candidate_item_name, (cu, candidate_item_type)) in known_items {
                let existing_type = combination.dict_entries.get(candidate_item_name);

                let new_type_possibly_undefined;
                let new_type = if let Some((eu, existing_type)) = existing_type {
                    new_type_possibly_undefined = *eu || *cu;
                    if candidate_item_type != existing_type {
                        Arc::new(combine_union_types(
                            existing_type,
                            candidate_item_type,
                            codebase,
                            overwrite_empty_array,
                        ))
                    } else {
                        existing_type.clone()
                    }
                } else {
                    let new_type = candidate_item_type.clone();
                    new_type_possibly_undefined = has_existing_entries || *cu;

                    new_type
                };

                combination.dict_entries.insert(
                    candidate_item_name.clone(),
                    (new_type_possibly_undefined, new_type),
                );

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
            for (_, (tu, _)) in combination.dict_entries.iter_mut() {
                *tu = true;
            }
        }

        return None;
    }

    // this probably won't ever happen, but the object top type
    // can eliminate variants
    if let TAtomic::TObject = atomic {
        combination.has_object_top_type = true;
        combination
            .value_types
            .retain(|_, t| !matches!(t, TAtomic::TNamedObject { .. }));
        combination.value_types.insert(type_key, atomic);

        return None;
    }

    // TODO (maybe) add support for Vector, Map etc.
    if let TAtomic::TNamedObject {
        ref name, is_this, ..
    } = atomic
    {
        if let Some(object_static) = combination.object_static.get(name) {
            if *object_static && !is_this {
                combination.object_static.insert(name.clone(), false);
            }
        } else {
            combination.object_static.insert(name.clone(), is_this);
        }
    }

    if let TAtomic::TNamedObject {
        name: ref fq_class_name,
        type_params: Some(type_params),
        ..
    } = atomic
    {
        if fq_class_name == "HH\\Container" {
            // dict<string, Foo>|Container<Bar> => Container<Foo|Bar>
            if let Some(ref dict_types) = combination.dict_type_params {
                let container_value_type = if let Some((_, container_types)) =
                    combination.object_type_params.get("HH\\Container")
                {
                    combine_union_types(
                        container_types.get(0).unwrap(),
                        &dict_types.1,
                        codebase,
                        false,
                    )
                } else {
                    dict_types.1.clone()
                };
                combination.object_type_params.insert(
                    "HH\\Container".to_string(),
                    (fq_class_name.clone(), vec![container_value_type]),
                );

                combination.dict_type_params = None;
                combination.has_dict = false;
            }

            // vec<Foo>|Container<Bar> => Container<Foo|Bar>
            if let Some(ref value_param) = combination.vec_type_param {
                let container_value_type = if let Some((_, container_types)) =
                    combination.object_type_params.get("HH\\Container")
                {
                    combine_union_types(
                        container_types.get(0).unwrap(),
                        value_param,
                        codebase,
                        false,
                    )
                } else {
                    value_param.clone()
                };
                combination.object_type_params.insert(
                    "HH\\Container".to_string(),
                    (fq_class_name.clone(), vec![container_value_type]),
                );

                combination.vec_type_param = None;
            }

            // KeyedContainer<string, Foo>|Container<Bar> = Container<Foo|Bar>
            if let Some((_, keyed_container_types)) =
                combination.object_type_params.get("HH\\KeyedContainer")
            {
                let container_value_type = if let Some((_, container_types)) =
                    combination.object_type_params.get("HH\\Container")
                {
                    combine_union_types(
                        container_types.get(0).unwrap(),
                        keyed_container_types.get(1).unwrap(),
                        codebase,
                        false,
                    )
                } else {
                    keyed_container_types.get(1).unwrap().clone()
                };
                combination.object_type_params.insert(
                    "HH\\Container".to_string(),
                    (fq_class_name.clone(), vec![container_value_type]),
                );

                combination.object_type_params.remove("HH\\KeyedContainer");
            }
        }

        if fq_class_name == "HH\\KeyedContainer" {
            let keyed_container_types = combination.object_type_params.get("HH\\KeyedContainer");
            // dict<string, Foo>|KeyedContainer<int, Bar> => KeyedContainer<string|int, Foo|Bar>
            if let Some(ref dict_types) = combination.dict_type_params {
                let container_key_type =
                    if let Some((_, keyed_container_types)) = keyed_container_types {
                        combine_union_types(
                            keyed_container_types.get(0).unwrap(),
                            &dict_types.0,
                            codebase,
                            false,
                        )
                    } else {
                        dict_types.1.clone()
                    };
                let container_value_type =
                    if let Some((_, keyed_container_types)) = keyed_container_types {
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
                    "HH\\KeyedContainer".to_string(),
                    (
                        "HH\\KeyedContainer".to_string(),
                        vec![container_key_type, container_value_type],
                    ),
                );

                combination.dict_type_params = None;
                combination.has_dict = false;
            }

            // vec<Foo>|KeyedContainer<string, Bar> => Container<int|string, Foo|Bar>
            if let Some(ref value_param) = combination.vec_type_param {
                let keyed_container_types =
                    combination.object_type_params.get("HH\\KeyedContainer");
                let container_key_type =
                    if let Some((_, keyed_container_types)) = keyed_container_types {
                        combine_union_types(
                            keyed_container_types.get(0).unwrap(),
                            &get_int(),
                            codebase,
                            false,
                        )
                    } else {
                        get_int()
                    };

                let container_value_type =
                    if let Some((_, keyed_container_types)) = keyed_container_types {
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
                    "HH\\KeyedContainer".to_string(),
                    (
                        "HH\\KeyedContainer".to_string(),
                        vec![container_key_type, container_value_type],
                    ),
                );

                combination.vec_type_param = None;
            }
        }

        if let Some((_, ref existing_type_params)) = combination.object_type_params.get(&type_key) {
            let mut new_type_params = Vec::new();
            let mut i = 0;
            for type_param in type_params {
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

                i += 1;
            }

            combination
                .object_type_params
                .insert(type_key, (fq_class_name.clone(), new_type_params));
        } else {
            combination
                .object_type_params
                .insert(type_key, (fq_class_name.clone(), type_params));
        }

        return None;
    }

    if let TAtomic::TEnumLiteralCase {
        enum_name,
        member_name,
        constraint_type,
    } = atomic
    {
        if combination.enum_types.contains(&enum_name) {
            return None;
        }

        combination
            .enum_value_types
            .entry(enum_name)
            .or_insert_with(FxHashMap::default)
            .insert(member_name, constraint_type);

        return None;
    }

    if let TAtomic::TEnum { name, .. } = atomic {
        combination.enum_value_types.remove(&name);
        combination.enum_types.insert(name);

        return None;
    }

    if let TAtomic::TNamedObject {
        name: ref fq_class_name,
        type_params: None,
        ..
    } = atomic
    {
        if !combination.has_object_top_type {
            if combination.value_types.contains_key(&type_key) {
                return None;
            }
        } else {
            return None;
        }

        if let None = codebase {
            combination.value_types.insert(type_key, atomic);
            return None;
        }

        let codebase = codebase.unwrap();

        if !codebase.class_or_interface_or_enum_exists(&type_key) {
            combination.value_types.insert(type_key, atomic);

            return None;
        }

        let is_class = codebase.class_exists(&type_key);

        let mut types_to_remove = Vec::new();
        for (key, named_object) in combination.value_types.iter() {
            if let TAtomic::TNamedObject {
                name: existing_name,
                ..
            } = &named_object
            {
                if codebase.class_exists(existing_name) {
                    // remove subclasses
                    if codebase.class_extends_or_implements(existing_name, fq_class_name) {
                        types_to_remove.push(key.clone());
                        continue;
                    }

                    if is_class {
                        // if covered by a parent class
                        if codebase.class_extends(fq_class_name, existing_name) {
                            return None;
                        }
                    }
                } else {
                    if codebase.interface_extends(existing_name, fq_class_name) {
                        types_to_remove.push(existing_name.clone());
                        continue;
                    }

                    if is_class {
                        // skip if interface is implemented by fq_class_name
                        if codebase.class_implements(fq_class_name, existing_name) {
                            return None;
                        }
                    } else {
                        if codebase.interface_extends(fq_class_name, existing_name) {
                            return None;
                        }
                    }
                }
            }
        }

        combination.value_types.insert(type_key, atomic);

        for type_key in types_to_remove {
            combination.value_types.remove(&type_key);
        }

        return None;
    }

    if let TAtomic::TScalar { .. } = atomic {
        combination.literal_strings = FxHashMap::default();
        combination.literal_ints = FxHashMap::default();
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

        combination.value_types.insert(type_key, atomic);
        return None;
    }

    if let TAtomic::TArraykey { .. } = atomic {
        if combination.value_types.contains_key("scalar") {
            return None;
        }

        combination.literal_strings = FxHashMap::default();
        combination.literal_ints = FxHashMap::default();
        combination
            .value_types
            .retain(|k, _| k != "string" && k != "int");

        combination.value_types.insert(type_key, atomic);
        return None;
    }

    if let TAtomic::TNum { .. } = atomic {
        if combination.value_types.contains_key("scalar") {
            return None;
        }

        combination.literal_ints = FxHashMap::default();
        combination
            .value_types
            .retain(|k, _| k != "float" && k != "int");

        combination.value_types.insert(type_key, atomic);
        return None;
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
            return None;
        }
    }

    if let TAtomic::TFloat | TAtomic::TInt | TAtomic::TLiteralInt { .. } = atomic {
        if combination.value_types.contains_key("num")
            || combination.value_types.contains_key("scalar")
        {
            return None;
        }
    }

    if let TAtomic::TString { .. } = atomic {
        combination.literal_strings = FxHashMap::default();
        combination.value_types.insert(type_key, atomic);
        return None;
    }

    if let TAtomic::TStringWithFlags(mut is_truthy, mut is_nonempty, is_nonspecific_literal) =
        atomic
    {
        if let Some(existing_string_type) = combination.value_types.get_mut("string") {
            if let TAtomic::TString = existing_string_type {
                return None;
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
                    return None;
                }

                *existing_string_type = TAtomic::TStringWithFlags(
                    *existing_is_truthy && is_truthy,
                    *existing_is_non_empty && is_nonempty,
                    *existing_is_nonspecific && is_nonspecific_literal,
                );
            }
            return None;
        }

        if is_truthy || is_nonempty {
            for (_, literal_string_type) in &combination.literal_strings {
                if let TAtomic::TLiteralString { value, .. } = literal_string_type {
                    if value == "" {
                        is_nonempty = false;
                        is_truthy = false;
                        break;
                    } else if value == "0" {
                        is_truthy = false;
                    }
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

        combination.literal_strings = FxHashMap::default();

        return None;
    }

    if let TAtomic::TLiteralString { value, .. } = &atomic {
        if let Some(existing_string_type) = combination.value_types.get_mut("string") {
            match existing_string_type {
                TAtomic::TString => return None,
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

                    return None;
                }

                _ => (),
            }
        } else if combination.literal_strings.len() > 20 {
            combination.literal_strings = FxHashMap::default();
            combination.value_types.insert(
                "string".to_string(),
                TAtomic::TStringWithFlags(true, false, true),
            );
        } else {
            combination.literal_strings.insert(type_key, atomic);
        }

        return None;
    }

    if let TAtomic::TInt = atomic {
        combination.literal_ints = FxHashMap::default();
        combination.value_types.insert(type_key, atomic);
        return None;
    }

    if let TAtomic::TLiteralInt { .. } = atomic {
        if let Some(existing_int_type) = combination.value_types.get("int") {
            if let TAtomic::TInt = existing_int_type {
                return None;
            }
        } else if combination.literal_ints.len() > 20 {
            combination.literal_ints = FxHashMap::default();
            combination
                .value_types
                .insert("int".to_string(), TAtomic::TInt);
        } else {
            combination.literal_ints.insert(type_key, atomic);
        }

        return None;
    }

    combination.value_types.insert(type_key, atomic);

    None
}
