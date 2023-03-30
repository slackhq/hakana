use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::t_atomic::DictKey;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::{StrId, EFFECT_WRITE_LOCAL, EFFECT_WRITE_PROPS};
use hakana_type::type_comparator::union_type_comparator;
use hakana_type::{get_arrayish_params, get_void};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::expr::call::arguments_analyzer;
use crate::expr::call_analyzer::{apply_effects, check_template_result};
use crate::expr::{echo_analyzer, exit_analyzer, expression_identifier, isset_analyzer};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler::reconciler;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::{expression_analyzer, formula_generator};
use hakana_reflection_info::assertion::Assertion;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::taint::SinkType;
use hakana_type::template::TemplateResult;
use indexmap::IndexMap;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::function_call_return_type_fetcher;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        (&Pos, &ast_defs::Id_),
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let name = expr.0 .1;

    let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

    let codebase = statements_analyzer.get_codebase();

    // we special-case this because exit is used in
    // `as` ternaries where the positions may be fake
    if name == "exit" || name == "die" {
        return exit_analyzer::analyze(statements_analyzer, expr.2, pos, analysis_data, context);
    }

    // we special-case this because isset is used in
    // null coalesce ternaries where the positions may be fake
    if name == "isset" {
        if expr.2.len() > 0 {
            let first_arg = &expr.2.first().unwrap().1;
            return isset_analyzer::analyze(
                statements_analyzer,
                first_arg,
                pos,
                analysis_data,
                context,
                if_body_context,
            );
        }
    }

    if name == "unset" || name == "\\unset" {
        if expr.2.len() > 0 {
            let first_arg = &expr.2.first().unwrap().1;
            context.inside_unset = true;
            let result = expression_analyzer::analyze(
                statements_analyzer,
                first_arg,
                analysis_data,
                context,
                if_body_context,
            );
            context.inside_unset = false;
            analysis_data
                .expr_effects
                .insert((pos.start_offset(), pos.end_offset()), EFFECT_WRITE_LOCAL);
            analysis_data.combine_effects(first_arg.pos(), pos, pos);
            analysis_data.set_expr_type(&pos, get_void());

            return result;
        }
    }

    if name == "echo" {
        return echo_analyzer::analyze(statements_analyzer, expr.2, pos, analysis_data, context);
    }

    let name = if name == "\\in_array" {
        statements_analyzer.get_interner().get("in_array").unwrap()
    } else if let Some(fq_name) = resolved_names.get(&expr.0 .0.start_offset()) {
        fq_name.clone()
    } else {
        panic!()
    };

    let function_storage = if let Some(function_storage) =
        get_named_function_info(statements_analyzer, &name, expr.0 .0)
    {
        function_storage
    } else {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentFunction,
                format!(
                    "Function {} is not defined",
                    statements_analyzer.get_interner().lookup(&name)
                ),
                statements_analyzer.get_hpos(&expr.0 .0),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return false;
    };

    let name = function_storage.name.clone();

    analysis_data.symbol_references.add_reference_to_symbol(
        &context.function_context,
        name.clone(),
        false,
    );

    let mut template_result = TemplateResult::new(IndexMap::new(), IndexMap::new());

    if !function_storage.template_types.is_empty() {
        template_result
            .template_types
            .extend(function_storage.template_types.clone());
    }

    let functionlike_id = FunctionLikeIdentifier::Function(function_storage.name.clone());

    arguments_analyzer::check_arguments_match(
        statements_analyzer,
        expr.1,
        expr.2,
        expr.3,
        &functionlike_id,
        &function_storage,
        None,
        analysis_data,
        context,
        if_body_context,
        &mut template_result,
        pos,
    );

    apply_effects(function_storage, analysis_data, pos, &expr.2);

    if let Some(effects) = analysis_data
        .expr_effects
        .get(&(pos.start_offset(), pos.end_offset()))
    {
        if effects > &EFFECT_WRITE_PROPS {
            context.remove_mutable_object_vars();
        }
    }

    if function_storage.ignore_taints_if_true {
        analysis_data.if_true_assertions.insert(
            (pos.start_offset(), pos.end_offset()),
            FxHashMap::from_iter([("hakana taints".to_string(), vec![Assertion::IgnoreTaints])]),
        );
    }

    check_template_result(
        statements_analyzer,
        &mut template_result,
        pos,
        &functionlike_id,
    );

    let stmt_type = function_call_return_type_fetcher::fetch(
        statements_analyzer,
        expr,
        pos,
        &functionlike_id,
        function_storage,
        template_result,
        analysis_data,
        context,
    );

    analysis_data.set_expr_type(&pos, stmt_type.clone());

    if stmt_type.is_nothing() && !context.inside_loop {
        context.has_returned = true;
    }

    let real_name = statements_analyzer.get_interner().lookup(&name);

    match real_name {
        "HH\\invariant" => {
            if let Some((_, first_arg)) = &expr.2.get(0) {
                process_invariant(first_arg, context, statements_analyzer, analysis_data);
            }
        }
        "HH\\Lib\\C\\contains_key"
        | "HH\\Lib\\Dict\\contains_key"
        | "HH\\Lib\\C\\contains"
        | "HH\\Lib\\Dict\\contains" => {
            let expr_var_id = expression_identifier::get_var_id(
                &expr.2[0].1,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                resolved_names,
                Some((
                    statements_analyzer.get_codebase(),
                    statements_analyzer.get_interner(),
                )),
            );

            if real_name == "HH\\Lib\\C\\contains" || real_name == "HH\\Lib\\Dict\\contains" {
                let container_type = analysis_data.get_expr_type(expr.2[0].1.pos()).cloned();
                let second_arg_type = analysis_data.get_expr_type(expr.2[1].1.pos()).cloned();
                check_array_key_or_value_type(
                    codebase,
                    statements_analyzer,
                    analysis_data,
                    container_type,
                    second_arg_type,
                    &pos,
                    false,
                    &real_name.to_string(),
                    &context.function_context.calling_functionlike_id,
                );
            } else if real_name == "HH\\Lib\\C\\contains_key"
                || real_name == "HH\\Lib\\Dict\\contains_key"
            {
                let container_type = analysis_data.get_expr_type(expr.2[0].1.pos()).cloned();

                if let Some(expr_var_id) = expr_var_id {
                    if let aast::Expr_::String(boxed) = &expr.2[1].1 .2 {
                        let dim_var_id = boxed.to_string();
                        analysis_data.if_true_assertions.insert(
                            (pos.start_offset(), pos.end_offset()),
                            FxHashMap::from_iter([(
                                expr_var_id.clone(),
                                vec![Assertion::HasArrayKey(DictKey::String(dim_var_id))],
                            )]),
                        );
                    } else if let aast::Expr_::Int(boxed) = &expr.2[1].1 .2 {
                        analysis_data.if_true_assertions.insert(
                            (pos.start_offset(), pos.end_offset()),
                            FxHashMap::from_iter([(
                                expr_var_id.clone(),
                                vec![Assertion::HasArrayKey(DictKey::Int(
                                    boxed.parse::<u32>().unwrap(),
                                ))],
                            )]),
                        );
                    } else {
                        if let Some(dim_var_id) = expression_identifier::get_dim_id(
                            &expr.2[1].1,
                            Some((
                                statements_analyzer.get_codebase(),
                                statements_analyzer.get_interner(),
                            )),
                            resolved_names,
                        ) {
                            analysis_data.if_true_assertions.insert(
                                (pos.start_offset(), pos.end_offset()),
                                FxHashMap::from_iter([(
                                    format!("{}[{}]", expr_var_id, dim_var_id),
                                    vec![Assertion::ArrayKeyExists],
                                )]),
                            );
                        }

                        let second_arg_type = analysis_data.get_expr_type(expr.2[1].1.pos());
                        if let Some(second_arg_type) = second_arg_type {
                            check_array_key_or_value_type(
                                codebase,
                                statements_analyzer,
                                analysis_data,
                                container_type,
                                Some(second_arg_type.clone()),
                                &pos,
                                true,
                                &real_name.to_string(),
                                &context.function_context.calling_functionlike_id,
                            );
                        }
                    }
                }
            }

            if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                let second_arg_var_id = expression_identifier::get_var_id(
                    &expr.2[1].1,
                    context.function_context.calling_class.as_ref(),
                    statements_analyzer.get_file_analyzer().get_file_source(),
                    resolved_names,
                    Some((
                        statements_analyzer.get_codebase(),
                        statements_analyzer.get_interner(),
                    )),
                );

                if let Some(expr_var_id) = second_arg_var_id {
                    analysis_data.if_true_assertions.insert(
                        (pos.start_offset(), pos.end_offset()),
                        FxHashMap::from_iter([(
                            "hakana taints".to_string(),
                            vec![Assertion::RemoveTaints(
                                expr_var_id.clone(),
                                SinkType::user_controllable_taints(),
                            )],
                        )]),
                    );
                }
            }
        }
        "HH\\Lib\\Str\\starts_with" => {
            if expr.2.len() == 2 {
                if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                    let expr_var_id = expression_identifier::get_var_id(
                        &expr.2[0].1,
                        context.function_context.calling_class.as_ref(),
                        statements_analyzer.get_file_analyzer().get_file_source(),
                        resolved_names,
                        Some((
                            statements_analyzer.get_codebase(),
                            statements_analyzer.get_interner(),
                        )),
                    );

                    let second_arg_type = analysis_data.get_expr_type(expr.2[1].1.pos());

                    // if we have a HH\Lib\Str\starts_with($foo, "/something") check
                    // we can remove url-specific taints
                    if let (Some(expr_var_id), Some(second_arg_type)) =
                        (expr_var_id, second_arg_type)
                    {
                        if let Some(str) = second_arg_type.get_single_literal_string_value() {
                            if str.len() > 1 && str != "http://" && str != "https://" {
                                analysis_data.if_true_assertions.insert(
                                    (pos.start_offset(), pos.end_offset()),
                                    FxHashMap::from_iter([(
                                        "hakana taints".to_string(),
                                        vec![Assertion::RemoveTaints(
                                            expr_var_id.clone(),
                                            FxHashSet::from_iter([
                                                SinkType::HtmlAttributeUri,
                                                SinkType::CurlUri,
                                                SinkType::RedirectUri,
                                            ]),
                                        )],
                                    )]),
                                );
                            }
                        }
                    }
                }
            }
        }
        "HH\\Lib\\Regex\\matches" => {
            if expr.2.len() == 2 {
                if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                    let expr_var_id = expression_identifier::get_var_id(
                        &expr.2[0].1,
                        context.function_context.calling_class.as_ref(),
                        statements_analyzer.get_file_analyzer().get_file_source(),
                        resolved_names,
                        Some((
                            statements_analyzer.get_codebase(),
                            statements_analyzer.get_interner(),
                        )),
                    );

                    let second_arg_type = analysis_data.get_expr_type(expr.2[1].1.pos());

                    // if we have a HH\Lib\Str\starts_with($foo, "/something") check
                    // we can remove url-specific taints
                    if let (Some(expr_var_id), Some(second_arg_type)) =
                        (expr_var_id, second_arg_type)
                    {
                        if let Some(str) = second_arg_type.get_single_literal_string_value() {
                            let mut hashes_to_remove = FxHashSet::default();

                            if str.starts_with("^") {
                                if str != "^http:\\/\\/"
                                    && str != "^https:\\/\\/"
                                    && str != "^https?:\\/\\/"
                                {
                                    hashes_to_remove.extend([
                                        SinkType::HtmlAttributeUri,
                                        SinkType::CurlUri,
                                        SinkType::RedirectUri,
                                    ]);

                                    if str.ends_with("$")
                                        && !str.contains(".*")
                                        && !str.contains(".+")
                                    {
                                        hashes_to_remove.extend([
                                            SinkType::HtmlTag,
                                            SinkType::CurlHeader,
                                            SinkType::CurlUri,
                                            SinkType::HtmlAttribute,
                                        ]);
                                    }
                                }
                            }

                            if !hashes_to_remove.is_empty() {
                                analysis_data.if_true_assertions.insert(
                                    (pos.start_offset(), pos.end_offset()),
                                    FxHashMap::from_iter([(
                                        "hakana taints".to_string(),
                                        vec![Assertion::RemoveTaints(
                                            expr_var_id.clone(),
                                            hashes_to_remove,
                                        )],
                                    )]),
                                );
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    true
}

fn process_invariant(
    first_arg: &aast::Expr<(), ()>,
    context: &mut ScopeContext,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) {
    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );
    // todo support $a = !($b || $c)
    let var_object_id = (first_arg.pos().start_offset(), first_arg.pos().end_offset());
    let assert_clauses = formula_generator::get_formula(
        var_object_id,
        var_object_id,
        first_arg,
        &assertion_context,
        analysis_data,
        true,
        false,
    );
    if let Ok(assert_clauses) = assert_clauses {
        let simplified_clauses = hakana_algebra::simplify_cnf({
            let mut c = context.clauses.iter().map(|v| &**v).collect::<Vec<_>>();
            c.extend(assert_clauses.iter());
            c
        });

        let (assert_type_assertions, active_type_assertions) =
            hakana_algebra::get_truths_from_formula(
                simplified_clauses.iter().collect(),
                None,
                &mut FxHashSet::default(),
            );

        let mut changed_var_ids = FxHashSet::default();

        if !assert_type_assertions.is_empty() {
            reconciler::reconcile_keyed_types(
                &assert_type_assertions,
                active_type_assertions,
                context,
                &mut changed_var_ids,
                &assert_type_assertions
                    .iter()
                    .map(|(k, _)| k.clone())
                    .collect(),
                statements_analyzer,
                analysis_data,
                first_arg.pos(),
                true,
                false,
                &FxHashMap::default(),
            );
        }

        context.clauses = if !changed_var_ids.is_empty() {
            ScopeContext::remove_reconciled_clauses(&simplified_clauses, &changed_var_ids).0
        } else {
            simplified_clauses
        }
        .into_iter()
        .map(|v| Rc::new(v))
        .collect();
    }
}

fn get_named_function_info<'a>(
    statements_analyzer: &'a StatementsAnalyzer,
    name: &StrId,
    _pos: &Pos,
) -> Option<&'a FunctionLikeInfo> {
    let codebase = statements_analyzer.get_codebase();

    codebase.functionlike_infos.get(name)
}

fn check_array_key_or_value_type(
    codebase: &CodebaseInfo,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    container_type: Option<TUnion>,
    arg_type: Option<TUnion>,
    pos: &Pos,
    is_key: bool,
    function_name: &String,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
) {
    let mut has_valid_container_type = false;
    let mut error_message = None;

    if let Some(container_type) = container_type {
        for atomic_type in &container_type.types {
            let arrayish_params = get_arrayish_params(&atomic_type, codebase);
            if let Some(ref arg_type) = arg_type {
                if let Some((params_key, param_value)) = arrayish_params {
                    let param = if is_key { params_key } else { param_value };

                    let offset_type_contained_by_expected =
                        union_type_comparator::can_expression_types_be_identical(
                            codebase,
                            &arg_type,
                            &param.clone(),
                            true,
                        );

                    if offset_type_contained_by_expected {
                        has_valid_container_type = true;
                    } else {
                        error_message = Some(format!(
                            "Second arg of {} expects type {}, saw {}",
                            function_name,
                            param.get_id(Some(&statements_analyzer.get_interner())),
                            arg_type.get_id(Some(&statements_analyzer.get_interner()))
                        ));
                    }
                };
            }
        }

        if let Some(error_message) = error_message {
            if !has_valid_container_type {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidContainsCheck,
                        error_message,
                        statements_analyzer.get_hpos(&pos),
                        &calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }
    }
}
