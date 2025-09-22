use hakana_code_info::code_location::{HPos, StmtStart};
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::FnEffect;
use hakana_code_info::ttype::get_arrayish_params;
use hakana_code_info::EFFECT_PURE;
use hakana_str::StrId;
use rustc_hash::FxHashSet;

use crate::custom_hook::AfterStmtAnalysisData;
use crate::expr::binop::assignment_analyzer;

use crate::expr::expression_identifier::{
    get_functionlike_id_from_call, get_static_functionlike_id_from_call,
};
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::control_action::ControlAction;
use crate::scope::loop_scope::LoopScope;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::{
    break_analyzer, continue_analyzer, do_analyzer, for_analyzer, foreach_analyzer,
    ifelse_analyzer, return_analyzer, switch_analyzer, try_analyzer, while_analyzer,
};
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TAtomic;
use oxidized::aast;

pub enum AnalysisError {
    UserError,
    InternalError(String, HPos),
}

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: &aast::Stmt<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    loop_scope: &mut Option<LoopScope>,
) -> Result<(), AnalysisError> {
    if let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset {
        if current_stmt_offset.line != stmt.0.line() as u32 {
            analysis_data.current_stmt_offset = Some(StmtStart {
                offset: stmt.0.start_offset() as u32,
                line: stmt.0.line() as u32,
                column: stmt.0.to_raw_span().start.column() as u16,
                add_newline: true,
            });
        }
    } else {
        analysis_data.current_stmt_offset = Some(StmtStart {
            offset: stmt.0.start_offset() as u32,
            line: stmt.0.line() as u32,
            column: stmt.0.to_raw_span().start.column() as u16,
            add_newline: true,
        });
    }
    analysis_data.current_stmt_end = Some(stmt.0.end_offset() as u32);

    if statements_analyzer.get_config().remove_fixmes {
        for (fixme_line, b) in analysis_data.hakana_fixme_or_ignores.iter_mut() {
            if *fixme_line == stmt.0.line() as u32 {
                for (_, (_, _, line_start, line_end, is_same_line)) in b {
                    *line_start = stmt.0.start_offset() as u32;
                    *line_end = stmt.0.end_offset() as u32;
                    *is_same_line = true;
                }
            }
        }
    }

    match &stmt.1 {
        aast::Stmt_::Expr(boxed) => {
            expression_analyzer::analyze(statements_analyzer, boxed, analysis_data, context, false)?;

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
            return_analyzer::analyze(stmt, statements_analyzer, analysis_data, context)?;
        }
        aast::Stmt_::If(boxed) => {
            ifelse_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &stmt.0,
                analysis_data,
                context,
                loop_scope,
            )?;
        }
        aast::Stmt_::While(boxed) => {
            while_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1),
                &stmt.0,
                analysis_data,
                context,
            )?;
        }
        aast::Stmt_::Do(boxed) => {
            do_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1),
                &stmt.0,
                analysis_data,
                context,
            )?;
        }
        aast::Stmt_::For(boxed) => {
            for_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3),
                &stmt.0,
                analysis_data,
                context,
            )?;
        }
        aast::Stmt_::Foreach(boxed) => {
            foreach_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &stmt.0,
                analysis_data,
                context,
            )?;
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
            )?;
        }
        aast::Stmt_::Throw(boxed) => {
            context.inside_throw = true;

            expression_analyzer::analyze(statements_analyzer, boxed, analysis_data, context, false)?;

            context.control_actions.insert(ControlAction::End);

            context.inside_throw = false;
            context.has_returned = true;
        }
        aast::Stmt_::Try(boxed) => {
            try_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                analysis_data,
                context,
                loop_scope,
            )?;
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
            )?;
        }
        aast::Stmt_::Using(boxed) => {
            for boxed_expr in &boxed.exprs.1 {
                expression_analyzer::analyze(
                    statements_analyzer,
                    boxed_expr,
                    analysis_data,
                    context,
                    false,
                )?;
            }

            for using_stmt in &boxed.block {
                analyze(
                    statements_analyzer,
                    using_stmt,
                    analysis_data,
                    context,
                    loop_scope,
                )?;
            }
        }
        aast::Stmt_::Block(boxed) => {
            for boxed_stmt in &boxed.1 {
                analyze(
                    statements_analyzer,
                    boxed_stmt,
                    analysis_data,
                    context,
                    loop_scope,
                )?;
            }
        }
        aast::Stmt_::Fallthrough => {} // do nothing
        aast::Stmt_::YieldBreak | aast::Stmt_::Match(_) => {
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
            return Err(AnalysisError::UserError);
        }
        aast::Stmt_::DeclareLocal(_) => {}
        aast::Stmt_::Concurrent(boxed) => {
            let concurrent_block_start = stmt.0.start_offset() as u32;
            let concurrent_block_end = stmt.0.end_offset() as u32;
            analysis_data
                .concurrent_block_boundaries
                .push((concurrent_block_start, concurrent_block_end));

            for boxed_stmt in &boxed.0 {
                analyze(
                    statements_analyzer,
                    boxed_stmt,
                    analysis_data,
                    context,
                    loop_scope,
                )?;
            }
        }
    }

    context.cond_referenced_var_ids = FxHashSet::default();

    for hook in &statements_analyzer.get_config().hooks {
        hook.after_stmt_analysis(
            analysis_data,
            AfterStmtAnalysisData {
                statements_analyzer,
                stmt,
                context,
            },
        );
    }

    analysis_data.applicable_fixme_start = stmt.0.end_offset() as u32;

    Ok(())
}

fn detect_unused_statement_expressions(
    boxed: &aast::Expr<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    stmt: &aast::Stmt<(), ()>,
    context: &mut BlockContext,
) {
    if let Some(issue_kind) = has_unused_must_use(boxed, statements_analyzer, analysis_data) {
        analysis_data.maybe_add_issue(
            Issue::new(
                issue_kind,
                "This is annotated with MustUse but the returned value is not used".to_string(),
                statements_analyzer.get_hpos(&stmt.0),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    if let Some(effect) = analysis_data.expr_effects.get(&(
        boxed.pos().start_offset() as u32,
        boxed.pos().end_offset() as u32,
    )) {
        if effect == &EFFECT_PURE && !matches!(boxed.2, aast::Expr_::New(..)) {
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

    match &boxed.2 {
        aast::Expr_::Call(boxed_call) => {
            let functionlike_id = get_static_functionlike_id_from_call(
                boxed_call,
                statements_analyzer.interner,
                statements_analyzer.file_analyzer.resolved_names,
            );

            if let Some(FunctionLikeIdentifier::Function(function_id)) = functionlike_id {
                let codebase = statements_analyzer.codebase;
                if let Some(functionlike_info) = codebase
                    .functionlike_infos
                    .get(&(function_id, StrId::EMPTY))
                {
                    if let Some(expr_type) = analysis_data.get_rc_expr_type(boxed.pos()).cloned() {
                        let function_name = statements_analyzer.interner.lookup(&function_id);

                        if !functionlike_info.user_defined
                            && matches!(functionlike_info.effects, FnEffect::Arg(..))
                            && expr_type.is_single()
                        {
                            let array_types = get_arrayish_params(expr_type.get_single(), codebase);

                            if let Some((_, value_type)) = array_types {
                                if !value_type.is_null() && !value_type.is_void() {
                                    analysis_data.maybe_add_issue(
                                        Issue::new(
                                            IssueKind::UnusedBuiltinReturnValue,
                                            format!(
                                                "The value {} returned from {} should be consumed",
                                                expr_type
                                                    .get_id(Some(statements_analyzer.interner)),
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
        aast::Expr_::Collection(_)
        | aast::Expr_::ValCollection(_)
        | aast::Expr_::KeyValCollection(_)
        | aast::Expr_::ArrayGet(_)
        | aast::Expr_::Shape(_)
        | aast::Expr_::Tuple(_) => {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::UnusedStatement,
                    "This statement includes an expression that has no effect".to_string(),
                    statements_analyzer.get_hpos(&stmt.0),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
        _ => (),
    }
}

fn has_unused_must_use(
    boxed: &aast::Expr<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<IssueKind> {
    match &boxed.2 {
        aast::Expr_::Call(boxed_call) => {
            let functionlike_id_from_call = get_functionlike_id_from_call(
                boxed_call,
                statements_analyzer.interner,
                statements_analyzer.file_analyzer.resolved_names,
                &analysis_data.expr_types,
            );
            if let Some(functionlike_id) = functionlike_id_from_call {
                let codebase = statements_analyzer.codebase;
                match functionlike_id {
                    FunctionLikeIdentifier::Function(function_id) => {
                        // For statements like "Asio\join(some_fn());"
                        // Asio\join does not count as "using" the value
                        if function_id == StrId::ASIO_JOIN {
                            for arg in boxed_call.args.iter() {
                                let has_unused = has_unused_must_use(
                                    &arg.to_expr_ref(),
                                    statements_analyzer,
                                    analysis_data,
                                );
                                if has_unused.is_some() {
                                    return has_unused;
                                }
                            }
                        }

                        if let Some(functionlike_info) = codebase
                            .functionlike_infos
                            .get(&(function_id, StrId::EMPTY))
                        {
                            return if functionlike_info.must_use {
                                Some(IssueKind::UnusedFunctionCall)
                            } else {
                                None
                            };
                        }
                    }
                    FunctionLikeIdentifier::Method(method_class, method_name) => {
                        if let Some(functionlike_info) = codebase
                            .functionlike_infos
                            .get(&(method_class, method_name))
                        {
                            return if functionlike_info.must_use {
                                Some(IssueKind::UnusedMethodCall)
                            } else {
                                None
                            };
                        }
                    }
                    FunctionLikeIdentifier::Closure(_, _) => (),
                }
            }
        }
        aast::Expr_::Await(await_expr) => {
            return has_unused_must_use(await_expr, statements_analyzer, analysis_data)
        }
        _ => (),
    }

    None
}

fn analyze_awaitall(
    boxed: (
        &Vec<(oxidized::tast::Lid, aast::Expr<(), ()>)>,
        &Vec<aast::Stmt<(), ()>>,
    ),
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    stmt: &aast::Stmt<(), ()>,
    loop_scope: &mut Option<LoopScope>,
) -> Result<(), AnalysisError> {
    context.inside_awaitall = true;

    // Track concurrent block boundaries for unused variable analysis
    if !boxed.0.is_empty() {
        let first_assignment = &boxed.0[0];
        let last_assignment = boxed.0.last().unwrap();
        let concurrent_block_start = first_assignment.0 .0.start_offset() as u32;
        let concurrent_block_end = last_assignment.1 .1.end_offset() as u32;
        analysis_data
            .concurrent_block_boundaries
            .push((concurrent_block_start, concurrent_block_end));
    }

    for (assignment_id, expr) in boxed.0 {
        expression_analyzer::analyze(statements_analyzer, expr, analysis_data, context, false)?;

        let mut assignment_type = None;

        if let Some(t) = analysis_data.get_expr_type(expr.pos()) {
            let parent_nodes = t.parent_nodes.clone();
            if t.is_single() {
                let inner = t.get_single();
                if let TAtomic::TAwaitable { value, .. } = inner {
                    let mut new = (**value).clone();

                    new.parent_nodes = parent_nodes;
                    assignment_type = Some(new)
                }
            }
        }

        assignment_analyzer::analyze(
            statements_analyzer,
            (
                &aast::Expr(
                    (),
                    assignment_id.0.clone(),
                    aast::Expr_::Lvar(Box::new(assignment_id.clone())),
                ),
                None,
                None,
            ),
            &stmt.0,
            assignment_type.as_ref(),
            analysis_data,
            context,
            None,
        )?;
    }

    for stmt in boxed.1 {
        analyze(
            statements_analyzer,
            stmt,
            analysis_data,
            context,
            loop_scope,
        )?;
    }

    context.inside_awaitall = false;

    Ok(())
}
