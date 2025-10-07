use std::collections::BTreeMap;

use hakana_algebra::Clause;
use oxidized::{aast, pos::Pos};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    formula_generator,
    function_analysis_data::FunctionAnalysisData,
    reconciler,
    scope::{BlockContext, loop_scope::LoopScope},
    statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};

use super::{control_analyzer::BreakContext, loop_analyzer, while_analyzer::get_and_expressions};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (&aast::Block<(), ()>, &aast::Expr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let mut do_context = context.clone();
    do_context.break_types.push(BreakContext::Loop);
    do_context.inside_loop = true;

    let mut loop_scope = LoopScope::new(context.locals.clone());

    let mut mixed_var_ids = vec![];

    for (var_id, var_type) in &loop_scope.parent_context_vars {
        if var_type.is_mixed() {
            mixed_var_ids.push(var_id);
        }
    }

    let cond_id = (stmt.1.1.start_offset() as u32, stmt.1.1.end_offset() as u32);

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class,
        context.function_context.calling_functionlike_id,
    );

    let mut while_clauses = formula_generator::get_formula(
        cond_id,
        cond_id,
        stmt.1,
        &assertion_context,
        analysis_data,
        true,
        false,
    )
    .unwrap_or_default();

    if while_clauses.is_empty() {
        while_clauses.push(Clause::new(
            BTreeMap::new(),
            cond_id,
            cond_id,
            Some(true),
            None,
            None,
        ));
    }

    let prev_loop_bounds = do_context.loop_bounds;

    do_context.loop_bounds = (pos.start_offset() as u32, pos.end_offset() as u32);
    // Store loop bounds for variable scoping analysis
    analysis_data.loop_boundaries.push(do_context.loop_bounds);

    let mut inner_loop_context = loop_analyzer::analyze(
        statements_analyzer,
        &stmt.0.0,
        get_and_expressions(stmt.1),
        vec![],
        &mut loop_scope,
        &mut do_context,
        context,
        analysis_data,
        true,
        true,
    )?;

    do_context.loop_bounds = prev_loop_bounds;

    let clauses_to_simplify = {
        let mut c = context
            .clauses
            .iter()
            .map(|v| (**v).clone())
            .collect::<Vec<_>>();
        c.extend(hakana_algebra::negate_formula(while_clauses).unwrap_or_default());
        c
    };

    let (negated_while_types, _) = hakana_algebra::get_truths_from_formula(
        hakana_algebra::simplify_cnf(clauses_to_simplify.iter().collect())
            .iter()
            .collect(),
        None,
        &mut FxHashSet::default(),
    );

    if !negated_while_types.is_empty() {
        reconciler::reconcile_keyed_types(
            &negated_while_types,
            BTreeMap::new(),
            &mut inner_loop_context,
            &mut FxHashSet::default(),
            &FxHashSet::default(),
            statements_analyzer,
            analysis_data,
            stmt.1.pos(),
            true,
            false,
            &FxHashMap::default(),
        );
    }

    for (var_id, var_type) in inner_loop_context.locals {
        context.locals.insert(var_id.clone(), var_type.clone());
    }

    Ok(())
}
