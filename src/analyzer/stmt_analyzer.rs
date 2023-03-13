use hakana_reflection_info::code_location::StmtStart;
use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::STR_AWAITABLE;

use crate::custom_hook::AfterStmtAnalysisData;
use crate::expr::assertion_finder::get_functionlike_id_from_call;
use crate::expr::binop::assignment_analyzer;

use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::loop_scope::LoopScope;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::{
    break_analyzer, continue_analyzer, do_analyzer, for_analyzer, foreach_analyzer,
    ifelse_analyzer, return_analyzer, switch_analyzer, try_analyzer, while_analyzer,
};
use crate::typed_ast::TastInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::t_atomic::TAtomic;
use oxidized::{aast, ast_defs};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: &aast::Stmt<(), ()>,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) -> bool {
    if let Some(ref mut current_stmt_offset) = tast_info.current_stmt_offset {
        if current_stmt_offset.line != stmt.0.line() {
            tast_info.current_stmt_offset = Some(StmtStart {
                offset: stmt.0.start_offset(),
                line: stmt.0.line(),
                column: stmt.0.to_raw_span().start.column() as usize,
                add_newline: true,
            });
        }
    } else {
        tast_info.current_stmt_offset = Some(StmtStart {
            offset: stmt.0.start_offset(),
            line: stmt.0.line(),
            column: stmt.0.to_raw_span().start.column() as usize,
            add_newline: true,
        });
    }

    match &stmt.1 {
        aast::Stmt_::Expr(boxed) => {
            if !expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                tast_info,
                context,
                &mut None,
            ) {
                return false;
            }

            if let aast::Expr_::Call(boxed_call) = &boxed.2 {
                let functionlike_id = get_functionlike_id_from_call(
                    boxed_call,
                    Some(statements_analyzer.get_interner()),
                    statements_analyzer.get_file_analyzer().resolved_names,
                );
                if let Some(functionlike_id) = functionlike_id {
                    if let FunctionLikeIdentifier::Function(function_id) = functionlike_id {
                        let codebase = statements_analyzer.get_codebase();
                        if let Some(functionlike_info) =
                            codebase.functionlike_infos.get(&function_id)
                        {
                            if functionlike_info.must_use {
                                tast_info.maybe_add_issue(
                                    Issue::new(
                                        IssueKind::UnusedFunctionCall,
                                        "This function is annotated with MustUse but the returned value is not used".to_string(),
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

                if let Some(expr_type) = tast_info.get_rc_expr_type(boxed.pos()).cloned() {
                    for atomic_type in &expr_type.types {
                        if let TAtomic::TNamedObject {
                            name: STR_AWAITABLE,
                            ..
                        } = atomic_type
                        {
                            tast_info.maybe_add_issue(
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
        }
        aast::Stmt_::Return(_) => {
            return_analyzer::analyze(stmt, statements_analyzer, tast_info, context);
        }
        aast::Stmt_::If(boxed) => {
            if !ifelse_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &stmt.0,
                tast_info,
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
                tast_info,
                context,
            ) {
                return false;
            }
        }
        aast::Stmt_::Do(boxed) => {
            if !do_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1),
                tast_info,
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
                tast_info,
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
                tast_info,
                context,
            ) {
                return false;
            }
        }
        aast::Stmt_::Noop => {
            // ignore
        }
        aast::Stmt_::Break => {
            break_analyzer::analyze(statements_analyzer, tast_info, context, loop_scope);
        }
        aast::Stmt_::Continue => {
            continue_analyzer::analyze(statements_analyzer, tast_info, context, loop_scope);
        }
        aast::Stmt_::Switch(boxed) => {
            switch_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &stmt.0,
                tast_info,
                context,
                loop_scope,
            );
        }
        aast::Stmt_::Throw(boxed) => {
            context.inside_throw = true;

            let analysis_result = expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                tast_info,
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
                tast_info,
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
                tast_info,
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
                    tast_info,
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
                    tast_info,
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
                    tast_info,
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
            tast_info.maybe_add_issue(
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
    }

    for hook in &statements_analyzer.get_config().hooks {
        hook.after_stmt_analysis(
            tast_info,
            AfterStmtAnalysisData {
                statements_analyzer,
                stmt: &stmt,
                context,
            },
        );
    }

    true
}

fn analyze_awaitall(
    boxed: (
        &Vec<(Option<oxidized::tast::Lid>, aast::Expr<(), ()>)>,
        &Vec<aast::Stmt<(), ()>>,
    ),
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    stmt: &aast::Stmt<(), ()>,
    loop_scope: &mut Option<LoopScope>,
) {
    context.inside_awaitall = true;

    for (assignment_id, expr) in boxed.0 {
        expression_analyzer::analyze(statements_analyzer, expr, tast_info, context, &mut None);

        if let Some(assignment_id) = assignment_id {
            let mut assignment_type = None;

            if let Some(t) = tast_info.get_expr_type(expr.pos()) {
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
                tast_info,
                context,
                false,
            )
            .ok();
        }
    }

    for stmt in boxed.1 {
        analyze(statements_analyzer, &stmt, tast_info, context, loop_scope);
    }

    context.inside_awaitall = false;
}
