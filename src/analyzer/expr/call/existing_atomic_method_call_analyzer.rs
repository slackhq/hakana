use std::rc::Rc;

use hakana_reflection_info::analysis_result::Replacement;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::{
    assertion::Assertion,
    data_flow::{node::DataFlowNode, path::PathKind},
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_reflection_info::{EFFECT_WRITE_LOCAL, EFFECT_WRITE_PROPS};
use hakana_str::StrId;
use hakana_type::get_null;
use hakana_type::template::standin_type_replacer;
use hakana_type::{
    add_union_type, get_arraykey, get_dict, get_mixed_any, template::TemplateResult,
};
use indexmap::IndexMap;
use oxidized::{
    aast,
    ast_defs::{self, Pos},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::expr::fetch::array_fetch_analyzer::add_array_fetch_dataflow;
use crate::stmt_analyzer::AnalysisError;
use crate::{
    expr::{
        call_analyzer::check_method_args, expression_identifier,
        fetch::array_fetch_analyzer::handle_array_access_on_dict,
    },
    function_analysis_data::FunctionAnalysisData,
    scope_analyzer::ScopeAnalyzer,
    scope_context::ScopeContext,
    statements_analyzer::StatementsAnalyzer,
};

use super::{
    atomic_method_call_analyzer::AtomicMethodCallAnalysisResult, class_template_param_collector,
    method_call_return_type_fetcher,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    mut classlike_name: StrId,
    method_name: &StrId,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    lhs_type_part: &TAtomic,
    pos: &Pos,
    method_name_pos: Option<&Pos>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    lhs_var_id: Option<&String>,
    lhs_var_pos: Option<&Pos>,
    result: &mut AtomicMethodCallAnalysisResult,
) -> Result<TUnion, AnalysisError> {
    analysis_data.symbol_references.add_reference_to_symbol(
        &context.function_context,
        classlike_name,
        false,
    );

    let codebase = statements_analyzer.get_codebase();

    if classlike_name == StrId::STATIC {
        classlike_name = context.function_context.calling_class.unwrap();
    }

    let method_id = MethodIdentifier(classlike_name, *method_name);

    result
        .existent_method_ids
        .insert(method_id.to_string(statements_analyzer.get_interner()));

    let declaring_method_id = codebase.get_declaring_method_id(&method_id);

    let classlike_storage = codebase.classlike_infos.get(&classlike_name).unwrap();

    analysis_data
        .symbol_references
        .add_reference_to_class_member(
            &context.function_context,
            (declaring_method_id.0, declaring_method_id.1),
            false,
        );

    if let Some(overridden_classlikes) = classlike_storage
        .overridden_method_ids
        .get(&declaring_method_id.1)
    {
        for overridden_classlike in overridden_classlikes {
            analysis_data
                .symbol_references
                .add_reference_to_overridden_class_member(
                    &context.function_context,
                    (*overridden_classlike, declaring_method_id.1),
                );
        }
    }

    let class_template_params =
        if classlike_name != StrId::VECTOR || *method_name != StrId::FROM_ITEMS {
            let declaring_classlike_storage =
                if let Some(s) = codebase.classlike_infos.get(&declaring_method_id.0) {
                    s
                } else {
                    return Err(AnalysisError::InternalError(
                        "could not load storage for declaring method".to_string(),
                        statements_analyzer.get_hpos(pos),
                    ));
                };

            class_template_param_collector::collect(
                codebase,
                declaring_classlike_storage,
                classlike_storage,
                Some(lhs_type_part),
            )
        } else {
            None
        };

    let functionlike_storage = if let Some(s) = codebase.get_method(&declaring_method_id) {
        s
    } else {
        return Err(AnalysisError::InternalError(
            "could not load storage for declaring method".to_string(),
            statements_analyzer.get_hpos(pos),
        ));
    };

    let functionlike_template_types = functionlike_storage.template_types.clone();

    let mut template_result = TemplateResult::new(
        functionlike_template_types,
        class_template_params.clone().unwrap_or(IndexMap::new()),
    );

    if !functionlike_storage.where_constraints.is_empty() {
        if let Some(class_template_params) = &class_template_params {
            for (template_name, where_type) in &functionlike_storage.where_constraints {
                let template_type = class_template_params
                    .get(template_name)
                    .unwrap()
                    .get(&declaring_method_id.0)
                    .unwrap();

                standin_type_replacer::replace(
                    where_type,
                    &mut template_result,
                    statements_analyzer.get_codebase(),
                    &Some(statements_analyzer.get_interner()),
                    &Some(template_type),
                    None,
                    None,
                    context.function_context.calling_functionlike_id.as_ref(),
                    true,
                    false,
                    None,
                    1,
                );
            }
        }
    }

    // .hhi for NumberFormatter was incorrect
    if classlike_name == StrId::NUMBER_FORMATTER {
        analysis_data.expr_effects.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            EFFECT_WRITE_PROPS,
        );
    }

    check_method_args(
        statements_analyzer,
        analysis_data,
        &method_id,
        functionlike_storage,
        call_expr,
        &mut template_result,
        context,
        if_body_context,
        pos,
        method_name_pos,
    )?;

    if functionlike_storage.ignore_taints_if_true {
        analysis_data.if_true_assertions.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            FxHashMap::from_iter([("hakana taints".to_string(), vec![Assertion::IgnoreTaints])]),
        );
    }

    if method_id.0 == StrId::SHAPES {
        if let Some(value) = handle_shapes_static_method(
            &method_id,
            call_expr,
            context,
            statements_analyzer,
            analysis_data,
            pos,
            codebase,
        ) {
            return Ok(value);
        }
    }

    let return_type_candidate = method_call_return_type_fetcher::fetch(
        statements_analyzer,
        analysis_data,
        context,
        call_expr,
        &method_id,
        &declaring_method_id,
        lhs_type_part,
        lhs_var_id,
        lhs_var_pos,
        functionlike_storage,
        classlike_storage,
        &template_result,
        pos,
    );

    // todo check method visibility

    // todo support if_this_is type

    // todo check for method call purity

    // todo apply assertions

    // todo dispatch after method call analysis events

    Ok(return_type_candidate)
}

fn handle_shapes_static_method(
    method_id: &MethodIdentifier,
    call_expr: (
        &Vec<oxidized::aast::Targ<()>>,
        &Vec<(oxidized::ast_defs::ParamKind, oxidized::aast::Expr<(), ()>)>,
        &Option<oxidized::aast::Expr<(), ()>>,
    ),
    context: &mut ScopeContext,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    codebase: &hakana_reflection_info::codebase_info::CodebaseInfo,
) -> Option<TUnion> {
    match method_id.1 {
        StrId::KEY_EXISTS => {
            if call_expr.1.len() == 2 {
                let expr_var_id = expression_identifier::get_var_id(
                    &call_expr.1[0].1,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some((
                        statements_analyzer.get_codebase(),
                        statements_analyzer.get_interner(),
                    )),
                );

                let dim_var_id = expression_identifier::get_dim_id(
                    &call_expr.1[1].1,
                    None,
                    &FxHashMap::default(),
                );

                if let Some(expr_var_id) = expr_var_id {
                    if let Some(mut dim_var_id) = dim_var_id {
                        if dim_var_id.starts_with('\'') {
                            dim_var_id = dim_var_id[1..(dim_var_id.len() - 1)].to_string();
                            analysis_data.if_true_assertions.insert(
                                (pos.start_offset() as u32, pos.end_offset() as u32),
                                FxHashMap::from_iter([(
                                    expr_var_id,
                                    vec![Assertion::HasArrayKey(DictKey::String(dim_var_id))],
                                )]),
                            );
                        } else {
                            analysis_data.if_true_assertions.insert(
                                (pos.start_offset() as u32, pos.end_offset() as u32),
                                FxHashMap::from_iter([(
                                    format!("{}[{}]", expr_var_id, dim_var_id),
                                    vec![Assertion::ArrayKeyExists],
                                )]),
                            );
                        }
                    }
                }
            }
        }

        StrId::REMOVE_KEY => {
            if call_expr.1.len() == 2 {
                let expr_var_id = expression_identifier::get_var_id(
                    &call_expr.1[0].1,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some((
                        statements_analyzer.get_codebase(),
                        statements_analyzer.get_interner(),
                    )),
                );
                let dim_var_id = expression_identifier::get_dim_id(
                    &call_expr.1[1].1,
                    None,
                    &FxHashMap::default(),
                );

                analysis_data.expr_effects.insert(
                    (pos.start_offset() as u32, pos.end_offset() as u32),
                    EFFECT_WRITE_LOCAL,
                );

                if let (Some(expr_var_id), Some(dim_var_id)) = (expr_var_id, dim_var_id) {
                    if let Some(expr_type) = context.vars_in_scope.get(&expr_var_id) {
                        let mut new_type = (**expr_type).clone();

                        let dim_var_id = dim_var_id[1..dim_var_id.len() - 1].to_string();

                        for atomic_type in new_type.types.iter_mut() {
                            if let TAtomic::TDict {
                                known_items: Some(ref mut known_items),
                                ..
                            } = atomic_type
                            {
                                known_items.remove(&DictKey::String(dim_var_id.clone()));
                            }
                        }

                        let assignment_node = DataFlowNode::get_for_assignment(
                            expr_var_id.clone(),
                            statements_analyzer.get_hpos(call_expr.1[0].1.pos()),
                        );

                        for parent_node in &expr_type.parent_nodes {
                            analysis_data.data_flow_graph.add_path(
                                parent_node,
                                &assignment_node,
                                PathKind::RemoveDictKey(dim_var_id.clone()),
                                None,
                                None,
                            );
                        }

                        new_type.parent_nodes = FxHashSet::from_iter([assignment_node.clone()]);

                        analysis_data.data_flow_graph.add_node(assignment_node);

                        context.vars_in_scope.insert(expr_var_id, Rc::new(new_type));
                    }
                }
            }
        }
        StrId::IDX => {
            if call_expr.1.len() >= 2 {
                let dict_type = analysis_data
                    .get_rc_expr_type(call_expr.1[0].1.pos())
                    .cloned();
                let dim_type = analysis_data
                    .get_rc_expr_type(call_expr.1[1].1.pos())
                    .cloned();

                let mut expr_type = None;

                if let (Some(dict_type), Some(dim_type)) = (dict_type, dim_type) {
                    let mut has_valid_expected_offset = false;
                    let mut has_possibly_undefined = false;
                    let mut has_matching_dict_key = false;
                    let is_nullable = dict_type.is_nullable();

                    for atomic_type in &dict_type.types {
                        if let TAtomic::TDict { .. } = atomic_type {
                            let mut expr_type_inner = handle_array_access_on_dict(
                                statements_analyzer,
                                pos,
                                analysis_data,
                                context,
                                atomic_type,
                                &dim_type,
                                false,
                                &mut has_valid_expected_offset,
                                true,
                                &mut has_possibly_undefined,
                                &mut has_matching_dict_key,
                            );

                            if !is_nullable && has_matching_dict_key {
                                if call_expr.1.len() == 2 {
                                    if has_possibly_undefined {
                                        expr_type_inner = add_union_type(
                                            expr_type_inner,
                                            &get_null(),
                                            codebase,
                                            false,
                                        );
                                    } else if !expr_type_inner.is_nothing() {
                                        if has_valid_expected_offset {
                                            handle_defined_shape_idx(
                                                call_expr,
                                                context,
                                                statements_analyzer,
                                                analysis_data,
                                                pos,
                                            );
                                        }
                                    } else {
                                        expr_type_inner = get_null();
                                    }
                                } else if !has_possibly_undefined && has_valid_expected_offset {
                                    handle_defined_shape_idx(
                                        call_expr,
                                        context,
                                        statements_analyzer,
                                        analysis_data,
                                        pos,
                                    );
                                }
                            } else if call_expr.1.len() == 2 && is_nullable {
                                expr_type_inner =
                                    add_union_type(expr_type_inner, &get_null(), codebase, false);
                            }

                            expr_type = Some(expr_type_inner);
                        }
                    }

                    if (is_nullable || has_possibly_undefined) && call_expr.1.len() > 2 {
                        let default_type = analysis_data.get_expr_type(call_expr.1[2].1.pos());
                        expr_type = expr_type.map(|expr_type| {
                            if let Some(default_type) = default_type {
                                add_union_type(expr_type, default_type, codebase, false)
                            } else {
                                get_mixed_any()
                            }
                        });
                    }

                    if let Some(mut expr_type) = expr_type {
                        add_array_fetch_dataflow(
                            statements_analyzer,
                            call_expr.1[0].1.pos(),
                            analysis_data,
                            None,
                            &mut expr_type,
                            &mut (*dim_type).clone(),
                        );
                        return Some(expr_type);
                    }
                }

                return Some(expr_type.unwrap_or(get_mixed_any()));
            }
        }
        StrId::AT => {
            if call_expr.1.len() == 2 {
                let dict_type = analysis_data
                    .get_rc_expr_type(call_expr.1[0].1.pos())
                    .cloned();
                let dim_type = analysis_data
                    .get_rc_expr_type(call_expr.1[1].1.pos())
                    .cloned();

                let mut expr_type = None;

                if let (Some(dict_type), Some(dim_type)) = (dict_type, dim_type) {
                    for atomic_type in &dict_type.types {
                        if let TAtomic::TDict { .. } = atomic_type {
                            let expr_type_inner = handle_array_access_on_dict(
                                statements_analyzer,
                                pos,
                                analysis_data,
                                context,
                                atomic_type,
                                &dim_type,
                                false,
                                &mut false,
                                true,
                                &mut false,
                                &mut false,
                            );

                            expr_type = Some(expr_type_inner);
                        }
                    }
                }

                return Some(expr_type.unwrap_or(get_mixed_any()));
            }
        }
        StrId::TO_DICT | StrId::TO_ARRAY => {
            let arg_type = analysis_data.get_expr_type(call_expr.1[0].1.pos()).cloned();

            return Some(if let Some(arg_type) = arg_type {
                if arg_type.is_mixed() {
                    get_dict(get_arraykey(true), get_mixed_any())
                } else {
                    arg_type
                }
            } else {
                get_mixed_any()
            });
        }
        _ => {}
    }

    None
}

fn handle_defined_shape_idx(
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    context: &mut ScopeContext,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
) {
    if statements_analyzer
        .get_config()
        .issues_to_fix
        .contains(&IssueKind::UnnecessaryShapesIdx)
        && !statements_analyzer.get_config().add_fixmes
    {
        if !analysis_data.add_replacement(
            (
                pos.start_offset() as u32,
                call_expr.1[0].1.pos().start_offset() as u32,
            ),
            Replacement::Remove,
        ) {
            return;
        }

        if !analysis_data.add_replacement(
            (
                call_expr.1[0].1.pos().end_offset() as u32,
                call_expr.1[1].1.pos().start_offset() as u32,
            ),
            Replacement::Substitute("[".to_string()),
        ) {
            return;
        }

        analysis_data.add_replacement(
            (
                call_expr.1[1].1.pos().end_offset() as u32,
                pos.end_offset() as u32,
            ),
            Replacement::Substitute("]".to_string()),
        );

        return;
    }

    let expr_var_id = expression_identifier::get_var_id(
        &call_expr.1[0].1,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some((
            statements_analyzer.get_codebase(),
            statements_analyzer.get_interner(),
        )),
    );

    let dim_var_id =
        expression_identifier::get_dim_id(&call_expr.1[1].1, None, &FxHashMap::default());

    if let (Some(expr_var_id), Some(dim_var_id)) = (expr_var_id, dim_var_id) {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::UnnecessaryShapesIdx,
                format!(
                    "The field {} is always present on the shape -- consider using {}[{}] instead",
                    dim_var_id, expr_var_id, dim_var_id
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            &statements_analyzer
                .get_file_analyzer()
                .get_file_source()
                .file_path_actual,
        );
    }
}
