use hakana_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, sync::Arc};

use crate::{
    code_location::FilePath,
    codebase_info::CodebaseInfo,
    data_flow::node::DataFlowNode,
    t_atomic::{DictKey, TAtomic, TDict, TVec},
    t_union::TUnion,
    type_resolution::TypeResolutionContext,
};
use comparison::{
    atomic_type_comparator::{self, expand_constant_value},
    type_comparison_result::TypeComparisonResult,
};
use itertools::Itertools;
use type_combiner::combine;

pub mod comparison;
pub mod template;
mod type_combination;
pub mod type_combiner;
pub mod type_expander;

#[inline]
pub fn wrap_atomic(tinner: TAtomic) -> TUnion {
    TUnion::new(vec![tinner])
}

#[inline]
pub fn get_int() -> TUnion {
    wrap_atomic(TAtomic::TInt)
}

#[inline]
pub fn get_literal_int(value: i64) -> TUnion {
    wrap_atomic(TAtomic::TLiteralInt { value })
}

#[inline]
pub fn get_string() -> TUnion {
    wrap_atomic(TAtomic::TString)
}

#[inline]
pub fn get_literal_string(value: String) -> TUnion {
    wrap_atomic(TAtomic::TLiteralString { value })
}

#[inline]
pub fn get_float() -> TUnion {
    wrap_atomic(TAtomic::TFloat)
}

#[inline]
pub fn get_mixed() -> TUnion {
    wrap_atomic(TAtomic::TMixed)
}

#[inline]
pub fn get_mixed_any() -> TUnion {
    wrap_atomic(TAtomic::TMixedWithFlags(true, false, false, false))
}

pub fn get_mixed_maybe_from_loop(from_loop_isset: bool) -> TUnion {
    wrap_atomic(if !from_loop_isset {
        TAtomic::TMixed
    } else {
        TAtomic::TMixedFromLoopIsset
    })
}

#[inline]
pub fn get_nothing() -> TUnion {
    wrap_atomic(TAtomic::TNothing)
}

#[inline]
pub fn get_placeholder() -> TUnion {
    wrap_atomic(TAtomic::TPlaceholder)
}

#[inline]
pub fn get_void() -> TUnion {
    wrap_atomic(TAtomic::TVoid)
}

#[inline]
pub fn get_null() -> TUnion {
    wrap_atomic(TAtomic::TNull)
}

#[inline]
pub fn get_num() -> TUnion {
    wrap_atomic(TAtomic::TNum)
}

#[inline]
pub fn get_arraykey(from_any: bool) -> TUnion {
    wrap_atomic(TAtomic::TArraykey { from_any })
}

#[inline]
pub fn get_bool() -> TUnion {
    wrap_atomic(TAtomic::TBool)
}

#[inline]
pub fn get_false() -> TUnion {
    wrap_atomic(TAtomic::TFalse)
}

#[inline]
pub fn get_true() -> TUnion {
    wrap_atomic(TAtomic::TTrue)
}

#[inline]
pub fn get_object() -> TUnion {
    wrap_atomic(TAtomic::TObject {})
}

#[inline]
pub fn get_named_object(
    name: StrId,
    type_resolution_context: Option<&TypeResolutionContext>,
) -> TUnion {
    if let Some(type_resolution_context) = type_resolution_context {
        if let Some(t) = type_resolution_context
            .template_type_map
            .iter()
            .find(|v| v.0 == name)
        {
            return wrap_atomic(TAtomic::TGenericClassname {
                param_name: name,
                defining_entity: t.1[0].0,
                as_type: Box::new((*(t.1[0].1.get_single())).clone()),
            });
        }
    }
    wrap_atomic(TAtomic::TNamedObject {
        name,
        type_params: None,
        is_this: false,
        extra_types: None,
        remapped_params: false,
    })
}

#[inline]
pub fn get_scalar() -> TUnion {
    wrap_atomic(TAtomic::TScalar {})
}

pub fn get_vec(type_param: TUnion) -> TUnion {
    wrap_atomic(TAtomic::TVec(TVec {
        known_items: None,
        type_param: Box::new(type_param),
        known_count: None,
        non_empty: false,
    }))
}

pub fn get_dict(key_param: TUnion, value_param: TUnion) -> TUnion {
    wrap_atomic(TAtomic::TDict(TDict {
        known_items: None,
        params: Some((Box::new(key_param), Box::new(value_param))),
        non_empty: false,
        shape_name: None,
    }))
}

pub fn get_keyset(type_param: TUnion) -> TUnion {
    wrap_atomic(TAtomic::TKeyset {
        type_param: Box::new(type_param),
        non_empty: false,
    })
}

pub fn get_mixed_vec() -> TUnion {
    get_vec(get_mixed_any())
}

pub fn get_mixed_dict() -> TUnion {
    get_dict(get_arraykey(true), get_mixed_any())
}

pub fn get_mixed_keyset() -> TUnion {
    wrap_atomic(TAtomic::TKeyset {
        type_param: Box::new(get_arraykey(true)),
        non_empty: false,
    })
}

#[inline]
pub fn add_optional_union_type(
    base_type: TUnion,
    maybe_type: Option<&TUnion>,
    codebase: &CodebaseInfo,
) -> TUnion {
    if let Some(type_2) = maybe_type {
        add_union_type(base_type, type_2, codebase, false)
    } else {
        base_type
    }
}

pub fn combine_optional_union_types(
    type_1: Option<&TUnion>,
    type_2: Option<&TUnion>,
    codebase: &CodebaseInfo,
) -> TUnion {
    if let Some(type_1) = type_1 {
        if let Some(type_2) = type_2 {
            combine_union_types(type_1, type_2, codebase, false)
        } else {
            type_1.clone()
        }
    } else {
        type_2.unwrap().clone()
    }
}

pub fn extend_dataflow_uniquely(
    type_1_nodes: &mut Vec<DataFlowNode>,
    type_2_nodes: Vec<DataFlowNode>,
) {
    type_1_nodes.extend(type_2_nodes);
    type_1_nodes.sort_by(|a, b| a.id.cmp(&b.id));
    type_1_nodes.dedup_by(|a, b| a.id.eq(&b.id));
}

pub fn combine_union_types(
    type_1: &TUnion,
    type_2: &TUnion,
    codebase: &CodebaseInfo,
    overwrite_empty_array: bool, // default false
) -> TUnion {
    if type_1 == type_2 {
        return type_1.clone();
    }

    let mut combined_type;

    if type_1.is_vanilla_mixed() && type_2.is_vanilla_mixed() {
        combined_type = get_mixed();
    } else {
        let mut all_atomic_types = type_1.types.clone();
        all_atomic_types.extend(type_2.types.clone());

        combined_type = TUnion::new(type_combiner::combine(
            all_atomic_types,
            codebase,
            overwrite_empty_array,
        ));

        if type_1.had_template && type_2.had_template {
            combined_type.had_template = true;
        }

        if type_1.reference_free && type_2.reference_free {
            combined_type.reference_free = true;
        }
    }

    if type_1.possibly_undefined_from_try || type_2.possibly_undefined_from_try {
        combined_type.possibly_undefined_from_try = true;
    }

    if type_1.ignore_falsable_issues || type_2.ignore_falsable_issues {
        combined_type.ignore_falsable_issues = true;
    }

    let type_1_parent_nodes_empty = type_1.parent_nodes.is_empty();
    let type_2_parent_nodes_empty = type_2.parent_nodes.is_empty();

    if !type_1_parent_nodes_empty || !type_2_parent_nodes_empty {
        if type_1_parent_nodes_empty {
            combined_type.parent_nodes.clone_from(&type_2.parent_nodes);
        } else if type_2_parent_nodes_empty {
            combined_type.parent_nodes.clone_from(&type_1.parent_nodes);
        } else {
            combined_type.parent_nodes.clone_from(&type_1.parent_nodes);
            extend_dataflow_uniquely(&mut combined_type.parent_nodes, type_2.parent_nodes.clone());
        }
    }

    combined_type
}

pub fn add_union_type(
    mut base_type: TUnion,
    other_type: &TUnion,
    codebase: &CodebaseInfo,
    overwrite_empty_array: bool, // default false
) -> TUnion {
    if &base_type == other_type {
        return base_type;
    }

    base_type.types = if base_type.is_vanilla_mixed() && other_type.is_vanilla_mixed() {
        base_type.types
    } else {
        let mut all_atomic_types = base_type.types.clone();
        all_atomic_types.extend(other_type.types.clone());

        type_combiner::combine(all_atomic_types, codebase, overwrite_empty_array)
    };

    if !other_type.had_template {
        base_type.had_template = false;
    }

    if !other_type.reference_free {
        base_type.reference_free = false;
    }

    if other_type.possibly_undefined_from_try {
        base_type.possibly_undefined_from_try = true;
    }

    if other_type.ignore_falsable_issues {
        base_type.ignore_falsable_issues = true;
    }

    if !other_type.parent_nodes.is_empty() {
        extend_dataflow_uniquely(&mut base_type.parent_nodes, other_type.parent_nodes.clone());
    }

    base_type
}

pub fn intersect_union_types_simple(
    type_1: &TUnion,
    type_2: &TUnion,
    codebase: &CodebaseInfo,
) -> Option<TUnion> {
    if type_1 == type_2 {
        return Some(type_1.clone());
    }

    match (type_1.is_single(), type_2.is_single()) {
        (true, true) => {
            intersect_atomic_with_atomic_simple(type_1.get_single(), type_2.get_single(), codebase)
                .map(wrap_atomic)
        }
        (false, true) => intersect_union_with_atomic_simple(type_1, type_2.get_single(), codebase),
        (true, false) => intersect_union_with_atomic_simple(type_2, type_1.get_single(), codebase),
        (false, false) => {
            let new_types = type_2
                .types
                .iter()
                .flat_map(|t| {
                    intersect_union_with_atomic_simple(type_1, t, codebase)
                        .unwrap_or(get_nothing())
                        .types
                })
                .collect::<Vec<_>>();

            let combined_union = TUnion::new(combine(new_types, codebase, false));

            if combined_union.is_nothing() {
                None
            } else {
                Some(combined_union)
            }
        }
    }
}

fn intersect_union_with_atomic_simple(
    existing_var_type: &TUnion,
    new_type: &TAtomic,
    codebase: &CodebaseInfo,
) -> Option<TUnion> {
    let mut acceptable_types = Vec::new();

    for existing_atomic in &existing_var_type.types {
        if let Some(intersected_atomic_type) =
            intersect_atomic_with_atomic_simple(existing_atomic, new_type, codebase)
        {
            acceptable_types.push(intersected_atomic_type);
        }
    }

    if !acceptable_types.is_empty() {
        if acceptable_types.len() > 1 {
            acceptable_types = combine(acceptable_types, codebase, false);
        }
        return Some(TUnion::new(acceptable_types));
    }

    None
}

fn intersect_atomic_with_atomic_simple(
    type_1_atomic: &TAtomic,
    type_2_atomic: &TAtomic,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let mut atomic_comparison_results = TypeComparisonResult::new();

    // Basic same-type cases
    match (type_1_atomic, type_2_atomic) {
        (TAtomic::TNull, TAtomic::TNull) => {
            return Some(TAtomic::TNull);
        }
        (TAtomic::TMixed | TAtomic::TMixedWithFlags(_, false, _, false), TAtomic::TNull) => {
            return Some(TAtomic::TNull);
        }
        (TAtomic::TNull, TAtomic::TMixedWithFlags(_, _, _, true)) => return None,
        (
            TAtomic::TObject { .. }
            | TAtomic::TClosure(_)
            | TAtomic::TAwaitable { .. }
            | TAtomic::TNamedObject { .. },
            TAtomic::TObject,
        ) => {
            return Some(type_1_atomic.clone());
        }
        (_, TAtomic::TObject) => {
            return None;
        }
        (type_1_atomic, TAtomic::TArraykey { .. }) => {
            if type_1_atomic.is_mixed() {
                return Some(TAtomic::TArraykey { from_any: false });
            } else if type_1_atomic.is_int()
                || type_1_atomic.is_string()
                || matches!(type_1_atomic, TAtomic::TArraykey { .. })
            {
                return Some(type_1_atomic.clone());
            } else if matches!(type_1_atomic, TAtomic::TNum) {
                return Some(TAtomic::TInt);
            } else {
                return None;
            }
        }
        (type_1_atomic, TAtomic::TNum) => {
            if type_1_atomic.is_mixed() {
                return Some(TAtomic::TNum);
            } else if type_1_atomic.is_int() || matches!(type_1_atomic, TAtomic::TFloat { .. }) {
                return Some(type_1_atomic.clone());
            } else if matches!(type_1_atomic, TAtomic::TArraykey { .. }) {
                return Some(TAtomic::TInt);
            } else {
                return None;
            }
        }
        (
            TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TTypename { .. }
            | TAtomic::TStringWithFlags(..)
            | TAtomic::TString { .. },
            TAtomic::TString,
        ) => {
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TMixed
            | TAtomic::TMixedWithFlags(..)
            | TAtomic::TMixedFromLoopIsset
            | TAtomic::TScalar
            | TAtomic::TArraykey { .. },
            TAtomic::TString,
        ) => {
            return Some(TAtomic::TString);
        }
        (type_1_atomic, TAtomic::TString) => {
            if atomic_type_comparator::is_contained_by(
                codebase,
                &FilePath(StrId::EMPTY),
                type_1_atomic,
                &TAtomic::TString,
                false,
                &mut TypeComparisonResult::new(),
            ) {
                return Some(type_1_atomic.clone());
            } else {
                return None;
            }
        }
        (TAtomic::TLiteralInt { .. } | TAtomic::TInt, TAtomic::TInt) => {
            return Some(type_1_atomic.clone());
        }
        (
            TAtomic::TMixed
            | TAtomic::TMixedWithFlags(..)
            | TAtomic::TScalar
            | TAtomic::TNum
            | TAtomic::TArraykey { .. }
            | TAtomic::TMixedFromLoopIsset,
            TAtomic::TInt,
        ) => {
            return Some(TAtomic::TInt);
        }
        (type_1_atomic, TAtomic::TInt) => {
            if atomic_type_comparator::is_contained_by(
                codebase,
                &FilePath(StrId::EMPTY),
                type_1_atomic,
                &TAtomic::TInt,
                false,
                &mut TypeComparisonResult::new(),
            ) {
                return Some(type_1_atomic.clone());
            } else {
                return None;
            }
        }
        (TAtomic::TDict(type_1_dict), TAtomic::TDict(type_2_dict)) => {
            return intersect_dicts_simple(type_1_dict, type_2_dict, codebase);
        }
        (
            TAtomic::TVec(TVec {
                known_items: type_1_known_items,
                type_param: type_1_param,
                ..
            }),
            TAtomic::TVec(TVec {
                known_items: type_2_known_items,
                type_param: type_2_param,
                ..
            }),
        ) => {
            return intersect_vecs_simple(
                type_1_param,
                type_2_param,
                type_1_known_items,
                type_2_known_items,
                codebase,
            );
        }
        (
            TAtomic::TKeyset {
                type_param: type_1_param,
                ..
            },
            TAtomic::TKeyset {
                type_param: type_2_param,
                ..
            },
        ) => {
            return intersect_union_types_simple(type_1_param, type_2_param, codebase).map(
                |intersected| TAtomic::TKeyset {
                    type_param: Box::new(intersected),
                    non_empty: false,
                },
            );
        }
        _ => (),
    }

    // Check if type_2 is contained in type_1
    if atomic_type_comparator::is_contained_by(
        codebase,
        &FilePath(StrId::EMPTY),
        type_2_atomic,
        type_1_atomic,
        true,
        &mut atomic_comparison_results,
    ) {
        let type_2_atomic =
            if let Some(replacement) = atomic_comparison_results.replacement_atomic_type {
                replacement
            } else {
                type_2_atomic.clone()
            };

        return Some(type_2_atomic);
    }

    // Check if type_1 is contained in type_2
    atomic_comparison_results = TypeComparisonResult::new();
    if atomic_type_comparator::is_contained_by(
        codebase,
        &FilePath(StrId::EMPTY),
        type_1_atomic,
        type_2_atomic,
        false,
        &mut atomic_comparison_results,
    ) {
        let type_1_atomic =
            if let Some(replacement) = atomic_comparison_results.replacement_atomic_type {
                replacement
            } else {
                type_1_atomic.clone()
            };

        return Some(type_1_atomic);
    }

    // Special cases for enums
    match (type_1_atomic, type_2_atomic) {
        (
            TAtomic::TEnum {
                name: type_1_name, ..
            },
            TAtomic::TEnum {
                name: type_2_name, ..
            },
        ) => {
            if let (Some(storage_1), Some(storage_2)) = (
                codebase.classlike_infos.get(type_1_name),
                codebase.classlike_infos.get(type_2_name),
            ) {
                for (_, c1) in &storage_1.constants {
                    for (_, c2) in &storage_2.constants {
                        let c1_value = expand_constant_value(c1, codebase);
                        let c2_value = expand_constant_value(c2, codebase);
                        if c1_value == c2_value {
                            return Some(type_2_atomic.clone());
                        }
                    }
                }
            }
        }
        (
            TAtomic::TNamedObject {
                name: type_1_name, ..
            },
            TAtomic::TNamedObject {
                name: type_2_name, ..
            },
        ) => {
            if (codebase.interface_exists(type_1_name)
                && codebase.can_intersect_interface(type_2_name))
                || (codebase.interface_exists(type_2_name)
                    && codebase.can_intersect_interface(type_1_name))
            {
                let mut type_1_atomic = type_1_atomic.clone();
                type_1_atomic.add_intersection_type(type_2_atomic.clone());
                return Some(type_1_atomic);
            }
        }
        _ => (),
    }

    None
}

fn intersect_dicts_simple(
    type_1_dict: &TDict,
    type_2_dict: &TDict,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let params = match (&type_1_dict.params, &type_2_dict.params) {
        (Some(type_1_params), Some(type_2_params)) => {
            let key = intersect_union_types_simple(&type_1_params.0, &type_2_params.0, codebase);
            let value = intersect_union_types_simple(&type_1_params.1, &type_2_params.1, codebase);

            if let (Some(key), Some(value)) = (key, value) {
                Some((Box::new(key), Box::new(value)))
            } else {
                return None;
            }
        }
        _ => None,
    };

    match (&type_1_dict.known_items, &type_2_dict.known_items) {
        (Some(type_1_known_items), Some(type_2_known_items)) => {
            let mut intersected_items = BTreeMap::new();

            for (type_2_key, type_2_value) in type_2_known_items {
                if let Some(type_1_value) = type_1_known_items.get(type_2_key) {
                    intersected_items.insert(
                        type_2_key.clone(),
                        (
                            type_2_value.0 && type_1_value.0,
                            if let Some(t) = intersect_union_types_simple(
                                &type_1_value.1,
                                &type_2_value.1,
                                codebase,
                            ) {
                                Arc::new(t)
                            } else {
                                return None;
                            },
                        ),
                    );
                } else if let Some(type_1_params) = &type_1_dict.params {
                    intersected_items.insert(
                        type_2_key.clone(),
                        (
                            type_2_value.0,
                            if let Some(t) = intersect_union_types_simple(
                                &type_1_params.1,
                                &type_2_value.1,
                                codebase,
                            ) {
                                Arc::new(t)
                            } else {
                                return None;
                            },
                        ),
                    );
                } else if !type_2_value.0 {
                    return None;
                }
            }

            Some(TAtomic::TDict(TDict {
                known_items: Some(intersected_items),
                params,
                non_empty: true,
                shape_name: None,
            }))
        }
        _ => Some(TAtomic::TDict(TDict {
            known_items: None,
            params,
            non_empty: true,
            shape_name: None,
        })),
    }
}

fn intersect_vecs_simple(
    type_1_param: &TUnion,
    type_2_param: &TUnion,
    type_1_known_items: &Option<BTreeMap<usize, (bool, TUnion)>>,
    type_2_known_items: &Option<BTreeMap<usize, (bool, TUnion)>>,
    codebase: &CodebaseInfo,
) -> Option<TAtomic> {
    let type_param = intersect_union_types_simple(type_1_param, type_2_param, codebase);

    match (type_1_known_items, type_2_known_items) {
        (Some(type_1_known_items), Some(type_2_known_items)) => {
            let mut type_2_known_items = type_2_known_items.clone();

            for (type_2_key, type_2_value) in type_2_known_items.iter_mut() {
                if let Some(type_1_value) = type_1_known_items.get(type_2_key) {
                    type_2_value.0 = type_2_value.0 && type_1_value.0;
                    type_2_value.1 =
                        intersect_union_types_simple(&type_1_value.1, &type_2_value.1, codebase)?;
                } else if !type_1_param.is_nothing() {
                    type_2_value.1 =
                        intersect_union_types_simple(type_1_param, &type_2_value.1, codebase)?;
                } else if !type_2_value.0 {
                    return None;
                }
            }

            if let Some(type_param) = type_param {
                Some(TAtomic::TVec(TVec {
                    known_items: Some(type_2_known_items),
                    type_param: Box::new(type_param),
                    non_empty: true,
                    known_count: None,
                }))
            } else {
                None
            }
        }
        _ => {
            if let Some(type_param) = type_param {
                Some(TAtomic::TVec(TVec {
                    known_items: None,
                    type_param: Box::new(type_param),
                    non_empty: false,
                    known_count: None,
                }))
            } else {
                None
            }
        }
    }
}

pub fn get_arrayish_params(atomic: &TAtomic, codebase: &CodebaseInfo) -> Option<(TUnion, TUnion)> {
    match atomic {
        TAtomic::TDict(TDict {
            params,
            known_items,
            ..
        }) => {
            let mut key_types = vec![];
            let mut value_param;

            if let Some(params) = params {
                key_types.extend(params.0.types.clone());
                value_param = (*params.1).clone();
            } else {
                key_types.push(TAtomic::TNothing);
                value_param = get_nothing();
            }

            if let Some(known_items) = known_items {
                for (key, (_, property_type)) in known_items {
                    key_types.push(match key {
                        DictKey::Int(i) => TAtomic::TLiteralInt { value: *i as i64 },
                        DictKey::String(k) => TAtomic::TLiteralString { value: k.clone() },
                        DictKey::Enum(c, m) => codebase
                            .get_class_constant_type(c, false, m, FxHashSet::default())
                            .unwrap()
                            .get_single_owned(),
                    });
                    value_param = add_union_type(value_param, property_type, codebase, false);
                }
            }

            let key_param = TUnion::new(combine(key_types, codebase, false));

            Some((key_param, value_param))
        }
        TAtomic::TVec(TVec {
            type_param,
            known_items,
            ..
        }) => {
            let mut key_types = vec![TAtomic::TNothing];
            let mut type_param = (**type_param).clone();

            if let Some(known_items) = known_items {
                for (key, (_, property_type)) in known_items {
                    key_types.push(TAtomic::TLiteralInt { value: *key as i64 });
                    type_param = combine_union_types(property_type, &type_param, codebase, false);
                }
            }

            let combined_known_keys = TUnion::new(combine(key_types, codebase, false));

            let key_param = if type_param.is_nothing() {
                combined_known_keys
            } else {
                add_union_type(get_int(), &combined_known_keys, codebase, false)
            };

            Some((key_param, type_param))
        }
        TAtomic::TKeyset { type_param, .. } => {
            Some(((**type_param).clone(), (**type_param).clone()))
        }
        TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } => match name {
            &StrId::KEYED_CONTAINER | &StrId::KEYED_TRAVERSABLE | &StrId::ANY_ARRAY => Some((
                type_params.first().unwrap().clone(),
                type_params.get(1).unwrap().clone(),
            )),
            &StrId::CONTAINER | &StrId::TRAVERSABLE => {
                Some((get_arraykey(true), type_params.first().unwrap().clone()))
            }
            _ => None,
        },
        _ => None,
    }
}

pub fn get_value_param(atomic: &TAtomic, codebase: &CodebaseInfo) -> Option<TUnion> {
    match atomic {
        TAtomic::TDict(TDict {
            params,
            known_items,
            ..
        }) => {
            let mut value_param;

            if let Some(params) = params {
                value_param = (*params.1).clone();
            } else {
                value_param = get_nothing();
            }

            if let Some(known_items) = known_items {
                for (_, property_type) in known_items.values() {
                    value_param = combine_union_types(property_type, &value_param, codebase, false);
                }
            }

            Some(value_param)
        }
        TAtomic::TVec(TVec {
            type_param,
            known_items,
            ..
        }) => {
            let mut type_param = (**type_param).clone();

            if let Some(known_items) = known_items {
                for (_, property_type) in known_items.values() {
                    type_param = combine_union_types(property_type, &type_param, codebase, false);
                }
            }

            Some(type_param)
        }
        TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } => match name {
            &StrId::KEYED_CONTAINER | &StrId::KEYED_TRAVERSABLE | &StrId::ANY_ARRAY => {
                Some(type_params.get(1).unwrap().clone())
            }
            &StrId::CONTAINER | &StrId::TRAVERSABLE => Some(type_params.first().unwrap().clone()),
            _ => None,
        },
        _ => None,
    }
}

pub fn get_union_syntax_type(
    union: &TUnion,
    codebase: &CodebaseInfo,
    interner: &Interner,
    is_valid: &mut bool,
) -> String {
    let mut t_atomic_strings = FxHashSet::default();

    let mut t_object_parents = FxHashMap::default();

    let is_nullable = union.is_nullable() && !union.is_mixed();

    for atomic in &union.types {
        if let TAtomic::TNull { .. } = atomic {
            continue;
        }

        t_atomic_strings.insert({
            let s = get_atomic_syntax_type(atomic, codebase, interner, is_valid);
            if let TAtomic::TNamedObject {
                name,
                type_params: None,
                ..
            } = atomic
            {
                if let Some(storage) = codebase.classlike_infos.get(name) {
                    if let Some(parent_class) = &storage.direct_parent_class {
                        t_object_parents.insert(*name, *parent_class);
                    }
                }
            }
            s
        });
    }

    if t_atomic_strings.len() == 2 && t_atomic_strings.contains("int") {
        if t_atomic_strings.contains("string") {
            t_atomic_strings = FxHashSet::from_iter(["arraykey".to_string()]);
        } else if t_atomic_strings.contains("float") {
            t_atomic_strings = FxHashSet::from_iter(["num".to_string()]);
        }
    }

    if t_atomic_strings.len() != 1 && t_atomic_strings.len() == t_object_parents.len() {
        let flattened_parents = t_object_parents
            .into_values()
            .map(|v| interner.lookup(&v).to_string())
            .collect::<FxHashSet<_>>();

        if flattened_parents.len() == 1 {
            t_atomic_strings = flattened_parents;
        }
    }

    if t_atomic_strings.len() != 1 {
        if t_atomic_strings.contains("mixed") {
            return "mixed".to_string();
        }

        if t_atomic_strings.contains("nonnull") {
            return "nonnull".to_string();
        }

        *is_valid = false;

        return "_".to_string();
    }

    if is_nullable { "?" } else { "" }.to_string() + t_atomic_strings.iter().join("").as_str()
}

pub fn get_atomic_syntax_type(
    atomic: &TAtomic,
    codebase: &CodebaseInfo,
    interner: &Interner,
    is_valid: &mut bool,
) -> String {
    match atomic {
        TAtomic::TArraykey { .. } => "arraykey".to_string(),
        TAtomic::TBool { .. } => "bool".to_string(),
        TAtomic::TClassname { as_type, .. } => {
            let as_string = get_atomic_syntax_type(as_type, codebase, interner, is_valid);
            let mut str = String::new();
            str += "classname<";
            str += as_string.as_str();
            str += ">";
            str
        }
        TAtomic::TTypename { as_type, .. } => {
            let as_string = get_atomic_syntax_type(as_type, codebase, interner, is_valid);
            let mut str = String::new();
            str += "typename<";
            str += as_string.as_str();
            str += ">";
            str
        }
        TAtomic::TAwaitable { value, .. } => {
            let value_string = get_union_syntax_type(value, codebase, interner, is_valid);
            let mut str = String::new();
            str += "Awaitable<";
            str += value_string.as_str();
            str += ">";
            str
        }
        TAtomic::TDict(TDict {
            params,
            known_items,
            shape_name,
            ..
        }) => {
            if let Some(shape_name) = shape_name {
                return if let Some(shape_member_name) = &shape_name.1 {
                    format!(
                        "{}::{}",
                        interner.lookup(&shape_name.0),
                        interner.lookup(shape_member_name)
                    )
                } else {
                    interner.lookup(&shape_name.0).to_string()
                };
            }

            if let Some(known_items) = known_items {
                if if let Some(params) = params {
                    params.0.is_arraykey() && params.1.is_mixed()
                } else {
                    true
                } {
                    let mut str = String::new();
                    str += "shape(";
                    let mut known_item_strings = vec![];

                    for (property, (pu, property_type)) in known_items {
                        known_item_strings.push({
                            let property_type_string =
                                get_union_syntax_type(property_type, codebase, interner, is_valid);
                            format!(
                                "{}'{}' => {}",
                                if *pu { "?".to_string() } else { "".to_string() },
                                property.to_string(Some(interner)),
                                property_type_string
                            )
                        })
                    }
                    str += known_item_strings.join(", ").as_str();

                    if !params.is_none() {
                        str += ", ...";
                    }

                    str += ")";
                    return str;
                }
            }

            if let Some(params) = params {
                let key_param = get_union_syntax_type(&params.0, codebase, interner, is_valid);
                let value_param = get_union_syntax_type(&params.1, codebase, interner, is_valid);
                format!("dict<{}, {}>", key_param, value_param)
            } else {
                "dict<nothing, nothing>".to_string()
            }
        }
        TAtomic::TEnum { name, .. } => interner.lookup(name).to_string(),
        TAtomic::TFalse { .. } => "bool".to_string(),
        TAtomic::TFloat { .. } => "float".to_string(),
        TAtomic::TClosure(_) => {
            *is_valid = false;
            // todo
            "_".to_string()
        }
        TAtomic::TClosureAlias { .. } => {
            *is_valid = false;
            // todo
            "_".to_string()
        }
        TAtomic::TInt { .. } => "int".to_string(),
        TAtomic::TObject => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TKeyset { type_param, .. } => {
            let type_param = get_union_syntax_type(type_param, codebase, interner, is_valid);
            format!("keyset<{}>", type_param)
        }
        TAtomic::TLiteralClassname { .. } => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TEnumLiteralCase { enum_name, .. } => interner.lookup(enum_name).to_string(),
        TAtomic::TMemberReference { classlike_name, .. } => {
            interner.lookup(classlike_name).to_string()
        }
        TAtomic::TLiteralInt { .. } => "int".to_string(),
        TAtomic::TLiteralString { .. } | TAtomic::TStringWithFlags(..) => "string".to_string(),
        TAtomic::TMixed | TAtomic::TMixedFromLoopIsset => "mixed".to_string(),
        TAtomic::TNamedObject {
            name, type_params, ..
        } => match type_params {
            None => interner.lookup(name).to_string(),
            Some(type_params) => {
                let mut param_strings = vec![];
                for param in type_params {
                    param_strings.push(get_union_syntax_type(param, codebase, interner, is_valid));
                }

                format!("{}<{}>", interner.lookup(name), param_strings.join(", "))
            }
        },
        TAtomic::TTypeAlias {
            name, type_params, ..
        } => {
            if type_params.is_none() {
                interner.lookup(name).to_string()
            } else {
                *is_valid = false;
                "_".to_string()
            }
        }
        TAtomic::TNothing => "nothing".to_string(),
        TAtomic::TNull { .. } => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TNum { .. } => "num".to_string(),
        TAtomic::TScalar => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TString { .. } => "string".to_string(),
        TAtomic::TGenericParam { param_name, .. } => interner.lookup(param_name).to_string(),
        TAtomic::TGenericClassname {
            param_name,
            defining_entity,
            ..
        } => format!(
            "classname<{}:{}>",
            interner.lookup(param_name),
            defining_entity.to_string(Some(interner))
        ),
        TAtomic::TGenericTypename {
            param_name,
            defining_entity,
            ..
        } => format!(
            "typename<{}:{}>",
            interner.lookup(param_name),
            defining_entity.to_string(Some(interner))
        ),
        TAtomic::TTrue { .. } => "bool".to_string(),
        TAtomic::TVec(TVec {
            type_param,
            known_items,
            ..
        }) => {
            if type_param.is_nothing() {
                if let Some(known_items) = known_items {
                    let mut known_item_strings = vec![];
                    let mut all_good = true;
                    for (i, (offset, (pu, t))) in known_items.iter().enumerate() {
                        if i == *offset && !pu {
                            known_item_strings
                                .push(get_union_syntax_type(t, codebase, interner, is_valid))
                        } else {
                            all_good = false;
                            break;
                        }
                    }

                    if all_good {
                        return format!("({})", known_item_strings.join(", "));
                    }
                }
            }

            let type_param = get_value_param(atomic, codebase).unwrap();

            let type_param = get_union_syntax_type(&type_param, codebase, interner, is_valid);
            format!("vec<{}>", type_param)
        }
        TAtomic::TVoid => "void".to_string(),
        TAtomic::TReference { .. } => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TPlaceholder => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TMixedWithFlags(is_any, ..) => {
            if *is_any {
                *is_valid = false;
                "_".to_string()
            } else {
                "mixed".to_string()
            }
        }
        TAtomic::TClassTypeConstant {
            class_type,
            member_name,
            ..
        } => {
            let lhs = get_atomic_syntax_type(class_type, codebase, interner, is_valid);
            format!("{}::{}", lhs, interner.lookup(member_name))
        }
        TAtomic::TEnumClassLabel { .. } => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TResource => "resource".to_string(),
        TAtomic::TTypeVariable { .. } => {
            *is_valid = false;
            // todo
            "_".to_string()
        }
    }
}
