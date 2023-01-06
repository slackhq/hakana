use std::sync::Arc;

use hakana_aast_helper::Uses;
use no_pos_hash::{position_insensitive_hash, Hasher, NoPosHash};
use rustc_hash::{FxHashMap, FxHashSet};

use hakana_reflection_info::{
    ast_signature::DefSignatureNode,
    class_constant_info::ConstantInfo,
    classlike_info::{ClassLikeInfo, Variance},
    code_location::HPos,
    codebase_info::{symbols::SymbolKind, CodebaseInfo},
    member_visibility::MemberVisibility,
    property_info::PropertyInfo,
    t_atomic::TAtomic,
    type_resolution::TypeResolutionContext,
    FileSource, StrId, ThreadedInterner,
};
use hakana_type::{get_mixed_any, get_named_object, wrap_atomic};
use indexmap::IndexMap;
use oxidized::{
    aast::{self, ClassConstKind},
    ast_defs::{self, ClassishKind},
};

use crate::{functionlike_scanner::adjust_location_from_comments, simple_type_inferer};
use crate::{get_uses_hash, typehint_resolver::get_type_from_hint};

pub(crate) fn scan(
    codebase: &mut CodebaseInfo,
    interner: &mut ThreadedInterner,
    all_custom_issues: &FxHashSet<String>,
    resolved_names: &FxHashMap<usize, StrId>,
    class_name: &StrId,
    classlike_node: &aast::Class_<(), ()>,
    file_source: &FileSource,
    user_defined: bool,
    comments: &Vec<(oxidized::tast::Pos, oxidized::prim_defs::Comment)>,
    uses_position: Option<(usize, usize)>,
    namespace_position: Option<(usize, usize)>,
    ast_nodes: &mut Vec<DefSignatureNode>,
    all_uses: &Uses,
) -> bool {
    let mut definition_location = HPos::new(&classlike_node.span, file_source.file_path, None);
    let name_location = HPos::new(classlike_node.name.pos(), file_source.file_path, None);

    adjust_location_from_comments(
        comments,
        &mut definition_location,
        file_source,
        &mut FxHashMap::default(),
        all_custom_issues,
    );

    let mut storage =
        match get_classlike_storage(codebase, class_name, definition_location, name_location) {
            Ok(value) => value,
            Err(value) => return value,
        };

    storage.user_defined = user_defined;

    let mut signature_end = storage.name_location.end_offset;

    storage.uses_position = uses_position;
    storage.namespace_position = namespace_position;

    if !classlike_node.tparams.is_empty() {
        let mut type_context = TypeResolutionContext {
            template_type_map: IndexMap::new(),
            template_supers: FxHashMap::default(),
        };

        for type_param_node in classlike_node.tparams.iter() {
            type_context.template_type_map.insert(
                type_param_node.name.1.clone(),
                FxHashMap::from_iter([(class_name.clone(), Arc::new(get_mixed_any()))]),
            );
        }

        for (i, type_param_node) in classlike_node.tparams.iter().enumerate() {
            signature_end = type_param_node.name.0.end_offset();

            if !type_param_node.constraints.is_empty() {
                signature_end = type_param_node
                    .constraints
                    .last()
                    .unwrap()
                    .1
                     .0
                    .end_offset();
            }

            let first_constraint = type_param_node.constraints.first();

            let template_as_type = if let Some((_, constraint_hint)) = first_constraint {
                get_type_from_hint(
                    &constraint_hint.1,
                    Some(&class_name),
                    &type_context,
                    resolved_names,
                )
                .unwrap()
            } else {
                get_mixed_any()
            };

            storage
                .template_types
                .insert(type_param_node.name.1.clone(), {
                    let mut h = FxHashMap::default();
                    h.insert(class_name.clone(), Arc::new(template_as_type));
                    h
                });

            match type_param_node.variance {
                ast_defs::Variance::Covariant => {
                    storage.generic_variance.insert(i, Variance::Covariant);
                }
                ast_defs::Variance::Contravariant => {
                    storage.generic_variance.insert(i, Variance::Contravariant);
                }
                ast_defs::Variance::Invariant => {
                    // default, do nothing

                    if class_name == &interner.intern("HH\\Vector".to_string()) {
                        // cheat here for vectors
                        storage.generic_variance.insert(i, Variance::Covariant);
                    } else {
                        storage.generic_variance.insert(i, Variance::Invariant);
                    }
                }
            }
        }
    }

    match classlike_node.kind {
        ClassishKind::Cclass(abstraction) => {
            storage.is_abstract = matches!(abstraction, ast_defs::Abstraction::Abstract);
            storage.is_final = classlike_node.final_;

            codebase
                .symbols
                .add_class_name(&class_name, Some(file_source.file_path));

            if let Some(parent_class) = classlike_node.extends.first() {
                if let oxidized::tast::Hint_::Happly(name, params) = &*parent_class.1 {
                    signature_end = name.0.end_offset();

                    let parent_name = resolved_names.get(&name.0.start_offset()).unwrap().clone();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset();
                    }

                    storage.direct_parent_class = Some(parent_name.clone());
                    storage.all_parent_classes.insert(parent_name.clone());

                    storage.template_extended_offsets.insert(
                        parent_name.clone(),
                        params
                            .iter()
                            .map(|param| {
                                Arc::new(
                                    get_type_from_hint(
                                        &param.1,
                                        Some(&class_name),
                                        &TypeResolutionContext {
                                            template_type_map: storage.template_types.clone(),
                                            template_supers: FxHashMap::default(),
                                        },
                                        resolved_names,
                                    )
                                    .unwrap(),
                                )
                            })
                            .collect(),
                    );
                }
            }

            for extended_interface in &classlike_node.implements {
                if let oxidized::tast::Hint_::Happly(name, params) = &*extended_interface.1 {
                    signature_end = name.0.end_offset();

                    let interface_name =
                        resolved_names.get(&name.0.start_offset()).unwrap().clone();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset();
                    }

                    storage
                        .direct_class_interfaces
                        .insert(interface_name.clone());
                    storage.all_class_interfaces.insert(interface_name.clone());

                    if interner.lookup(*class_name) == "SimpleXMLElement"
                        && interner.lookup(interface_name) == "HH\\Traversable"
                    {
                        storage.template_extended_offsets.insert(
                            interface_name,
                            vec![Arc::new(get_named_object(*class_name))],
                        );
                    } else {
                        storage.template_extended_offsets.insert(
                            interface_name.clone(),
                            params
                                .iter()
                                .map(|param| {
                                    Arc::new(
                                        get_type_from_hint(
                                            &param.1,
                                            Some(&class_name),
                                            &TypeResolutionContext {
                                                template_type_map: storage.template_types.clone(),
                                                template_supers: FxHashMap::default(),
                                            },
                                            resolved_names,
                                        )
                                        .unwrap(),
                                    )
                                })
                                .collect(),
                        );
                    }
                }
            }
        }
        ClassishKind::CenumClass(abstraction) => {
            storage.is_abstract = matches!(abstraction, ast_defs::Abstraction::Abstract);
            storage.is_final = classlike_node.final_;

            storage.kind = SymbolKind::EnumClass;

            codebase
                .symbols
                .add_enum_class_name(&class_name, Some(file_source.file_path));

            if let Some(enum_node) = &classlike_node.enum_ {
                storage.enum_type = Some(
                    get_type_from_hint(
                        &enum_node.base.1,
                        None,
                        &TypeResolutionContext::new(),
                        resolved_names,
                    )
                    .unwrap()
                    .get_single_owned(),
                );
            }

            // We inherit from this class so methods like `coerce` works
            let enum_class = interner.intern("HH\\BuiltinEnumClass".to_string());

            storage.direct_parent_class = Some(enum_class);
            storage.all_parent_classes.insert(enum_class);

            let mut params = Vec::new();

            params.push(Arc::new(wrap_atomic(TAtomic::TNamedObject {
                name: class_name.clone(),
                type_params: None,
                is_this: false,
                extra_types: None,
                remapped_params: false,
            })));
        }
        ClassishKind::Cinterface => {
            storage.kind = SymbolKind::Interface;
            codebase
                .symbols
                .add_interface_name(&class_name, Some(file_source.file_path));

            handle_reqs(classlike_node, resolved_names, &mut storage, class_name);

            for parent_interface in &classlike_node.extends {
                if let oxidized::tast::Hint_::Happly(name, params) = &*parent_interface.1 {
                    signature_end = name.0.end_offset();

                    let parent_name = resolved_names.get(&name.0.start_offset()).unwrap().clone();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset();
                    }

                    storage.direct_parent_interfaces.insert(parent_name.clone());
                    storage.all_parent_interfaces.insert(parent_name.clone());

                    storage.template_extended_offsets.insert(
                        parent_name.clone(),
                        params
                            .iter()
                            .map(|param| {
                                Arc::new(
                                    get_type_from_hint(
                                        &param.1,
                                        Some(&class_name),
                                        &TypeResolutionContext {
                                            template_type_map: storage.template_types.clone(),
                                            template_supers: FxHashMap::default(),
                                        },
                                        resolved_names,
                                    )
                                    .unwrap(),
                                )
                            })
                            .collect(),
                    );
                }
            }
        }
        ClassishKind::Ctrait => {
            storage.kind = SymbolKind::Trait;

            codebase
                .symbols
                .add_trait_name(&class_name, Some(file_source.file_path));

            handle_reqs(classlike_node, resolved_names, &mut storage, class_name);

            for extended_interface in &classlike_node.implements {
                if let oxidized::tast::Hint_::Happly(name, params) = &*extended_interface.1 {
                    signature_end = name.0.end_offset();

                    let interface_name =
                        resolved_names.get(&name.0.start_offset()).unwrap().clone();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset();
                    }

                    storage
                        .direct_class_interfaces
                        .insert(interface_name.clone());
                    storage.all_class_interfaces.insert(interface_name.clone());

                    storage.template_extended_offsets.insert(
                        interface_name.clone(),
                        params
                            .iter()
                            .map(|param| {
                                Arc::new(
                                    get_type_from_hint(
                                        &param.1,
                                        Some(&class_name),
                                        &TypeResolutionContext {
                                            template_type_map: storage.template_types.clone(),
                                            template_supers: FxHashMap::default(),
                                        },
                                        resolved_names,
                                    )
                                    .unwrap(),
                                )
                            })
                            .collect(),
                    );
                }
            }
        }
        ClassishKind::Cenum => {
            storage.kind = SymbolKind::Enum;

            // We inherit from this class so methods like `coerce` works
            let enum_class = interner.intern("HH\\BuiltinEnum".to_string());

            storage.direct_parent_class = Some(enum_class.clone());
            storage.all_parent_classes.insert(enum_class.clone());

            let mut params = Vec::new();

            params.push(Arc::new(wrap_atomic(TAtomic::TEnum {
                name: class_name.clone(),
                base_type: None,
            })));

            if let Some(enum_node) = &classlike_node.enum_ {
                signature_end = enum_node.base.0.end_offset();

                storage.enum_type = Some(
                    get_type_from_hint(
                        &enum_node.base.1,
                        None,
                        &TypeResolutionContext::new(),
                        resolved_names,
                    )
                    .unwrap()
                    .get_single_owned(),
                );

                if let Some(constraint) = &enum_node.constraint {
                    signature_end = constraint.0.end_offset();

                    storage.enum_constraint = Some(Box::new(
                        get_type_from_hint(
                            &constraint.1,
                            None,
                            &TypeResolutionContext::new(),
                            resolved_names,
                        )
                        .unwrap()
                        .get_single_owned(),
                    ));
                }
            }

            storage
                .template_extended_offsets
                .insert(enum_class.clone(), params);

            codebase
                .symbols
                .add_enum_name(&class_name, Some(file_source.file_path));
        }
    }

    let uses_hash = get_uses_hash(all_uses.symbol_uses.get(&class_name).unwrap_or(&vec![]));

    let mut def_signature_node = DefSignatureNode {
        name: *class_name,
        start_offset: storage.def_location.start_offset,
        end_offset: storage.def_location.end_offset,
        start_line: storage.def_location.start_line,
        end_line: storage.def_location.end_line,
        children: Vec::new(),
        signature_hash: xxhash_rust::xxh3::xxh3_64(
            file_source.file_contents[storage.def_location.start_offset..signature_end].as_bytes(),
        )
        .wrapping_add(uses_hash),
        body_hash: None,
    };

    for trait_use in &classlike_node.uses {
        let trait_type = get_type_from_hint(
            &trait_use.1,
            None,
            &TypeResolutionContext {
                template_type_map: IndexMap::new(),
                template_supers: FxHashMap::default(),
            },
            resolved_names,
        )
        .unwrap()
        .get_single_owned();

        if let TAtomic::TReference { name, .. } = trait_type {
            storage.used_traits.insert(name.clone());

            let mut hasher = rustc_hash::FxHasher::default();
            name.0.hash(&mut hasher);

            def_signature_node.signature_hash = def_signature_node
                .signature_hash
                .wrapping_add(hasher.finish());
        }
    }

    for class_const_node in &classlike_node.consts {
        visit_class_const_declaration(
            class_const_node,
            resolved_names,
            &mut storage,
            file_source,
            &codebase,
            interner,
            &mut def_signature_node.children,
            all_uses,
        );
    }

    for class_typeconst_node in &classlike_node.typeconsts {
        if !class_typeconst_node.is_ctx {
            visit_class_typeconst_declaration(
                class_typeconst_node,
                resolved_names,
                &mut storage,
                file_source,
                interner,
                &mut def_signature_node.children,
                all_uses,
            );
        }
    }

    storage.specialize_instance = true;

    let codegen_id = interner.intern("Codegen".to_string());
    let sealed_id = interner.intern("__Sealed".to_string());

    for user_attribute in &classlike_node.user_attributes {
        let name = resolved_names
            .get(&user_attribute.name.0.start_offset())
            .unwrap()
            .clone();

        if name == codegen_id {
            storage.generated = true;
        }

        if name == sealed_id {
            let mut child_classlikes = FxHashSet::default();

            for attribute_param_expr in &user_attribute.params {
                let attribute_param_type = simple_type_inferer::infer(
                    codebase,
                    &mut FxHashMap::default(),
                    attribute_param_expr,
                    resolved_names,
                );

                if let Some(attribute_param_type) = attribute_param_type {
                    for atomic in attribute_param_type.types.into_iter() {
                        match atomic {
                            TAtomic::TLiteralClassname { name: value } => {
                                child_classlikes.insert(value);
                            }
                            _ => (),
                        }
                    }
                }
            }

            storage.child_classlikes = Some(child_classlikes);
        }
    }

    // todo iterate over enum cases

    for class_property_node in &classlike_node.vars {
        visit_property_declaration(
            class_property_node,
            resolved_names,
            &mut storage,
            file_source,
            interner,
            &mut def_signature_node.children,
            all_uses,
        );
    }

    for xhp_attribute in &classlike_node.xhp_attrs {
        visit_xhp_attribute(
            xhp_attribute,
            resolved_names,
            &mut storage,
            &file_source,
            interner,
        );
    }

    codebase.classlike_infos.insert(class_name.clone(), storage);

    ast_nodes.push(def_signature_node);

    true
}

fn handle_reqs(
    classlike_node: &aast::Class_<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    storage: &mut ClassLikeInfo,
    class_name: &StrId,
) {
    for req in &classlike_node.reqs {
        if let oxidized::tast::Hint_::Happly(name, params) = &*req.0 .1 {
            let require_name = resolved_names.get(&name.0.start_offset()).unwrap().clone();

            match &req.1 {
                aast::RequireKind::RequireExtends => {
                    storage.direct_parent_class = Some(require_name.clone());
                    storage.all_parent_classes.insert(require_name.clone());
                }
                aast::RequireKind::RequireImplements => {
                    storage
                        .direct_parent_interfaces
                        .insert(require_name.clone());
                    storage.all_parent_interfaces.insert(require_name.clone());
                }
                aast::RequireKind::RequireClass => todo!(),
            };

            storage.template_extended_offsets.insert(
                require_name.clone(),
                params
                    .iter()
                    .map(|param| {
                        Arc::new(
                            get_type_from_hint(
                                &param.1,
                                Some(&class_name),
                                &TypeResolutionContext {
                                    template_type_map: storage.template_types.clone(),
                                    template_supers: FxHashMap::default(),
                                },
                                resolved_names,
                            )
                            .unwrap(),
                        )
                    })
                    .collect(),
            );
        }
    }
}

fn visit_xhp_attribute(
    xhp_attribute: &aast::XhpAttr<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    interner: &mut ThreadedInterner,
) {
    let mut attribute_type_location = None;
    let mut attribute_type = if let Some(hint) = &xhp_attribute.0 .1 {
        attribute_type_location = Some(HPos::new(&hint.0, file_source.file_path, None));
        get_type_from_hint(
            &hint.1,
            None,
            &TypeResolutionContext {
                template_type_map: IndexMap::new(),
                template_supers: FxHashMap::default(),
            },
            resolved_names,
        )
        .unwrap()
    } else {
        get_mixed_any()
    };

    if !(if let Some(attr_tag) = &xhp_attribute.2 {
        attr_tag.is_required()
    } else {
        false
    }) && !attribute_type.is_mixed()
        && xhp_attribute.1.expr.is_none()
    {
        attribute_type.types.push(TAtomic::TNull);
    }

    let property_storage = PropertyInfo {
        is_static: false,
        visibility: MemberVisibility::Protected,
        pos: Some(HPos::new(
            xhp_attribute.1.id.pos(),
            file_source.file_path,
            None,
        )),
        stmt_pos: Some(HPos::new(
            &xhp_attribute.1.span,
            file_source.file_path,
            None,
        )),
        type_pos: attribute_type_location,
        type_: attribute_type,
        has_default: xhp_attribute.1.expr.is_some(),
        soft_readonly: false,
        is_promoted: false,
        is_internal: false,
    };

    let attribute_id = interner.intern(xhp_attribute.1.id.1.clone());

    classlike_storage
        .declaring_property_ids
        .insert(attribute_id, classlike_storage.name.clone());
    classlike_storage
        .appearing_property_ids
        .insert(attribute_id, classlike_storage.name.clone());
    classlike_storage
        .inheritable_property_ids
        .insert(attribute_id, classlike_storage.name.clone());
    classlike_storage
        .properties
        .insert(attribute_id, property_storage);
}

fn visit_class_const_declaration(
    const_node: &aast::ClassConst<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    codebase: &CodebaseInfo,
    interner: &mut ThreadedInterner,
    def_child_signature_nodes: &mut Vec<DefSignatureNode>,
    all_uses: &Uses,
) {
    let mut provided_type = None;

    let mut supplied_type_location = None;

    if let Some(supplied_type_hint) = &const_node.type_ {
        provided_type = get_type_from_hint(
            &*supplied_type_hint.1,
            Some(&classlike_storage.name),
            &TypeResolutionContext {
                template_type_map: classlike_storage.template_types.clone(),
                template_supers: FxHashMap::default(),
            },
            resolved_names,
        );

        supplied_type_location = Some(HPos::new(
            &supplied_type_hint.0,
            file_source.file_path,
            None,
        ));
    }

    let def_pos = HPos::new(&const_node.span, file_source.file_path, None);

    let name = interner.intern(const_node.id.1.clone());

    let uses_hash = get_uses_hash(
        all_uses
            .symbol_member_uses
            .get(&(classlike_storage.name, name))
            .unwrap_or(&vec![]),
    );

    def_child_signature_nodes.push(DefSignatureNode {
        name,
        start_offset: def_pos.start_offset,
        end_offset: def_pos.end_offset,
        start_line: def_pos.start_line,
        end_line: def_pos.end_line,
        signature_hash: position_insensitive_hash(const_node).wrapping_add(uses_hash),
        body_hash: None,
        children: vec![],
    });

    let const_storage = ConstantInfo {
        pos: def_pos,
        type_pos: supplied_type_location,
        provided_type,
        inferred_type: if let ClassConstKind::CCAbstract(Some(const_expr))
        | ClassConstKind::CCConcrete(const_expr) = &const_node.kind
        {
            simple_type_inferer::infer(
                codebase,
                &mut FxHashMap::default(),
                const_expr,
                resolved_names,
            )
        } else {
            None
        },
        unresolved_value: None,
        is_abstract: matches!(const_node.kind, ClassConstKind::CCAbstract(..)),
    };

    classlike_storage.constants.insert(name, const_storage);
}

fn visit_class_typeconst_declaration(
    const_node: &aast::ClassTypeconstDef<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    interner: &mut ThreadedInterner,
    def_child_signature_nodes: &mut Vec<DefSignatureNode>,
    all_uses: &Uses,
) {
    let const_type = match &const_node.kind {
        aast::ClassTypeconst::TCAbstract(_) => {
            interner.intern(const_node.name.1.clone());
            return;
        }
        aast::ClassTypeconst::TCConcrete(const_node) => Some(
            get_type_from_hint(
                &const_node.c_tc_type.1,
                Some(&classlike_storage.name),
                &TypeResolutionContext {
                    template_type_map: classlike_storage.template_types.clone(),
                    template_supers: FxHashMap::default(),
                },
                resolved_names,
            )
            .unwrap(),
        ),
    };

    let def_pos = HPos::new(&const_node.span, file_source.file_path, None);

    let name = interner.intern(const_node.name.1.clone());

    let uses_hash = get_uses_hash(
        all_uses
            .symbol_member_uses
            .get(&(classlike_storage.name, name))
            .unwrap_or(&vec![]),
    );

    def_child_signature_nodes.push(DefSignatureNode {
        name,
        start_offset: def_pos.start_offset,
        end_offset: def_pos.end_offset,
        start_line: def_pos.start_line,
        end_line: def_pos.end_line,
        signature_hash: position_insensitive_hash(const_node).wrapping_add(uses_hash),
        body_hash: None,
        children: vec![],
    });

    classlike_storage.type_constants.insert(name, const_type);
}

fn visit_property_declaration(
    property_node: &aast::ClassVar<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    interner: &mut ThreadedInterner,
    def_child_signature_nodes: &mut Vec<DefSignatureNode>,
    all_uses: &Uses,
) {
    let mut property_type = None;

    let mut property_type_location = None;

    if let Some(property_type_hint) = &property_node.type_.1 {
        property_type = get_type_from_hint(
            &*property_type_hint.1,
            Some(&classlike_storage.name),
            &TypeResolutionContext {
                template_type_map: classlike_storage.template_types.clone(),
                template_supers: FxHashMap::default(),
            },
            resolved_names,
        );

        property_type_location = Some(HPos::new(
            &property_type_hint.0,
            file_source.file_path,
            None,
        ));
    }

    let def_pos = HPos::new(&property_node.span, file_source.file_path, None);

    let property_ref_id = interner.intern(property_node.id.1.clone());

    let uses_hash = get_uses_hash(
        all_uses
            .symbol_member_uses
            .get(&(classlike_storage.name, property_ref_id))
            .unwrap_or(&vec![]),
    );

    def_child_signature_nodes.push(DefSignatureNode {
        name: property_ref_id,
        start_offset: def_pos.start_offset,
        end_offset: def_pos.end_offset,
        start_line: def_pos.start_line,
        end_line: def_pos.end_line,
        signature_hash: xxhash_rust::xxh3::xxh3_64(
            file_source.file_contents[def_pos.start_offset..def_pos.end_offset].as_bytes(),
        )
        .wrapping_add(uses_hash),
        body_hash: None,
        children: vec![],
    });

    let property_storage = PropertyInfo {
        is_static: property_node.is_static,
        visibility: match property_node.visibility {
            ast_defs::Visibility::Private => MemberVisibility::Private,
            ast_defs::Visibility::Public | ast_defs::Visibility::Internal => {
                MemberVisibility::Public
            }
            ast_defs::Visibility::Protected => MemberVisibility::Protected,
        },
        pos: Some(HPos::new(
            property_node.id.pos(),
            file_source.file_path,
            None,
        )),
        stmt_pos: Some(def_pos),
        type_pos: property_type_location,
        type_: property_type.unwrap_or(get_mixed_any()),
        has_default: property_node.expr.is_some(),
        soft_readonly: false,
        is_promoted: false,
        is_internal: matches!(property_node.visibility, ast_defs::Visibility::Internal),
    };

    classlike_storage
        .declaring_property_ids
        .insert(property_ref_id, classlike_storage.name.clone());

    classlike_storage
        .appearing_property_ids
        .insert(property_ref_id, classlike_storage.name.clone());

    if !matches!(property_node.visibility, ast_defs::Visibility::Private) {
        classlike_storage
            .inheritable_property_ids
            .insert(property_ref_id, classlike_storage.name.clone());
    }

    classlike_storage
        .properties
        .insert(property_ref_id, property_storage);
}

fn get_classlike_storage(
    codebase: &mut CodebaseInfo,
    class_name: &StrId,
    definition_pos: HPos,
    name_pos: HPos,
) -> Result<ClassLikeInfo, bool> {
    let mut storage;
    if let Some(duplicate_storage) = codebase.classlike_infos.get(class_name) {
        if !codebase.register_stub_files {
            return Err(false);
        } else {
            //is_classlike_overridden = true;

            storage = duplicate_storage.clone();
            storage.is_populated = false;
            storage.all_class_interfaces = FxHashSet::default();
            storage.direct_class_interfaces = FxHashSet::default();
            storage.is_stubbed = true;

            // todo maybe handle dependent classlikes
        }
    } else {
        storage = ClassLikeInfo::new(class_name.clone(), definition_pos, name_pos);
    }
    storage.is_user_defined = !codebase.register_stub_files;
    storage.is_stubbed = codebase.register_stub_files;
    Ok(storage)
}
