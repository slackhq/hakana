use std::collections::BTreeMap;

use crate::{
    expression_analyzer,
    scope_analyzer::ScopeAnalyzer,
    scope_context::{if_scope::IfScope, ScopeContext},
    stmt_analyzer::AnalysisError,
};
use hakana_reflection_info::{
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    functionlike_identifier::FunctionLikeIdentifier,
    issue::{Issue, IssueKind},
    t_union::TUnion,
};
use oxidized::{aast, ast, ast_defs::Pos};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    function_analysis_data::FunctionAnalysisData, reconciler,
    statements_analyzer::StatementsAnalyzer,
};

use super::if_conditional_scope::IfConditionalScope;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    cond: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    outer_context: &ScopeContext,
    if_scope: &mut IfScope,
) -> Result<IfConditionalScope, AnalysisError> {
    let mut outer_context = outer_context.clone();
    let mut old_outer_context = outer_context.clone();
    let mut has_outer_context_changes = false;

    if !if_scope.negated_clauses.is_empty() {
        let mut changed_var_ids = FxHashSet::default();

        if !if_scope.negated_types.is_empty() {
            let mut tmp_context = outer_context.clone();

            reconciler::reconcile_keyed_types(
                &if_scope.negated_types,
                BTreeMap::new(),
                &mut tmp_context,
                &mut changed_var_ids,
                &FxHashSet::default(),
                statements_analyzer,
                analysis_data,
                cond.pos(),
                true,
                false,
                &FxHashMap::default(),
            );

            if !changed_var_ids.is_empty() {
                outer_context = tmp_context;
                has_outer_context_changes = true;
            }
        }
    }

    // get the first expression in the if, which should be evaluated on its own
    // this allows us to update the context of $matches in
    // if (!preg_match('/a/', 'aa', $matches)) {
    //   exit
    // }
    // echo $matches[0];
    let externally_applied_if_cond_expr = get_definitely_evaluated_expression_after_if(cond);
    let internally_applied_if_cond_expr = get_definitely_evaluated_expression_inside_if(cond);

    let mut if_context = None;

    if externally_applied_if_cond_expr != internally_applied_if_cond_expr {
        if_context = Some(
            if has_outer_context_changes {
                &outer_context
            } else {
                &old_outer_context
            }
            .clone(),
        );
    }

    let pre_condition_vars_in_scope = if has_outer_context_changes {
        &outer_context
    } else {
        &old_outer_context
    }
    .vars_in_scope
    .clone();

    let pre_referenced_var_ids = if has_outer_context_changes {
        &outer_context
    } else {
        &old_outer_context
    }
    .cond_referenced_var_ids
    .clone();

    if has_outer_context_changes {
        &mut outer_context
    } else {
        &mut old_outer_context
    }
    .cond_referenced_var_ids = FxHashSet::default();

    let pre_assigned_var_ids = if has_outer_context_changes {
        &outer_context
    } else {
        &old_outer_context
    }
    .assigned_var_ids
    .clone();

    if has_outer_context_changes {
        &mut outer_context
    } else {
        &mut old_outer_context
    }
    .assigned_var_ids = FxHashMap::default();

    let was_inside_conditional = outer_context.inside_conditional;

    outer_context.inside_conditional = true;

    expression_analyzer::analyze(
        statements_analyzer,
        externally_applied_if_cond_expr,
        analysis_data,
        if has_outer_context_changes {
            &mut outer_context
        } else {
            &mut old_outer_context
        },
        &mut None,
    )?;

    let first_cond_assigned_var_ids = if has_outer_context_changes {
        &outer_context
    } else {
        &old_outer_context
    }
    .assigned_var_ids
    .clone();

    if has_outer_context_changes {
        &mut outer_context
    } else {
        &mut old_outer_context
    }
    .assigned_var_ids
    .extend(pre_assigned_var_ids);

    let first_cond_referenced_var_ids = if has_outer_context_changes {
        &outer_context
    } else {
        &old_outer_context
    }
    .cond_referenced_var_ids
    .clone();

    if has_outer_context_changes {
        &mut outer_context
    } else {
        &mut old_outer_context
    }
    .cond_referenced_var_ids
    .extend(pre_referenced_var_ids);

    if has_outer_context_changes {
        &mut outer_context
    } else {
        &mut old_outer_context
    }
    .inside_conditional = was_inside_conditional;

    let mut if_context = if let Some(if_context) = if_context {
        Some(if_context)
    } else {
        Some(
            if has_outer_context_changes {
                &outer_context
            } else {
                &old_outer_context
            }
            .clone(),
        )
    };

    let mut if_conditional_context = if_context.clone().unwrap();

    // we need to clone the current context so our ongoing updates
    // to $outer_context don't mess with elseif/else blocks
    let post_if_context = if has_outer_context_changes {
        &outer_context
    } else {
        &old_outer_context
    }
    .clone();

    let mut cond_referenced_var_ids;
    let assigned_in_conditional_var_ids;

    if internally_applied_if_cond_expr != cond || externally_applied_if_cond_expr != cond {
        if_conditional_context.assigned_var_ids = FxHashMap::default();
        if_conditional_context.cond_referenced_var_ids = FxHashSet::default();

        let was_inside_conditional = if_conditional_context.inside_conditional;

        if_conditional_context.inside_conditional = true;

        expression_analyzer::analyze(
            statements_analyzer,
            cond,
            analysis_data,
            &mut if_conditional_context,
            &mut if_context,
        )?;

        add_branch_dataflow(statements_analyzer, cond, analysis_data);

        if_conditional_context.inside_conditional = was_inside_conditional;

        if_conditional_context
            .cond_referenced_var_ids
            .extend(first_cond_referenced_var_ids);
        cond_referenced_var_ids = if_conditional_context.cond_referenced_var_ids.clone();

        if_conditional_context
            .assigned_var_ids
            .extend(first_cond_assigned_var_ids);
        assigned_in_conditional_var_ids = if_conditional_context.assigned_var_ids.clone();
    } else {
        cond_referenced_var_ids = first_cond_referenced_var_ids.clone();
        assigned_in_conditional_var_ids = first_cond_assigned_var_ids.clone();
    }

    let newish_var_ids = if_conditional_context
        .vars_in_scope.into_keys()
        .filter(|k| {
            !pre_condition_vars_in_scope.contains_key(k)
                && !cond_referenced_var_ids.contains(k)
                && !assigned_in_conditional_var_ids.contains_key(k)
        })
        .collect::<FxHashSet<_>>();

    if let Some(cond_type) = analysis_data.get_rc_expr_type(cond.pos()).cloned() {
        handle_paradoxical_condition(
            statements_analyzer,
            analysis_data,
            cond.pos(),
            &outer_context.function_context.calling_functionlike_id,
            &cond_type,
        );
    }

    cond_referenced_var_ids.retain(|k| !assigned_in_conditional_var_ids.contains_key(k));

    cond_referenced_var_ids.extend(newish_var_ids);

    Ok(IfConditionalScope {
        if_body_context: if_context.unwrap(),
        post_if_context,
        outer_context: if has_outer_context_changes {
            outer_context
        } else {
            old_outer_context
        },
        cond_referenced_var_ids,
    })
}

fn get_definitely_evaluated_expression_after_if(stmt: &aast::Expr<(), ()>) -> &aast::Expr<(), ()> {
    match &stmt.2 {
        aast::Expr_::Binop(boxed) => {
            // todo handle <expr> === true

            if let ast::Bop::Ampamp = boxed.bop {
                return get_definitely_evaluated_expression_after_if(&boxed.lhs);
            }

            return stmt;
        }
        aast::Expr_::Unop(boxed) => {
            if let ast::Uop::Unot = boxed.0 {
                let inner_expr = get_definitely_evaluated_expression_inside_if(&boxed.1);

                if inner_expr != &boxed.1 {
                    return inner_expr;
                }
            }
        }
        _ => {}
    }

    stmt
}

fn get_definitely_evaluated_expression_inside_if(stmt: &aast::Expr<(), ()>) -> &aast::Expr<(), ()> {
    match &stmt.2 {
        aast::Expr_::Binop(boxed) => {
            // todo handle <expr> === true

            if let ast::Bop::Barbar = boxed.bop {
                return get_definitely_evaluated_expression_inside_if(&boxed.lhs);
            }

            return stmt;
        }
        aast::Expr_::Unop(boxed) => {
            if let ast::Uop::Unot = boxed.0 {
                let inner_expr = get_definitely_evaluated_expression_after_if(&boxed.1);

                if inner_expr != &boxed.1 {
                    return inner_expr;
                }
            }
        }
        _ => {}
    }

    stmt
}

pub(crate) fn add_branch_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    cond: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        // todo maybe useful in the future
        return;
    }

    let conditional_type = analysis_data
        .expr_types
        .get(&(cond.1.start_offset() as u32, cond.1.end_offset() as u32));

    if let Some(conditional_type) = conditional_type {
        if !conditional_type.parent_nodes.is_empty() {
            let branch_node = DataFlowNode::get_for_variable_sink(
                "branch".to_string(),
                statements_analyzer.get_hpos(cond.pos()),
            );

            for parent_node in &conditional_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    parent_node,
                    &branch_node,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }

            if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
                analysis_data.data_flow_graph.add_node(branch_node);
            }
        }
    }
}

pub(crate) fn handle_paradoxical_condition(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    expr_type: &TUnion,
) {
    if expr_type.is_always_falsy() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::ImpossibleTruthinessCheck,
                format!(
                    "Type {} is never truthy",
                    expr_type.get_id(Some(statements_analyzer.get_interner()))
                ),
                statements_analyzer.get_hpos(pos),
                calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    } else if expr_type.is_always_truthy() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::RedundantTruthinessCheck,
                format!(
                    "Type {} is always truthy",
                    expr_type.get_id(Some(statements_analyzer.get_interner()))
                ),
                statements_analyzer.get_hpos(pos),
                calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }
}
