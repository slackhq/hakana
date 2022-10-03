use rustc_hash::{FxHashMap, FxHashSet};

use hakana_reflection_info::{
    codebase_info::{symbols::Symbol, CodebaseInfo},
    functionlike_parameter::FunctionLikeParameter,
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use itertools::Itertools;
use type_combiner::combine;

pub mod template;
mod type_combination;
pub mod type_combiner;
pub mod type_comparator;
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
    wrap_atomic(TAtomic::TMixedAny)
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
pub fn get_named_object(name: Symbol) -> TUnion {
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
    wrap_atomic(TAtomic::TVec {
        known_items: None,
        type_param,
        known_count: None,
        non_empty: false,
    })
}

pub fn get_dict(key_param: TUnion, value_param: TUnion) -> TUnion {
    wrap_atomic(TAtomic::TDict {
        known_items: None,
        params: Some((key_param, value_param)),
        non_empty: false,
        shape_name: None,
    })
}

pub fn get_keyset(type_param: TUnion) -> TUnion {
    wrap_atomic(TAtomic::TKeyset { type_param })
}

pub fn get_mixed_closure() -> TUnion {
    wrap_atomic(TAtomic::TClosure {
        params: vec![FunctionLikeParameter {
            name: "variadic".to_string(),
            is_inout: false,
            signature_type: None,
            is_optional: false,
            is_nullable: false,
            default_type: None,
            location: None,
            signature_type_location: None,
            is_variadic: true,
            taint_sinks: None,
            assert_untainted: false,
            type_inferred: false,
            expect_variable: false,
            promoted_property: false,
            attributes: Vec::new(),
            removed_taints_when_returning_true: None,
        }],
        return_type: None,
        effects: None,
    })
}

pub fn get_mixed_vec() -> TUnion {
    get_vec(get_mixed_any())
}

pub fn get_mixed_dict() -> TUnion {
    get_dict(get_arraykey(true), get_mixed_any())
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
        let mut all_atomic_types = type_1
            .types
            .clone()
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>();
        all_atomic_types.extend(
            type_2
                .types
                .clone()
                .into_iter()
                .map(|(_, v)| v)
                .collect::<Vec<_>>(),
        );

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

    if !type_1.parent_nodes.is_empty() || !type_2.parent_nodes.is_empty() {
        let mut parent_nodes = type_1.parent_nodes.clone();
        parent_nodes.extend(type_2.parent_nodes.clone());
        combined_type.parent_nodes = parent_nodes;
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
        let mut all_atomic_types = base_type
            .types
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>();
        all_atomic_types.extend(
            other_type
                .types
                .clone()
                .into_iter()
                .map(|(_, v)| v)
                .collect::<Vec<_>>(),
        );

        type_combiner::combine(all_atomic_types, codebase, overwrite_empty_array)
            .into_iter()
            .map(|v| (v.get_key(), v))
            .collect()
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
        base_type
            .parent_nodes
            .extend(other_type.parent_nodes.clone());
    }

    base_type
}

pub fn intersect_union_types(
    _type_1: &TUnion,
    _type_2: &TUnion,
    _codebase: &CodebaseInfo,
) -> Option<TUnion> {
    None
}

pub fn get_arrayish_params(atomic: &TAtomic, codebase: &CodebaseInfo) -> Option<(TUnion, TUnion)> {
    match atomic {
        TAtomic::TDict {
            params,
            known_items,
            ..
        } => {
            let mut key_types = vec![];
            let mut value_param;

            if let Some(params) = params {
                key_types.extend(
                    params
                        .0
                        .types
                        .clone()
                        .into_iter()
                        .map(|(_, v)| v)
                        .collect::<Vec<_>>(),
                );
                value_param = params.1.clone();
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
                            .get_class_constant_type(c, m, FxHashSet::default())
                            .unwrap()
                            .get_single_owned(),
                    });
                    value_param = add_union_type(value_param, property_type, codebase, false);
                }
            }

            let key_param = TUnion::new(combine(key_types, codebase, false));

            Some((key_param, value_param))
        }
        TAtomic::TVec {
            type_param,
            known_items,
            ..
        } => {
            let mut key_types = vec![TAtomic::TNothing];
            let mut type_param = type_param.clone();

            if let Some(known_items) = known_items {
                for (key, (_, property_type)) in known_items {
                    key_types.push(TAtomic::TLiteralInt {
                        value: key.clone() as i64,
                    });
                    type_param =
                        combine_union_types(property_type, &type_param, codebase, false);
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
        TAtomic::TKeyset { type_param, .. } => Some((type_param.clone(), type_param.clone())),
        TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } => {
            if **name == "HH\\KeyedContainer"
                || **name == "HH\\KeyedTraversable"
                || **name == "HH\\AnyArray"
            {
                Some((
                    type_params.get(0).unwrap().clone(),
                    type_params.get(1).unwrap().clone(),
                ))
            } else if **name == "HH\\Container" || **name == "HH\\Traversable" {
                Some((get_arraykey(true), type_params.get(0).unwrap().clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn get_value_param(atomic: &TAtomic, codebase: &CodebaseInfo) -> Option<TUnion> {
    match atomic {
        TAtomic::TDict {
            params,
            known_items,
            ..
        } => {
            let mut value_param;

            if let Some(params) = params {
                value_param = params.1.clone();
            } else {
                value_param = get_nothing();
            }

            if let Some(known_items) = known_items {
                for (_, (_, property_type)) in known_items {
                    value_param =
                        combine_union_types(property_type, &value_param, codebase, false);
                }
            }

            Some(value_param)
        }
        TAtomic::TVec {
            type_param,
            known_items,
            ..
        } => {
            let mut type_param = type_param.clone();

            if let Some(known_items) = known_items {
                for (_, (_, property_type)) in known_items {
                    type_param =
                        combine_union_types(property_type, &type_param, codebase, false);
                }
            }

            Some(type_param)
        }
        TAtomic::TNamedObject {
            name,
            type_params: Some(type_params),
            ..
        } => {
            if **name == "HH\\KeyedContainer"
                || **name == "HH\\KeyedTraversable"
                || **name == "HH\\AnyArray"
            {
                Some(type_params.get(1).unwrap().clone())
            } else if **name == "HH\\Container" || **name == "HH\\Traversable" {
                Some(type_params.get(0).unwrap().clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn is_array_container(name: &String) -> bool {
    name == "HH\\Traversable"
        || name == "HH\\KeyedTraversable"
        || name == "HH\\Container"
        || name == "HH\\KeyedContainer"
        || name == "HH\\AnyArray"
}

pub fn get_union_syntax_type(
    union: &TUnion,
    codebase: &CodebaseInfo,
    is_valid: &mut bool,
) -> String {
    let mut t_atomic_strings = FxHashSet::default();

    let mut t_object_parents = FxHashMap::default();

    let is_nullable = union.is_nullable() && !union.is_mixed();

    for (_, atomic) in &union.types {
        if let TAtomic::TNull { .. } = atomic {
            continue;
        }

        t_atomic_strings.insert({
            let s = get_atomic_syntax_type(atomic, codebase, is_valid);
            if let TAtomic::TNamedObject {
                name,
                type_params: None,
                ..
            } = atomic
            {
                if let Some(storage) = codebase.classlike_infos.get(name) {
                    if let Some(parent_class) = &storage.direct_parent_class {
                        t_object_parents.insert(name.clone(), parent_class.clone());
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
            .into_iter()
            .map(|(_, v)| (*v).clone())
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
    is_valid: &mut bool,
) -> String {
    match atomic {
        TAtomic::TArraykey { .. } => "arraykey".to_string(),
        TAtomic::TBool { .. } => "bool".to_string(),
        TAtomic::TClassname { as_type, .. } => {
            let as_string = get_atomic_syntax_type(as_type, codebase, is_valid);
            let mut str = String::new();
            str += "classname<";
            str += as_string.as_str();
            str += ">";
            str
        }
        TAtomic::TDict {
            params,
            known_items,
            shape_name,
            ..
        } => {
            if let Some(shape_name) = shape_name {
                return (**shape_name).clone();
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
                                get_union_syntax_type(property_type, codebase, is_valid);
                            format!(
                                "{}'{}' => {}",
                                if *pu { "?".to_string() } else { "".to_string() },
                                property.to_string(),
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

            return if let Some(params) = params {
                let key_param = get_union_syntax_type(&params.0, codebase, is_valid);
                let value_param = get_union_syntax_type(&params.1, codebase, is_valid);
                format!("dict<{}, {}>", key_param, value_param)
            } else {
                "dict<nothing, nothing>".to_string()
            };
        }
        TAtomic::TEnum { name } => (**name).clone(),
        TAtomic::TFalsyMixed { .. } => "mixed".to_string(),
        TAtomic::TFalse { .. } => "bool".to_string(),
        TAtomic::TFloat { .. } => "float".to_string(),
        TAtomic::TClosure { .. } => {
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
        TAtomic::TNonnullMixed { .. } => "nonnull".to_string(),
        TAtomic::TKeyset { type_param, .. } => {
            let type_param = get_union_syntax_type(type_param, codebase, is_valid);
            format!("keyset<{}>", type_param)
        }
        TAtomic::TLiteralClassname { .. } => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TEnumLiteralCase { enum_name, .. } => (**enum_name).clone(),
        TAtomic::TLiteralInt { .. } => "int".to_string(),
        TAtomic::TLiteralString { .. } | TAtomic::TStringWithFlags(..) => "string".to_string(),
        TAtomic::TMixed | TAtomic::TMixedFromLoopIsset => "mixed".to_string(),
        TAtomic::TNamedObject {
            name, type_params, ..
        } => match type_params {
            None => (**name).clone(),
            Some(type_params) => {
                let mut param_strings = vec![];
                for param in type_params {
                    param_strings.push(get_union_syntax_type(param, codebase, is_valid));
                }

                format!("{}<{}>", name, param_strings.join(", "))
            }
        },
        TAtomic::TTypeAlias {
            name, type_params, ..
        } => {
            if let None = type_params {
                (**name).clone()
            } else {
                *is_valid = false;
                "_".to_string()
            }
        }
        TAtomic::TTruthyMixed { .. } => "mixed".to_string(),
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
        TAtomic::TTemplateParam { param_name, .. } => param_name.clone(),
        TAtomic::TTemplateParamClass {
            param_name,
            defining_entity,
            ..
        } => format!("classname<{}:{}>", param_name, defining_entity),
        TAtomic::TTemplateParamType {
            param_name,
            defining_entity,
            ..
        } => format!("typename<{}:{}>", param_name, defining_entity),
        TAtomic::TTrue { .. } => "bool".to_string(),
        TAtomic::TVec {
            type_param,
            known_items,
            ..
        } => {
            if type_param.is_nothing() {
                if let Some(known_items) = known_items {
                    let mut known_item_strings = vec![];
                    let mut all_good = true;
                    for (i, (offset, (pu, t))) in known_items.iter().enumerate() {
                        if i == *offset && !pu {
                            known_item_strings.push(get_union_syntax_type(t, codebase, is_valid))
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

            let type_param = get_union_syntax_type(&type_param, codebase, is_valid);
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
        TAtomic::TMixedAny => {
            *is_valid = false;
            "_".to_string()
        }
        TAtomic::TClassTypeConstant {
            class_type,
            member_name,
        } => {
            let lhs = get_atomic_syntax_type(class_type, codebase, is_valid);
            format!("{}::{}", lhs, member_name)
        }
        TAtomic::TEnumClassLabel { .. } => {
            *is_valid = false;
            "_".to_string()
        }
    }
}
