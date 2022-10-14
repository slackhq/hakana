use std::rc::Rc;

use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::StrId;
use hakana_reflection_info::{
    assertion::Assertion,
    codebase_info::symbols::Symbol,
    data_flow::{node::DataFlowNode, path::PathKind},
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
};
use hakana_type::template::standin_type_replacer;
use hakana_type::{
    add_union_type, get_arraykey, get_dict, get_mixed_any, template::TemplateResult,
};
use indexmap::IndexMap;
use oxidized::{
    aast,
    ast_defs::{self, Pos},
};
use rustc_hash::FxHashMap;

use crate::{
    expr::{
        call_analyzer::check_method_args, expression_identifier,
        fetch::array_fetch_analyzer::handle_array_access_on_dict,
    },
    scope_analyzer::ScopeAnalyzer,
    scope_context::ScopeContext,
    statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};

use super::{
    atomic_method_call_analyzer::AtomicMethodCallAnalysisResult, class_template_param_collector,
    method_call_return_type_fetcher,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    mut classlike_name: Symbol,
    method_name: &StrId,
    call_expr: (
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    lhs_type_part: &TAtomic,
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    lhs_var_id: Option<&String>,
    lhs_var_pos: Option<&Pos>,
    result: &mut AtomicMethodCallAnalysisResult,
) -> TUnion {
    tast_info
        .symbol_references
        .add_reference_to_symbol(&context.function_context, classlike_name.clone());

    let codebase = statements_analyzer.get_codebase();

    if codebase.interner.lookup(classlike_name) == "static" {
        classlike_name = context.function_context.calling_class.clone().unwrap();
    }

    let method_id = MethodIdentifier(classlike_name.clone(), method_name.clone());

    result
        .existent_method_ids
        .insert(method_id.to_string(&codebase.interner));

    let declaring_method_id = codebase.get_declaring_method_id(&method_id);

    let classlike_storage = codebase.classlike_infos.get(&classlike_name).unwrap();

    tast_info.symbol_references.add_reference_to_class_member(
        &context.function_context,
        (declaring_method_id.0, declaring_method_id.1),
    );

    if let Some(overridden_classlikes) = classlike_storage
        .overridden_method_ids
        .get(&declaring_method_id.1)
    {
        for overridden_classlike in overridden_classlikes {
            tast_info
                .symbol_references
                .add_reference_to_overridden_class_member(
                    &context.function_context,
                    (overridden_classlike.clone(), declaring_method_id.1),
                );
        }
    }

    let mut class_template_params = if codebase.interner.lookup(classlike_name) != "HH\\Vector"
        || codebase.interner.lookup(*method_name) != "fromItems"
    {
        class_template_param_collector::collect(
            codebase,
            codebase
                .classlike_infos
                .get(&declaring_method_id.0)
                .unwrap(),
            classlike_storage,
            Some(lhs_type_part),
            lhs_var_id.unwrap_or(&"".to_string()) == "$this",
        )
    } else {
        None
    };

    let functionlike_storage = codebase.get_method(&declaring_method_id).unwrap();

    let mut template_result = TemplateResult::new(
        functionlike_storage.template_types.clone(),
        class_template_params.clone().unwrap_or(IndexMap::new()),
    );

    if !functionlike_storage.where_constraints.is_empty() {
        if let Some(ref mut class_template_params) = class_template_params {
            for (template_name, where_type) in &functionlike_storage.where_constraints {
                println!("{}", pos.line());
                let template_type = class_template_params
                    .get(template_name)
                    .unwrap()
                    .get(&classlike_name)
                    .unwrap();

                standin_type_replacer::replace(
                    &where_type,
                    &mut template_result,
                    statements_analyzer.get_codebase(),
                    &Some(template_type.clone()),
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

    if !check_method_args(
        statements_analyzer,
        tast_info,
        &method_id,
        functionlike_storage,
        call_expr,
        &mut template_result,
        context,
        if_body_context,
        pos,
    ) {
        tast_info.expr_effects.insert(
            (pos.start_offset(), pos.end_offset()),
            crate::typed_ast::IMPURE,
        );

        return get_mixed_any();
    }

    if functionlike_storage.ignore_taints_if_true {
        tast_info.if_true_assertions.insert(
            (pos.start_offset(), pos.end_offset()),
            FxHashMap::from_iter([("hakana taints".to_string(), vec![Assertion::IgnoreTaints])]),
        );
    }

    if codebase.interner.lookup(method_id.0) == "HH\\Shapes" {
        if let Some(value) = handle_shapes_static_method(
            &method_id,
            call_expr,
            context,
            statements_analyzer,
            tast_info,
            pos,
            codebase,
        ) {
            return value;
        }
    }

    let return_type_candidate = method_call_return_type_fetcher::fetch(
        statements_analyzer,
        tast_info,
        context,
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

    return_type_candidate
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
    tast_info: &mut TastInfo,
    pos: &Pos,
    codebase: &hakana_reflection_info::codebase_info::CodebaseInfo,
) -> Option<TUnion> {
    match codebase.interner.lookup(method_id.1) {
        "keyExists" => {
            if call_expr.1.len() == 2 {
                let expr_var_id = expression_identifier::get_var_id(
                    &call_expr.1[0].1,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().get_file_source(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some(statements_analyzer.get_codebase()),
                );

                let dim_var_id = expression_identifier::get_dim_id(
                    &call_expr.1[1].1,
                    None,
                    &FxHashMap::default(),
                );

                if let Some(expr_var_id) = expr_var_id {
                    if let Some(mut dim_var_id) = dim_var_id {
                        if dim_var_id.starts_with("'") {
                            dim_var_id = dim_var_id[1..(dim_var_id.len() - 1)].to_string();
                            tast_info.if_true_assertions.insert(
                                (pos.start_offset(), pos.end_offset()),
                                FxHashMap::from_iter([(
                                    expr_var_id,
                                    vec![Assertion::HasArrayKey(DictKey::String(dim_var_id))],
                                )]),
                            );
                        } else {
                            tast_info.if_true_assertions.insert(
                                (pos.start_offset(), pos.end_offset()),
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

        "removeKey" => {
            if call_expr.1.len() == 2 {
                let expr_var_id = expression_identifier::get_var_id(
                    &call_expr.1[0].1,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().get_file_source(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some(statements_analyzer.get_codebase()),
                );
                let dim_var_id = expression_identifier::get_dim_id(
                    &call_expr.1[1].1,
                    None,
                    &FxHashMap::default(),
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
                            statements_analyzer.get_hpos(&call_expr.1[0].1.pos()),
                        );

                        for (_, parent_node) in &expr_type.parent_nodes {
                            tast_info.data_flow_graph.add_path(
                                parent_node,
                                &assignment_node,
                                PathKind::RemoveDictKey(dim_var_id.clone()),
                                None,
                                None,
                            );
                        }

                        new_type.parent_nodes = FxHashMap::from_iter([(
                            assignment_node.get_id().clone(),
                            assignment_node.clone(),
                        )]);

                        tast_info.data_flow_graph.add_node(assignment_node);

                        context.vars_in_scope.insert(expr_var_id, Rc::new(new_type));
                    }
                }
            }
        }
        "idx" => {
            if call_expr.1.len() >= 2 {
                let dict_type = tast_info.get_rc_expr_type(call_expr.1[0].1.pos()).cloned();
                let dim_type = tast_info.get_rc_expr_type(call_expr.1[1].1.pos()).cloned();

                let mut expr_type = None;

                if let (Some(dict_type), Some(dim_type)) = (dict_type, dim_type) {
                    let mut has_valid_expected_offset = false;

                    for atomic_type in &dict_type.types {
                        if let TAtomic::TDict { .. } = atomic_type {
                            let mut has_possibly_undefined = false;
                            let mut expr_type_inner = handle_array_access_on_dict(
                                statements_analyzer,
                                pos,
                                tast_info,
                                context,
                                atomic_type,
                                &*dim_type,
                                false,
                                &mut has_valid_expected_offset,
                                true,
                                &mut has_possibly_undefined,
                            );

                            if has_possibly_undefined && call_expr.1.len() == 2 {
                                expr_type_inner.add_type(TAtomic::TNull);
                            }

                            expr_type = Some(expr_type_inner);
                        }
                    }

                    if !has_valid_expected_offset && call_expr.1.len() > 2 {
                        let default_type = tast_info.get_expr_type(call_expr.1[2].1.pos());
                        expr_type = if let Some(expr_type) = expr_type {
                            Some(if let Some(default_type) = default_type {
                                add_union_type(expr_type, default_type, codebase, false)
                            } else {
                                get_mixed_any()
                            })
                        } else {
                            None
                        };
                    }
                }

                return Some(expr_type.unwrap_or(get_mixed_any()));
            }
        }
        "toDict" | "toArray" => {
            let arg_type = tast_info.get_expr_type(call_expr.1[0].1.pos()).cloned();

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
