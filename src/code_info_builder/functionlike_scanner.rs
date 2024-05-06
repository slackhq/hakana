use std::sync::Arc;

use crate::simple_type_inferer;
use crate::typehint_resolver::get_type_from_hint;
use crate::typehint_resolver::get_type_from_optional_hint;
use hakana_reflection_info::attribute_info::AttributeInfo;
use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::codebase_info::symbols::SymbolKind;
use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::FnEffect;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::functionlike_info::MetaStart;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::issue::get_issue_from_comment;
use hakana_reflection_info::issue::IssueKind;
use hakana_reflection_info::member_visibility::MemberVisibility;
use hakana_reflection_info::method_info::MethodInfo;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::taint::string_to_sink_types;
use hakana_reflection_info::taint::string_to_source_types;
use hakana_reflection_info::type_resolution::TypeResolutionContext;
use hakana_reflection_info::FileSource;
use hakana_reflection_info::GenericParent;
use hakana_reflection_info::VarId;
use hakana_reflection_info::EFFECT_IMPURE;
use hakana_str::{StrId, ThreadedInterner};
use hakana_type::get_mixed_any;
use oxidized::aast;
use oxidized::aast::Stmt;
use oxidized::ast::UserAttribute;
use oxidized::ast_defs;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;
use oxidized::tast;
use oxidized::tast::WhereConstraintHint;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub(crate) fn scan_method(
    interner: &mut ThreadedInterner,
    all_custom_issues: &FxHashSet<String>,
    resolved_names: &FxHashMap<u32, StrId>,
    m: &aast::Method_<(), ()>,
    classlike_name: StrId,
    classlike_storage: &mut ClassLikeInfo,
    comments: &Vec<(Pos, Comment)>,
    file_source: &FileSource,
    user_defined: bool,
) -> (StrId, FunctionLikeInfo) {
    let method_name = interner.intern(m.name.1.clone());

    let mut type_resolution_context = TypeResolutionContext {
        template_type_map: classlike_storage.template_types.clone(),
        template_supers: vec![],
    };

    let mut functionlike_info = get_functionlike(
        interner,
        all_custom_issues,
        Some(method_name),
        &m.span,
        Some(&m.name.0),
        &m.tparams,
        &m.params,
        &m.body.fb_ast.0,
        &m.ret,
        &m.fun_kind,
        &m.user_attributes.0,
        &m.ctxs,
        &m.where_constraints,
        &mut type_resolution_context,
        Some(&classlike_name),
        resolved_names,
        comments,
        file_source,
        user_defined,
    );

    functionlike_info.is_production_code &=
        file_source.is_production_code && classlike_storage.is_production_code;

    if classlike_name == StrId::BUILTIN_ENUM
        && (method_name == StrId::COERCE
            || method_name == StrId::ASSERT
            || method_name == StrId::ASSERT_ALL)
    {
        functionlike_info.has_throw = true;
    }

    let mut method_info = MethodInfo::new();

    method_info.is_static = m.static_;
    method_info.is_final = m.final_ || classlike_storage.is_final;
    method_info.is_abstract = m.abstract_;
    method_info.visibility = match m.visibility {
        ast_defs::Visibility::Private => MemberVisibility::Private,
        ast_defs::Visibility::Public | ast_defs::Visibility::Internal => MemberVisibility::Public,
        ast_defs::Visibility::Protected => MemberVisibility::Protected,
    };

    if !matches!(m.visibility, ast_defs::Visibility::Private)
        || method_name != StrId::CONSTRUCT
        || matches!(classlike_storage.kind, SymbolKind::Trait)
    {
        classlike_storage
            .inheritable_method_ids
            .insert(method_name, classlike_name);
    }

    for param_node in &m.params {
        if param_node.visibility.is_some() {
            add_promoted_param_property(param_node, classlike_storage, interner);
        }
    }

    functionlike_info.type_resolution_context = Some(type_resolution_context);
    functionlike_info.method_info = Some(Box::new(method_info));

    (method_name, functionlike_info)
}

fn add_promoted_param_property(
    param_node: &aast::FunParam<(), ()>,
    classlike_storage: &mut ClassLikeInfo,
    interner: &mut ThreadedInterner,
) {
    let param_node_id = interner.intern(param_node.name[1..].to_string());

    if let Some(property_storage) = classlike_storage.properties.get_mut(&param_node_id) {
        property_storage.is_promoted = true;
    }
}

pub(crate) fn get_functionlike(
    interner: &mut ThreadedInterner,
    all_custom_issues: &FxHashSet<String>,
    name: Option<StrId>,
    def_pos: &Pos,
    name_pos: Option<&Pos>,
    tparams: &[aast::Tparam<(), ()>],
    params: &[aast::FunParam<(), ()>],
    stmts: &[Stmt<(), ()>],
    ret: &aast::TypeHint<()>,
    fun_kind: &ast_defs::FunKind,
    user_attributes: &Vec<UserAttribute>,
    contexts: &Option<tast::Contexts>,
    where_constraints: &Vec<WhereConstraintHint>,
    type_context: &mut TypeResolutionContext,
    this_name: Option<&StrId>,
    resolved_names: &FxHashMap<u32, StrId>,
    comments: &Vec<(Pos, Comment)>,
    file_source: &FileSource,
    user_defined: bool,
) -> FunctionLikeInfo {
    let definition_location = HPos::new(def_pos, file_source.file_path);

    let mut suppressed_issues = vec![];

    let mut meta_start = MetaStart {
        start_offset: definition_location.start_offset,
        start_line: definition_location.start_line,
        start_column: definition_location.start_column,
    };

    adjust_location_from_comments(
        comments,
        &mut meta_start,
        file_source,
        &mut suppressed_issues,
        all_custom_issues,
    );

    let mut functionlike_info = FunctionLikeInfo::new(definition_location, meta_start);

    let mut template_supers = vec![];

    if !tparams.is_empty() {
        for type_param_node in tparams.iter() {
            let param_name = resolved_names
                .get(&(type_param_node.name.0.start_offset() as u32))
                .unwrap();
            type_context.template_type_map.push((
                *param_name,
                vec![(
                    GenericParent::FunctionLike(name.unwrap()),
                    Arc::new(get_mixed_any()),
                )],
            ));
        }

        for type_param_node in tparams.iter() {
            let param_name = resolved_names
                .get(&(type_param_node.name.0.start_offset() as u32))
                .unwrap();

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
                        file_source.file_path,
                        constraint_hint.0.start_offset() as u32,
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
                        file_source.file_path,
                        constraint_hint.0.start_offset() as u32,
                    )
                    .unwrap();

                    super_type.types.push(TAtomic::TGenericParam {
                        param_name: *param_name,
                        as_type: Box::new(if let Some(template_as_type) = &template_as_type {
                            template_as_type.clone()
                        } else {
                            get_mixed_any()
                        }),
                        defining_entity: GenericParent::FunctionLike(name.unwrap()),
                        extra_types: None,
                    });

                    template_supers.push((*param_name, super_type));
                }
            }

            functionlike_info.template_types.push((
                *param_name,
                vec![(
                    GenericParent::FunctionLike(name.unwrap()),
                    Arc::new(template_as_type.unwrap_or(get_mixed_any())),
                )],
            ));
        }

        for where_hint in where_constraints {
            let where_first = get_type_from_hint(
                &where_hint.0 .1,
                this_name,
                type_context,
                resolved_names,
                file_source.file_path,
                where_hint.0 .0.start_offset() as u32,
            )
            .unwrap()
            .get_single_owned();

            let where_second = get_type_from_hint(
                &where_hint.2 .1,
                this_name,
                type_context,
                resolved_names,
                file_source.file_path,
                where_hint.2 .0.start_offset() as u32,
            )
            .unwrap();

            if let TAtomic::TGenericParam { param_name, .. } = where_first {
                if let ast_defs::ConstraintKind::ConstraintEq = where_hint.1 {
                    functionlike_info
                        .where_constraints
                        .push((param_name, where_second));
                }
            }
        }
    }

    for row in &functionlike_info.template_types {
        let mut matched = false;
        for existing_template_type in type_context.template_type_map.iter_mut() {
            if existing_template_type.0 == row.0 {
                existing_template_type.1.clone_from(&row.1);
                matched = true;
                break;
            }
        }
        if !matched {
            type_context.template_type_map.push((row.0, row.1.clone()));
        }
    }

    if !params.is_empty() {
        functionlike_info.params = convert_param_nodes(
            params,
            resolved_names,
            type_context,
            file_source,
            all_custom_issues,
            comments
                .iter()
                .filter(|comment| comment.0.start_offset() > def_pos.start_offset())
                .collect(),
        );
        functionlike_info.params.shrink_to_fit();
    }

    type_context.template_supers = template_supers;
    functionlike_info.return_type = get_type_from_optional_hint(
        ret.get_hint(),
        None,
        type_context,
        resolved_names,
        file_source.file_path,
    );

    functionlike_info.user_defined = user_defined;
    functionlike_info.is_closure = name.is_none();

    if let Some(name_pos) = name_pos {
        let name_offset = 9 + if fun_kind.is_async() { 6 } else { 0 };

        let (_, name_line_start_offset, name_start_offset) =
            name_pos.to_start_and_end_lnum_bol_offset().0;
        functionlike_info.def_location.start_offset = name_start_offset as u32 - name_offset;
        functionlike_info.def_location.start_column =
            (name_start_offset - name_line_start_offset) as u16 - name_offset as u16;
    }

    for user_attribute in user_attributes {
        let attribute_name = *resolved_names
            .get(&(user_attribute.name.0.start_offset() as u32))
            .unwrap();

        functionlike_info.attributes.push(AttributeInfo {
            name: attribute_name,
        });

        match attribute_name {
            StrId::HAKANA_SECURITY_ANALYSIS_SOURCE => {
                let mut source_types = vec![];

                for attribute_param_expr in &user_attribute.params {
                    let attribute_param_type =
                        simple_type_inferer::infer(attribute_param_expr, resolved_names);

                    if let Some(attribute_param_type) = attribute_param_type {
                        if let Some(str) = attribute_param_type.get_single_literal_string_value() {
                            if let Some(source_type) = string_to_source_types(str) {
                                source_types.push(source_type);
                            }
                        }
                    }
                }

                functionlike_info.taint_source_types = source_types;
            }
            StrId::HAKANA_SECURITY_ANALYSIS_SPECIALIZE_CALL => {
                functionlike_info.specialize_call = true;
            }
            StrId::HAKANA_SECURITY_ANALYSIS_IGNORE_PATH => {
                functionlike_info.ignore_taint_path = true;
            }
            StrId::HAKANA_TEST_ONLY => {
                functionlike_info.is_production_code = false;
            }
            StrId::HAKANA_SECURITY_ANALYSIS_IGNORE_PATH_IF_TRUE => {
                functionlike_info.ignore_taints_if_true = true;
            }
            StrId::HAKANA_SECURITY_ANALYSIS_SANITIZE | StrId::HAKANA_FIND_PATHS_SANITIZE => {
                let mut removed_types = vec![];

                for attribute_param_expr in &user_attribute.params {
                    let attribute_param_type =
                        simple_type_inferer::infer(attribute_param_expr, resolved_names);
                    if let Some(attribute_param_type) = attribute_param_type {
                        attribute_param_type
                            .get_literal_string_values()
                            .into_iter()
                            .for_each(|value| {
                                if let Some(str) = value {
                                    removed_types.extend(string_to_sink_types(str));
                                }
                            })
                    }
                }

                functionlike_info.removed_taints = removed_types;
            }
            StrId::HAKANA_MUST_USE => {
                functionlike_info.must_use = true;
            }
            StrId::ENTRY_POINT => {
                functionlike_info.dynamically_callable = true;
                functionlike_info.ignore_taint_path = true;
            }
            StrId::DYNAMICALLY_CALLABLE => {
                functionlike_info.dynamically_callable = true;
            }
            StrId::CODEGEN => {
                functionlike_info.generated = true;
            }
            StrId::OVERRIDE => {
                functionlike_info.overriding = true;
            }
            _ => {}
        }
    }

    if let Some(ret) = &ret.1 {
        functionlike_info.return_type_location = Some(HPos::new(&ret.0, file_source.file_path));
    }

    if let Some(name_pos) = name_pos {
        functionlike_info.name_location = Some(HPos::new(name_pos, file_source.file_path));
    }

    functionlike_info.suppressed_issues = suppressed_issues;

    functionlike_info.is_async = fun_kind.is_async();
    functionlike_info.effects = if let Some(contexts) = contexts {
        get_effect_from_contexts(contexts, &functionlike_info, interner)
    } else if name.is_none() {
        FnEffect::Unknown
    } else {
        FnEffect::Some(EFFECT_IMPURE)
    };

    if matches!(functionlike_info.effects, FnEffect::Pure) || this_name.is_none() {
        functionlike_info.specialize_call = true;
    }

    if stmts.len() == 1 && !functionlike_info.is_async {
        let stmt = &stmts[0];

        if let aast::Stmt_::Return(expr) = &stmt.1 {
            if let Some(expr) = expr.as_ref() {
                if let Some(function_id) =
                    get_async_version(expr, resolved_names, &functionlike_info.params, interner)
                {
                    functionlike_info.async_version = Some(function_id);
                }
            }
        }
    }

    // todo light inference based on function body contents

    functionlike_info
}

fn get_effect_from_contexts(
    contexts: &tast::Contexts,
    functionlike_info: &FunctionLikeInfo,
    interner: &mut ThreadedInterner,
) -> FnEffect {
    if contexts.1.is_empty() {
        FnEffect::Pure
    } else if contexts.1.len() == 1 {
        let context = &contexts.1[0];

        if let tast::Hint_::HfunContext(boxed) = &*context.1 {
            let position = functionlike_info
                .params
                .iter()
                .position(|p| interner.lookup(p.name.0) == boxed);

            if let Some(position) = position {
                FnEffect::Arg(position as u8)
            } else {
                panic!()
            }
        } else {
            FnEffect::Some(EFFECT_IMPURE)
        }
    } else {
        FnEffect::Some(EFFECT_IMPURE)
    }
}

fn get_async_version(
    expr: &oxidized::ast::Expr,
    resolved_names: &FxHashMap<u32, StrId>,
    params: &[FunctionLikeParameter],
    interner: &mut ThreadedInterner,
) -> Option<FunctionLikeIdentifier> {
    if let aast::Expr_::Call(call) = &expr.2 {
        if let aast::Expr_::Id(boxed_id) = &call.func.2 {
            if let Some(fn_id) = resolved_names.get(&(boxed_id.0.start_offset() as u32)) {
                if fn_id == &StrId::ASIO_JOIN && call.args.len() == 1 {
                    let first_join_expr = &call.args[0].1;

                    if let aast::Expr_::Call(call) = &first_join_expr.2 {
                        if !is_async_call_is_same_as_sync(&call.args, params, interner) {
                            return None;
                        }

                        match &call.func.2 {
                            aast::Expr_::Id(boxed_id) => {
                                if let Some(fn_id) =
                                    resolved_names.get(&(boxed_id.0.start_offset() as u32))
                                {
                                    return Some(FunctionLikeIdentifier::Function(*fn_id));
                                }
                            }
                            aast::Expr_::ClassConst(boxed) => {
                                let (class_id, rhs_expr) = (&boxed.0, &boxed.1);

                                if let aast::ClassId_::CIexpr(lhs_expr) = &class_id.2 {
                                    if let aast::Expr_::Id(id) = &lhs_expr.2 {
                                        if let Some(class_name) =
                                            resolved_names.get(&(id.0.start_offset() as u32))
                                        {
                                            return Some(FunctionLikeIdentifier::Method(
                                                *class_name,
                                                interner.intern(rhs_expr.1.clone()),
                                            ));
                                        }
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }

    None
}

fn is_async_call_is_same_as_sync(
    call_args: &[(ast_defs::ParamKind, aast::Expr<(), ()>)],
    params: &[FunctionLikeParameter],
    interner: &mut ThreadedInterner,
) -> bool {
    for (offset, (_, call_arg_expr)) in call_args.iter().enumerate() {
        if let aast::Expr_::Lvar(id) = &call_arg_expr.2 {
            if let Some(param) = params.get(offset) {
                if interner.lookup(param.name.0) != id.1 .1 {
                    return false;
                }
            } else {
                return false;
            }
        } else {
            return false;
        }
    }

    true
}

#[allow(clippy::ptr_arg)]
pub(crate) fn adjust_location_from_comments(
    comments: &Vec<(Pos, Comment)>,
    meta_start: &mut MetaStart,
    file_source: &FileSource,
    suppressed_issues: &mut Vec<(IssueKind, HPos)>,
    all_custom_issues: &FxHashSet<String>,
) {
    for (comment_pos, comment) in comments.iter().rev() {
        let (start, end) = comment_pos.to_start_and_end_lnum_bol_offset();
        let (start_line, _, start_offset) = start;
        let (end_line, _, _) = end;

        if meta_start.start_line as usize == (end_line + 1)
            || meta_start.start_line as usize == (end_line + 2)
        {
            match comment {
                Comment::CmtLine(_) => {
                    meta_start.start_line = start_line as u32;
                    meta_start.start_offset = start_offset as u32;
                }
                Comment::CmtBlock(text) => {
                    let trimmed_text = if let Some(trimmed_text) = text.strip_prefix('*') {
                        trimmed_text.trim()
                    } else {
                        text.trim()
                    };

                    if let Some(Ok(issue_kind)) =
                        get_issue_from_comment(trimmed_text, all_custom_issues)
                    {
                        let comment_pos = HPos::new(comment_pos, file_source.file_path);
                        suppressed_issues.push((issue_kind, comment_pos));
                    }

                    meta_start.start_line = start_line as u32;
                    meta_start.start_offset = start_offset as u32;
                }
            }
        }
    }
}

fn convert_param_nodes(
    param_nodes: &[aast::FunParam<(), ()>],
    resolved_names: &FxHashMap<u32, StrId>,
    type_context: &TypeResolutionContext,
    file_source: &FileSource,
    all_custom_issues: &FxHashSet<String>,
    mut comments: Vec<&(Pos, Comment)>,
) -> Vec<FunctionLikeParameter> {
    param_nodes
        .iter()
        .map(|param_node| {
            let mut location = HPos::new(&param_node.pos, file_source.file_path);

            if let Some(param_type) = &param_node.type_hint.1 {
                location.start_offset = param_type.0.start_offset() as u32;
                location.start_line = param_type.0.line() as u32;
                location.start_column = (location.start_offset as usize
                    - param_type.0.to_start_and_end_lnum_bol_offset().0 .1)
                    as u16;
            }

            let param_id = *resolved_names
                .get(&(param_node.pos.start_offset() as u32))
                .unwrap();

            let mut suppressed_issues = vec![];

            let mut meta_start = MetaStart {
                start_offset: location.start_offset,
                start_line: location.start_line,
                start_column: location.start_column,
            };

            adjust_location_from_comments(
                &comments.iter().map(|c| (*c).clone()).collect(),
                &mut meta_start,
                file_source,
                &mut suppressed_issues,
                all_custom_issues,
            );

            location.start_offset = meta_start.start_offset;
            location.start_column = meta_start.start_column;
            location.start_line = meta_start.start_line;

            comments.retain(|c| c.0.start_offset() > param_node.pos.end_offset());

            let mut param = FunctionLikeParameter::new(
                VarId(param_id),
                location,
                HPos::new(&param_node.pos, file_source.file_path),
            );

            if !suppressed_issues.is_empty() {
                param.suppressed_issues = Some(suppressed_issues);
            }

            param.is_variadic = param_node.is_variadic;
            param.signature_type = if let Some(param_type) = &param_node.type_hint.1 {
                get_type_from_hint(
                    &param_type.1,
                    None,
                    type_context,
                    resolved_names,
                    file_source.file_path,
                    param_type.0.start_offset() as u32,
                )
            } else {
                None
            };
            param.is_inout = matches!(param_node.callconv, ast_defs::ParamKind::Pinout(_));
            param.signature_type_location = param_node
                .type_hint
                .1
                .as_ref()
                .map(|param_type| HPos::new(&param_type.0, file_source.file_path));
            for user_attribute in &param_node.user_attributes {
                let name = resolved_names
                    .get(&(user_attribute.name.0.start_offset() as u32))
                    .unwrap();

                param.attributes.push(AttributeInfo { name: *name });

                match *name {
                    StrId::HAKANA_SECURITY_ANALYSIS_SINK => {
                        let mut sink_types = vec![];

                        for attribute_param_expr in &user_attribute.params {
                            let attribute_param_type =
                                simple_type_inferer::infer(attribute_param_expr, resolved_names);

                            if let Some(attribute_param_type) = attribute_param_type {
                                if let Some(str) =
                                    attribute_param_type.get_single_literal_string_value()
                                {
                                    sink_types.extend(string_to_sink_types(str));
                                }
                            }
                        }

                        param.taint_sinks = Some(sink_types);
                    }
                    StrId::HAKANA_SECURITY_ANALYSIS_REMOVE_TAINTS_WHEN_RETURNING_TRUE => {
                        let mut removed_taints = vec![];

                        for attribute_param_expr in &user_attribute.params {
                            let attribute_param_type =
                                simple_type_inferer::infer(attribute_param_expr, resolved_names);

                            if let Some(attribute_param_type) = attribute_param_type {
                                if let Some(str) =
                                    attribute_param_type.get_single_literal_string_value()
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
            param
        })
        .collect()
}
