use hakana_reflection_info::code_location::StmtStart;
use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::{EFFECT_PURE, STR_AWAITABLE, STR_CONSTRUCT};
use hakana_type::get_arrayish_params;
use rustc_hash::FxHashSet;

use crate::custom_hook::AfterStmtAnalysisData;
use crate::expr::assertion_finder::get_functionlike_id_from_call;
use crate::expr::binop::assignment_analyzer;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::loop_scope::LoopScope;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::{
    break_analyzer, continue_analyzer, do_analyzer, for_analyzer, foreach_analyzer,
    ifelse_analyzer, return_analyzer, switch_analyzer, try_analyzer, while_analyzer,
};
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::t_atomic::TAtomic;
use oxidized::{aast, ast_defs};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: &aast::Stmt<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) -> bool {
    if let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset {
        if current_stmt_offset.line != stmt.0.line() {
            analysis_data.current_stmt_offset = Some(StmtStart {
                offset: stmt.0.start_offset(),
                line: stmt.0.line(),
                column: stmt.0.to_raw_span().start.column() as usize,
                add_newline: true,
            });
        }
    } else {
        analysis_data.current_stmt_offset = Some(StmtStart {
            offset: stmt.0.start_offset(),
            line: stmt.0.line(),
            column: stmt.0.to_raw_span().start.column() as usize,
            add_newline: true,
        });
    }

    if statements_analyzer.get_config().remove_fixmes {
        for (fixme_line, b) in analysis_data.hakana_fixme_or_ignores.iter_mut() {
            if fixme_line == &stmt.0.line() {
                for (_, (_, _, end, is_same_line)) in b {
                    *end = stmt.0.start_offset() as u64;
                    *is_same_line = true;
                }
            }
        }
    }

    match &stmt.1 {
        aast::Stmt_::Expr(boxed) => {
            if !expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                analysis_data,
                context,
                &mut None,
            ) {
                return false;
            }

            if statements_analyzer.get_config().find_unused_expressions {
                detect_unused_statement_expressions(
                    boxed,
                    statements_analyzer,
                    analysis_data,
                    stmt,
                    context,
                );
            }
        }
        aast::Stmt_::Return(_) => {
            return_analyzer::analyze(stmt, statements_analyzer, analysis_data, context);
        }
        aast::Stmt_::If(boxed) => {
            if !ifelse_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &stmt.0,
                analysis_data,
                context,
                loop_scope,
            ) {
                return false;
            }
        }
        aast::Stmt_::While(boxed) => {
            if !while_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1),
                analysis_data,
                context,
            ) {
                return false;
            }
        }
        aast::Stmt_::Do(boxed) => {
            if !do_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1),
                analysis_data,
                context,
            ) {
                return false;
            }
        }
        aast::Stmt_::For(boxed) => {
            if !for_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3),
                &stmt.0,
                analysis_data,
                context,
            ) {
                return false;
            }
        }
        aast::Stmt_::Foreach(boxed) => {
            if !foreach_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &stmt.0,
                analysis_data,
                context,
            ) {
                return false;
            }
        }
        aast::Stmt_::Noop => {
            // ignore
        }
        aast::Stmt_::Break => {
            break_analyzer::analyze(statements_analyzer, analysis_data, context, loop_scope);
        }
        aast::Stmt_::Continue => {
            continue_analyzer::analyze(statements_analyzer, analysis_data, context, loop_scope);
        }
        aast::Stmt_::Switch(boxed) => {
            switch_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &stmt.0,
                analysis_data,
                context,
                loop_scope,
            );
        }
        aast::Stmt_::Throw(boxed) => {
            context.inside_throw = true;

            let analysis_result = expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                analysis_data,
                context,
                &mut None,
            );

            context.inside_throw = false;
            context.has_returned = true;

            if !analysis_result {
                return false;
            }
        }
        aast::Stmt_::Try(boxed) => {
            if !try_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                analysis_data,
                context,
                loop_scope,
            ) {
                return false;
            }
        }
        aast::Stmt_::Markup(_) => {
            // opening tag, do nothing
        }
        aast::Stmt_::Awaitall(boxed) => {
            analyze_awaitall(
                (&boxed.0, &boxed.1 .0),
                statements_analyzer,
                analysis_data,
                context,
                stmt,
                loop_scope,
            );
        }
        aast::Stmt_::Using(boxed) => {
            for boxed_expr in &boxed.exprs.1 {
                if !expression_analyzer::analyze(
                    statements_analyzer,
                    &boxed_expr,
                    analysis_data,
                    context,
                    &mut None,
                ) {
                    return false;
                }
            }

            for using_stmt in &boxed.block {
                if !analyze(
                    statements_analyzer,
                    using_stmt,
                    analysis_data,
                    context,
                    loop_scope,
                ) {
                    return false;
                }
            }
        }
        aast::Stmt_::Block(boxed) => {
            for boxed_stmt in boxed {
                if !analyze(
                    statements_analyzer,
                    boxed_stmt,
                    analysis_data,
                    context,
                    loop_scope,
                ) {
                    return false;
                }
            }
        }
        aast::Stmt_::Fallthrough => {} // do nothing
        aast::Stmt_::YieldBreak | aast::Stmt_::AssertEnv(_) => {
            //println!("{:#?}", stmt);
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::UnrecognizedStatement,
                    "Unrecognized statement".to_string(),
                    statements_analyzer.get_hpos(&stmt.0),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
            return false;
        }
        aast::Stmt_::DeclareLocal(_) => {},
    }

    context.cond_referenced_var_ids = FxHashSet::default();

    for hook in &statements_analyzer.get_config().hooks {
        hook.after_stmt_analysis(
            analysis_data,
            AfterStmtAnalysisData {
                statements_analyzer,
                stmt: &stmt,
                context,
            },
        );
    }

    true
}

fn detect_unused_statement_expressions(
    boxed: &Box<aast::Expr<(), ()>>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    stmt: &aast::Stmt<(), ()>,
    context: &mut ScopeContext,
) {
    let functionlike_id = if let aast::Expr_::Call(boxed_call) = &boxed.2 {
        get_functionlike_id_from_call(
            boxed_call,
            Some(statements_analyzer.get_interner()),
            statements_analyzer.get_file_analyzer().resolved_names,
        )
    } else {
        None
    };

    if let Some(effect) = analysis_data
        .expr_effects
        .get(&(boxed.pos().start_offset(), boxed.pos().end_offset()))
    {
        if effect == &EFFECT_PURE {
            let mut is_constructor_call = false;
            let mut fn_can_throw = false;

            if let Some(functionlike_id) = functionlike_id {
                match functionlike_id {
                    FunctionLikeIdentifier::Function(function_id) => {
                        let function_name = statements_analyzer.get_interner().lookup(&function_id);

                        if function_name == "HH\\invariant"
                            || function_name == "HH\\invariant_violation"
                            || function_name == "trigger_error"
                            || function_name == "function_exists"
                            || function_name == "class_exists"
                            || function_name == "HH\\set_frame_metadata"
                            || function_name == "HH\\Lib\\C\\firstx"
                            || function_name == "HH\\Lib\\C\\lastx"
                            || function_name == "HH\\Lib\\C\\onlyx"
                            || function_name.contains("assert")
                        {
                            fn_can_throw = true;
                        }
                    }
                    FunctionLikeIdentifier::Method(_, method_name_id) => {
                        if method_name_id == STR_CONSTRUCT {
                            is_constructor_call = true;
                        }

                        let method_name =
                            statements_analyzer.get_interner().lookup(&method_name_id);

                        if method_name == "assert" {
                            fn_can_throw = true;
                        }
                    }
                }
            };

            if !is_constructor_call && !fn_can_throw {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::UnusedStatement,
                        "This statement has no effect and can be removed".to_string(),
                        statements_analyzer.get_hpos(&stmt.0),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
                return;
            }
        }
    }

    if let Some(functionlike_id) = functionlike_id {
        if let FunctionLikeIdentifier::Function(function_id) = functionlike_id {
            let codebase = statements_analyzer.get_codebase();
            if let Some(functionlike_info) = codebase.functionlike_infos.get(&function_id) {
                if functionlike_info.must_use {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::UnusedFunctionCall,
                            "This function is annotated with MustUse but the returned value is not used".to_string(),
                            statements_analyzer.get_hpos(&stmt.0),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                } else if let Some(expr_type) = analysis_data.get_rc_expr_type(boxed.pos()).cloned()
                {
                    let function_name = statements_analyzer.get_interner().lookup(&function_id);

                    if function_name.starts_with("HH\\Lib\\Keyset\\")
                        || function_name.starts_with("HH\\Lib\\Vec\\")
                        || function_name.starts_with("HH\\Lib\\Dict\\")
                    {
                        if expr_type.is_single() {
                            let array_types = get_arrayish_params(expr_type.get_single(), codebase);

                            if let Some((_, value_type)) = array_types {
                                if !value_type.is_null() && !value_type.is_void() {
                                    analysis_data.maybe_add_issue(
                                        Issue::new(
                                            IssueKind::UnusedBuiltinReturnValue,
                                            format!(
                                                "The value {} returned from {} should be consumed",
                                                expr_type.get_id(Some(
                                                    statements_analyzer.get_interner()
                                                )),
                                                function_name
                                            ),
                                            statements_analyzer.get_hpos(&stmt.0),
                                            &context.function_context.calling_functionlike_id,
                                        ),
                                        statements_analyzer.get_config(),
                                        statements_analyzer.get_file_path_actual(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(expr_type) = analysis_data.get_rc_expr_type(boxed.pos()).cloned() {
            if expr_type.has_awaitable_types() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::UnusedAwaitable,
                        "This awaitable is never awaited".to_string(),
                        statements_analyzer.get_hpos(&stmt.0),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }
    }
}

fn analyze_awaitall(
    boxed: (
        &Vec<(Option<oxidized::tast::Lid>, aast::Expr<(), ()>)>,
        &Vec<aast::Stmt<(), ()>>,
    ),
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    stmt: &aast::Stmt<(), ()>,
    loop_scope: &mut Option<LoopScope>,
) {
    context.inside_awaitall = true;

    for (assignment_id, expr) in boxed.0 {
        expression_analyzer::analyze(statements_analyzer, expr, analysis_data, context, &mut None);

        if let Some(assignment_id) = assignment_id {
            let mut assignment_type = None;

            if let Some(t) = analysis_data.get_expr_type(expr.pos()) {
                let parent_nodes = t.parent_nodes.clone();
                if t.is_single() {
                    let inner = t.get_single();
                    if let TAtomic::TNamedObject {
                        name: STR_AWAITABLE,
                        type_params: Some(type_params),
                        ..
                    } = inner
                    {
                        let mut new = type_params.get(0).unwrap().clone();

                        new.parent_nodes = parent_nodes;
                        assignment_type = Some(new)
                    }
                }
            }

            assignment_analyzer::analyze(
                statements_analyzer,
                (
                    &ast_defs::Bop::Eq(None),
                    &aast::Expr(
                        (),
                        assignment_id.0.clone(),
                        aast::Expr_::Lvar(Box::new(assignment_id.clone())),
                    ),
                    None,
                ),
                &stmt.0,
                assignment_type.as_ref(),
                analysis_data,
                context,
                false,
            )
            .ok();
        }
    }

    for stmt in boxed.1 {
        analyze(
            statements_analyzer,
            &stmt,
            analysis_data,
            context,
            loop_scope,
        );
    }

    context.inside_awaitall = false;
}
