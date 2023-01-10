use std::sync::Arc;

use super::Context;
use crate::simple_type_inferer;
use crate::typehint_resolver::get_type_from_hint;
use crate::typehint_resolver::get_type_from_optional_hint;
use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::codebase_info::symbols::SymbolKind;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::functionlike_info::FnEffect;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::issue::get_issue_from_comment;
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::member_visibility::MemberVisibility;
use hakana_reflection_info::method_info::MethodInfo;
use hakana_reflection_info::property_info::PropertyInfo;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::taint::string_to_sink_types;
use hakana_reflection_info::taint::string_to_source_types;
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use hakana_reflection_info::FileSource;
use hakana_reflection_info::StrId;
use hakana_reflection_info::ThreadedInterner;
use hakana_type::get_mixed_any;
use oxidized::aast;
use oxidized::ast::UserAttribute;
use oxidized::ast_defs;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;
use oxidized::tast;
use oxidized::tast::WhereConstraintHint;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub(crate) fn scan_method(
    codebase: &mut CodebaseInfo,
    interner: &mut ThreadedInterner,
    all_custom_issues: &FxHashSet<String>,
    resolved_names: &FxHashMap<usize, StrId>,
    m: &aast::Method_<(), ()>,
    c: &mut Context,
    comments: &Vec<(Pos, Comment)>,
    file_source: &FileSource,
) -> (StrId, FunctionLikeInfo) {
    let classlike_name = c.classlike_name.clone().unwrap();
    let method_name = interner.intern(m.name.1.clone());

    let mut type_resolution_context = TypeResolutionContext {
        template_type_map: codebase
            .classlike_infos
            .get(&classlike_name)
            .unwrap()
            .template_types
            .clone(),
        template_supers: FxHashMap::default(),
    };

    let functionlike_id = format!("{}::{}", interner.lookup(classlike_name), m.name.1);

    let mut functionlike_info = get_functionlike(
        &codebase,
        interner,
        all_custom_issues,
        method_name,
        &m.span,
        Some(&m.name.0),
        &m.tparams,
        &m.params,
        &m.ret,
        &m.fun_kind,
        &m.user_attributes,
        &m.ctxs,
        &m.where_constraints,
        &mut type_resolution_context,
        Some(&classlike_name),
        resolved_names,
        &functionlike_id,
        comments,
        file_source,
        false,
    );

    let mut classlike_storage = codebase.classlike_infos.get_mut(&classlike_name).unwrap();

    let mut method_info = MethodInfo::new();

    method_info.defining_fqcln = Some(classlike_name.clone());
    method_info.is_static = m.static_;
    method_info.is_final = m.final_ || classlike_storage.is_final;
    method_info.is_abstract = m.abstract_;
    method_info.visibility = match m.visibility {
        ast_defs::Visibility::Private => MemberVisibility::Private,
        ast_defs::Visibility::Public | ast_defs::Visibility::Internal => MemberVisibility::Public,
        ast_defs::Visibility::Protected => MemberVisibility::Protected,
    };

    classlike_storage
        .appearing_method_ids
        .insert(method_name.clone(), classlike_name.clone());
    classlike_storage
        .declaring_method_ids
        .insert(method_name.clone(), classlike_name.clone());

    if !matches!(m.visibility, ast_defs::Visibility::Private)
        || method_name != StrId::construct()
        || matches!(classlike_storage.kind, SymbolKind::Trait)
    {
        classlike_storage
            .inheritable_method_ids
            .insert(method_name.clone(), classlike_name.clone());
    }

    for param_node in &m.params {
        if let Some(param_visibility) = param_node.visibility {
            add_promoted_param_property(
                param_node,
                param_visibility,
                resolved_names,
                &classlike_name,
                &mut classlike_storage,
                file_source,
                interner,
            );
        }
    }

    functionlike_info.type_resolution_context = Some(type_resolution_context);
    functionlike_info.method_info = Some(method_info);

    (method_name, functionlike_info)
}

fn add_promoted_param_property(
    param_node: &aast::FunParam<(), ()>,
    param_visibility: ast_defs::Visibility,
    resolved_names: &FxHashMap<usize, StrId>,
    classlike_name: &StrId,
    classlike_storage: &mut ClassLikeInfo,
    file_source: &FileSource,
    interner: &mut ThreadedInterner,
) {
    let signature_type_location = if let Some(param_type) = &param_node.type_hint.1 {
        Some(HPos::new(&param_type.0, file_source.file_path, None))
    } else {
        None
    };
    let property_storage = PropertyInfo {
        is_static: false,
        visibility: match param_visibility {
            ast_defs::Visibility::Private => MemberVisibility::Private,
            ast_defs::Visibility::Public | ast_defs::Visibility::Internal => {
                MemberVisibility::Public
            }
            ast_defs::Visibility::Protected => MemberVisibility::Protected,
        },
        pos: Some(HPos::new(&param_node.pos, file_source.file_path, None)),
        stmt_pos: Some(HPos::new(&param_node.pos, file_source.file_path, None)),
        type_pos: signature_type_location,
        type_: get_type_from_optional_hint(
            &param_node.type_hint.1,
            None,
            &TypeResolutionContext {
                template_type_map: classlike_storage.template_types.clone(),
                template_supers: FxHashMap::default(),
            },
            resolved_names,
        )
        .unwrap_or(get_mixed_any()),
        has_default: param_node.expr.is_some(),
        soft_readonly: false,
        is_promoted: false,
        is_internal: matches!(param_visibility, ast_defs::Visibility::Internal),
    };

    let param_node_id = interner.intern(param_node.name.clone());

    if !matches!(param_visibility, ast_defs::Visibility::Private) {
        classlike_storage
            .inheritable_property_ids
            .insert(param_node_id, classlike_name.clone());
    };

    classlike_storage
        .declaring_property_ids
        .insert(param_node_id, classlike_name.clone());
    classlike_storage
        .appearing_property_ids
        .insert(param_node_id, classlike_name.clone());
    classlike_storage
        .initialized_properties
        .insert(param_node_id);
    classlike_storage
        .properties
        .insert(param_node_id, property_storage);
}

pub(crate) fn get_functionlike(
    codebase: &CodebaseInfo,
    interner: &mut ThreadedInterner,
    all_custom_issues: &FxHashSet<String>,
    name: StrId,
    def_pos: &Pos,
    name_pos: Option<&Pos>,
    tparams: &Vec<aast::Tparam<(), ()>>,
    params: &Vec<aast::FunParam<(), ()>>,
    ret: &aast::TypeHint<()>,
    fun_kind: &ast_defs::FunKind,
    user_attributes: &Vec<UserAttribute>,
    contexts: &Option<tast::Contexts>,
    where_constraints: &Vec<WhereConstraintHint>,
    type_context: &mut TypeResolutionContext,
    this_name: Option<&StrId>,
    resolved_names: &FxHashMap<usize, StrId>,
    functionlike_id: &String,
    comments: &Vec<(Pos, Comment)>,
    file_source: &FileSource,
    is_anonymous: bool,
) -> FunctionLikeInfo {
    let mut definition_location = HPos::new(def_pos, file_source.file_path, None);

    let mut suppressed_issues = FxHashMap::default();

    adjust_location_from_comments(
        comments,
        &mut definition_location,
        file_source,
        &mut suppressed_issues,
        all_custom_issues,
    );

    let mut functionlike_info = FunctionLikeInfo::new(name.clone(), definition_location);

    let mut template_supers = FxHashMap::default();

    if !tparams.is_empty() {
        let fn_id = "fn-".to_string() + functionlike_id.as_str();
        let fn_id = interner.intern(fn_id);

        for type_param_node in tparams.iter() {
            type_context.template_type_map.insert(
                type_param_node.name.1.clone(),
                FxHashMap::from_iter([(fn_id.clone(), Arc::new(get_mixed_any()))]),
            );
        }

        for type_param_node in tparams.iter() {
            let mut template_as_type = None;

            for (constraint_type, constraint_hint) in &type_param_node.constraints {
                if let ast_defs::ConstraintKind::ConstraintAs
                | ast_defs::ConstraintKind::ConstraintEq = constraint_type
                {
                    template_as_type = get_type_from_hint(
                        &constraint_hint.1,
                        this_name,
                        type_context,
                        resolved_names,
                    );
                }
            }

            for (constraint_type, constraint_hint) in &type_param_node.constraints {
                if let ast_defs::ConstraintKind::ConstraintSuper = constraint_type {
                    let mut super_type = get_type_from_hint(
                        &constraint_hint.1,
                        this_name,
                        type_context,
                        resolved_names,
                    )
                    .unwrap();

                    super_type.types.push(TAtomic::TGenericParam {
                        param_name: type_param_node.name.1.clone(),
                        as_type: if let Some(template_as_type) = &template_as_type {
                            template_as_type.clone()
                        } else {
                            get_mixed_any()
                        },
                        defining_entity: fn_id.clone(),
                        from_class: false,
                        extra_types: None,
                    });

                    template_supers.insert(type_param_node.name.1.clone(), super_type);
                }
            }

            functionlike_info
                .template_types
                .insert(type_param_node.name.1.clone(), {
                    FxHashMap::from_iter([(
                        fn_id.clone(),
                        Arc::new(template_as_type.unwrap_or(get_mixed_any())),
                    )])
                });
        }

        for where_hint in where_constraints {
            let where_first =
                get_type_from_hint(&where_hint.0 .1, this_name, type_context, resolved_names)
                    .unwrap()
                    .get_single_owned();

            let where_second =
                get_type_from_hint(&where_hint.2 .1, this_name, type_context, resolved_names)
                    .unwrap();

            match where_first {
                TAtomic::TGenericParam { param_name, .. } => match where_hint.1 {
                    ast_defs::ConstraintKind::ConstraintEq => {
                        functionlike_info
                            .where_constraints
                            .push((param_name, where_second));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    type_context
        .template_type_map
        .extend(functionlike_info.template_types.clone());

    functionlike_info.params = convert_param_nodes(
        codebase,
        interner,
        params,
        resolved_names,
        &type_context,
        file_source,
    );
    type_context.template_supers = template_supers;
    functionlike_info.return_type =
        get_type_from_optional_hint(ret.get_hint(), None, &type_context, resolved_names);

    for user_attribute in user_attributes {
        let name = resolved_names
            .get(&user_attribute.name.0.start_offset())
            .unwrap()
            .clone();

        match interner.lookup(name) {
            "Hakana\\SecurityAnalysis\\Source" => {
                let mut source_types = FxHashSet::default();

                for attribute_param_expr in &user_attribute.params {
                    let attribute_param_type = simple_type_inferer::infer(
                        codebase,
                        &mut FxHashMap::default(),
                        attribute_param_expr,
                        resolved_names,
                    );

                    if let Some(attribute_param_type) = attribute_param_type {
                        if let Some(str) =
                            attribute_param_type.get_single_literal_string_value(&codebase.interner)
                        {
                            source_types.extend(string_to_source_types(str));
                        }
                    }
                }

                functionlike_info.taint_source_types = source_types;
            }
            "Hakana\\SecurityAnalysis\\SpecializeCall" => {
                functionlike_info.specialize_call = true;
            }
            "Hakana\\SecurityAnalysis\\IgnorePath" => {
                functionlike_info.ignore_taint_path = true;
            }
            "Hakana\\SecurityAnalysis\\IgnorePathIfTrue" => {
                functionlike_info.ignore_taints_if_true = true;
            }
            "Hakana\\SecurityAnalysis\\Sanitize" | "Hakana\\FindPaths\\Sanitize" => {
                let mut removed_types = FxHashSet::default();

                for attribute_param_expr in &user_attribute.params {
                    let attribute_param_type = simple_type_inferer::infer(
                        codebase,
                        &mut FxHashMap::default(),
                        attribute_param_expr,
                        resolved_names,
                    );
                    if let Some(attribute_param_type) = attribute_param_type {
                        attribute_param_type
                            .get_literal_string_values(&codebase.interner)
                            .into_iter()
                            .for_each(|value| {
                                if let Some(str) = value {
                                    removed_types.extend(string_to_sink_types(str));
                                }
                            })
                    }
                }

                functionlike_info.removed_taints = Some(removed_types);
            }
            "__EntryPoint" => {
                functionlike_info.dynamically_callable = true;
                functionlike_info.ignore_taint_path = true;
            }
            "__DynamicallyCallable" => {
                functionlike_info.dynamically_callable = true;
            }
            "Codegen" => {
                functionlike_info.generated = true;
            }
            _ => {}
        }
    }

    if let Some(ret) = &ret.1 {
        functionlike_info.return_type_location =
            Some(HPos::new(&ret.0, file_source.file_path, None));
    }

    if let Some(name_pos) = name_pos {
        functionlike_info.name_location = Some(HPos::new(name_pos, file_source.file_path, None));
    }

    if !suppressed_issues.is_empty() {
        functionlike_info.suppressed_issues = Some(suppressed_issues);
    }

    functionlike_info.is_async = fun_kind.is_async();
    functionlike_info.effects = if let Some(contexts) = contexts {
        if contexts.1.len() == 0 {
            FnEffect::None
        } else if contexts.1.len() == 1 {
            let context = &contexts.1[0];

            if let tast::Hint_::HfunContext(boxed) = &*context.1 {
                let position = functionlike_info
                    .params
                    .iter()
                    .position(|p| &p.name == boxed);

                if let Some(position) = position {
                    FnEffect::Arg(position as u8)
                } else {
                    panic!()
                }
            } else {
                FnEffect::Some(7)
            }
        } else {
            FnEffect::Some(7)
        }
    } else {
        if is_anonymous {
            FnEffect::Unknown
        } else {
            FnEffect::Some(7)
        }
    };

    if matches!(functionlike_info.effects, FnEffect::None) || !functionlike_id.contains("::") {
        functionlike_info.specialize_call = true;
    }

    // todo light inference based on function body contents

    functionlike_info
}

pub(crate) fn adjust_location_from_comments(
    comments: &Vec<(Pos, Comment)>,
    definition_location: &mut HPos,
    file_source: &FileSource,
    suppressed_issues: &mut FxHashMap<IssueKind, HPos>,
    all_custom_issues: &FxHashSet<String>,
) {
    for (comment_pos, comment) in comments.iter().rev() {
        let (start, end) = comment_pos.to_start_and_end_lnum_bol_offset();
        let (start_line, _, start_offset) = start;
        let (end_line, _, _) = end;

        if (end_line + 1) == definition_location.start_line {
            match comment {
                Comment::CmtLine(_) => {
                    definition_location.start_line = start_line;
                    definition_location.start_offset = start_offset;
                }
                Comment::CmtBlock(text) => {
                    let trimmed_text = if text.starts_with("*") {
                        text[1..].trim()
                    } else {
                        text.trim()
                    };

                    if let Some(issue_kind) =
                        get_issue_from_comment(trimmed_text, all_custom_issues)
                    {
                        let comment_pos = HPos::new(comment_pos, file_source.file_path, None);
                        suppressed_issues.insert(issue_kind, comment_pos);
                    }

                    definition_location.start_line = start_line;
                    definition_location.start_offset = start_offset;
                }
            }
        }
    }
}

fn convert_param_nodes(
    codebase: &CodebaseInfo,
    interner: &mut ThreadedInterner,
    param_nodes: &Vec<aast::FunParam<(), ()>>,
    resolved_names: &FxHashMap<usize, StrId>,
    type_context: &TypeResolutionContext,
    file_source: &FileSource,
) -> Vec<FunctionLikeParameter> {
    param_nodes
        .iter()
        .map(|param_node| {
            let mut param = FunctionLikeParameter::new(param_node.name.clone());

            param.is_variadic = param_node.is_variadic;
            param.signature_type = if let Some(param_type) = &param_node.type_hint.1 {
                get_type_from_hint(&*param_type.1, None, type_context, resolved_names)
            } else {
                None
            };
            param.is_inout = matches!(param_node.callconv, ast_defs::ParamKind::Pinout(_));
            param.signature_type_location = if let Some(param_type) = &param_node.type_hint.1 {
                Some(HPos::new(&param_type.0, file_source.file_path, None))
            } else {
                None
            };
            for user_attribute in &param_node.user_attributes {
                let name = resolved_names
                    .get(&user_attribute.name.0.start_offset())
                    .unwrap();

                match interner.lookup(*name) {
                    "Hakana\\SecurityAnalysis\\Sink" => {
                        let mut sink_types = FxHashSet::default();

                        for attribute_param_expr in &user_attribute.params {
                            let attribute_param_type = simple_type_inferer::infer(
                                codebase,
                                &mut FxHashMap::default(),
                                attribute_param_expr,
                                resolved_names,
                            );

                            if let Some(attribute_param_type) = attribute_param_type {
                                if let Some(str) = attribute_param_type
                                    .get_single_literal_string_value(&codebase.interner)
                                {
                                    sink_types.extend(string_to_sink_types(str));
                                }
                            }
                        }

                        param.taint_sinks = Some(sink_types);
                    }
                    "Hakana\\SecurityAnalysis\\RemoveTaintsWhenReturningTrue" => {
                        let mut removed_taints = FxHashSet::default();

                        for attribute_param_expr in &user_attribute.params {
                            let attribute_param_type = simple_type_inferer::infer(
                                codebase,
                                &mut FxHashMap::default(),
                                attribute_param_expr,
                                resolved_names,
                            );

                            if let Some(attribute_param_type) = attribute_param_type {
                                if let Some(str) = attribute_param_type
                                    .get_single_literal_string_value(&codebase.interner)
                                {
                                    removed_taints.extend(string_to_sink_types(str));
                                }
                            }
                        }

                        param.removed_taints_when_returning_true = Some(removed_taints);
                    }
                    _ => {}
                }
            }
            param.promoted_property = param_node.visibility.is_some();
            param.is_optional = param_node.expr.is_some();
            param.location = Some(HPos::new(&param_node.pos, file_source.file_path, None));
            param
        })
        .collect()
}
