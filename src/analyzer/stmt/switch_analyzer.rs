use hakana_code_info::{
    ttype::{combine_union_types, get_mixed_any},
    var_name::VarName,
};

use indexmap::IndexMap;
use oxidized::{aast, aast::Pos};
use std::rc::Rc;

use crate::{
    expr::expression_identifier,
    expression_analyzer,
    function_analysis_data::FunctionAnalysisData,
    scope::{
        control_action::ControlAction, loop_scope::LoopScope, switch_scope::SwitchScope,
        BlockContext,
    },
    statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};

use super::{if_conditional_analyzer::add_branch_dataflow, switch_case_analyzer::analyze_case};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &Vec<aast::Case<(), ()>>,
        &Option<aast::DefaultCase<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    loop_scope: &mut Option<LoopScope>,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    context.inside_conditional = true;

    expression_analyzer::analyze(statements_analyzer, stmt.0, analysis_data, context)?;

    context.inside_conditional = false;

    add_branch_dataflow(statements_analyzer, stmt.0, analysis_data);

    let switch_var_id = if let Some(switch_var_id) = expression_identifier::get_var_id(
        stmt.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.file_analyzer.resolved_names,
        Some((statements_analyzer.codebase, statements_analyzer.interner)),
    ) {
        switch_var_id
    } else {
        let switch_var_id = format!("$-tmp_switch-{}", stmt.0 .1.start_offset());

        context.locals.insert(
            VarName::new(switch_var_id.clone()),
            analysis_data
                .get_rc_expr_type(&stmt.0 .1)
                .cloned()
                .unwrap_or(Rc::new(get_mixed_any())),
        );
        switch_var_id
    };

    let mut condition_is_fake = false;

    let fake_switch_condition = if switch_var_id.starts_with("$-tmp_switch-") {
        condition_is_fake = true;

        Some(aast::Expr(
            (),
            stmt.0 .1.clone(),
            aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
                stmt.0 .1.clone(),
                (0, switch_var_id.clone()),
            ))),
        ))
    } else {
        None
    };

    let original_context = context.clone();

    let has_default = stmt.2.is_some();

    let mut cases = stmt.1.iter().enumerate().collect::<IndexMap<_, _>>();
    cases.reverse();

    let mut switch_scope = SwitchScope::new();

    let mut all_options_returned = true;

    cases.reverse();

    let mut previous_empty_cases = vec![];

    let switch_var_id = VarName::new(switch_var_id);

    for (i, case) in &cases {
        if !case
            .1
            .iter()
            .any(|s| !matches!(s.1, aast::Stmt_::Fallthrough))
        {
            previous_empty_cases.push(*case);
            continue;
        }

        analyze_case(
            statements_analyzer,
            stmt,
            fake_switch_condition.as_ref().unwrap_or(stmt.0),
            condition_is_fake,
            &switch_var_id,
            Some(&case.0),
            case.0.pos(),
            case.1 .0.clone(),
            &previous_empty_cases,
            analysis_data,
            context,
            &original_context,
            *i == (cases.len() - 1) && stmt.2.is_none(),
            &mut switch_scope,
            loop_scope,
        )?;

        previous_empty_cases = vec![];
    }

    if let Some(default_case) = stmt.2 {
        let case_exit_type = analyze_case(
            statements_analyzer,
            stmt,
            fake_switch_condition.as_ref().unwrap_or(stmt.0),
            condition_is_fake,
            &switch_var_id,
            None,
            &default_case.0,
            default_case.1 .0.clone(),
            &previous_empty_cases,
            analysis_data,
            context,
            &original_context,
            true,
            &mut switch_scope,
            loop_scope,
        )?;

        if case_exit_type != ControlAction::Return {
            all_options_returned = false;
        }
    }

    let mut possibly_redefined_vars = switch_scope.possibly_redefined_vars.unwrap_or_default();
    if let Some(new_locals) = switch_scope.new_locals {
        possibly_redefined_vars.retain(|k, _| !new_locals.contains_key(k));
        context.locals.extend(new_locals);
    }

    if let Some(redefined_vars) = &switch_scope.redefined_vars {
        possibly_redefined_vars.retain(|k, _| !redefined_vars.contains_key(k));
        context.locals.extend(redefined_vars.clone());
    }

    for (var_id, var_type) in possibly_redefined_vars {
        if let Some(context_type) = context.locals.get(&var_id).cloned() {
            context.locals.insert(
                var_id.clone(),
                Rc::new(combine_union_types(
                    &var_type,
                    &context_type,
                    codebase,
                    false,
                )),
            );
        }
    }

    analysis_data
        .fully_matched_switch_offsets
        .insert(pos.start_offset());

    context
        .assigned_var_ids
        .extend(switch_scope.new_assigned_var_ids);
    context.has_returned = all_options_returned && has_default;

    Ok(())
}
