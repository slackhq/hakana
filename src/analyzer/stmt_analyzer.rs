use rustc_hash::FxHashMap;

use crate::custom_hook::AfterStmtAnalysisData;
use crate::expr::binop::assignment_analyzer;
use crate::expression_analyzer::{self};

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
    if !tast_info.expr_types.len() > 10 {
        tast_info.expr_types = FxHashMap::default();
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
                boxed,
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
                ),
                statements_analyzer.get_config(),
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
    boxed: &Box<(
        Vec<(Option<oxidized::tast::Lid>, aast::Expr<(), ()>)>,
        Vec<aast::Stmt<(), ()>>,
    )>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    stmt: &aast::Stmt<(), ()>,
    loop_scope: &mut Option<LoopScope>,
) {
    for (assignment_id, expr) in &boxed.0 {
        expression_analyzer::analyze(statements_analyzer, expr, tast_info, context, &mut None);

        if let Some(assignment_id) = assignment_id {
            let mut assignment_type = None;

            if let Some(t) = tast_info.get_expr_type(expr.pos()) {
                let parent_nodes = t.parent_nodes.clone();
                if t.is_single() {
                    let inner = t.get_single();
                    if let TAtomic::TNamedObject {
                        name,
                        type_params: Some(type_params),
                        ..
                    } = inner
                    {
                        if name == "HH\\Awaitable" {
                            let mut new = type_params.get(0).unwrap().clone();

                            new.parent_nodes = parent_nodes;
                            assignment_type = Some(new)
                        }
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
                    Some(expr),
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

    for stmt in &boxed.1 {
        analyze(statements_analyzer, &stmt, tast_info, context, loop_scope);
    }
}
