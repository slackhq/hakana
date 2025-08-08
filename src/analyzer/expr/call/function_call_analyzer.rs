use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::t_atomic::DictKey;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::comparison::union_type_comparator;
use hakana_code_info::ttype::{get_arrayish_params, get_void};
use hakana_code_info::{VarId, EFFECT_WRITE_LOCAL, EFFECT_WRITE_PROPS};
use hakana_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::expr::call::arguments_analyzer;
use crate::expr::call_analyzer::{apply_effects, check_template_result};
use crate::expr::{echo_analyzer, exit_analyzer, expression_identifier, isset_analyzer};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::control_action::ControlAction;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{expression_analyzer, formula_generator};
use hakana_code_info::assertion::Assertion;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::taint::SinkType;
use hakana_code_info::ttype::template::TemplateResult;
use indexmap::IndexMap;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::function_call_return_type_fetcher;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        (&Pos, &ast_defs::Id_),
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    is_sub_expression: bool,
) -> Result<(), AnalysisError> {
    let name = expr.0 .1;

    let resolved_names = statements_analyzer.file_analyzer.resolved_names;

    // we special-case this because exit is used in
    // `as` ternaries where the positions may be fake
    if name == "exit" || name == "die" {
        exit_analyzer::analyze(statements_analyzer, expr.2, pos, analysis_data, context)?;
        return Ok(());
    }

    // we special-case this because isset is used in
    // null coalesce ternaries where the positions may be fake
    if name == "isset" && !expr.2.is_empty() {
        let first_arg = &expr.2[0].to_expr_ref();
        return isset_analyzer::analyze(
            statements_analyzer,
            first_arg,
            pos,
            analysis_data,
            context,
        );
    }

    if (name == "unset" || name == "\\unset") && !expr.2.is_empty() {
        let first_arg = &expr.2.first().unwrap().to_expr_ref();
        context.inside_unset = true;
        expression_analyzer::analyze(statements_analyzer, first_arg, analysis_data, context, true)?;
        context.inside_unset = false;
        analysis_data.expr_effects.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            EFFECT_WRITE_LOCAL,
        );
        analysis_data.combine_effects(first_arg.pos(), pos, pos);
        analysis_data.set_expr_type(pos, get_void());

        return Ok(());
    }

    if name == "echo" {
        return echo_analyzer::analyze(statements_analyzer, expr.2, pos, analysis_data, context);
    }

    let name = if name == "\\in_array" {
        StrId::IN_ARRAY
    } else if let Some(fq_name) = resolved_names.get(&(expr.0 .0.start_offset() as u32)) {
        *fq_name
    } else {
        return Err(AnalysisError::InternalError(
            "Cannot resolve function name".to_string(),
            statements_analyzer.get_hpos(pos),
        ));
    };

    let codebase = statements_analyzer.codebase;

    let function_storage =
        if let Some(function_storage) = codebase.functionlike_infos.get(&(name, StrId::EMPTY)) {
            function_storage
        } else {
            let interned_name = statements_analyzer.interner.lookup(&name);

            // ignore non-existent functions that are in HH\
            // as these can differ between Hakana and hh_server
            if !interned_name.starts_with("HH\\") && !interned_name.starts_with("xhprof_") {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::NonExistentFunction,
                        format!("Function {} is not defined", interned_name),
                        statements_analyzer.get_hpos(expr.0 .0),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                analysis_data.symbol_references.add_reference_to_symbol(
                    &context.function_context,
                    name,
                    false,
                );
            }

            return Ok(());
        };

    if function_storage.user_defined {
        analysis_data.symbol_references.add_reference_to_symbol(
            &context.function_context,
            name,
            false,
        );
    }

    let mut template_result = TemplateResult::new(IndexMap::new(), IndexMap::new());

    if !function_storage.template_types.is_empty() {
        template_result
            .template_types
            .extend(function_storage.template_types.clone());
    }

    let functionlike_id = FunctionLikeIdentifier::Function(name);

    arguments_analyzer::check_arguments_match(
        statements_analyzer,
        expr.1,
        expr.2,
        expr.3,
        &functionlike_id,
        function_storage,
        None,
        analysis_data,
        context,
        &mut template_result,
        pos,
        Some(expr.0 .0),
    )?;

    apply_effects(
        functionlike_id,
        function_storage,
        analysis_data,
        pos,
        expr.2,
    );

    if let Some(effects) = analysis_data
        .expr_effects
        .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
    {
        if effects > &EFFECT_WRITE_PROPS {
            context.remove_mutable_object_vars();
        }
    }

    if function_storage.ignore_taints_if_true {
        analysis_data.if_true_assertions.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            FxHashMap::from_iter([("hakana taints".to_string(), vec![Assertion::IgnoreTaints])]),
        );
    }

    check_template_result(
        statements_analyzer,
        &mut template_result,
        pos,
        &functionlike_id,
    );

    if let Some(banned_message) = function_storage.banned_function_message {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::BannedFunction,
                statements_analyzer
                    .interner
                    .lookup(&banned_message)
                    .to_string(),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    if !function_storage.is_production_code
        && function_storage.user_defined
        && context.function_context.is_production(codebase)
    {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::TestOnlyCall,
                format!(
                    "Cannot call test-only function {} from non-test context",
                    statements_analyzer.interner.lookup(&name)
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        )
    }

    check_service_calls(
        statements_analyzer,
        pos,
        analysis_data,
        context,
        codebase,
        function_storage,
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

    analysis_data.set_expr_type(pos, stmt_type.clone());

    if stmt_type.is_nothing() && !context.inside_loop {
        context.has_returned = true;
        context.control_actions.insert(ControlAction::End);
        return Ok(());
    }

    match name {
        StrId::INVARIANT => {
            if let Some(first_arg) = &expr.2.first() {
                process_invariant(
                    first_arg.to_expr_ref(),
                    context,
                    statements_analyzer,
                    analysis_data,
                );
            }
        }
        StrId::LIB_C_IS_EMPTY => {
            let expr_var_id = expression_identifier::get_var_id(
                &expr.2[0].to_expr_ref(),
                context.function_context.calling_class,
                resolved_names,
                Some((statements_analyzer.codebase, statements_analyzer.interner)),
            );
            if let Some(expr_var_id) = expr_var_id {
                analysis_data.if_true_assertions.insert(
                    (pos.start_offset() as u32, pos.end_offset() as u32),
                    FxHashMap::from_iter([(expr_var_id, vec![Assertion::EmptyCountable])]),
                );
            }
        }
        StrId::LIB_C_CONTAINS
        | StrId::LIB_C_CONTAINS_KEY
        | StrId::LIB_DICT_CONTAINS
        | StrId::LIB_DICT_CONTAINS_KEY => {
            let expr_var_id = expression_identifier::get_var_id(
                &expr.2[0].to_expr_ref(),
                context.function_context.calling_class,
                resolved_names,
                Some((statements_analyzer.codebase, statements_analyzer.interner)),
            );

            if (name == StrId::LIB_C_CONTAINS || name == StrId::LIB_DICT_CONTAINS)
                && expr.2.len() == 2
            {
                let container_type = analysis_data
                    .get_expr_type(expr.2[0].to_expr_ref().pos())
                    .cloned();
                let second_arg_type = analysis_data
                    .get_expr_type(expr.2[1].to_expr_ref().pos())
                    .cloned();
                check_array_key_or_value_type(
                    codebase,
                    statements_analyzer,
                    analysis_data,
                    container_type,
                    second_arg_type,
                    pos,
                    false,
                    name,
                    &context.function_context.calling_functionlike_id,
                );
            } else if expr.2.len() >= 2 {
                let container_type = analysis_data
                    .get_expr_type(expr.2[0].to_expr_ref().pos())
                    .cloned();

                if let Some(expr_var_id) = expr_var_id {
                    if let aast::Expr_::String(boxed) = &expr.2[1].to_expr_ref().2 {
                        let dim_var_id = boxed.to_string();
                        analysis_data.if_true_assertions.insert(
                            (pos.start_offset() as u32, pos.end_offset() as u32),
                            FxHashMap::from_iter([(
                                expr_var_id.clone(),
                                vec![Assertion::HasArrayKey(DictKey::String(dim_var_id))],
                            )]),
                        );
                    } else if let aast::Expr_::Int(boxed) = &expr.2[1].to_expr_ref().2 {
                        analysis_data.if_true_assertions.insert(
                            (pos.start_offset() as u32, pos.end_offset() as u32),
                            FxHashMap::from_iter([(
                                expr_var_id.clone(),
                                vec![Assertion::HasArrayKey(DictKey::Int(
                                    boxed.parse::<u64>().unwrap(),
                                ))],
                            )]),
                        );
                    } else {
                        if let Some(dim_var_id) = expression_identifier::get_dim_id(
                            &expr.2[1].to_expr_ref(),
                            Some((statements_analyzer.codebase, statements_analyzer.interner)),
                            resolved_names,
                        ) {
                            analysis_data.if_true_assertions.insert(
                                (pos.start_offset() as u32, pos.end_offset() as u32),
                                FxHashMap::from_iter([(
                                    format!("{}[{}]", expr_var_id, dim_var_id),
                                    vec![Assertion::ArrayKeyExists],
                                )]),
                            );
                        }

                        let second_arg_type =
                            analysis_data.get_expr_type(expr.2[1].to_expr_ref().pos());
                        if let Some(second_arg_type) = second_arg_type {
                            check_array_key_or_value_type(
                                codebase,
                                statements_analyzer,
                                analysis_data,
                                container_type,
                                Some(second_arg_type.clone()),
                                pos,
                                true,
                                name,
                                &context.function_context.calling_functionlike_id,
                            );
                        }
                    }
                }
            }

            if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                let second_arg_var_id = expression_identifier::get_var_id(
                    &expr.2[1].to_expr_ref(),
                    context.function_context.calling_class,
                    resolved_names,
                    Some((statements_analyzer.codebase, statements_analyzer.interner)),
                );

                if let Some(expr_var_id) = second_arg_var_id {
                    if let Some(expr_var_interned_id) =
                        statements_analyzer.interner.get(&expr_var_id)
                    {
                        analysis_data.if_true_assertions.insert(
                            (pos.start_offset() as u32, pos.end_offset() as u32),
                            FxHashMap::from_iter([(
                                "hakana taints".to_string(),
                                vec![Assertion::RemoveTaints(
                                    VarId(expr_var_interned_id),
                                    SinkType::user_controllable_taints(),
                                )],
                            )]),
                        );
                    }
                }
            }
        }
        StrId::LIB_STR_STARTS_WITH => {
            if expr.2.len() == 2 {
                if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                    let expr_var_id = expression_identifier::get_var_id(
                        &expr.2[0].to_expr_ref(),
                        context.function_context.calling_class,
                        resolved_names,
                        Some((statements_analyzer.codebase, statements_analyzer.interner)),
                    );

                    let second_arg_type =
                        analysis_data.get_expr_type(expr.2[1].to_expr_ref().pos());

                    // if we have a HH\Lib\Str\starts_with($foo, "/something") check
                    // we can remove url-specific taints
                    if let (Some(expr_var_id), Some(second_arg_type)) =
                        (expr_var_id, second_arg_type)
                    {
                        if let Some(str) = second_arg_type.get_single_literal_string_value() {
                            if str.len() > 1 && str != "http://" && str != "https://" {
                                if let Some(id) = statements_analyzer.interner.get(&expr_var_id) {
                                    analysis_data.if_true_assertions.insert(
                                        (pos.start_offset() as u32, pos.end_offset() as u32),
                                        FxHashMap::from_iter([(
                                            "hakana taints".to_string(),
                                            vec![Assertion::RemoveTaints(
                                                VarId(id),
                                                vec![
                                                    SinkType::HtmlAttributeUri,
                                                    SinkType::CurlUri,
                                                    SinkType::RedirectUri,
                                                ],
                                            )],
                                        )]),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        StrId::LIB_REGEX_MATCHES => {
            if expr.2.len() == 2 {
                if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                    let expr_var_id = expression_identifier::get_var_id(
                        &expr.2[0].to_expr_ref(),
                        context.function_context.calling_class,
                        resolved_names,
                        Some((statements_analyzer.codebase, statements_analyzer.interner)),
                    );

                    let second_arg_type =
                        analysis_data.get_expr_type(expr.2[1].to_expr_ref().pos());

                    // if we have a HH\Lib\Str\starts_with($foo, "/something") check
                    // we can remove url-specific taints
                    if let (Some(expr_var_id), Some(second_arg_type)) =
                        (expr_var_id, second_arg_type)
                    {
                        if let Some(str) = second_arg_type.get_single_literal_string_value() {
                            let mut hashes_to_remove = vec![];

                            if str.starts_with('^')
                                && str != "^http:\\/\\/"
                                && str != "^https:\\/\\/"
                                && str != "^https?:\\/\\/"
                            {
                                hashes_to_remove.extend([
                                    SinkType::HtmlAttributeUri,
                                    SinkType::CurlUri,
                                    SinkType::RedirectUri,
                                ]);

                                if str.ends_with('$') && !str.contains(".*") && !str.contains(".+")
                                {
                                    hashes_to_remove.extend([
                                        SinkType::HtmlTag,
                                        SinkType::CurlHeader,
                                        SinkType::CurlUri,
                                        SinkType::HtmlAttribute,
                                    ]);
                                }
                            }

                            if !hashes_to_remove.is_empty() {
                                if let Some(id) = statements_analyzer.interner.get(&expr_var_id) {
                                    analysis_data.if_true_assertions.insert(
                                        (pos.start_offset() as u32, pos.end_offset() as u32),
                                        FxHashMap::from_iter([(
                                            "hakana taints".to_string(),
                                            vec![Assertion::RemoveTaints(
                                                VarId(id),
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
        }
        StrId::ASIO_JOIN => {
            if context.inside_async {
                let issue = Issue::new(
                    IssueKind::NoJoinInAsyncFunction,
                    "Prefer to use the await keyword instead of blocking by calling HH\\Asio\\join() inside an async function.".to_string(),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                );

                let config = statements_analyzer.get_config();

                if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
                    // Only replace code that's not already covered by a FIXME
                    if context.inside_await
                        || !context.function_context.is_production(codebase)
                        || analysis_data.get_matching_hakana_fixme(&issue).is_none()
                    {
                        analysis_data.add_replacement(
                            (pos.start_offset() as u32, expr.0 .0.end_offset() as u32 + 1),
                            Replacement::Substitute(format!(
                                "{}await ",
                                if is_sub_expression { "(" } else { "" }
                            )),
                        );

                        if !is_sub_expression {
                            analysis_data.add_replacement(
                                (pos.end_offset() as u32 - 1, pos.end_offset() as u32),
                                Replacement::Substitute("".to_string()),
                            );
                        }
                    }
                } else {
                    analysis_data.maybe_add_issue(
                        issue,
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }
        }
        _ => {}
    }

    Ok(())
}

pub(crate) fn check_service_calls(
    statements_analyzer: &StatementsAnalyzer<'_>,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    codebase: &CodebaseInfo,
    function_storage: &FunctionLikeInfo,
) {
    // Check service calls
    if function_storage.is_production_code
        && function_storage.user_defined
        && context.function_context.is_production(codebase)
    {
        let mut expected_service_calls = function_storage.service_calls.iter().collect::<Vec<_>>();
        expected_service_calls.extend(function_storage.transitive_service_calls.iter());

        for service in expected_service_calls {
            // Add each transitive service call to the actual_service_calls set
            analysis_data.actual_service_calls.insert(service.clone());

            if !context
                .function_context
                .can_transitively_call_service(codebase, service)
            {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::MissingIndirectServiceCallsAttribute,
                        format!(
                            "This function transitively calls service '{}' but lacks the <<Hakana\\IndirectlyCallsService('{}')>> attribute",
                            service, service
                        ),
                        statements_analyzer.get_hpos(pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }
    }
}

fn process_invariant(
    first_arg: &aast::Expr<(), ()>,
    context: &mut BlockContext,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) {
    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class,
        context.function_context.calling_functionlike_id,
    );
    // todo support $a = !($b || $c)
    let var_object_id = (
        first_arg.pos().start_offset() as u32,
        first_arg.pos().end_offset() as u32,
    );
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
                &assert_type_assertions.keys().cloned().collect(),
                statements_analyzer,
                analysis_data,
                first_arg.pos(),
                true,
                false,
                &FxHashMap::default(),
            );
        }

        context.clauses = if !changed_var_ids.is_empty() {
            BlockContext::remove_reconciled_clauses(&simplified_clauses, &changed_var_ids).0
        } else {
            simplified_clauses
        }
        .into_iter()
        .map(Rc::new)
        .collect();
    }
}

fn check_array_key_or_value_type(
    codebase: &CodebaseInfo,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    container_type: Option<TUnion>,
    arg_type: Option<TUnion>,
    pos: &Pos,
    is_key: bool,
    function_name: StrId,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
) {
    let mut has_valid_container_type = false;
    let mut error_message = None;

    if let Some(container_type) = container_type {
        for atomic_type in &container_type.types {
            let arrayish_params = get_arrayish_params(atomic_type, codebase);
            if let Some(ref arg_type) = arg_type {
                if let Some((params_key, param_value)) = arrayish_params {
                    let param = if is_key { params_key } else { param_value };

                    let offset_type_contained_by_expected =
                        union_type_comparator::can_expression_types_be_identical(
                            codebase,
                            statements_analyzer.get_file_path(),
                            arg_type,
                            &param.clone(),
                            true,
                        );

                    if offset_type_contained_by_expected {
                        has_valid_container_type = true;
                    } else {
                        error_message = Some(format!(
                            "Second arg of {} expects type {}, saw {}",
                            statements_analyzer.interner.lookup(&function_name),
                            param.get_id(Some(statements_analyzer.interner)),
                            arg_type.get_id(Some(statements_analyzer.interner))
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
                        statements_analyzer.get_hpos(pos),
                        calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }
    }
}
