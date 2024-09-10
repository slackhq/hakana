use std::sync::Arc;

use hakana_analyzer::config::Config;
use hakana_code_info::classlike_info::{ClassConstantType, ClassLikeInfo};
use hakana_code_info::codebase_info::symbols::SymbolKind;
use hakana_code_info::codebase_info::{CodebaseInfo, Symbols};
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::member_visibility::MemberVisibility;
use hakana_code_info::symbol_references::{ReferenceSource, SymbolReferences};
use hakana_code_info::t_atomic::{populate_atomic_type, TAtomic};
use hakana_code_info::t_union::{populate_union_type, TUnion};
use hakana_code_info::GenericParent;
use hakana_str::{Interner, StrId};
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

// as currently constructed this is not efficient memory-wise
pub fn populate_codebase(
    codebase: &mut CodebaseInfo,
    interner: &Interner,
    symbol_references: &mut SymbolReferences,
    safe_symbols: FxHashSet<StrId>,
    safe_symbol_members: FxHashSet<(StrId, StrId)>,
    config: &Config,
) {
    let new_classlike_names = codebase
        .classlike_infos
        .iter()
        .filter(|(name, storage)| {
            !storage.is_populated || (storage.user_defined && !safe_symbols.contains(name))
        })
        .map(|(k, _)| *k)
        .collect::<Vec<_>>();

    for k in &new_classlike_names {
        if let Some(classlike_info) = codebase.classlike_infos.get_mut(k) {
            classlike_info.is_populated = false;
            classlike_info.declaring_property_ids = FxHashMap::default();
            classlike_info.appearing_property_ids = FxHashMap::default();
            classlike_info.declaring_method_ids = FxHashMap::default();
            classlike_info.appearing_method_ids = FxHashMap::default();
        }
    }

    for k in &new_classlike_names {
        populate_classlike_storage(k, codebase, symbol_references, &safe_symbols);
    }

    for (name, v) in codebase.functionlike_infos.iter_mut() {
        populate_functionlike_storage(
            v,
            &codebase.symbols,
            &if name.1 == StrId::EMPTY || v.is_closure {
                ReferenceSource::Symbol(true, name.0)
            } else {
                ReferenceSource::ClasslikeMember(true, name.0, name.1)
            },
            symbol_references,
            v.user_defined && !safe_symbols.contains(&name.0),
            config,
        );
    }

    for (name, storage) in codebase.classlike_infos.iter_mut() {
        let userland_force_repopulation = storage.user_defined && !safe_symbols.contains(name);

        for (prop_name, v) in storage.properties.iter_mut() {
            populate_union_type(
                &mut v.type_,
                &codebase.symbols,
                &ReferenceSource::ClasslikeMember(true, *name, *prop_name),
                symbol_references,
                userland_force_repopulation,
            );
        }

        for (_, map) in storage.template_extended_params.iter_mut() {
            for (_, v) in map {
                if v.needs_population() || userland_force_repopulation {
                    populate_union_type(
                        Arc::make_mut(v),
                        &codebase.symbols,
                        &ReferenceSource::Symbol(true, *name),
                        symbol_references,
                        userland_force_repopulation,
                    );
                }
            }
        }

        for (_, map) in storage.template_types.iter_mut() {
            for (_, v) in map {
                if v.needs_population() || userland_force_repopulation {
                    populate_union_type(
                        Arc::make_mut(v),
                        &codebase.symbols,
                        &ReferenceSource::Symbol(true, *name),
                        symbol_references,
                        userland_force_repopulation,
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
                    userland_force_repopulation,
                );
            }

            if let Some(inferred_type) = constant.inferred_type.as_mut() {
                populate_atomic_type(
                    inferred_type,
                    &codebase.symbols,
                    &ReferenceSource::Symbol(true, *name),
                    symbol_references,
                    userland_force_repopulation,
                );
            }
        }

        for (_, type_constant_info) in storage.type_constants.iter_mut() {
            match type_constant_info {
                ClassConstantType::Concrete(type_) | ClassConstantType::Abstract(Some(type_)) => {
                    populate_union_type(
                        type_,
                        &codebase.symbols,
                        &ReferenceSource::Symbol(true, *name),
                        symbol_references,
                        userland_force_repopulation,
                    );
                }
                _ => {}
            }
        }

        if let Some(ref mut enum_type) = storage.enum_type {
            populate_atomic_type(
                enum_type,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
                userland_force_repopulation,
            );
        }

        if let Some(ref mut enum_constraint) = storage.enum_constraint {
            populate_atomic_type(
                enum_constraint,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
                userland_force_repopulation,
            );
        }
    }

    for (name, type_alias) in codebase.type_definitions.iter_mut() {
        for attribute_info in &type_alias.attributes {
            symbol_references.add_symbol_reference_to_symbol(*name, attribute_info.name, true);
        }

        populate_union_type(
            &mut type_alias.actual_type,
            &codebase.symbols,
            &ReferenceSource::Symbol(true, *name),
            symbol_references,
            type_alias.user_defined,
        );

        if let Some(ref mut as_type) = type_alias.as_type {
            populate_union_type(
                as_type,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
                type_alias.user_defined,
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
                !safe_symbols.contains(name),
            );
        }

        if let Some(inferred_type) = constant.inferred_type.as_mut() {
            populate_atomic_type(
                inferred_type,
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *name),
                symbol_references,
                !safe_symbols.contains(name),
            );
        }
    }

    let mut direct_classlike_descendants = FxHashMap::default();

    let mut all_classlike_descendants = FxHashMap::default();

    for (classlike_name, classlike_storage) in &codebase.classlike_infos {
        for parent_interface in &classlike_storage.all_parent_interfaces {
            all_classlike_descendants
                .entry(*parent_interface)
                .or_insert_with(FxHashSet::default)
                .insert(*classlike_name);
        }

        for parent_interface in &classlike_storage.direct_parent_interfaces {
            direct_classlike_descendants
                .entry(*parent_interface)
                .or_insert_with(FxHashSet::default)
                .insert(*classlike_name);
        }

        for parent_class in &classlike_storage.all_parent_classes {
            all_classlike_descendants
                .entry(*parent_class)
                .or_insert_with(FxHashSet::default)
                .insert(*classlike_name);
        }

        for used_trait in &classlike_storage.used_traits {
            all_classlike_descendants
                .entry(*used_trait)
                .or_default()
                .insert(*classlike_name);
        }

        if let Some(parent_class) = classlike_storage.direct_parent_class {
            direct_classlike_descendants
                .entry(parent_class)
                .or_insert_with(FxHashSet::default)
                .insert(*classlike_name);
        }
    }

    all_classlike_descendants.retain(|k, _| !interner.lookup(k).starts_with("HH\\"));
    direct_classlike_descendants.retain(|k, _| !interner.lookup(k).starts_with("HH\\"));

    codebase.all_classlike_descendants = all_classlike_descendants;
    codebase.direct_classlike_descendants = direct_classlike_descendants;
    codebase.safe_symbols = safe_symbols;
    codebase.safe_symbol_members = safe_symbol_members;
}

fn populate_functionlike_storage(
    storage: &mut FunctionLikeInfo,
    codebase_symbols: &Symbols,
    reference_source: &ReferenceSource,
    symbol_references: &mut SymbolReferences,
    force_type_population: bool,
    config: &Config,
) {
    if storage.is_populated && !force_type_population {
        return;
    }

    storage.is_populated = true;

    if !storage.user_defined && !storage.is_closure {
        if let ReferenceSource::Symbol(true, function_id) = reference_source {
            if let Some(banned_message) = config.banned_builtin_functions.get(function_id) {
                storage.banned_function_message = Some(*banned_message);
            }
        }
    }

    for attribute_info in &storage.attributes {
        match reference_source {
            ReferenceSource::Symbol(_, a) => {
                symbol_references.add_symbol_reference_to_symbol(*a, attribute_info.name, true)
            }
            ReferenceSource::ClasslikeMember(_, a, b) => symbol_references
                .add_class_member_reference_to_symbol((*a, *b), attribute_info.name, true),
        }
    }

    if let Some(ref mut return_type) = storage.return_type {
        populate_union_type(
            return_type,
            codebase_symbols,
            reference_source,
            symbol_references,
            force_type_population,
        );
    }

    for param in storage.params.iter_mut() {
        if let Some(ref mut param_type) = param.signature_type {
            populate_union_type(
                param_type,
                codebase_symbols,
                reference_source,
                symbol_references,
                force_type_population,
            );
        }

        for attribute_info in &param.attributes {
            match reference_source {
                ReferenceSource::Symbol(in_signature, a) => symbol_references
                    .add_symbol_reference_to_symbol(*a, attribute_info.name, *in_signature),
                ReferenceSource::ClasslikeMember(in_signature, a, b) => symbol_references
                    .add_class_member_reference_to_symbol(
                        (*a, *b),
                        attribute_info.name,
                        *in_signature,
                    ),
            }
        }
    }

    for (_, type_param_map) in storage.template_types.iter_mut() {
        for (_, v) in type_param_map {
            if force_type_population || v.needs_population() {
                populate_union_type(
                    Arc::make_mut(v),
                    codebase_symbols,
                    reference_source,
                    symbol_references,
                    force_type_population,
                );
            }
        }
    }

    if let Some(ref mut type_resolution_context) = storage.type_resolution_context {
        for (_, type_param_map) in type_resolution_context.template_type_map.iter_mut() {
            for (_, v) in type_param_map {
                if force_type_population || v.needs_population() {
                    populate_union_type(
                        Arc::make_mut(v),
                        codebase_symbols,
                        reference_source,
                        symbol_references,
                        force_type_population,
                    );
                }
            }
        }
    }

    for (_, where_type) in storage.where_constraints.iter_mut() {
        populate_union_type(
            where_type,
            codebase_symbols,
            reference_source,
            symbol_references,
            force_type_population,
        );
    }
}

fn populate_classlike_storage(
    classlike_name: &StrId,
    codebase: &mut CodebaseInfo,
    symbol_references: &mut SymbolReferences,
    safe_symbols: &FxHashSet<StrId>,
) {
    let mut storage = if let Some(storage) = codebase.classlike_infos.remove(classlike_name) {
        storage
    } else {
        return;
    };

    if storage.is_populated {
        codebase.classlike_infos.insert(*classlike_name, storage);
        return;
    }

    for attribute_info in &storage.attributes {
        symbol_references.add_symbol_reference_to_symbol(storage.name, attribute_info.name, true);
    }

    for property_id in storage.properties.keys() {
        storage
            .declaring_property_ids
            .insert(*property_id, *classlike_name);
        storage
            .appearing_property_ids
            .insert(*property_id, *classlike_name);
    }

    for method_name in &storage.methods {
        storage
            .declaring_method_ids
            .insert(*method_name, *classlike_name);
        storage
            .appearing_method_ids
            .insert(*method_name, *classlike_name);
    }

    for (_, param_types) in storage.template_extended_offsets.iter_mut() {
        for param_type in param_types {
            populate_union_type(
                Arc::make_mut(param_type),
                &codebase.symbols,
                &ReferenceSource::Symbol(true, *classlike_name),
                symbol_references,
                !safe_symbols.contains(classlike_name),
            );
        }
    }

    for trait_name in &storage.used_traits.clone() {
        populate_data_from_trait(
            &mut storage,
            codebase,
            trait_name,
            symbol_references,
            safe_symbols,
        );
    }

    if let Some(parent_classname) = &storage.direct_parent_class.clone() {
        populate_data_from_parent_classlike(
            &mut storage,
            codebase,
            parent_classname,
            symbol_references,
            safe_symbols,
        );
    }

    for direct_enum_extends in &storage.enum_class_extends.clone() {
        populate_data_from_parent_classlike(
            &mut storage,
            codebase,
            direct_enum_extends,
            symbol_references,
            safe_symbols,
        );
    }

    for direct_parent_interface in &storage.direct_parent_interfaces.clone() {
        populate_interface_data_from_parent_interface(
            &mut storage,
            codebase,
            direct_parent_interface,
            symbol_references,
            safe_symbols,
        );
    }

    // todo add file references for cache invalidation

    if storage.immutable {
        for (_, property_storage) in storage.properties.iter_mut() {
            if !property_storage.is_static {
                property_storage.soft_readonly = true;
            }
        }
    }

    if storage.specialize_instance {
        for method_name in &storage.methods {
            let functionlike_storage = codebase
                .functionlike_infos
                .get_mut(&(storage.name, *method_name))
                .unwrap();
            if let Some(method_storage) = &functionlike_storage.method_info {
                if !method_storage.is_static {
                    functionlike_storage.specialize_call = true;
                }
            }
        }
    }

    storage.all_parent_interfaces.shrink_to_fit();
    storage.all_parent_classes.shrink_to_fit();
    storage.direct_parent_interfaces.shrink_to_fit();
    storage.appearing_method_ids.shrink_to_fit();
    storage.declaring_method_ids.shrink_to_fit();
    storage.appearing_property_ids.shrink_to_fit();
    storage.declaring_property_ids.shrink_to_fit();
    storage.methods.shrink_to_fit();

    storage.is_populated = true;

    codebase.classlike_infos.insert(*classlike_name, storage);
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
            .map(|v| (*v.0, v.1.clone()))
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
    codebase: &mut CodebaseInfo,
    parent_storage_interface: &StrId,
    symbol_references: &mut SymbolReferences,
    safe_symbols: &FxHashSet<StrId>,
) {
    populate_classlike_storage(
        parent_storage_interface,
        codebase,
        symbol_references,
        safe_symbols,
    );

    symbol_references.add_symbol_reference_to_symbol(storage.name, *parent_storage_interface, true);

    let parent_interface_storage = if let Some(parent_interface_storage) =
        codebase.classlike_infos.get(parent_storage_interface)
    {
        parent_interface_storage
    } else {
        storage.invalid_dependencies.push(*parent_storage_interface);
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
    codebase: &mut CodebaseInfo,
    parent_storage_interface: &StrId,
    symbol_references: &mut SymbolReferences,
    safe_symbols: &FxHashSet<StrId>,
) {
    populate_classlike_storage(
        parent_storage_interface,
        codebase,
        symbol_references,
        safe_symbols,
    );

    symbol_references.add_symbol_reference_to_symbol(storage.name, *parent_storage_interface, true);

    let implemented_interface_storage = if let Some(implemented_interface_storage) =
        codebase.classlike_infos.get(parent_storage_interface)
    {
        implemented_interface_storage
    } else {
        storage.invalid_dependencies.push(*parent_storage_interface);
        return;
    };

    populate_interface_data_from_parent_or_implemented_interface(
        storage,
        implemented_interface_storage,
    );

    inherit_methods_from_parent(storage, implemented_interface_storage, codebase);

    storage
        .all_parent_interfaces
        .extend(implemented_interface_storage.all_parent_interfaces.clone());
}

fn populate_data_from_parent_classlike(
    storage: &mut ClassLikeInfo,
    codebase: &mut CodebaseInfo,
    parent_storage_class: &StrId,
    symbol_references: &mut SymbolReferences,
    safe_symbols: &FxHashSet<StrId>,
) {
    populate_classlike_storage(
        parent_storage_class,
        codebase,
        symbol_references,
        safe_symbols,
    );

    symbol_references.add_symbol_reference_to_symbol(storage.name, *parent_storage_class, true);

    let parent_storage = codebase.classlike_infos.get(parent_storage_class);

    let parent_storage = if let Some(parent_storage) = parent_storage {
        parent_storage
    } else {
        storage.invalid_dependencies.push(*parent_storage_class);
        return;
    };

    storage
        .all_parent_classes
        .extend(parent_storage.all_parent_classes.clone());

    extend_template_params(storage, parent_storage);

    inherit_methods_from_parent(storage, parent_storage, codebase);
    inherit_properties_from_parent(storage, parent_storage);

    storage
        .all_parent_interfaces
        .extend(parent_storage.all_parent_interfaces.clone());
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
            .map(|v| (*v.0, v.1.clone()))
            .collect::<FxHashMap<_, _>>(),
    );

    for (name, type_info) in &parent_storage.type_constants {
        if !storage.type_constants.contains_key(name) {
            storage.type_constants.insert(*name, type_info.clone());
        }
    }

    if parent_storage.preserve_constructor_signature {
        storage.preserve_constructor_signature = true;
    }

    // todo update parent storage dependent classlikes maybe?
}

fn populate_data_from_trait(
    storage: &mut ClassLikeInfo,
    codebase: &mut CodebaseInfo,
    trait_name: &StrId,
    symbol_references: &mut SymbolReferences,
    safe_symbols: &FxHashSet<StrId>,
) {
    populate_classlike_storage(trait_name, codebase, symbol_references, safe_symbols);

    symbol_references.add_symbol_reference_to_symbol(storage.name, *trait_name, true);

    let trait_storage = codebase.classlike_infos.get(trait_name);

    let trait_storage = if let Some(trait_storage) = trait_storage {
        trait_storage
    } else {
        storage.invalid_dependencies.push(*trait_name);
        return;
    };

    storage.constants.extend(
        trait_storage
            .constants
            .iter()
            .filter(|(k, _)| !storage.constants.contains_key(*k))
            .map(|v| (*v.0, v.1.clone()))
            .collect::<FxHashMap<_, _>>(),
    );

    storage
        .all_parent_interfaces
        .extend(trait_storage.direct_parent_interfaces.clone());

    for (name, type_info) in &trait_storage.type_constants {
        if let Some(ClassConstantType::Concrete(_)) = storage.type_constants.get(name) {
            // do nothing
        } else {
            storage.type_constants.insert(*name, type_info.clone());
        }
    }

    extend_template_params(storage, trait_storage);

    inherit_methods_from_parent(storage, trait_storage, codebase);
    inherit_properties_from_parent(storage, trait_storage);
}

#[allow(clippy::needless_borrow)]
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
            *method_name,
            if is_trait {
                *classlike_name
            } else {
                *appearing_classlike
            },
        );

        if codebase
            .functionlike_infos
            .contains_key(&(*classlike_name, *method_name))
        {
            storage
                .potential_declaring_method_ids
                .insert(*method_name, {
                    let mut h = FxHashSet::default();
                    h.insert(*classlike_name);
                    h
                });
        } else {
            if let Some(parent_potential_method_ids) = parent_storage
                .potential_declaring_method_ids
                .get(method_name)
            {
                storage
                    .potential_declaring_method_ids
                    .insert(*method_name, parent_potential_method_ids.clone());
            }

            let entry = storage
                .potential_declaring_method_ids
                .entry(*method_name)
                .or_default();

            entry.insert(*classlike_name);
            entry.insert(parent_storage.name);
        }
    }

    for (method_name, declaring_class) in &parent_storage.inheritable_method_ids {
        if *method_name != StrId::EMPTY || parent_storage.preserve_constructor_signature {
            storage
                .overridden_method_ids
                .entry(*method_name)
                .or_default()
                .insert(*declaring_class);

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
                } else if let Some(storage) = codebase.classlike_infos.get(existing_declaring_class)
                {
                    storage
                } else {
                    continue;
                };

                if !matches!(existing_declaring_class_storage.kind, SymbolKind::Interface) {
                    if let Some(functionlike_storage) = codebase
                        .functionlike_infos
                        .get(&(existing_declaring_class_storage.name, *method_name))
                    {
                        if let Some(method_info) = &functionlike_storage.method_info {
                            if !method_info.is_abstract {
                                continue;
                            }

                            if let Some(functionlike_storage) = codebase
                                .functionlike_infos
                                .get(&(storage.name, *method_name))
                            {
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
            .insert(*method_name, *declaring_class);

        // traits can pass down methods from other traits,
        // but not from their require extends/implements parents
        if !matches!(storage.kind, SymbolKind::Trait)
            || !storage.required_classlikes.contains(&parent_storage.name)
        {
            storage
                .inheritable_method_ids
                .insert(*method_name, *declaring_class);
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
            *property_name,
            if is_trait {
                *classlike_name
            } else {
                *appearing_classlike
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
            .insert(*property_name, *declaring_classlike);
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
                .entry(*property_name)
                .or_default()
                .push(*inheritable_classlike);
        }

        storage
            .inheritable_property_ids
            .insert(*property_name, *inheritable_classlike);
    }
}

fn extend_template_params(storage: &mut ClassLikeInfo, parent_storage: &ClassLikeInfo) {
    if !parent_storage.template_types.is_empty() {
        storage
            .template_extended_params
            .insert(parent_storage.name, IndexMap::new());

        if let Some(parent_offsets) = storage.template_extended_offsets.get(&parent_storage.name) {
            for (i, extended_type) in parent_offsets.iter().enumerate() {
                let parent_template_type_names = parent_storage
                    .template_types
                    .iter()
                    .map(|(k, _)| k)
                    .collect::<Vec<_>>();

                let mapped_name = parent_template_type_names.get(i).cloned();

                if let Some(mapped_name) = mapped_name {
                    let param_map = storage
                        .template_extended_params
                        .get_mut(&parent_storage.name)
                        .unwrap();
                    param_map.insert(*mapped_name, extended_type.clone());
                }
            }

            for (t_storage_class, type_map) in &parent_storage.template_extended_params {
                let existing = storage.template_extended_params.clone();
                for (i, type_) in type_map {
                    storage
                        .template_extended_params
                        .entry(*t_storage_class)
                        .or_default()
                        .insert(*i, extend_type(type_, &existing));
                }
            }
        } else {
            for (template_name, template_type_map) in &parent_storage.template_types {
                for (_, template_type) in template_type_map {
                    storage
                        .template_extended_params
                        .entry(parent_storage.name)
                        .or_default()
                        .insert(*template_name, template_type.clone());
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
            defining_entity: GenericParent::ClassLike(defining_entity),
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
