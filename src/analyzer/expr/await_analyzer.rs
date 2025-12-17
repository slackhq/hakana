use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::EFFECT_IMPURE;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::{extend_dataflow_uniquely, get_mixed_any};
use oxidized::aast;
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    boxed: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    if !context.inside_async {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::AwaitInSyncContext,
                "Cannot use await in a non-async function".to_string(),
                statements_analyzer.get_hpos(expr.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let was_inside_use = context.inside_general_use;
    context.inside_general_use = true;
    let was_inside_await = context.inside_await;
    context.inside_await = true;
    expression_analyzer::analyze(statements_analyzer, boxed, analysis_data, context, true)?;
    context.inside_general_use = was_inside_use;
    context.inside_await = was_inside_await;

    // Increment await calls
    analysis_data.await_calls_count += 1;

    let mut awaited_stmt_type = analysis_data
        .get_expr_type(boxed.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    let awaited_types = awaited_stmt_type.types.drain(..).collect::<Vec<_>>();

    let mut new_types = vec![];

    for atomic_type in awaited_types {
        if let TAtomic::TAwaitable { value } = atomic_type {
            let inside_type = (*value).clone();
            extend_dataflow_uniquely(
                &mut awaited_stmt_type.parent_nodes,
                inside_type.parent_nodes,
            );
            new_types.extend(inside_type.types);
            analysis_data.expr_effects.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                EFFECT_IMPURE,
            );
        } else {
            new_types.push(atomic_type);
            analysis_data.expr_effects.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                EFFECT_IMPURE,
            );
        }
    }

    awaited_stmt_type.types = new_types;

    analysis_data.expr_types.insert(
        (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
        Rc::new(awaited_stmt_type),
    );

    analysis_data.has_await = true;

    Ok(())
}
