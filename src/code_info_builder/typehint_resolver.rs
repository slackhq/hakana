use hakana_reflection_info::codebase_info::symbols::Symbol;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::t_atomic::DictKey;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use hakana_reflection_info::StrId;
use hakana_type::*;
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
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    TAtomic::TVec {
        type_param: get_type_from_hint(&hint.1, classlike_name, type_context, resolved_names),
        known_count: None,
        non_empty: false,
        known_items: None,
    }
}

fn get_tuple_type_from_hints(
    hints: &Vec<Hint>,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    TAtomic::TVec {
        type_param: get_nothing(),
        known_count: Some(hints.len()),
        non_empty: true,
        known_items: Some({
            let mut map = BTreeMap::new();
            let mut i = 0;

            for hint in hints {
                map.insert(
                    i,
                    (
                        false,
                        get_type_from_hint(&hint.1, classlike_name, type_context, resolved_names),
                    ),
                );
                i += 1;
            }

            map
        }),
    }
}

fn get_keyset_type_from_hint(
    hint: &Hint,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    TAtomic::TKeyset {
        type_param: get_type_from_hint(&hint.1, classlike_name, type_context, resolved_names),
    }
}

fn get_classname_type_from_hint(
    hint: &Hint,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    if let Hint_::Happly(id, type_params) = &*hint.1 {
        let as_type = get_reference_type(
            id,
            type_params,
            classlike_name,
            type_context,
            resolved_names,
        );

        if let TAtomic::TTemplateParam {
            param_name,
            defining_entity,
            ..
        } = &as_type
        {
            return TAtomic::TTemplateParamClass {
                param_name: param_name.clone(),
                defining_entity: defining_entity.clone(),
                as_type: Box::new(as_type),
            };
        }
        TAtomic::TClassname {
            as_type: Box::new(as_type),
        }
    } else {
        TAtomic::TMixed
    }
}

fn get_typename_type_from_hint(
    hint: &Hint,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    if let Hint_::Happly(id, type_params) = &*hint.1 {
        let as_type = get_reference_type(
            id,
            type_params,
            classlike_name,
            type_context,
            resolved_names,
        );

        if let TAtomic::TTemplateParam {
            param_name,
            defining_entity,
            ..
        } = as_type
        {
            return TAtomic::TTemplateParamType {
                param_name,
                defining_entity,
            };
        }

        TAtomic::TMixed
    } else {
        TAtomic::TMixed
    }
}

fn get_dict_type_from_hints(
    key_hint: Option<&Hint>,
    value_hint: Option<&Hint>,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    TAtomic::TDict {
        params: Some((
            if let Some(k) = &key_hint {
                get_type_from_hint(&k.1, classlike_name, type_context, resolved_names)
            } else {
                get_arraykey(true)
            },
            if let Some(v) = &value_hint {
                get_type_from_hint(&v.1, classlike_name, type_context, resolved_names)
            } else {
                get_mixed_any()
            },
        )),
        known_items: None,
        non_empty: false,
        shape_name: None,
    }
}

fn get_shape_type_from_hints(
    shape_info: &NastShapeInfo,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    let mut known_items = BTreeMap::new();

    for field in &shape_info.field_map {
        let field_type =
            get_type_from_hint(&field.hint.1, classlike_name, type_context, resolved_names);

        match &field.name {
            ast_defs::ShapeFieldName::SFlitInt(int) => {
                known_items.insert(
                    DictKey::Int(int.1.parse::<u32>().unwrap()),
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
                let lhs_name = resolved_names.get(&lhs.0.start_offset()).unwrap();
                let rhs_name = resolved_names.get(&name.0.start_offset()).unwrap();
                known_items.insert(
                    DictKey::Enum(*lhs_name, *rhs_name),
                    (field.optional, Arc::new(field_type)),
                );
            }
        }
    }

    TAtomic::TDict {
        params: if shape_info.allows_unknown_fields {
            Some((get_arraykey(true), get_mixed()))
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
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TAtomic {
    let mut params = function_info
        .param_tys
        .iter()
        .enumerate()
        .map(|(i, param_type)| {
            let mut param = FunctionLikeParameter::new("".to_string());
            let param_info = function_info.param_info.get(i).unwrap();

            param.is_variadic = false;
            param.is_inout = if let Some(param_info) = param_info {
                matches!(param_info.kind, ParamKind::Pinout(_))
            } else {
                false
            };
            param.signature_type = Some(get_type_from_hint(
                &param_type.1,
                classlike_name,
                type_context,
                resolved_names,
            ));

            param
        })
        .collect::<Vec<_>>();

    if let Some(variadic_type) = &function_info.variadic_ty {
        let mut param = FunctionLikeParameter::new("".to_string());

        param.is_variadic = true;
        param.signature_type = Some(get_type_from_hint(
            &variadic_type.1,
            classlike_name,
            type_context,
            resolved_names,
        ));

        params.push(param);
    }

    TAtomic::TClosure {
        params,
        return_type: Some(get_type_from_hint(
            &function_info.return_ty.1,
            classlike_name,
            type_context,
            resolved_names,
        )),
        effects: None,
        closure_id: StrId::anonymous_fn(),
    }
}

fn get_reference_type(
    applied_type: &Id,
    extra_info: &Vec<Hint>,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
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
            StrId::this()
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
        .into_iter()
        .map(|hint| get_type_from_hint(&hint.1, classlike_name, type_context, resolved_names))
        .collect();

    if type_name == "Generator" {
        return TAtomic::TNamedObject {
            name: *resolved_names.get(&applied_type.0.start_offset()).unwrap(),
            type_params: if type_params.len() == 3 {
                Some(vec![
                    type_params.get(0).unwrap().clone(),
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
            name: StrId::member_of(),
            type_params: Some(type_params),
            as_type: None,
        };
    }

    if let Some(defining_entities) = type_context.template_type_map.get(type_name) {
        return get_template_type(defining_entities, type_name);
    }

    let resolved_name = resolved_names.get(&applied_type.0.start_offset()).unwrap();

    TAtomic::TReference {
        name: resolved_name.clone(),
        type_params: if type_params.is_empty() {
            None
        } else {
            Some(type_params)
        },
    }
}

fn get_template_type(
    defining_entities: &FxHashMap<Symbol, Arc<TUnion>>,
    type_name: &String,
) -> TAtomic {
    let as_type = defining_entities.values().next().unwrap().clone();
    let defining_entity = defining_entities.keys().next().unwrap().clone();

    return TAtomic::TTemplateParam {
        param_name: type_name.clone(),
        as_type: (*as_type).clone(),
        defining_entity,
        from_class: false,
        extra_types: None,
    };
}

pub fn get_type_from_hint(
    hint: &Hint_,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> TUnion {
    let mut types = Vec::new();

    let base = match hint {
        Hint_::Happly(id, extra_info) => {
            let applied_type = &id.1;

            if let Some(type_name) = type_context.template_supers.get(applied_type) {
                return type_name.clone();
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
                        get_vec_type_from_hint(first, classlike_name, type_context, resolved_names)
                    } else {
                        TAtomic::TVec {
                            type_param: get_mixed_any(),
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
                ),
                "keyset" => get_keyset_type_from_hint(
                    extra_info.first().unwrap(),
                    classlike_name,
                    type_context,
                    resolved_names,
                ),
                "classname" => get_classname_type_from_hint(
                    extra_info.first().unwrap(),
                    classlike_name,
                    type_context,
                    resolved_names,
                ),
                "typename" => get_typename_type_from_hint(
                    extra_info.first().unwrap(),
                    classlike_name,
                    type_context,
                    resolved_names,
                ),
                "vec_or_dict" | "varray_or_darray" => {
                    types.push(TAtomic::TVec {
                        known_items: None,
                        type_param: wrap_atomic(TAtomic::TMixedWithFlags(
                            true, false, false, false,
                        )),
                        non_empty: false,
                        known_count: None,
                    });
                    TAtomic::TDict {
                        known_items: None,
                        params: Some((
                            get_arraykey(true),
                            wrap_atomic(TAtomic::TMixedWithFlags(true, false, false, false)),
                        )),
                        non_empty: false,
                        shape_name: None,
                    }
                }
                "resource" => TAtomic::TMixed,
                "_" => TAtomic::TPlaceholder,
                _ => {
                    get_reference_type(id, extra_info, classlike_name, type_context, resolved_names)
                }
            }
        }
        Hint_::Hmixed => TAtomic::TMixed,
        Hint_::Hany => TAtomic::TMixedWithFlags(true, false, false, false),
        Hint_::Hshape(shape_info) => {
            get_shape_type_from_hints(shape_info, classlike_name, type_context, resolved_names)
        }
        Hint_::Htuple(tuple_hints) => {
            get_tuple_type_from_hints(tuple_hints, classlike_name, type_context, resolved_names)
        }
        Hint_::Hoption(inner) => {
            types.push(TAtomic::TNull);
            let union = get_type_from_hint(&inner.1, classlike_name, type_context, resolved_names);

            let mut last = None;

            for atomic_type in union.types.into_iter() {
                if let None = last {
                    last = Some(atomic_type);
                } else {
                    types.push(atomic_type);
                }
            }

            last.unwrap()
        }
        Hint_::Hlike(_) => panic!(),
        Hint_::Hfun(hint_fun) => {
            get_function_type_from_hints(hint_fun, classlike_name, type_context, resolved_names)
        }
        Hint_::Haccess(class, type_names) => {
            let mut inner_type =
                get_type_from_hint(&class.1, classlike_name, type_context, resolved_names)
                    .get_single_owned();

            for type_id in type_names {
                inner_type = TAtomic::TClassTypeConstant {
                    class_type: Box::new(inner_type),
                    member_name: type_id.1.clone(),
                };
            }

            inner_type
        }
        Hint_::Hsoft(hint) => {
            return get_type_from_hint(&hint.1, classlike_name, type_context, resolved_names);
        }
        Hint_::Herr => panic!(),
        Hint_::Hnonnull => TAtomic::TMixedWithFlags(false, false, false, true),
        Hint_::Habstr(_, _) => panic!(),
        Hint_::HvecOrDict(_, _) => panic!(),
        Hint_::Hprim(_) => panic!(),
        Hint_::Hthis => panic!(),
        Hint_::Hdynamic => panic!(),
        Hint_::Hnothing => TAtomic::TNothing,
        Hint_::Hunion(_) => panic!(),
        Hint_::Hintersection(_) => TAtomic::TObject,
        Hint_::HfunContext(_) => panic!(),
        Hint_::Hvar(_) => panic!(),
        Hint_::Hrefinement(_, _) => panic!(),
    };

    types.push(base);

    TUnion::new(types)
}

pub fn get_type_from_optional_hint(
    hint: &Option<Hint>,
    classlike_name: Option<&Symbol>,
    type_context: &TypeResolutionContext,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> Option<TUnion> {
    match hint {
        Some(x) => Some(get_type_from_hint(
            &x.1,
            classlike_name,
            type_context,
            resolved_names,
        )),
        _ => None,
    }
}
