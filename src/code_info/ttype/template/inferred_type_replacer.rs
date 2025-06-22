use std::sync::Arc;

use crate::{
    codebase_info::CodebaseInfo,
    t_atomic::{TAtomic, TDict, TVec},
    t_union::TUnion,
    GenericParent,
};
use hakana_str::StrId;
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ttype::{get_nothing, type_combiner, wrap_atomic};

use super::{
    standin_type_replacer::{self, get_most_specific_type_from_bounds},
    TemplateBound, TemplateResult,
};

pub fn replace(
    union: &TUnion,
    template_result: &TemplateResult,
    codebase: &CodebaseInfo,
) -> TUnion {
    let mut keys_to_unset = FxHashSet::default();

    let mut new_types = Vec::new();

    for atomic_type in &union.types {
        let mut atomic_type = atomic_type.clone();
        atomic_type = replace_atomic(atomic_type, template_result, codebase);

        match &atomic_type {
            TAtomic::TGenericParam {
                param_name,
                defining_entity,
                as_type,
                extra_types,
                ..
            } => {
                let key = param_name;

                let template_type = replace_template_param(
                    &template_result.lower_bounds,
                    param_name,
                    defining_entity,
                    codebase,
                    as_type,
                    extra_types,
                    key,
                );

                if let Some(template_type) = template_type {
                    keys_to_unset.insert(*key);

                    for template_type_part in template_type.types {
                        new_types.push(template_type_part);
                    }
                } else {
                    new_types.push(atomic_type);
                }
            }
            TAtomic::TGenericClassname {
                param_name,
                defining_entity,
                ..
            } => {
                if let Some(bounds) = template_result
                    .lower_bounds
                    .get(param_name)
                    .unwrap_or(&FxHashMap::default())
                    .get(defining_entity)
                {
                    let template_type = get_most_specific_type_from_bounds(bounds, codebase);

                    let mut class_template_type = None;

                    for template_type_part in &template_type.types {
                        if template_type_part.is_mixed()
                            || matches!(template_type_part, TAtomic::TObject)
                        {
                            class_template_type = Some(TAtomic::TClassname {
                                as_type: Box::new(TAtomic::TObject),
                            });
                        } else if let TAtomic::TNamedObject { .. } = template_type_part {
                            class_template_type = Some(TAtomic::TClassname {
                                as_type: Box::new(template_type_part.clone()),
                            });
                        } else if let TAtomic::TGenericParam {
                            as_type,
                            param_name,
                            defining_entity,
                            ..
                        } = template_type_part
                        {
                            let first_atomic_type = as_type.get_single();

                            class_template_type = Some(TAtomic::TGenericClassname {
                                param_name: *param_name,
                                as_type: Box::new(first_atomic_type.clone()),
                                defining_entity: *defining_entity,
                            })
                        }
                    }

                    if let Some(class_template_type) = class_template_type {
                        keys_to_unset.insert(*param_name);
                        new_types.push(class_template_type);
                    }
                }
            }
            TAtomic::TGenericTypename {
                param_name,
                defining_entity,
                ..
            } => {
                if let Some(bounds) = template_result
                    .lower_bounds
                    .get(param_name)
                    .unwrap_or(&FxHashMap::default())
                    .get(defining_entity)
                {
                    let template_type = get_most_specific_type_from_bounds(bounds, codebase);

                    let mut class_template_type = None;

                    for template_type_part in &template_type.types {
                        if template_type_part.is_mixed() {
                            class_template_type = Some(TAtomic::TTypename {
                                as_type: Box::new(TAtomic::TObject),
                            });
                        } else if let TAtomic::TTypeAlias {
                            name: type_name,
                            newtype: type_newtype,
                            ..
                        } = template_type_part
                        {
                            class_template_type = Some(TAtomic::TTypename {
                                as_type: Box::new(TAtomic::TTypeAlias {
                                    name: *type_name,
                                    type_params: None,
                                    as_type: None,
                                    newtype: *type_newtype,
                                }),
                            });
                        } else if let TAtomic::TGenericParam {
                            as_type,
                            param_name,
                            defining_entity,
                            ..
                        } = template_type_part
                        {
                            let first_atomic_type = as_type.get_single();

                            class_template_type = Some(TAtomic::TGenericTypename {
                                param_name: *param_name,
                                as_type: Box::new(first_atomic_type.clone()),
                                defining_entity: *defining_entity,
                            });
                        }
                    }

                    if let Some(class_template_type) = class_template_type {
                        keys_to_unset.insert(*param_name);
                        new_types.push(class_template_type);
                    }
                }
            }
            _ => {
                new_types.push(atomic_type);
            }
        }
    }

    let mut union = union.clone();

    if new_types.is_empty() {
        return get_nothing();
    }

    union.types = type_combiner::combine(new_types, codebase, false);

    union
}

fn replace_template_param(
    inferred_lower_bounds: &IndexMap<StrId, FxHashMap<GenericParent, Vec<TemplateBound>>>,
    param_name: &StrId,
    defining_entity: &GenericParent,
    codebase: &CodebaseInfo,
    as_type: &TUnion,
    extra_types: &Option<Vec<TAtomic>>,
    key: &StrId,
) -> Option<TUnion> {
    let mut template_type = None;
    let traversed_type = standin_type_replacer::get_root_template_type(
        inferred_lower_bounds,
        param_name,
        defining_entity,
        FxHashSet::default(),
        codebase,
    );

    if let Some(traversed_type) = traversed_type {
        let template_type_inner = if !as_type.is_mixed() && traversed_type.is_mixed() {
            if as_type.is_arraykey() {
                wrap_atomic(TAtomic::TArraykey { from_any: true })
            } else {
                as_type.clone()
            }
        } else {
            traversed_type.clone()
        };

        if let Some(_extra_types) = extra_types {
            for _atomic_template_type in &template_type_inner.types {
                // todo handle extra types
            }
        }

        template_type = Some(template_type_inner);
    } else {
        for (_, template_type_map) in inferred_lower_bounds {
            for map_defining_entity in template_type_map.keys() {
                let classlike_name = match map_defining_entity {
                    GenericParent::ClassLike(e) => e,
                    _ => {
                        continue;
                    }
                };

                if let Some(classlike_info) = codebase.classlike_infos.get(classlike_name) {
                    if let Some(param_map) =
                        classlike_info.template_extended_params.get(classlike_name)
                    {
                        if let Some(param_inner) = param_map.get(key) {
                            let template_name =
                                if let TAtomic::TGenericParam { param_name, .. } =
                                    param_inner.get_single()
                                {
                                    param_name
                                } else {
                                    panic!()
                                };
                            if let Some(bounds_map) = inferred_lower_bounds.get(template_name) {
                                if let Some(bounds) = bounds_map.get(map_defining_entity) {
                                    template_type = Some(
                                        standin_type_replacer::get_most_specific_type_from_bounds(
                                            bounds, codebase,
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    template_type
}

fn replace_atomic(
    mut atomic: TAtomic,
    template_result: &TemplateResult,
    codebase: &CodebaseInfo,
) -> TAtomic {
    match atomic {
        TAtomic::TVec(TVec {
            ref mut type_param,
            ref mut known_items,
            ..
        }) => {
            *type_param = Box::new(replace(type_param, template_result, codebase));

            if let Some(known_items) = known_items {
                for (_, t) in known_items.values_mut() {
                    *t = replace(t, template_result, codebase);
                }
            }
        }
        TAtomic::TDict(TDict {
            ref mut params,
            ref mut known_items,
            ..
        }) => {
            if let Some(params) = params {
                params.0 = Box::new(replace(&params.0, template_result, codebase));
                params.1 = Box::new(replace(&params.1, template_result, codebase));
            }

            if let Some(known_items) = known_items {
                for (_, t) in known_items.values_mut() {
                    *t = Arc::new(replace(t, template_result, codebase));
                }
            }
        }
        TAtomic::TKeyset {
            ref mut type_param, ..
        } => {
            *type_param = Box::new(replace(type_param, template_result, codebase));
        }
        TAtomic::TNamedObject {
            type_params: Some(ref mut type_params),
            ..
        } => {
            for type_param in type_params {
                *type_param = replace(type_param, template_result, codebase);
            }
        }
        TAtomic::TAwaitable { ref mut value, .. } => {
            *value = Box::new(replace(value, template_result, codebase));
        }
        TAtomic::TClosure(ref mut closure) => {
            for param in closure.params.iter_mut() {
                if let Some(ref mut t) = param.signature_type {
                    *t = Box::new(replace(t, template_result, codebase));
                }
            }

            if let Some(ref mut return_type) = closure.return_type {
                *return_type = replace(return_type, template_result, codebase);
            }
        }
        TAtomic::TTypeAlias {
            type_params: Some(ref mut type_params),
            ..
        } => {
            for type_param in type_params {
                *type_param = replace(type_param, template_result, codebase);
            }
        }
        _ => (),
    }

    atomic
}
