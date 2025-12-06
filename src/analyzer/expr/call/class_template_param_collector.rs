use std::sync::Arc;

use hakana_code_info::t_atomic::{TGenericParam, TNamedObject};
use hakana_str::StrId;
use rustc_hash::FxHashMap;

use hakana_code_info::ttype::{add_optional_union_type, get_mixed_any, wrap_atomic};
use hakana_code_info::{
    GenericParent, classlike_info::ClassLikeInfo, codebase_info::CodebaseInfo, t_atomic::TAtomic,
    t_union::TUnion,
};
use indexmap::IndexMap;

pub(crate) fn collect(
    codebase: &CodebaseInfo,
    class_storage: &ClassLikeInfo,
    static_class_storage: &ClassLikeInfo,
    lhs_type_part: Option<&TAtomic>, // default None
) -> Option<IndexMap<StrId, FxHashMap<GenericParent, TUnion>>> {
    let template_types = &class_storage.template_types;

    if template_types.is_empty() {
        return None;
    }

    let mut class_template_params = IndexMap::new();

    let e = &static_class_storage.template_extended_params;

    if let Some(TAtomic::TNamedObject(TNamedObject {
        type_params: Some(lhs_type_params),
        ..
    })) = &lhs_type_part
    {
        if class_storage.name == static_class_storage.name
            && !static_class_storage.template_types.is_empty()
        {
            for (i, (type_name, _)) in class_storage.template_types.iter().enumerate() {
                if let Some(type_param) = lhs_type_params.get(i) {
                    class_template_params
                        .entry(*type_name)
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            GenericParent::ClassLike(class_storage.name),
                            type_param.clone(),
                        );
                }
            }
        }

        for (template_name, _) in template_types {
            if class_template_params.contains_key(template_name) {
                continue;
            }

            if class_storage.name != static_class_storage.name {
                if let Some(input_type_extends) = e
                    .get(&class_storage.name)
                    .unwrap_or(&IndexMap::new())
                    .get(template_name)
                {
                    let output_type_extends = resolve_template_param(
                        codebase,
                        input_type_extends,
                        static_class_storage,
                        lhs_type_params,
                    );

                    class_template_params
                        .entry(*template_name)
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            GenericParent::ClassLike(class_storage.name),
                            output_type_extends.unwrap_or(get_mixed_any()),
                        );
                }
            }

            class_template_params
                .entry(*template_name)
                .or_insert_with(FxHashMap::default)
                .entry(GenericParent::ClassLike(class_storage.name))
                .or_insert(get_mixed_any());
        }
    }

    for (template_name, type_map) in template_types {
        for (template_classname, type_) in type_map {
            if class_storage.name != static_class_storage.name {
                if let Some(extended_type) = e
                    .get(&class_storage.name)
                    .unwrap_or(&IndexMap::new())
                    .get(template_name)
                {
                    class_template_params
                        .entry(*template_name)
                        .or_insert_with(FxHashMap::default)
                        .entry(GenericParent::ClassLike(class_storage.name))
                        .or_insert(TUnion::new(expand_type(
                            extended_type,
                            e,
                            &static_class_storage.name,
                            &static_class_storage.template_types,
                        )));
                }
            }

            let self_call = if let Some(TAtomic::TNamedObject(TNamedObject {
                is_this: true,
                name: self_class_name,
                ..
            })) = lhs_type_part
            {
                template_classname == &GenericParent::ClassLike(*self_class_name)
            } else {
                false
            };

            if !self_call {
                class_template_params
                    .entry(*template_name)
                    .or_insert_with(FxHashMap::default)
                    .entry(GenericParent::ClassLike(class_storage.name))
                    .or_insert((**type_).clone());
            }
        }
    }

    Some(class_template_params)
}

pub(crate) fn resolve_template_param(
    codebase: &CodebaseInfo,
    input_type_extends: &TUnion,
    static_class_storage: &ClassLikeInfo,
    type_params: &Vec<TUnion>,
) -> Option<TUnion> {
    let mut output_type_extends = None;

    for type_extends_atomic in &input_type_extends.types {
        if let TAtomic::TGenericParam(TGenericParam {
            param_name,
            defining_entity: GenericParent::ClassLike(defining_entity),
            ..
        }) = &type_extends_atomic
        {
            if let Some(entry) = static_class_storage
                .template_types
                .iter()
                .enumerate()
                .find(|(_, (k, _))| k == param_name)
            {
                let mapped_offset = entry.0;

                if let Some(type_param) = type_params.get(mapped_offset) {
                    output_type_extends = Some(add_optional_union_type(
                        type_param.clone(),
                        output_type_extends.as_ref(),
                        codebase,
                    ));
                }
            } else if let Some(input_type_extends) = static_class_storage
                .template_extended_params
                .get(defining_entity)
                .unwrap_or(&IndexMap::new())
                .get(param_name)
            {
                let nested_output_type = resolve_template_param(
                    codebase,
                    input_type_extends,
                    static_class_storage,
                    type_params,
                );

                if let Some(nested_output_type) = nested_output_type {
                    output_type_extends = Some(add_optional_union_type(
                        nested_output_type,
                        output_type_extends.as_ref(),
                        codebase,
                    ));
                }
            }
        } else {
            output_type_extends = Some(add_optional_union_type(
                wrap_atomic(type_extends_atomic.clone()),
                output_type_extends.as_ref(),
                codebase,
            ));
        }
    }

    output_type_extends
}

fn expand_type(
    input_type_extends: &Arc<TUnion>,
    e: &FxHashMap<StrId, IndexMap<StrId, Arc<TUnion>>>,
    static_classlike_name: &StrId,
    static_template_types: &Vec<(StrId, Vec<(GenericParent, Arc<TUnion>)>)>,
) -> Vec<TAtomic> {
    let mut output_type_extends = Vec::new();

    for type_extends_atomic in &input_type_extends.types {
        if let Some(extended_type) = if let TAtomic::TGenericParam(TGenericParam {
            param_name,
            defining_entity: GenericParent::ClassLike(defining_entity),
            ..
        }) = type_extends_atomic
        {
            if static_classlike_name != defining_entity
                || !static_template_types.iter().any(|(k, _)| k == param_name)
            {
                if let Some(extended_type_map) = e.get(defining_entity) {
                    extended_type_map.get(param_name)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        } {
            output_type_extends.extend(expand_type(
                extended_type,
                e,
                static_classlike_name,
                static_template_types,
            ));
        } else {
            // todo handle TClassConstant

            output_type_extends.push(type_extends_atomic.clone());
        }
    }

    output_type_extends
}
