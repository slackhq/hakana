use std::sync::Arc;

use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::codebase_info::symbols::SymbolKind;
use hakana_reflection_info::codebase_info::{CodebaseInfo, Symbols};
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::member_visibility::MemberVisibility;
use hakana_reflection_info::symbol_references::{ReferenceSource, SymbolReferences};
use hakana_reflection_info::t_atomic::{populate_atomic_type, TAtomic};
use hakana_reflection_info::t_union::{populate_union_type, TUnion};
use hakana_reflection_info::{method_info, Interner, StrId};
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

// as currently constructed this is not efficient memory-wise
pub fn populate_codebase(
    codebase: &mut CodebaseInfo,
    interner: &Interner,
    symbol_references: &mut SymbolReferences,
) {
    let mut all_classlike_descendants = FxHashMap::default();

    let classlike_names = codebase
        .classlike_infos
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();

    for k in &classlike_names {
        populate_classlike_storage(
            k,
            &mut all_classlike_descendants,
            codebase,
            symbol_references,
        );
    }

    for (name, v) in codebase.functionlike_infos.iter_mut() {
        populate_functionlike_storage(
            v,
            &codebase.symbols,
            &ReferenceSource::Symbol(true, *name),
            symbol_references,
        );
    }

    for (name, storage) in codebase.classlike_infos.iter_mut() {
        for (method_name, v) in storage.methods.iter_mut() {
            populate_functionlike_storage(
                v,
                &codebase.symbols,
                &ReferenceSource::ClasslikeMember(true, *name, *method_name),
                symbol_references,
            );
        }

        for (prop_name, v) in storage.properties.iter_mut() {
            populate_union_type(
                &mut v.type_,
                &codebase.symbols,
                &ReferenceSource::ClasslikeMember(true, *name, *prop_name),
                symbol_references,
            );
        }

        for (_, map) in storage.template_extended_params.iter_mut() {
            for (_, v) in map {
                if v.needs_population() {
                    populate_union_type(
                        Arc::make_mut(v),
                        &codebase.symbols,
                        &ReferenceSource::Symbol(true, *name),
                        symbol_references,
                    );
                }
            }
        }

        for (_, map) in storage.template_types.iter_mut() {
            for (_, v) in map {
                if v.needs_population() {
                    populate_union_type(
                        Arc::make_mut(v),
                        &codebase.symbols,
                        &ReferenceSource::Symbol(true, *name),
                        symbol_references,
                    );
                }
            }
        }

        for (_, constant) in storage.constants.iter_mut() {
            if let Some(provided_type) = constant.provided_type.as_mut() {
                populate_union_type(
                    provided_type,
                    &codebase.symbols,
                    &ReferenceSource::Symbol(true, *name),
                    symbol_references,
                );
            }
        }

        for (_, constant_type) in storage.type_constants.iter_mut() {
            if let Some(constant_type) = constant_type {
                populate_union_type(
                    constant_type,
                    &codebase.symbols,
                    &ReferenceSource::Symbol(true, *name),
                    symbol_references,
                );
            }
        }

        if let Some(ref mut enum_type) = storage.enum_type {
            populate_atomic_type(
                enum_type,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
            );
        }

        if let Some(ref mut enum_constraint) = storage.enum_constraint {
            populate_atomic_type(
                enum_constraint,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
            );
        }
    }

    for (name, type_alias) in codebase.type_definitions.iter_mut() {
        populate_union_type(
            &mut type_alias.actual_type,
            &codebase.symbols,
            &ReferenceSource::Symbol(true, *name),
            symbol_references,
        );

        if let Some(ref mut as_type) = type_alias.as_type {
            populate_union_type(
                as_type,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
            );
        }
    }

    for (name, constant) in codebase.constant_infos.iter_mut() {
        if let Some(provided_type) = constant.provided_type.as_mut() {
            populate_union_type(
                provided_type,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
            );
        }
    }

    for (name, file_info) in codebase.files.iter_mut() {
        for (_, functionlike_info) in file_info.closure_infos.iter_mut() {
            populate_functionlike_storage(
                functionlike_info,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
            );
        }
    }

    for (classlike_name, classlike_storage) in &codebase.classlike_infos {
        for parent_interface in &classlike_storage.all_parent_interfaces {
            all_classlike_descendants
                .entry(parent_interface.clone())
                .or_insert_with(FxHashSet::default)
                .insert(classlike_name.clone());
        }

        for class_interface in &classlike_storage.all_class_interfaces {
            all_classlike_descendants
                .entry(class_interface.clone())
                .or_insert_with(FxHashSet::default)
                .insert(classlike_name.clone());
        }

        for parent_class in &classlike_storage.all_parent_classes {
            all_classlike_descendants
                .entry(parent_class.clone())
                .or_insert_with(FxHashSet::default)
                .insert(classlike_name.clone());
        }
    }

    all_classlike_descendants.retain(|k, _| !interner.lookup(k).starts_with("HH\\"));

    codebase.classlike_descendants = all_classlike_descendants;
}

fn populate_functionlike_storage(
    storage: &mut FunctionLikeInfo,
    codebase_symbols: &Symbols,
    reference_source: &ReferenceSource,
    symbol_references: &mut SymbolReferences,
) {
    if let Some(ref mut return_type) = storage.return_type {
        populate_union_type(
            return_type,
            &codebase_symbols,
            reference_source,
            symbol_references,
        );
    }

    for param in storage.params.iter_mut() {
        if let Some(ref mut param_type) = param.signature_type {
            populate_union_type(
                param_type,
                &codebase_symbols,
                reference_source,
                symbol_references,
            );
        }
    }

    for (_, type_param_map) in storage.template_types.iter_mut() {
        for (_, v) in type_param_map {
            if v.needs_population() {
                populate_union_type(
                    Arc::make_mut(v),
                    &codebase_symbols,
                    reference_source,
                    symbol_references,
                );
            }
        }
    }

    for (_, where_type) in storage.where_constraints.iter_mut() {
        populate_union_type(
            where_type,
            &codebase_symbols,
            reference_source,
            symbol_references,
        );
    }
}

fn populate_classlike_storage(
    classlike_name: &StrId,
    all_classlike_descendants: &mut FxHashMap<StrId, FxHashSet<StrId>>,
    codebase: &mut CodebaseInfo,
    symbol_references: &mut SymbolReferences,
) {
    let mut storage = if let Some(storage) = codebase.classlike_infos.remove(classlike_name) {
        storage
    } else {
        return;
    };

    if storage.is_populated {
        codebase
            .classlike_infos
            .insert(classlike_name.clone(), storage);
        return;
    }

    if let Some(classlike_descendants) = all_classlike_descendants.get(classlike_name) {
        if classlike_descendants.contains(classlike_name) {
            codebase
                .classlike_infos
                .insert(classlike_name.clone(), storage);
            // todo complain about circular reference
            return;
        }
    }

    for trait_name in &storage.used_traits.clone() {
        populate_data_from_trait(
            &mut storage,
            all_classlike_descendants,
            codebase,
            trait_name,
            symbol_references,
        );
    }

    if let Some(parent_classname) = &storage.direct_parent_class.clone() {
        populate_data_from_parent_classlike(
            &mut storage,
            all_classlike_descendants,
            codebase,
            parent_classname,
            symbol_references,
        );
    }

    for direct_parent_interface in &storage.direct_parent_interfaces.clone() {
        populate_interface_data_from_parent_interface(
            &mut storage,
            all_classlike_descendants,
            codebase,
            direct_parent_interface,
            symbol_references,
        );
    }

    for direct_class_interface in &storage.direct_class_interfaces.clone() {
        populate_data_from_implemented_interface(
            &mut storage,
            all_classlike_descendants,
            codebase,
            direct_class_interface,
            symbol_references,
        );
    }

    // todo add file references for cache invalidation

    if storage.immutable {
        for (_, functionlike_storage) in storage.methods.iter_mut() {
            if let Some(method_storage) = functionlike_storage.method_info.as_mut() {
                if !method_storage.is_static {
                    method_storage.immutable = true;
                }
            }
        }

        for (_, property_storage) in storage.properties.iter_mut() {
            if !property_storage.is_static {
                property_storage.soft_readonly = true;
            }
        }
    }

    if storage.specialize_instance {
        for (_, functionlike_storage) in storage.methods.iter_mut() {
            if let Some(method_storage) = functionlike_storage.method_info.as_mut() {
                if !method_storage.is_static {
                    functionlike_storage.specialize_call = true;
                }
            }
        }
    }

    storage.is_populated = true;

    codebase
        .classlike_infos
        .insert(classlike_name.clone(), storage);
}

fn populate_interface_data_from_parent_or_implemented_interface(
    storage: &mut ClassLikeInfo,
    interface_storage: &ClassLikeInfo,
) {
    storage.constants.extend(
        interface_storage
            .constants
            .iter()
            .filter(|(k, _)| !storage.constants.contains_key(*k))
            .map(|v| (v.0.clone(), v.1.clone()))
            .collect::<FxHashMap<_, _>>(),
    );

    storage
        .invalid_dependencies
        .extend(interface_storage.invalid_dependencies.clone());

    extend_template_params(storage, interface_storage);

    // todo update dependent classlikes
}

fn populate_interface_data_from_parent_interface(
    storage: &mut ClassLikeInfo,
    all_classlike_descendants: &mut FxHashMap<StrId, FxHashSet<StrId>>,
    codebase: &mut CodebaseInfo,
    parent_storage_interface: &StrId,
    symbol_references: &mut SymbolReferences,
) {
    populate_classlike_storage(
        parent_storage_interface,
        all_classlike_descendants,
        codebase,
        symbol_references,
    );

    symbol_references.add_symbol_reference_to_symbol(storage.name, *parent_storage_interface, true);

    let parent_interface_storage = if let Some(parent_interface_storage) =
        codebase.classlike_infos.get(parent_storage_interface)
    {
        parent_interface_storage
    } else {
        storage
            .invalid_dependencies
            .push(parent_storage_interface.clone());
        return;
    };

    populate_interface_data_from_parent_or_implemented_interface(storage, parent_interface_storage);

    inherit_methods_from_parent(storage, parent_interface_storage, codebase);

    storage
        .all_parent_interfaces
        .extend(parent_interface_storage.all_parent_interfaces.clone());
}

fn populate_data_from_implemented_interface(
    storage: &mut ClassLikeInfo,
    all_classlike_descendants: &mut FxHashMap<StrId, FxHashSet<StrId>>,
    codebase: &mut CodebaseInfo,
    parent_storage_interface: &StrId,

    symbol_references: &mut SymbolReferences,
) {
    populate_classlike_storage(
        parent_storage_interface,
        all_classlike_descendants,
        codebase,
        symbol_references,
    );

    symbol_references.add_symbol_reference_to_symbol(storage.name, *parent_storage_interface, true);

    let implemented_interface_storage = if let Some(implemented_interface_storage) =
        codebase.classlike_infos.get(parent_storage_interface)
    {
        implemented_interface_storage
    } else {
        storage
            .invalid_dependencies
            .push(parent_storage_interface.clone());
        return;
    };

    populate_interface_data_from_parent_or_implemented_interface(
        storage,
        implemented_interface_storage,
    );

    inherit_methods_from_parent(storage, implemented_interface_storage, codebase);

    storage
        .all_class_interfaces
        .extend(implemented_interface_storage.all_parent_interfaces.clone());
}

fn populate_data_from_parent_classlike(
    storage: &mut ClassLikeInfo,
    all_classlike_descendants: &mut FxHashMap<StrId, FxHashSet<StrId>>,
    codebase: &mut CodebaseInfo,
    parent_storage_class: &StrId,
    symbol_references: &mut SymbolReferences,
) {
    populate_classlike_storage(
        parent_storage_class,
        all_classlike_descendants,
        codebase,
        symbol_references,
    );

    symbol_references.add_symbol_reference_to_symbol(storage.name, *parent_storage_class, true);

    let parent_storage = codebase.classlike_infos.get(parent_storage_class);

    let parent_storage = if let Some(parent_storage) = parent_storage {
        parent_storage
    } else {
        storage
            .invalid_dependencies
            .push(parent_storage_class.clone());
        return;
    };

    storage
        .all_parent_classes
        .extend(parent_storage.all_parent_classes.clone());

    extend_template_params(storage, parent_storage);

    inherit_methods_from_parent(storage, parent_storage, codebase);
    inherit_properties_from_parent(storage, parent_storage);

    storage
        .all_class_interfaces
        .extend(parent_storage.all_class_interfaces.clone());
    storage
        .invalid_dependencies
        .extend(parent_storage.invalid_dependencies.clone());

    if parent_storage.has_visitor_issues {
        storage.has_visitor_issues = true;
    }

    storage
        .used_traits
        .extend(parent_storage.used_traits.clone());

    storage.constants.extend(
        parent_storage
            .constants
            .iter()
            .filter(|(k, _)| !storage.constants.contains_key(*k))
            .map(|v| (v.0.clone(), v.1.clone()))
            .collect::<FxHashMap<_, _>>(),
    );

    storage
        .type_constants
        .extend(parent_storage.type_constants.clone());

    if parent_storage.preserve_constructor_signature {
        storage.preserve_constructor_signature = true;
    }

    // todo update parent storage dependent classlikes maybe?
}

fn populate_data_from_trait(
    storage: &mut ClassLikeInfo,
    all_classlike_descendants: &mut FxHashMap<StrId, FxHashSet<StrId>>,
    codebase: &mut CodebaseInfo,
    trait_name: &StrId,
    symbol_references: &mut SymbolReferences,
) {
    populate_classlike_storage(
        trait_name,
        all_classlike_descendants,
        codebase,
        symbol_references,
    );

    symbol_references.add_symbol_reference_to_symbol(storage.name, *trait_name, true);

    let trait_storage = codebase.classlike_infos.get(trait_name);

    let trait_storage = if let Some(trait_storage) = trait_storage {
        trait_storage
    } else {
        storage.invalid_dependencies.push(trait_name.clone());
        return;
    };

    all_classlike_descendants
        .entry(trait_name.clone())
        .or_insert_with(FxHashSet::default)
        .insert(storage.name.clone());

    storage
        .all_class_interfaces
        .extend(trait_storage.direct_class_interfaces.clone());

    inherit_methods_from_parent(storage, trait_storage, codebase);
    inherit_properties_from_parent(storage, trait_storage);
}

fn inherit_methods_from_parent(
    storage: &mut ClassLikeInfo,
    parent_storage: &ClassLikeInfo,
    codebase: &CodebaseInfo,
) {
    let classlike_name = &storage.name;

    for (method_name, appearing_classlike) in &parent_storage.appearing_method_ids {
        if storage.appearing_method_ids.contains_key(method_name) {
            continue;
        }

        let is_trait = matches!(storage.kind, SymbolKind::Trait);

        storage.appearing_method_ids.insert(
            method_name.clone(),
            if is_trait {
                classlike_name.clone()
            } else {
                appearing_classlike.clone()
            },
        );

        if storage.methods.contains_key(method_name) {
            storage
                .potential_declaring_method_ids
                .insert(method_name.clone(), {
                    let mut h = FxHashSet::default();
                    h.insert(classlike_name.clone());
                    h
                });
        } else {
            if let Some(parent_potential_method_ids) = parent_storage
                .potential_declaring_method_ids
                .get(method_name)
            {
                storage
                    .potential_declaring_method_ids
                    .insert(method_name.clone(), parent_potential_method_ids.clone());
            }

            let entry = storage
                .potential_declaring_method_ids
                .entry(method_name.clone())
                .or_insert_with(FxHashSet::default);

            entry.insert(classlike_name.clone());
            entry.insert(parent_storage.name.clone());
        }
    }

    for (method_name, declaring_class) in &parent_storage.inheritable_method_ids {
        if *method_name != StrId::construct() || parent_storage.preserve_constructor_signature {
            storage
                .overridden_method_ids
                .entry(method_name.clone())
                .or_insert_with(FxHashSet::default)
                .insert(declaring_class.clone());

            if let Some(map) = storage.overridden_method_ids.get_mut(method_name) {
                map.extend(
                    parent_storage
                        .overridden_method_ids
                        .get(method_name)
                        .cloned()
                        .unwrap_or(FxHashSet::default()),
                );
            }
        }

        if let Some(existing_declaring_class) = storage.declaring_method_ids.get(method_name) {
            if existing_declaring_class != declaring_class {
                let existing_declaring_class_storage = if existing_declaring_class == &storage.name
                {
                    &storage
                } else {
                    codebase
                        .classlike_infos
                        .get(existing_declaring_class)
                        .unwrap()
                };

                if !matches!(existing_declaring_class_storage.kind, SymbolKind::Interface) {
                    if let Some(functionlike_storage) =
                        existing_declaring_class_storage.methods.get(method_name)
                    {
                        if let Some(method_info) = &functionlike_storage.method_info {
                            if !method_info.is_abstract {
                                continue;
                            }

                            if let Some(functionlike_storage) = storage.methods.get(method_name) {
                                if let Some(method_info) = &functionlike_storage.method_info {
                                    if method_info.is_abstract {
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        storage
            .declaring_method_ids
            .insert(method_name.clone(), declaring_class.clone());

        // traits can pass down methods from other traits,
        // but not from their require extends/implements parents
        if !matches!(storage.kind, SymbolKind::Trait)
            || !storage.required_classlikes.contains(&parent_storage.name)
        {
            storage
                .inheritable_method_ids
                .insert(method_name.clone(), declaring_class.clone());
        }
    }
}

fn inherit_properties_from_parent(storage: &mut ClassLikeInfo, parent_storage: &ClassLikeInfo) {
    let classlike_name = &storage.name;

    // register where they appear (can never be in a trait)
    for (property_name, appearing_classlike) in &parent_storage.appearing_property_ids {
        if storage.appearing_property_ids.contains_key(property_name) {
            continue;
        }

        if !matches!(parent_storage.kind, SymbolKind::Trait) {
            if let Some(parent_property_storage) = parent_storage.properties.get(property_name) {
                if matches!(
                    parent_property_storage.visibility,
                    MemberVisibility::Private
                ) {
                    continue;
                }
            }
        }

        let is_trait = matches!(storage.kind, SymbolKind::Trait);

        storage.appearing_property_ids.insert(
            property_name.clone(),
            if is_trait {
                classlike_name.clone()
            } else {
                appearing_classlike.clone()
            },
        );
    }

    // register where they're declared
    for (property_name, declaring_classlike) in &parent_storage.declaring_property_ids {
        if storage.declaring_property_ids.contains_key(property_name) {
            continue;
        }

        if !matches!(parent_storage.kind, SymbolKind::Trait) {
            if let Some(parent_property_storage) = parent_storage.properties.get(property_name) {
                if matches!(
                    parent_property_storage.visibility,
                    MemberVisibility::Private
                ) {
                    continue;
                }
            }
        }

        storage
            .declaring_property_ids
            .insert(property_name.clone(), declaring_classlike.clone());
    }

    // register inheritance
    for (property_name, inheritable_classlike) in &parent_storage.inheritable_property_ids {
        if !matches!(parent_storage.kind, SymbolKind::Trait) {
            if let Some(parent_property_storage) = parent_storage.properties.get(property_name) {
                if matches!(
                    parent_property_storage.visibility,
                    MemberVisibility::Private
                ) {
                    continue;
                }
            }

            storage
                .overridden_property_ids
                .entry(property_name.clone())
                .or_insert_with(Vec::new)
                .push(inheritable_classlike.clone());
        }

        storage
            .inheritable_property_ids
            .insert(property_name.clone(), inheritable_classlike.clone());
    }
}

fn extend_template_params(storage: &mut ClassLikeInfo, parent_storage: &ClassLikeInfo) {
    if !parent_storage.template_types.is_empty() {
        storage
            .template_extended_params
            .insert(parent_storage.name.clone(), IndexMap::new());

        if let Some(parent_offsets) = storage.template_extended_offsets.get(&parent_storage.name) {
            for (i, extended_type) in parent_offsets.iter().enumerate() {
                let parent_template_type_names =
                    parent_storage.template_types.keys().collect::<Vec<_>>();

                let mapped_name = parent_template_type_names.get(i).cloned();

                if let Some(mapped_name) = mapped_name {
                    let param_map = storage
                        .template_extended_params
                        .get_mut(&parent_storage.name)
                        .unwrap();
                    param_map.insert(mapped_name.clone(), extended_type.clone());
                }
            }

            for (t_storage_class, type_map) in &parent_storage.template_extended_params {
                let existing = storage.template_extended_params.clone();
                for (i, type_) in type_map {
                    storage
                        .template_extended_params
                        .entry(t_storage_class.clone())
                        .or_insert_with(IndexMap::new)
                        .insert(i.clone(), extend_type(type_, &existing));
                }
            }
        } else {
            for (template_name, template_type_map) in &parent_storage.template_types {
                for (_, template_type) in template_type_map {
                    storage
                        .template_extended_params
                        .entry(parent_storage.name.clone())
                        .or_insert_with(IndexMap::new)
                        .insert(template_name.clone(), template_type.clone());
                }

                storage
                    .template_extended_params
                    .extend(parent_storage.template_extended_params.clone());
            }
        }
    } else {
        storage
            .template_extended_params
            .extend(parent_storage.template_extended_params.clone());
    }
}

fn extend_type(
    type_: &Arc<TUnion>,
    template_extended_params: &FxHashMap<StrId, IndexMap<StrId, Arc<TUnion>>>,
) -> Arc<TUnion> {
    if !type_.has_template() {
        return type_.clone();
    }

    let mut extended_types = Vec::new();

    let mut cloned = type_.types.clone();

    while let Some(atomic_type) = cloned.pop() {
        if let TAtomic::TGenericParam {
            defining_entity,
            param_name,
            ..
        } = &atomic_type
        {
            if let Some(ex) = template_extended_params.get(defining_entity) {
                if let Some(referenced_type) = ex.get(param_name) {
                    extended_types.extend(referenced_type.types.clone());
                    continue;
                }
            }
        }

        extended_types.push(atomic_type);
    }

    Arc::new(TUnion::new(extended_types))
}
