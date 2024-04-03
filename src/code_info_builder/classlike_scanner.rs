use std::sync::Arc;

use hakana_aast_helper::Uses;
use hakana_str::{StrId, ThreadedInterner};
use no_pos_hash::{position_insensitive_hash, Hasher, NoPosHash};
use rustc_hash::{FxHashMap, FxHashSet};

use hakana_reflection_info::{
    ast_signature::DefSignatureNode,
    attribute_info::AttributeInfo,
    class_constant_info::ConstantInfo,
    classlike_info::{ClassConstantType, ClassLikeInfo, Variance},
    code_location::HPos,
    codebase_info::{symbols::SymbolKind, CodebaseInfo},
    functionlike_info::MetaStart,
    member_visibility::MemberVisibility,
    property_info::{PropertyInfo, PropertyKind},
    t_atomic::TAtomic,
    type_resolution::TypeResolutionContext,
    FileSource, GenericParent,
};
use hakana_type::{get_mixed_any, get_named_object, wrap_atomic};
use oxidized::{
    aast::{self, ClassConstKind},
    ast_defs::{self, ClassishKind},
};

use crate::{
    functionlike_scanner::{self, adjust_location_from_comments},
    get_function_hashes, simple_type_inferer,
};
use crate::{get_uses_hash, typehint_resolver::get_type_from_hint};

pub(crate) fn scan(
    codebase: &mut CodebaseInfo,
    interner: &mut ThreadedInterner,
    all_custom_issues: &FxHashSet<String>,
    resolved_names: &FxHashMap<u32, StrId>,
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
    let definition_location = HPos::new(&classlike_node.span, file_source.file_path);
    let name_location = HPos::new(classlike_node.name.pos(), file_source.file_path);

    let mut meta_start = MetaStart {
        start_offset: definition_location.start_offset,
        start_line: definition_location.start_line,
        start_column: definition_location.start_column,
    };

    let mut suppressed_issues = vec![];

    adjust_location_from_comments(
        comments,
        &mut meta_start,
        file_source,
        &mut suppressed_issues,
        all_custom_issues,
    );

    let mut storage = match get_classlike_storage(
        codebase,
        class_name,
        definition_location,
        meta_start,
        name_location,
    ) {
        Ok(value) => value,
        Err(value) => return value,
    };

    storage.user_defined = user_defined;

    let mut signature_end = storage.name_location.end_offset;

    storage.uses_position = uses_position;
    storage.namespace_position = namespace_position;

    storage.is_production_code &= file_source.is_production_code;

    storage.suppressed_issues = suppressed_issues;

    if !classlike_node.tparams.is_empty() {
        let mut type_context = TypeResolutionContext {
            template_type_map: vec![],
            template_supers: vec![],
        };

        for type_param_node in classlike_node.tparams.iter() {
            let param_name = resolved_names
                .get(&(type_param_node.name.0.start_offset() as u32))
                .unwrap();
            type_context.template_type_map.push((
                *param_name,
                vec![(
                    GenericParent::ClassLike(*class_name),
                    Arc::new(get_mixed_any()),
                )],
            ));
        }

        for (i, type_param_node) in classlike_node.tparams.iter().enumerate() {
            signature_end = type_param_node.name.0.end_offset() as u32;

            if !type_param_node.constraints.is_empty() {
                signature_end = type_param_node
                    .constraints
                    .last()
                    .unwrap()
                    .1
                     .0
                    .end_offset() as u32;
            }

            let first_constraint = type_param_node.constraints.first();

            let template_as_type = if let Some((_, constraint_hint)) = first_constraint {
                get_type_from_hint(
                    &constraint_hint.1,
                    Some(class_name),
                    &type_context,
                    resolved_names,
                    file_source.file_path,
                    constraint_hint.0.start_offset() as u32,
                )
                .unwrap()
            } else {
                get_mixed_any()
            };

            let param_name = resolved_names
                .get(&(type_param_node.name.0.start_offset() as u32))
                .unwrap();

            storage.template_types.push((
                *param_name,
                vec![(
                    GenericParent::ClassLike(*class_name),
                    Arc::new(template_as_type),
                )],
            ));

            match type_param_node.variance {
                ast_defs::Variance::Covariant => {
                    storage.generic_variance.insert(i, Variance::Covariant);
                    storage.template_readonly.insert(*param_name);
                }
                ast_defs::Variance::Contravariant => {
                    storage.generic_variance.insert(i, Variance::Contravariant);
                }
                ast_defs::Variance::Invariant => {
                    // default, do nothing
                    if class_name == &interner.intern_str("HH\\Vector") {
                        // cheat here for vectors
                        storage.generic_variance.insert(i, Variance::Covariant);
                    } else {
                        storage.generic_variance.insert(i, Variance::Invariant);
                    }

                    storage.template_readonly.insert(*param_name);
                }
            }
        }
    }

    match classlike_node.kind {
        ClassishKind::Cclass(abstraction) => {
            storage.is_abstract = matches!(abstraction, ast_defs::Abstraction::Abstract);
            storage.is_final = classlike_node.final_;

            codebase.symbols.add_class_name(class_name);

            if let Some(parent_class) = classlike_node.extends.first() {
                if let oxidized::tast::Hint_::Happly(name, params) = &*parent_class.1 {
                    signature_end = name.0.end_offset() as u32;

                    let parent_name = *resolved_names.get(&(name.0.start_offset() as u32)).unwrap();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset() as u32;
                    }

                    storage.direct_parent_class = Some(parent_name);
                    storage.all_parent_classes.push(parent_name);

                    storage.template_extended_offsets.insert(
                        parent_name,
                        params
                            .iter()
                            .map(|param| {
                                Arc::new(
                                    get_type_from_hint(
                                        &param.1,
                                        Some(class_name),
                                        &TypeResolutionContext {
                                            template_type_map: storage.template_types.clone(),
                                            template_supers: vec![],
                                        },
                                        resolved_names,
                                        file_source.file_path,
                                        param.0.start_offset() as u32,
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
                    signature_end = name.0.end_offset() as u32;

                    let interface_name =
                        *resolved_names.get(&(name.0.start_offset() as u32)).unwrap();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset() as u32;
                    }

                    storage.direct_class_interfaces.push(interface_name);
                    storage.all_class_interfaces.push(interface_name);

                    if class_name == &StrId::SIMPLE_XML_ELEMENT
                        && interface_name == StrId::TRAVERSABLE
                    {
                        storage.template_extended_offsets.insert(
                            interface_name,
                            vec![Arc::new(get_named_object(*class_name))],
                        );
                    } else {
                        storage.template_extended_offsets.insert(
                            interface_name,
                            params
                                .iter()
                                .map(|param| {
                                    Arc::new(
                                        get_type_from_hint(
                                            &param.1,
                                            Some(class_name),
                                            &TypeResolutionContext {
                                                template_type_map: storage.template_types.clone(),
                                                template_supers: vec![],
                                            },
                                            resolved_names,
                                            file_source.file_path,
                                            param.0.start_offset() as u32,
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

            codebase.symbols.add_enum_class_name(class_name);

            if let Some(enum_node) = &classlike_node.enum_ {
                storage.enum_type = Some(
                    get_type_from_hint(
                        &enum_node.base.1,
                        None,
                        &TypeResolutionContext::new(),
                        resolved_names,
                        file_source.file_path,
                        enum_node.base.0.start_offset() as u32,
                    )
                    .unwrap()
                    .get_single_owned(),
                );
            }

            storage.direct_parent_class = Some(StrId::BUILTIN_ENUM_CLASS);
            storage.all_parent_classes.push(StrId::BUILTIN_ENUM_CLASS);

            storage.template_extended_offsets.insert(
                StrId::BUILTIN_ENUM_CLASS,
                vec![Arc::new(wrap_atomic(TAtomic::TTypeAlias {
                    name: StrId::MEMBER_OF,
                    type_params: Some(vec![
                        wrap_atomic(TAtomic::TNamedObject {
                            name: *class_name,
                            type_params: None,
                            is_this: false,
                            extra_types: None,
                            remapped_params: false,
                        }),
                        wrap_atomic(storage.enum_type.clone().unwrap()),
                    ]),
                    as_type: None,
                }))],
            );
        }
        ClassishKind::Cinterface => {
            storage.kind = SymbolKind::Interface;
            codebase.symbols.add_interface_name(class_name);

            handle_reqs(
                classlike_node,
                resolved_names,
                &mut storage,
                class_name,
                file_source,
            );

            for parent_interface in &classlike_node.extends {
                if let oxidized::tast::Hint_::Happly(name, params) = &*parent_interface.1 {
                    signature_end = name.0.end_offset() as u32;

                    let parent_name = *resolved_names.get(&(name.0.start_offset() as u32)).unwrap();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset() as u32;
                    }

                    storage.direct_parent_interfaces.push(parent_name);
                    storage.all_parent_interfaces.push(parent_name);

                    storage.template_extended_offsets.insert(
                        parent_name,
                        params
                            .iter()
                            .map(|param| {
                                Arc::new(
                                    get_type_from_hint(
                                        &param.1,
                                        Some(class_name),
                                        &TypeResolutionContext {
                                            template_type_map: storage.template_types.clone(),
                                            template_supers: vec![],
                                        },
                                        resolved_names,
                                        file_source.file_path,
                                        param.0.start_offset() as u32,
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

            codebase.symbols.add_trait_name(class_name);

            handle_reqs(
                classlike_node,
                resolved_names,
                &mut storage,
                class_name,
                file_source,
            );

            for extended_interface in &classlike_node.implements {
                if let oxidized::tast::Hint_::Happly(name, params) = &*extended_interface.1 {
                    signature_end = name.0.end_offset() as u32;

                    let interface_name =
                        *resolved_names.get(&(name.0.start_offset() as u32)).unwrap();

                    if !params.is_empty() {
                        signature_end = params.last().unwrap().0.end_offset() as u32;
                    }

                    storage.direct_class_interfaces.push(interface_name);
                    storage.all_class_interfaces.push(interface_name);

                    storage.template_extended_offsets.insert(
                        interface_name,
                        params
                            .iter()
                            .map(|param| {
                                Arc::new(
                                    get_type_from_hint(
                                        &param.1,
                                        Some(class_name),
                                        &TypeResolutionContext {
                                            template_type_map: storage.template_types.clone(),
                                            template_supers: vec![],
                                        },
                                        resolved_names,
                                        file_source.file_path,
                                        param.0.start_offset() as u32,
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

            storage.direct_parent_class = Some(StrId::BUILTIN_ENUM);
            storage.all_parent_classes.push(StrId::BUILTIN_ENUM);

            if let Some(enum_node) = &classlike_node.enum_ {
                signature_end = enum_node.base.0.end_offset() as u32;

                storage.enum_type = Some(
                    get_type_from_hint(
                        &enum_node.base.1,
                        None,
                        &TypeResolutionContext::new(),
                        resolved_names,
                        file_source.file_path,
                        enum_node.base.0.start_offset() as u32,
                    )
                    .unwrap()
                    .get_single_owned(),
                );

                if let Some(constraint) = &enum_node.constraint {
                    signature_end = constraint.0.end_offset() as u32;

                    storage.enum_constraint = Some(Box::new(
                        get_type_from_hint(
                            &constraint.1,
                            None,
                            &TypeResolutionContext::new(),
                            resolved_names,
                            file_source.file_path,
                            constraint.0.start_offset() as u32,
                        )
                        .unwrap()
                        .get_single_owned(),
                    ));
                }
            }

            storage.template_extended_offsets.insert(
                StrId::BUILTIN_ENUM,
                vec![Arc::new(wrap_atomic(TAtomic::TEnum {
                    name: *class_name,
                    base_type: None,
                }))],
            );

            codebase.symbols.add_enum_name(class_name);
        }
    }

    let uses_hash = get_uses_hash(all_uses.symbol_uses.get(class_name).unwrap_or(&vec![]));

    let mut signature_hash = xxhash_rust::xxh3::xxh3_64(
        file_source.file_contents
            [storage.def_location.start_offset as usize..signature_end as usize]
            .as_bytes(),
    )
    .wrapping_add(uses_hash);

    for xhp_attribute in &classlike_node.xhp_attrs {
        if let Some(r) = xhp_attribute.2 {
            // required attributes are _essentially_ part of the
            // XHP class's signature
            if r.is_required() {
                let attr_name_pos = xhp_attribute.1.id.pos();
                let attribute_id = interner.intern(xhp_attribute.1.id.1.clone());

                let attr_uses_hash = get_uses_hash(
                    all_uses
                        .symbol_member_uses
                        .get(&(*class_name, attribute_id))
                        .unwrap_or(&vec![]),
                );

                signature_hash = signature_hash
                    .wrapping_add(xxhash_rust::xxh3::xxh3_64(
                        file_source.file_contents
                            [attr_name_pos.start_offset()..attr_name_pos.end_offset()]
                            .as_bytes(),
                    ))
                    .wrapping_add(attr_uses_hash);
            }
        }
    }

    let mut def_signature_node = DefSignatureNode {
        name: *class_name,
        start_offset: storage.meta_start.start_offset,
        end_offset: storage.def_location.end_offset,
        start_line: storage.meta_start.start_line,
        end_line: storage.def_location.end_line,
        children: Vec::new(),
        signature_hash,
        body_hash: None,
        is_function: false,
        is_constant: false,
    };

    for trait_use in &classlike_node.uses {
        let trait_type = get_type_from_hint(
            &trait_use.1,
            None,
            &TypeResolutionContext {
                template_type_map: storage.template_types.clone(),
                template_supers: vec![],
            },
            resolved_names,
            file_source.file_path,
            trait_use.0.start_offset() as u32,
        )
        .unwrap()
        .get_single_owned();

        if let TAtomic::TReference {
            name, type_params, ..
        } = trait_type
        {
            storage.used_traits.insert(name);

            let mut hasher = rustc_hash::FxHasher::default();
            name.0.hash(&mut hasher);

            def_signature_node.signature_hash = def_signature_node
                .signature_hash
                .wrapping_add(hasher.finish());

            if let Some(type_params) = type_params {
                storage
                    .template_extended_offsets
                    .insert(name, type_params.into_iter().map(Arc::new).collect());
            }
        }
    }

    for class_const_node in &classlike_node.consts {
        visit_class_const_declaration(
            class_const_node,
            resolved_names,
            &mut storage,
            file_source,
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

    for user_attribute in &classlike_node.user_attributes {
        let name = *resolved_names
            .get(&(user_attribute.name.0.start_offset() as u32))
            .unwrap();

        signature_hash =
            signature_hash.wrapping_add(xxhash_rust::xxh3::xxh3_64(&name.0.to_le_bytes()));

        if name == StrId::CODEGEN {
            storage.generated = true;
        }

        if name == StrId::HAKANA_IMMUTABLE {
            storage.immutable = true;
        }

        if name == StrId::HAKANA_TEST_ONLY {
            storage.is_production_code = false;
        }

        storage.attributes.push(AttributeInfo { name });

        if name == StrId::SEALED {
            let mut child_classlikes = FxHashSet::default();

            for attribute_param_expr in &user_attribute.params {
                let attribute_param_type =
                    simple_type_inferer::infer(attribute_param_expr, resolved_names);

                if let Some(attribute_param_type) = attribute_param_type {
                    for atomic in attribute_param_type.types.into_iter() {
                        if let TAtomic::TLiteralClassname { name: value } = atomic {
                            child_classlikes.insert(value);
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
            file_source,
            &mut def_signature_node.children,
            interner,
            all_uses,
        );
    }

    for m in &classlike_node.methods {
        let (method_name, functionlike_storage) = functionlike_scanner::scan_method(
            interner,
            all_custom_issues,
            resolved_names,
            m,
            *class_name,
            &mut storage,
            file_source.comments,
            file_source,
            user_defined,
        );

        let (signature_hash, body_hash) = get_function_hashes(
            &file_source.file_contents,
            &functionlike_storage.def_location,
            &m.name,
            &m.tparams,
            &m.params,
            &functionlike_storage.attributes,
            &m.ret,
            all_uses
                .symbol_member_uses
                .get(&(*class_name, method_name))
                .unwrap_or(&vec![]),
        );
        def_signature_node.children.push(DefSignatureNode {
            name: method_name,
            start_offset: functionlike_storage.def_location.start_offset,
            end_offset: functionlike_storage.def_location.end_offset,
            start_line: functionlike_storage.def_location.start_line,
            end_line: functionlike_storage.def_location.end_line,
            signature_hash,
            body_hash: Some(body_hash),
            children: vec![],
            is_function: true,
            is_constant: false,
        });

        if !storage.template_readonly.is_empty()
            && matches!(m.visibility, ast_defs::Visibility::Public)
            && method_name != StrId::CONSTRUCT
        {
            for param in &functionlike_storage.params {
                if let Some(param_type) = &param.signature_type {
                    let template_types = param_type.get_template_types();

                    for template_type in template_types {
                        if let TAtomic::TGenericParam { param_name, .. } = template_type {
                            storage.template_readonly.remove(param_name);
                        }
                    }
                }
            }
        }

        storage.methods.push(method_name);

        codebase
            .functionlike_infos
            .insert((*class_name, method_name), functionlike_storage);
    }

    storage.properties.shrink_to_fit();
    storage.methods.shrink_to_fit();

    codebase.classlike_infos.insert(*class_name, storage);

    def_signature_node.children.shrink_to_fit();

    ast_nodes.push(def_signature_node);

    true
}

fn handle_reqs(
    classlike_node: &aast::Class_<(), ()>,
    resolved_names: &FxHashMap<u32, StrId>,
    storage: &mut ClassLikeInfo,
    class_name: &StrId,
    file_source: &FileSource,
) {
    for req in &classlike_node.reqs {
        if let oxidized::tast::Hint_::Happly(name, params) = &*req.0 .1 {
            let require_name = *resolved_names.get(&(name.0.start_offset() as u32)).unwrap();

            match &req.1 {
                aast::RequireKind::RequireExtends => {
                    storage.direct_parent_class = Some(require_name);
                    storage.all_parent_classes.push(require_name);
                    storage.required_classlikes.push(require_name);
                }
                aast::RequireKind::RequireImplements => {
                    storage.direct_class_interfaces.push(require_name);
                    storage.all_class_interfaces.push(require_name);
                    storage.required_classlikes.push(require_name);
                }
                aast::RequireKind::RequireClass => todo!(),
            };

            storage.template_extended_offsets.insert(
                require_name,
                params
                    .iter()
                    .map(|param| {
                        Arc::new(
                            get_type_from_hint(
                                &param.1,
                                Some(class_name),
                                &TypeResolutionContext {
                                    template_type_map: storage.template_types.clone(),
                                    template_supers: vec![],
                                },
                                resolved_names,
                                file_source.file_path,
                                param.0.start_offset() as u32,
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
    resolved_names: &FxHashMap<u32, StrId>,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    def_child_signature_nodes: &mut Vec<DefSignatureNode>,
    interner: &mut ThreadedInterner,
    all_uses: &Uses,
) {
    let mut attribute_type_location = None;
    let mut attribute_type = if let Some(hint) = &xhp_attribute.0 .1 {
        attribute_type_location = Some(HPos::new(&hint.0, file_source.file_path));
        get_type_from_hint(
            &hint.1,
            None,
            &TypeResolutionContext {
                template_type_map: vec![],
                template_supers: vec![],
            },
            resolved_names,
            file_source.file_path,
            hint.0.start_offset() as u32,
        )
        .unwrap()
    } else {
        get_mixed_any()
    };

    let is_required = if let Some(attr_tag) = &xhp_attribute.2 {
        attr_tag.is_required()
    } else {
        false
    };

    if !is_required && !attribute_type.is_mixed() && xhp_attribute.1.expr.is_none() {
        attribute_type.types.push(TAtomic::TNull);
    }

    let mut stmt_pos = HPos::new(&xhp_attribute.1.span, file_source.file_path);

    if let Some(type_hint) = &xhp_attribute.0 .1 {
        let (line, bol, offset) = type_hint.0.to_start_and_end_lnum_bol_offset().0;
        stmt_pos.start_offset = offset as u32;
        stmt_pos.start_line = line as u32;
        stmt_pos.start_column = (offset - bol) as u16;
    }

    if let Some(attr_tag) = &xhp_attribute.2 {
        match attr_tag {
            oxidized::tast::XhpAttrTag::Required => {
                stmt_pos.end_offset += 10;
                stmt_pos.end_column += 10;
            }
            oxidized::tast::XhpAttrTag::LateInit => {
                stmt_pos.end_offset += 11;
                stmt_pos.end_column += 11;
            }
        }
    }

    let attribute_id = interner.intern(xhp_attribute.1.id.1.clone());

    let uses_hash = get_uses_hash(
        all_uses
            .symbol_member_uses
            .get(&(classlike_storage.name, attribute_id))
            .unwrap_or(&vec![]),
    );

    def_child_signature_nodes.push(DefSignatureNode {
        name: attribute_id,
        start_offset: stmt_pos.start_offset,
        end_offset: stmt_pos.end_offset,
        start_line: stmt_pos.start_line,
        end_line: stmt_pos.end_line,
        signature_hash: xxhash_rust::xxh3::xxh3_64(
            file_source.file_contents[stmt_pos.start_offset as usize..stmt_pos.end_offset as usize]
                .as_bytes(),
        )
        .wrapping_add(uses_hash),
        body_hash: None,
        children: vec![],
        is_function: false,
        is_constant: false,
    });

    let name_pos = HPos::new(xhp_attribute.1.id.pos(), file_source.file_path);

    let property_storage = PropertyInfo {
        is_static: false,
        visibility: MemberVisibility::Protected,
        kind: PropertyKind::XhpAttribute { is_required },
        pos: Some(name_pos),
        stmt_pos: Some(stmt_pos),
        type_pos: attribute_type_location,
        type_: attribute_type,
        has_default: xhp_attribute.1.expr.is_some(),
        soft_readonly: false,
        is_promoted: false,
        is_internal: false,
        suppressed_issues: None,
    };

    classlike_storage
        .inheritable_property_ids
        .insert(attribute_id, classlike_storage.name);
    classlike_storage
        .properties
        .insert(attribute_id, property_storage);
}

fn visit_class_const_declaration(
    const_node: &aast::ClassConst<(), ()>,
    resolved_names: &FxHashMap<u32, StrId>,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    interner: &mut ThreadedInterner,
    def_child_signature_nodes: &mut Vec<DefSignatureNode>,
    all_uses: &Uses,
) {
    let mut provided_type = None;

    let mut supplied_type_location = None;

    if let Some(supplied_type_hint) = &const_node.type_ {
        provided_type = get_type_from_hint(
            &supplied_type_hint.1,
            Some(&classlike_storage.name),
            &TypeResolutionContext {
                template_type_map: classlike_storage.template_types.clone(),
                template_supers: vec![],
            },
            resolved_names,
            file_source.file_path,
            supplied_type_hint.0.start_offset() as u32,
        );

        supplied_type_location = Some(HPos::new(&supplied_type_hint.0, file_source.file_path));
    }

    let def_pos = HPos::new(&const_node.span, file_source.file_path);

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
        is_function: false,
        is_constant: true,
    });

    let const_storage = ConstantInfo {
        pos: def_pos,
        type_pos: supplied_type_location,
        provided_type,
        inferred_type: if let ClassConstKind::CCAbstract(Some(const_expr))
        | ClassConstKind::CCConcrete(const_expr) = &const_node.kind
        {
            simple_type_inferer::infer(const_expr, resolved_names)
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
    resolved_names: &FxHashMap<u32, StrId>,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    interner: &mut ThreadedInterner,
    def_child_signature_nodes: &mut Vec<DefSignatureNode>,
    all_uses: &Uses,
) {
    let class_constant_type = match &const_node.kind {
        aast::ClassTypeconst::TCAbstract(abstract_node) => {
            ClassConstantType::Abstract(if let Some(hint) = &abstract_node.as_constraint {
                Some(
                    get_type_from_hint(
                        &hint.1,
                        Some(&classlike_storage.name),
                        &TypeResolutionContext {
                            template_type_map: classlike_storage.template_types.clone(),
                            template_supers: vec![],
                        },
                        resolved_names,
                        file_source.file_path,
                        hint.0.start_offset() as u32,
                    )
                    .unwrap(),
                )
            } else {
                None
            })
        }
        aast::ClassTypeconst::TCConcrete(const_node) => ClassConstantType::Concrete(
            get_type_from_hint(
                &const_node.c_tc_type.1,
                Some(&classlike_storage.name),
                &TypeResolutionContext {
                    template_type_map: classlike_storage.template_types.clone(),
                    template_supers: vec![],
                },
                resolved_names,
                file_source.file_path,
                const_node.c_tc_type.0.start_offset() as u32,
            )
            .unwrap(),
        ),
    };

    let def_pos = HPos::new(&const_node.span, file_source.file_path);

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
        is_function: false,
        is_constant: true,
    });

    classlike_storage
        .type_constants
        .insert(name, class_constant_type);
}

fn visit_property_declaration(
    property_node: &aast::ClassVar<(), ()>,
    resolved_names: &FxHashMap<u32, StrId>,
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
            &property_type_hint.1,
            Some(&classlike_storage.name),
            &TypeResolutionContext {
                template_type_map: classlike_storage.template_types.clone(),
                template_supers: vec![],
            },
            resolved_names,
            file_source.file_path,
            property_type_hint.0.start_offset() as u32,
        );

        property_type_location = Some(HPos::new(&property_type_hint.0, file_source.file_path));
    }

    let def_pos = HPos::new(&property_node.span, file_source.file_path);

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
            file_source.file_contents[def_pos.start_offset as usize..def_pos.end_offset as usize]
                .as_bytes(),
        )
        .wrapping_add(uses_hash),
        body_hash: None,
        children: vec![],
        is_function: false,
        is_constant: false,
    });

    if !classlike_storage.template_readonly.is_empty()
        && matches!(property_node.visibility, ast_defs::Visibility::Public)
    {
        if let Some(property_type) = &property_type {
            let template_types = property_type.get_template_types();

            for template_type in template_types {
                if let TAtomic::TGenericParam { param_name, .. } = template_type {
                    classlike_storage.template_readonly.remove(param_name);
                }
            }
        }
    }

    let property_storage = PropertyInfo {
        is_static: property_node.is_static,
        visibility: match property_node.visibility {
            ast_defs::Visibility::Private => MemberVisibility::Private,
            ast_defs::Visibility::Public | ast_defs::Visibility::Internal => {
                MemberVisibility::Public
            }
            ast_defs::Visibility::Protected => MemberVisibility::Protected,
        },
        pos: Some(HPos::new(property_node.id.pos(), file_source.file_path)),
        kind: PropertyKind::Property,
        stmt_pos: Some(def_pos),
        type_pos: property_type_location,
        type_: property_type.unwrap_or(get_mixed_any()),
        has_default: property_node.expr.is_some(),
        soft_readonly: false,
        is_promoted: false,
        is_internal: matches!(property_node.visibility, ast_defs::Visibility::Internal),
        suppressed_issues: None,
    };

    if !matches!(property_node.visibility, ast_defs::Visibility::Private) {
        classlike_storage
            .inheritable_property_ids
            .insert(property_ref_id, classlike_storage.name);
    }

    classlike_storage
        .properties
        .insert(property_ref_id, property_storage);
}

fn get_classlike_storage(
    codebase: &mut CodebaseInfo,
    class_name: &StrId,
    definition_pos: HPos,
    meta_start: MetaStart,
    name_pos: HPos,
) -> Result<ClassLikeInfo, bool> {
    if codebase.classlike_infos.contains_key(class_name) {
        Err(false)
    } else {
        Ok(ClassLikeInfo::new(
            *class_name,
            definition_pos,
            meta_start,
            name_pos,
        ))
    }
}
