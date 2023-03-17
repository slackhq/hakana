use std::collections::BTreeMap;

use hakana_algebra::Clause;
use oxidized::aast;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    formula_generator,
    reconciler::reconciler,
    scope_context::{loop_scope::LoopScope, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
    typed_ast::FunctionAnalysisData,
};

use super::{
    control_analyzer::BreakContext, ifelse_analyzer::remove_clauses_with_mixed_vars, loop_analyzer,
    while_analyzer::get_and_expressions,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (&aast::Block<(), ()>, &aast::Expr<(), ()>),
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> bool {
    let mut do_context = context.clone();
    do_context.break_types.push(BreakContext::Loop);
    do_context.inside_loop = true;

    let mut loop_scope = LoopScope::new(context.vars_in_scope.clone());

    let mut mixed_var_ids = vec![];

    for (var_id, var_type) in &loop_scope.parent_context_vars {
        if var_type.is_mixed() {
            mixed_var_ids.push(var_id);
        }
    }

    let cond_id = (stmt.1 .1.start_offset(), stmt.1 .1.end_offset());

    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
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
    .unwrap_or(vec![]);

    while_clauses = remove_clauses_with_mixed_vars(while_clauses, mixed_var_ids, cond_id);

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

    let (analysis_result, mut inner_loop_context) = loop_analyzer::analyze(
        statements_analyzer,
        &stmt.0 .0,
        get_and_expressions(stmt.1),
        vec![],
        &mut loop_scope,
        &mut do_context,
        context,
        analysis_data,
        true,
        true,
    );

    let clauses_to_simplify = {
        let mut c = context
            .clauses
            .iter()
            .map(|v| (**v).clone())
            .collect::<Vec<_>>();
        c.extend(hakana_algebra::negate_formula(while_clauses).unwrap_or(vec![]));
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

    for (var_id, var_type) in inner_loop_context.vars_in_scope {
        context
            .vars_in_scope
            .insert(var_id.clone(), var_type.clone());
    }

    return analysis_result;
}
