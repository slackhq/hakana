use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::expr::call::arguments_analyzer;
use crate::expr::call_analyzer::check_template_result;
use crate::expr::{echo_analyzer, exit_analyzer, expression_identifier, isset_analyzer};
use crate::formula_generator;
use crate::reconciler::reconciler;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use function_context::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::assertion::Assertion;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::taint::SinkType;
use hakana_type::get_int;
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let mut name = expr.0 .1.clone();

    let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

    // we special-case this because exit is used in
    // `as` ternaries where the positions may be fake
    if name == "exit" || name == "die" {
        return exit_analyzer::analyze(statements_analyzer, expr.2, pos, tast_info, context);
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
                tast_info,
                context,
                if_body_context,
            );
        }
    }

    if name == "\\in_array" {
        name = "in_array".to_string();
    } else if let Some(fq_name) = resolved_names.get(&pos.start_offset()) {
        name = fq_name.clone();
    } else if let Some(fq_name) = resolved_names.get(&(pos.start_offset() + 1)) {
        name = fq_name.clone();
    }

    if name == "echo" {
        return echo_analyzer::analyze(statements_analyzer, expr.2, pos, tast_info, context);
    }

    if name == "rand" {
        tast_info.set_expr_type(&pos, get_int());
        return true;
    }

    let function_storage = if let Some(function_storage) =
        get_named_function_info(statements_analyzer, &name, expr.0 .0)
    {
        function_storage
    } else {
        tast_info.maybe_add_issue(Issue::new(
            IssueKind::NonExistentFunction,
            format!("Function {} is not defined", name),
            statements_analyzer.get_hpos(&expr.0 .0),
        ));

        return false;
    };

    tast_info
        .symbol_references
        .add_reference_to_symbol(&context.function_context, name.clone());

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
        tast_info,
        context,
        if_body_context,
        &mut template_result,
        pos,
    );

    if !function_storage.pure {
        context.remove_mutable_object_vars();
    }

    if function_storage.ignore_taints_if_true {
        tast_info.if_true_assertions.insert(
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

    if function_storage.pure {
        let mut was_pure_span = true;

        for arg in expr.2 {
            if !tast_info
                .pure_exprs
                .contains(&(arg.1.pos().start_offset(), arg.1.pos().end_offset()))
            {
                was_pure_span = false;
                break;
            }
        }

        if was_pure_span {
            tast_info
                .pure_exprs
                .insert((pos.start_offset(), pos.end_offset()));
        }
    }

    let stmt_type = function_call_return_type_fetcher::fetch(
        statements_analyzer,
        expr,
        pos,
        &functionlike_id,
        function_storage,
        template_result,
        tast_info,
        context,
    );

    tast_info.set_expr_type(&pos, stmt_type.clone());

    if stmt_type.is_nothing() && !context.inside_loop {
        context.has_returned = true;
    }

    if name == "HH\\invariant" {
        if let Some((_, first_arg)) = &expr.2.get(0) {
            process_function_effects(first_arg, context, statements_analyzer, tast_info);
        }
    } else if name == "HH\\Lib\\C\\contains_key"
        || name == "HH\\Lib\\Dict\\contains_key"
        || name == "HH\\Lib\\C\\contains"
        || name == "HH\\Lib\\Dict\\contains"
    {
        if name == "HH\\Lib\\C\\contains_key" || name == "HH\\Lib\\Dict\\contains_key" {
            let expr_var_id = expression_identifier::get_extended_var_id(
                &expr.2[0].1,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                resolved_names,
            );

            let dim_var_id = expression_identifier::get_dim_id(&expr.2[1].1);

            if let Some(expr_var_id) = expr_var_id {
                if let Some(mut dim_var_id) = dim_var_id {
                    if dim_var_id.starts_with("'") {
                        dim_var_id = dim_var_id[1..(dim_var_id.len() - 1)].to_string();
                        tast_info.if_true_assertions.insert(
                            (pos.start_offset(), pos.end_offset()),
                            FxHashMap::from_iter([(
                                format!("{}", expr_var_id),
                                vec![Assertion::HasArrayKey(dim_var_id)],
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

        if tast_info.data_flow_graph.kind == GraphKind::Taint {
            let second_arg_var_id = expression_identifier::get_extended_var_id(
                &expr.2[1].1,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                resolved_names,
            );

            if let Some(expr_var_id) = second_arg_var_id {
                tast_info.if_true_assertions.insert(
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
    } else if name == "HH\\Lib\\Str\\starts_with" && expr.2.len() == 2 {
        if tast_info.data_flow_graph.kind == GraphKind::Taint {
            let expr_var_id = expression_identifier::get_extended_var_id(
                &expr.2[0].1,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                resolved_names,
            );

            let second_arg_type = tast_info.get_expr_type(expr.2[1].1.pos());

            // if we have a HH\Lib\Str\starts_with($foo, "/something") check
            // we can remove url-specific taints
            if let (Some(expr_var_id), Some(second_arg_type)) = (expr_var_id, second_arg_type) {
                if let Some(str) = second_arg_type.get_single_literal_string_value() {
                    if str.len() > 1 && str != "http://" && str != "https://" {
                        tast_info.if_true_assertions.insert(
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
    } else if name == "HH\\Lib\\Regex\\matches" && expr.2.len() == 2 {
        if tast_info.data_flow_graph.kind == GraphKind::Taint {
            let expr_var_id = expression_identifier::get_extended_var_id(
                &expr.2[0].1,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                resolved_names,
            );

            let second_arg_type = tast_info.get_expr_type(expr.2[1].1.pos());

            // if we have a HH\Lib\Str\starts_with($foo, "/something") check
            // we can remove url-specific taints
            if let (Some(expr_var_id), Some(second_arg_type)) = (expr_var_id, second_arg_type) {
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

                            if str.ends_with("$") && !str.contains(".*") && !str.contains(".+") {
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
                        tast_info.if_true_assertions.insert(
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

    true
}

fn process_function_effects(
    first_arg: &aast::Expr<(), ()>,
    context: &mut ScopeContext,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
) {
    let assertion_context =
        statements_analyzer.get_assertion_context(context.function_context.calling_class.as_ref());
    // todo support $a = !($b || $c)
    let var_object_id = (first_arg.pos().start_offset(), first_arg.pos().end_offset());
    let assert_clauses = formula_generator::get_formula(
        var_object_id,
        var_object_id,
        first_arg,
        &assertion_context,
        tast_info,
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
                tast_info,
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
    name: &String,
    _pos: &Pos,
) -> Option<&'a FunctionLikeInfo> {
    let codebase = statements_analyzer.get_codebase();

    codebase.functionlike_infos.get(name)
}
