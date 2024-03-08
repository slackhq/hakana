use hakana_reflection_info::code_location::FilePath;
use hakana_reflection_info::functionlike_parameter::FnParameter;
use hakana_reflection_info::t_atomic::DictKey;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use hakana_reflection_info::StrId;
use hakana_reflection_info::EFFECT_IMPURE;
use hakana_reflection_info::EFFECT_PURE;
use hakana_type::get_arraykey;
use hakana_type::get_mixed_any;
use hakana_type::get_nothing;
use hakana_type::wrap_atomic;
use oxidized::aast::Hint;
use oxidized::aast::Hint_;
use oxidized::aast_defs::NastShapeInfo;
use oxidized::ast::Id;
use oxidized::ast_defs;
use oxidized::ast_defs::ParamKind;
use oxidized::tast::HintFun;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::sync::Arc;

fn get_vec_type_from_hint(
    hint: &Hint,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    TAtomic::TVec {
        type_param: Box::new(
            get_type_from_hint(
                &hint.1,
                classlike_name,
                type_context,
                resolved_names,
                file_path,
                hint.0.start_offset() as u32,
            )
            .unwrap(),
        ),
        known_count: None,
        non_empty: false,
        known_items: None,
    }
}

fn get_tuple_type_from_hints(
    hints: &Vec<Hint>,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    TAtomic::TVec {
        type_param: Box::new(get_nothing()),
        known_count: Some(hints.len()),
        non_empty: true,
        known_items: Some({
            let mut map = BTreeMap::new();

            for (i, hint) in hints.iter().enumerate() {
                map.insert(
                    i,
                    (
                        false,
                        get_type_from_hint(
                            &hint.1,
                            classlike_name,
                            type_context,
                            resolved_names,
                            file_path,
                            hint.0.start_offset() as u32,
                        )
                        .unwrap(),
                    ),
                );
            }

            map
        }),
    }
}

fn get_keyset_type_from_hint(
    hint: &Hint,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    TAtomic::TKeyset {
        type_param: Box::new(
            get_type_from_hint(
                &hint.1,
                classlike_name,
                type_context,
                resolved_names,
                file_path,
                hint.0.start_offset() as u32,
            )
            .unwrap(),
        ),
    }
}

fn get_classname_type_from_hint(
    hint: &Hint,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    if let Some(inner_type) = get_type_from_hint(
        &hint.1,
        classlike_name,
        type_context,
        resolved_names,
        file_path,
        hint.0.start_offset() as u32,
    ) {
        let as_type = inner_type.get_single_owned();

        if let TAtomic::TGenericParam {
            param_name,
            defining_entity,
            as_type,
            ..
        } = as_type
        {
            TAtomic::TGenericClassname {
                param_name,
                defining_entity,
                as_type: Box::new(as_type.get_single_owned()),
            }
        } else {
            TAtomic::TClassname {
                as_type: Box::new(as_type),
            }
        }
    } else {
        TAtomic::TMixedWithFlags(true, false, false, false)
    }
}

fn get_typename_type_from_hint(
    hint: &Hint,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    if let Some(inner_type) = get_type_from_hint(
        &hint.1,
        classlike_name,
        type_context,
        resolved_names,
        file_path,
        hint.0.start_offset() as u32,
    ) {
        let as_type = inner_type.get_single_owned();

        if let TAtomic::TGenericParam {
            param_name,
            defining_entity,
            as_type,
            ..
        } = as_type
        {
            TAtomic::TGenericTypename {
                param_name,
                defining_entity,
                as_type: Box::new(as_type.get_single_owned()),
            }
        } else {
            TAtomic::TTypename {
                as_type: Box::new(as_type),
            }
        }
    } else {
        TAtomic::TMixedWithFlags(true, false, false, false)
    }
}

fn get_dict_type_from_hints(
    key_hint: Option<&Hint>,
    value_hint: Option<&Hint>,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    TAtomic::TDict {
        params: Some((
            Box::new(if let Some(k) = &key_hint {
                get_type_from_hint(
                    &k.1,
                    classlike_name,
                    type_context,
                    resolved_names,
                    file_path,
                    k.0.start_offset() as u32,
                )
                .unwrap()
            } else {
                get_arraykey(true)
            }),
            Box::new(if let Some(v) = &value_hint {
                get_type_from_hint(
                    &v.1,
                    classlike_name,
                    type_context,
                    resolved_names,
                    file_path,
                    v.0.start_offset() as u32,
                )
                .unwrap()
            } else {
                get_mixed_any()
            }),
        )),
        known_items: None,
        non_empty: false,
        shape_name: None,
    }
}

fn get_shape_type_from_hints(
    shape_info: &NastShapeInfo,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    let mut known_items = BTreeMap::new();

    for field in &shape_info.field_map {
        let field_type = get_type_from_hint(
            &field.hint.1,
            classlike_name,
            type_context,
            resolved_names,
            file_path,
            field.hint.0.start_offset() as u32,
        )
        .unwrap();

        match &field.name {
            ast_defs::ShapeFieldName::SFlitInt(int) => {
                known_items.insert(
                    DictKey::Int(int.1.parse::<u64>().unwrap()),
                    (field.optional, Arc::new(field_type)),
                );
            }
            ast_defs::ShapeFieldName::SFlitStr(name) => {
                known_items.insert(
                    DictKey::String(name.1.to_string()),
                    (field.optional, Arc::new(field_type)),
                );
            }
            ast_defs::ShapeFieldName::SFclassConst(lhs, name) => {
                let lhs_name = resolved_names.get(&(lhs.0.start_offset() as u32)).unwrap();
                let rhs_name = resolved_names.get(&(name.0.start_offset() as u32)).unwrap();
                known_items.insert(
                    DictKey::Enum(*lhs_name, *rhs_name),
                    (field.optional, Arc::new(field_type)),
                );
            }
        }
    }

    TAtomic::TDict {
        params: if shape_info.allows_unknown_fields {
            Some((Box::new(get_arraykey(true)), Box::new(get_mixed_any())))
        } else {
            None
        },

        known_items: if known_items.is_empty() {
            None
        } else {
            Some(known_items)
        },
        non_empty: false,
        shape_name: None,
    }
}

fn get_function_type_from_hints(
    function_info: &HintFun,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
    offset: u32,
) -> TAtomic {
    let mut params = function_info
        .param_tys
        .iter()
        .enumerate()
        .map(|(i, param_type)| {
            let param_info = function_info.param_info.get(i).unwrap();

            FnParameter {
                is_inout: if let Some(param_info) = param_info {
                    matches!(param_info.kind, ParamKind::Pinout(_))
                } else {
                    false
                },
                signature_type: get_type_from_hint(
                    &param_type.1,
                    classlike_name,
                    type_context,
                    resolved_names,
                    file_path,
                    param_type.0.start_offset() as u32,
                )
                .map(Box::new),
                is_variadic: false,
                is_optional: false,
            }
        })
        .collect::<Vec<_>>();

    if let Some(variadic_type) = &function_info.variadic_ty {
        let param = FnParameter {
            is_inout: false,
            signature_type: get_type_from_hint(
                &variadic_type.1,
                classlike_name,
                type_context,
                resolved_names,
                file_path,
                variadic_type.0.start_offset() as u32,
            )
            .map(Box::new),
            is_variadic: true,
            is_optional: false,
        };

        params.push(param);
    }

    TAtomic::TClosure {
        params,
        return_type: get_type_from_hint(
            &function_info.return_ty.1,
            classlike_name,
            type_context,
            resolved_names,
            file_path,
            function_info.return_ty.0.start_offset() as u32,
        )
        .map(Box::new),
        effects: if let Some(contexts) = &function_info.ctxs {
            Some(if contexts.1.is_empty() {
                EFFECT_PURE
            } else {
                EFFECT_IMPURE
            })
        } else {
            Some(EFFECT_IMPURE)
        },
        closure_id: (file_path, offset),
    }
}

fn get_reference_type(
    applied_type: &Id,
    extra_info: &[Hint],
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> TAtomic {
    let type_name = &applied_type.1;

    // special case classname<mixed> and classname<nonnull>
    if type_name == "mixed" || type_name == "nonnull" {
        return TAtomic::TObject;
    }

    // static & self are used in class type constants
    if type_name == "this" || type_name == "static" || type_name == "self" {
        let class_name = if let Some(classlike_name) = classlike_name {
            *classlike_name
        } else {
            StrId::THIS
        };

        return TAtomic::TNamedObject {
            name: class_name,
            type_params: None,
            is_this: type_name != "self",
            extra_types: None,
            remapped_params: false,
        };
    }

    let type_params: Vec<TUnion> = extra_info
        .iter()
        .map(|hint| {
            get_type_from_hint(
                &hint.1,
                classlike_name,
                type_context,
                resolved_names,
                file_path,
                hint.0.start_offset() as u32,
            )
            .unwrap()
        })
        .collect();

    if type_name == "Generator" {
        return TAtomic::TNamedObject {
            name: *resolved_names.get(&(applied_type.0.start_offset() as u32)).unwrap(),
            type_params: if type_params.len() == 3 {
                Some(vec![
                    type_params.first().unwrap().clone(),
                    type_params.get(1).unwrap().clone(),
                    type_params.get(2).unwrap().clone(),
                ])
            } else {
                Some(vec![get_arraykey(true), get_mixed_any(), get_mixed_any()])
            },
            is_this: false,
            extra_types: None,
            remapped_params: false,
        };
    }

    if type_name == "\\HH\\MemberOf" {
        return TAtomic::TTypeAlias {
            name: StrId::MEMBER_OF,
            type_params: Some(type_params),
            as_type: None,
        };
    }

    let resolved_name =
        if let Some(resolved_name) = resolved_names.get(&(applied_type.0.start_offset() as u32)) {
            resolved_name
        } else {
            return TAtomic::TMixed;
        };

    if let Some(defining_entities) = type_context.template_type_map.get(resolved_name) {
        return get_template_type(defining_entities, resolved_name);
    }

    TAtomic::TReference {
        name: *resolved_name,
        type_params: if type_params.is_empty() {
            None
        } else {
            Some(type_params)
        },
    }
}

fn get_template_type(
    defining_entities: &FxHashMap<StrId, Arc<TUnion>>,
    type_name: &StrId,
) -> TAtomic {
    let (defining_entity, as_type) = defining_entities.iter().next().unwrap();

    TAtomic::TGenericParam {
        param_name: *type_name,
        as_type: Box::new((**as_type).clone()),
        defining_entity: *defining_entity,
        from_class: false,
        extra_types: None,
    }
}

pub fn get_type_from_hint(
    hint: &Hint_,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
    offset: u32,
) -> Option<TUnion> {
    let mut types = Vec::new();

    let base = match hint {
        Hint_::Happly(id, extra_info) => {
            let applied_type = &id.1;

            if let Some(resolved_name) = resolved_names.get(&(id.0.start_offset() as u32)) {
                if let Some(type_name) = type_context.template_supers.get(resolved_name) {
                    return Some(type_name.clone());
                }
            }

            match applied_type.as_str() {
                "int" => TAtomic::TInt,
                "string" => TAtomic::TString,
                "arraykey" => TAtomic::TArraykey { from_any: false },
                "bool" => TAtomic::TBool,
                "float" => TAtomic::TFloat,
                "nonnull" => TAtomic::TMixedWithFlags(false, false, false, true),
                "null" => TAtomic::TNull,
                "nothing" => TAtomic::TNothing,
                "noreturn" => TAtomic::TNothing,
                "void" => TAtomic::TVoid,
                "num" => TAtomic::TNum,
                "mixed" => TAtomic::TMixed,
                "dynamic" => TAtomic::TMixedWithFlags(true, false, false, false),
                "vec" | "HH\\varray" | "varray" => {
                    if let Some(first) = extra_info.first() {
                        get_vec_type_from_hint(
                            first,
                            classlike_name,
                            type_context,
                            resolved_names,
                            file_path,
                        )
                    } else {
                        TAtomic::TVec {
                            type_param: Box::new(get_mixed_any()),
                            known_items: None,
                            known_count: None,
                            non_empty: false,
                        }
                    }
                }
                "dict" | "HH\\darray" | "darray" => get_dict_type_from_hints(
                    extra_info.first(),
                    extra_info.get(1),
                    classlike_name,
                    type_context,
                    resolved_names,
                    file_path,
                ),
                "keyset" => {
                    if let Some(param) = extra_info.first() {
                        get_keyset_type_from_hint(
                            param,
                            classlike_name,
                            type_context,
                            resolved_names,
                            file_path,
                        )
                    } else {
                        TAtomic::TKeyset {
                            type_param: Box::new(get_mixed_any()),
                        }
                    }
                }
                "classname" => {
                    if let Some(param) = extra_info.first() {
                        get_classname_type_from_hint(
                            param,
                            classlike_name,
                            type_context,
                            resolved_names,
                            file_path,
                        )
                    } else {
                        get_reference_type(
                            id,
                            extra_info,
                            classlike_name,
                            type_context,
                            resolved_names,
                            file_path,
                        )
                    }
                }
                "typename" => {
                    if let Some(param) = extra_info.first() {
                        get_typename_type_from_hint(
                            param,
                            classlike_name,
                            type_context,
                            resolved_names,
                            file_path,
                        )
                    } else {
                        get_reference_type(
                            id,
                            extra_info,
                            classlike_name,
                            type_context,
                            resolved_names,
                            file_path,
                        )
                    }
                }
                "vec_or_dict" | "varray_or_darray" => {
                    types.push(TAtomic::TVec {
                        known_items: None,
                        type_param: Box::new(wrap_atomic(TAtomic::TMixedWithFlags(
                            true, false, false, false,
                        ))),
                        non_empty: false,
                        known_count: None,
                    });
                    TAtomic::TDict {
                        known_items: None,
                        params: Some((
                            Box::new(get_arraykey(true)),
                            Box::new(wrap_atomic(TAtomic::TMixedWithFlags(
                                true, false, false, false,
                            ))),
                        )),
                        non_empty: false,
                        shape_name: None,
                    }
                }
                "resource" => TAtomic::TResource,
                "_" => TAtomic::TPlaceholder,
                "HH\\FIXME\\MISSING_RETURN_TYPE" | "\\HH\\FIXME\\MISSING_RETURN_TYPE" => {
                    return None;
                }
                _ => get_reference_type(
                    id,
                    extra_info,
                    classlike_name,
                    type_context,
                    resolved_names,
                    file_path,
                ),
            }
        }
        Hint_::Hmixed => TAtomic::TMixed,
        Hint_::Hshape(shape_info) => get_shape_type_from_hints(
            shape_info,
            classlike_name,
            type_context,
            resolved_names,
            file_path,
        ),
        Hint_::Htuple(tuple_hints) => get_tuple_type_from_hints(
            tuple_hints,
            classlike_name,
            type_context,
            resolved_names,
            file_path,
        ),
        Hint_::Hoption(inner) => {
            types.push(TAtomic::TNull);
            let union = get_type_from_hint(
                &inner.1,
                classlike_name,
                type_context,
                resolved_names,
                file_path,
                inner.0.start_offset() as u32,
            )
            .unwrap();

            let mut last = None;

            for atomic_type in union.types.into_iter() {
                if last.is_none() {
                    last = Some(atomic_type);
                } else {
                    types.push(atomic_type);
                }
            }

            last.unwrap()
        }
        Hint_::Hlike(inner) => {
            let union = get_type_from_hint(
                &inner.1,
                classlike_name,
                type_context,
                resolved_names,
                file_path,
                inner.0.start_offset() as u32,
            )
            .unwrap();

            let mut last = None;

            for atomic_type in union.types.into_iter() {
                if last.is_none() {
                    last = Some(atomic_type);
                } else {
                    types.push(atomic_type);
                }
            }

            last.unwrap()
        }
        Hint_::Hfun(hint_fun) => get_function_type_from_hints(
            hint_fun,
            classlike_name,
            type_context,
            resolved_names,
            file_path,
            offset,
        ),
        Hint_::Haccess(class, type_names) => {
            let mut inner_type = get_type_from_hint(
                &class.1,
                None,
                type_context,
                resolved_names,
                file_path,
                class.0.start_offset() as u32,
            )
            .unwrap()
            .get_single_owned();

            for type_id in type_names {
                inner_type = TAtomic::TClassTypeConstant {
                    class_type: Box::new(inner_type),
                    member_name: if let Some(resolved_name) =
                        resolved_names.get(&(type_id.0.start_offset() as u32))
                    {
                        *resolved_name
                    } else {
                        return None;
                    },
                };
            }

            inner_type
        }
        Hint_::Hsoft(hint) => {
            return get_type_from_hint(
                &hint.1,
                classlike_name,
                type_context,
                resolved_names,
                file_path,
                hint.0.start_offset() as u32,
            );
        }
        Hint_::Hnonnull => TAtomic::TMixedWithFlags(false, false, false, true),
        Hint_::Habstr(_, _) => panic!(),
        Hint_::HvecOrDict(_, _) => panic!(),
        Hint_::Hprim(_) => panic!(),
        Hint_::Hthis => panic!(),
        Hint_::Hdynamic => panic!(),
        Hint_::Hnothing => TAtomic::TNothing,
        Hint_::Hunion(union_hints) => {
            let mut all_atomic_types = vec![];
            for inner_hint in union_hints {
                let inner_type = get_type_from_hint(
                    &inner_hint.1,
                    classlike_name,
                    type_context,
                    resolved_names,
                    file_path,
                    inner_hint.0.start_offset() as u32,
                );

                if let Some(inner_type) = inner_type {
                    all_atomic_types.extend(inner_type.types);
                }
            }

            let base = all_atomic_types.pop().unwrap();
            types.extend(all_atomic_types);
            base
        }
        Hint_::Hintersection(_) => TAtomic::TObject,
        Hint_::HfunContext(_) => panic!(),
        Hint_::Hvar(_) => panic!(),
        Hint_::Hrefinement(_, _) => panic!(),
        Hint_::HclassArgs(_) => panic!(),
        Hint_::Hwildcard => TAtomic::TPlaceholder,
    };

    types.push(base);

    Some(TUnion::new(types))
}

pub fn get_type_from_optional_hint(
    hint: &Option<Hint>,
    classlike_name: Option<&StrId>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<u32, StrId>,
    file_path: FilePath,
) -> Option<TUnion> {
    match hint {
        Some(x) => get_type_from_hint(
            &x.1,
            classlike_name,
            type_context,
            resolved_names,
            file_path,
            x.0.start_offset() as u32,
        ),
        _ => None,
    }
}

pub fn get_type_references_from_hint(
    hint: &Hint,
    resolved_names: &FxHashMap<u32, StrId>,
) -> Vec<(StrId, usize, usize)> {
    let mut refs = vec![];
    match &*hint.1 {
        Hint_::Happly(id, type_params) => {
            let applied_type = &id.1;

            match applied_type.as_str() {
                "int"
                | "string"
                | "arraykey"
                | "bool"
                | "float"
                | "nonnull"
                | "null"
                | "nothing"
                | "noreturn"
                | "void"
                | "num"
                | "mixed"
                | "dynamic"
                | "vec"
                | "HH\\vec"
                | "HH\\varray"
                | "varray"
                | "dict"
                | "HH\\dict"
                | "HH\\darray"
                | "darray"
                | "classname"
                | "typename"
                | "vec_or_dict"
                | "varray_or_darray"
                | "resource"
                | "_"
                | "HH\\FIXME\\MISSING_RETURN_TYPE"
                | "\\HH\\FIXME\\MISSING_RETURN_TYPE" => {}
                _ => {
                    if let Some(resolved_name) = resolved_names.get(&(id.0.start_offset() as u32)) {
                        refs.push((*resolved_name, id.0.start_offset(), id.0.end_offset()));
                    }
                }
            }

            for type_param in type_params {
                refs.extend(get_type_references_from_hint(type_param, resolved_names));
            }
        }
        Hint_::Hshape(shape_info) => {
            for field in &shape_info.field_map {
                refs.extend(get_type_references_from_hint(&field.hint, resolved_names));

                if let ast_defs::ShapeFieldName::SFclassConst(lhs, _) = &field.name {
                    let lhs_name = resolved_names.get(&(lhs.0.start_offset() as u32)).unwrap();
                    refs.push((*lhs_name, lhs.0.start_offset(), lhs.0.end_offset()));
                }
            }
        }
        Hint_::Htuple(tuple_hints) => {
            for hint in tuple_hints {
                refs.extend(get_type_references_from_hint(hint, resolved_names));
            }
        }
        Hint_::Hoption(inner) => {
            refs.extend(get_type_references_from_hint(inner, resolved_names));
        }
        Hint_::Hfun(hint_fun) => {
            for param_hint in &hint_fun.param_tys {
                refs.extend(get_type_references_from_hint(param_hint, resolved_names));
            }
            refs.extend(get_type_references_from_hint(
                &hint_fun.return_ty,
                resolved_names,
            ));
        }
        Hint_::Haccess(class, _) => {
            refs.extend(get_type_references_from_hint(class, resolved_names));
        }
        Hint_::Hsoft(hint) => {
            refs.extend(get_type_references_from_hint(hint, resolved_names));
        }
        _ => {}
    }

    refs
}
