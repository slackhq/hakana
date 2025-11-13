use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::member_visibility::MemberVisibility;
use hakana_code_info::method_identifier::MethodIdentifier;
use hakana_code_info::t_atomic::DictKey;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::comparison::union_type_comparator;
use hakana_code_info::ttype::{get_arrayish_params, get_void};
use hakana_code_info::{EFFECT_WRITE_LOCAL, EFFECT_WRITE_PROPS, VarId};
use hakana_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::expr::call::arguments_analyzer;
use crate::expr::call_analyzer::{apply_effects, check_template_result};
use crate::expr::{echo_analyzer, exit_analyzer, expression_identifier, isset_analyzer};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::BlockContext;
use crate::scope::control_action::ControlAction;
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
    let name = expr.0.1;

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
    } else if let Some(fq_name) = resolved_names.get(&(expr.0.0.start_offset() as u32)) {
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
                        statements_analyzer.get_hpos(expr.0.0),
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

        if statements_analyzer
            .get_config()
            .collect_goto_definition_locations
        {
            analysis_data.definition_locations.insert(
                (expr.0.0.start_offset() as u32, expr.0.0.end_offset() as u32),
                (name, StrId::EMPTY),
            );
        }
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
        Some(expr.0.0),
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

    check_php_standard_library(statements_analyzer, pos, analysis_data, context, name);

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
                            (pos.start_offset() as u32, expr.0.0.end_offset() as u32 + 1),
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
        _ => {
            // Check for ImplicitAsioJoin - functions that have async versions
            if let Some(async_version) = function_storage.async_version {
                check_implicit_asio_join(
                    statements_analyzer,
                    pos,
                    expr.0.0,
                    analysis_data,
                    context,
                    functionlike_id,
                    async_version,
                    is_sub_expression,
                    None,
                );
            }
        }
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

pub(crate) fn check_implicit_asio_join(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    name_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    functionlike_id: FunctionLikeIdentifier,
    async_version: FunctionLikeIdentifier,
    is_sub_expression: bool,
    lhs_expr: Option<&aast::Expr<(), ()>>,
) {
    let issue = Issue::new(
        IssueKind::ImplicitAsioJoin,
        format!(
            "Call to a {} {} that just wraps an async version {}",
            if let FunctionLikeIdentifier::Method(_, _) = functionlike_id {
                "method"
            } else {
                "function"
            },
            functionlike_id.to_string(statements_analyzer.interner),
            get_async_version_name(
                async_version,
                functionlike_id,
                lhs_expr,
                context,
                statements_analyzer.interner,
                false
            )
            .unwrap_or_else(|| "unknown".to_string())
        ),
        statements_analyzer.get_hpos(pos),
        &context.function_context.calling_functionlike_id,
    );

    let config = statements_analyzer.get_config();

    // If the candidate method is not callable from the current context, report an issue but don't autofix,
    // to make the optimization possibility known without emitting invalid code.
    if config.issues_to_fix.contains(&issue.kind)
        && !config.add_fixmes
        && can_call_async_version(statements_analyzer, context, functionlike_id, async_version)
    {
        // Only replace code that's not already covered by a FIXME
        if !context
            .function_context
            .is_production(statements_analyzer.codebase)
            || analysis_data.get_matching_hakana_fixme(&issue).is_none()
        {
            if let Some(replacement_fn) = get_async_version_name(
                async_version,
                functionlike_id,
                lhs_expr,
                context,
                statements_analyzer.interner,
                true,
            ) {
                // The await expression emitted by autofixing a sync wrapper may be part of an expression chain.
                // Try to account for this by seeing if the end of the current call also lines up with
                // the end of the current statement.
                let should_wrap_await =
                    if let Some(current_stmt_end) = analysis_data.current_stmt_end {
                        // offset by one to account for statement-closing comma
                        is_sub_expression || (1 + pos.end_offset() as u32) != current_stmt_end
                    } else {
                        is_sub_expression
                    };

                let await_or_join = if context.inside_async {
                    format!("{}await ", if should_wrap_await { "(" } else { "" },)
                } else {
                    "Asio\\join(".into()
                };

                // Call analysis may run more than once for a given invocation,
                // such as a variable of base type T that may be assigned subtype T' or T"
                // in alternate loop branches. Guard against duplicate inserts resulting from this.
                let await_or_join_insert_offset = pos.start_offset() as u32;
                let should_insert = match analysis_data.insertions.get(&await_or_join_insert_offset)
                {
                    Some(existing_insertions) => !existing_insertions.contains(&await_or_join),
                    _ => true,
                };

                if should_insert {
                    analysis_data.insert_at(await_or_join_insert_offset, await_or_join);
                }

                // Instance methods may be invoked on an arbitrary left-hand side expression,
                // e.g. (new FooClass()). Ensure we leave this untouched when converting the method call.
                if lhs_expr.is_some() {
                    analysis_data.add_replacement(
                        (name_pos.start_offset() as u32, name_pos.end_offset() as u32),
                        Replacement::Substitute(replacement_fn),
                    );
                } else {
                    analysis_data.add_replacement(
                        (pos.start_offset() as u32, name_pos.end_offset() as u32),
                        Replacement::Substitute(replacement_fn),
                    );
                }

                if should_wrap_await || !context.inside_async {
                    analysis_data.add_replacement(
                        (pos.end_offset() as u32, pos.end_offset() as u32),
                        Replacement::Substitute(")".to_string()),
                    );
                }
            }
        }
    } else {
        analysis_data.maybe_add_issue(issue, config, statements_analyzer.get_file_path_actual());
    }
}

fn get_async_version_name(
    async_version: FunctionLikeIdentifier,
    functionlike_id: FunctionLikeIdentifier,
    lhs_expr: Option<&aast::Expr<(), ()>>,
    context: &BlockContext,
    interner: &hakana_str::Interner,
    localize_string: bool,
) -> Option<String> {
    match async_version {
        FunctionLikeIdentifier::Function(id) => Some(
            if localize_string {
                "\\".to_string()
            } else {
                "".to_string()
            } + interner.lookup(&id),
        ),
        FunctionLikeIdentifier::Method(mut class_name, method_name) => Some({
            // When autofixing instance method calls, we need to preserve the original LHS expression
            // that the method is being invoked on, so only return the target method name.
            if let Some(_) = lhs_expr {
                if localize_string {
                    return Some(interner.lookup(&method_name).into());
                }
            }

            let mut is_local = false;

            if let FunctionLikeIdentifier::Method(existing_class_name, _) = functionlike_id {
                if class_name == StrId::SELF || class_name == StrId::STATIC {
                    if context.function_context.calling_class != Some(existing_class_name) {
                        class_name = existing_class_name;
                    } else {
                        is_local = true;
                    }
                }
            }

            format!(
                "{}{}::{}",
                if !is_local && localize_string {
                    "\\"
                } else {
                    ""
                },
                interner.lookup(&class_name),
                interner.lookup(&method_name)
            )
        }),
        _ => None,
    }
}

/// Check whether the possible async version of a sync function
/// is callable (visible) from the current context.
fn can_call_async_version(
    statements_analyzer: &StatementsAnalyzer,
    context: &BlockContext,
    sync_version: FunctionLikeIdentifier,
    async_version: FunctionLikeIdentifier,
) -> bool {
    // If the async version is a method, consider whether it's visible from the calling context.
    if let Some(method_id) = async_version.as_method_identifier() {
        // The sync method may have been called via a relative reference
        // such as self/static/parent. Consult the sync version to determine what class this corresponds to.
        let invocation_target_class = match method_id {
            MethodIdentifier(StrId::SELF | StrId::STATIC, _) => sync_version
                .as_method_identifier()
                .map(|MethodIdentifier(cls, _)| cls),
            MethodIdentifier(StrId::PARENT, _) => sync_version
                .as_method_identifier()
                .map(|MethodIdentifier(cls, _)| cls)
                .and_then(|cls| statements_analyzer.codebase.classlike_infos.get(&cls))
                .and_then(|cls| cls.direct_parent_class),
            MethodIdentifier(declaring_class, _) => Some(declaring_class),
        };
        let MethodIdentifier(_, method_name) = method_id;

        invocation_target_class
            .and_then(|appearing_class| {
                statements_analyzer
                    .codebase
                    .classlike_infos
                    .get(&appearing_class)
            })
            .and_then(|cls| cls.appearing_method_ids.get(&method_name))
            .and_then(|declaring_class| {
                let resolved_async_version_id = MethodIdentifier(*declaring_class, method_name);
                let method_info = statements_analyzer
                    .codebase
                    .get_method(&resolved_async_version_id)
                    .and_then(|method| method.method_info.as_ref())?;

                let calling_class = context.function_context.calling_class;

                match method_info.visibility {
                    MemberVisibility::Private => calling_class.map(|cls| cls.eq(&declaring_class)),
                    MemberVisibility::Protected => calling_class.map(|cls| {
                        statements_analyzer
                            .codebase
                            .class_extends_or_implements(&cls, &declaring_class)
                    }),
                    MemberVisibility::Public => Some(true),
                }
            })
            .unwrap_or(false)
    } else {
        true
    }
}

fn check_php_standard_library(
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    name: StrId,
) {
    let interner = statements_analyzer.interner;

    let replacement = match name {
        // String functions
        StrId::UCWORDS => Some("HH\\Lib\\Str\\capitalize_words"),
        StrId::UCFIRST => Some("HH\\Lib\\Str\\capitalize"),
        StrId::STRTOLOWER => Some("HH\\Lib\\Str\\lowercase"),
        StrId::STRTOUPPER => Some("HH\\Lib\\Str\\uppercase"),
        StrId::STR_REPLACE => Some("HH\\Lib\\Str\\replace"),
        StrId::STR_IREPLACE => Some("HH\\Lib\\Str\\replace_ci"),
        StrId::STRPOS => Some("HH\\Lib\\Str\\search"),
        StrId::STRIPOS => Some("HH\\Lib\\Str\\search_ci"),
        StrId::STRRPOS => Some("HH\\Lib\\Str\\search_last"),
        StrId::IMPLODE => Some("HH\\Lib\\Str\\join"),
        StrId::JOIN => Some("HH\\Lib\\Str\\join"),
        StrId::SUBSTR_REPLACE => Some("HH\\Lib\\Str\\splice"),
        StrId::SUBSTR => Some(
            "HH\\Lib\\Str\\slice or one of Str\\{starts_with, ends_with, strip_prefix, strip_suffix}",
        ),
        StrId::STR_REPEAT => Some("HH\\Lib\\Str\\repeat"),
        StrId::TRIM => Some("HH\\Lib\\Str\\trim"),
        StrId::LTRIM => Some("HH\\Lib\\Str\\trim_left"),
        StrId::RTRIM => Some("HH\\Lib\\Str\\trim_right"),
        StrId::STRLEN => Some("HH\\Lib\\Str\\length"),
        StrId::SPRINTF => Some("HH\\Lib\\Str\\format"),
        StrId::STR_SPLIT => Some("HH\\Lib\\Str\\chunk"),
        StrId::STRCMP => Some("HH\\Lib\\Str\\compare"),
        StrId::STRCASECMP => Some("HH\\Lib\\Str\\compare_ci"),
        StrId::NUMBER_FORMAT => Some("HH\\Lib\\Str\\format_number"),

        // Math functions
        StrId::ROUND => Some("HH\\Lib\\Math\\round"),
        StrId::CEIL => Some("HH\\Lib\\Math\\ceil"),
        StrId::FLOOR => Some("HH\\Lib\\Math\\floor"),
        StrId::ARRAY_SUM => Some("HH\\Lib\\Math\\sum"),
        StrId::INTDIV => Some("HH\\Lib\\Math\\int_div"),
        StrId::EXP => Some("HH\\Lib\\Math\\exp"),
        StrId::ABS => Some("HH\\Lib\\Math\\abs"),
        StrId::BASE_CONVERT => Some("HH\\Lib\\Math\\base_convert"),
        StrId::COS => Some("HH\\Lib\\Math\\cos"),
        StrId::SIN => Some("HH\\Lib\\Math\\sin"),
        StrId::TAN => Some("HH\\Lib\\Math\\tan"),
        StrId::SQRT => Some("HH\\Lib\\Math\\sqrt"),
        StrId::LOG => Some("HH\\Lib\\Math\\log"),
        StrId::MIN => Some("HH\\Lib\\Math\\min"),
        StrId::MAX => Some("HH\\Lib\\Math\\max"),

        // Container functions
        StrId::COUNT => Some("HH\\Lib\\C\\count"),

        _ => None,
    };

    if let Some(replacement) = replacement {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::PHPStandardLibrary,
                format!("Use {} instead of {}", replacement, interner.lookup(&name)),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }
}
