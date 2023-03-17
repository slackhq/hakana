use hakana_reflection_info::{codebase_info::CodebaseInfo, Interner, StrId};
use hakana_type::{combine_union_types, get_mixed_any};
use indexmap::IndexMap;
use oxidized::{aast, aast::Pos};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc};

use crate::{
    expr::expression_identifier,
    expression_analyzer,
    scope_analyzer::ScopeAnalyzer,
    scope_context::{
        control_action::ControlAction, loop_scope::LoopScope, switch_scope::SwitchScope,
        ScopeContext,
    },
    statements_analyzer::StatementsAnalyzer,
    typed_ast::FunctionAnalysisData,
};

use super::{
    control_analyzer::{self, BreakContext},
    if_conditional_analyzer::add_branch_dataflow,
    switch_case_analyzer::analyze_case,
};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    stmt: (
        &aast::Expr<(), ()>,
        &Vec<aast::Case<(), ()>>,
        &Option<aast::DefaultCase<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    loop_scope: &mut Option<LoopScope>,
) {
    let codebase = statements_analyzer.get_codebase();

    context.inside_conditional = true;

    if !expression_analyzer::analyze(
        statements_analyzer,
        stmt.0,
        analysis_data,
        context,
        &mut None,
    ) {
        context.inside_conditional = false;
        return;
    }

    context.inside_conditional = false;

    add_branch_dataflow(statements_analyzer, &stmt.0, analysis_data);

    let switch_var_id = if let Some(switch_var_id) = expression_identifier::get_var_id(
        stmt.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some((
            statements_analyzer.get_codebase(),
            statements_analyzer.get_interner(),
        )),
    ) {
        switch_var_id
    } else {
        let switch_var_id = format!("$-tmp_switch-{}", stmt.0 .1.start_offset());

        context.vars_in_scope.insert(
            switch_var_id.clone(),
            Rc::new(
                analysis_data
                    .get_expr_type(&stmt.0 .1)
                    .cloned()
                    .unwrap_or(get_mixed_any()),
            ),
        );
        switch_var_id
    };

    let original_context = context.clone();

    let mut last_case_exit_type = ControlAction::Break;

    let mut case_exit_types = FxHashMap::default();

    let has_default = stmt.2.is_some();

    let mut case_action_map = FxHashMap::default();

    let mut cases = stmt
        .1
        .iter()
        .enumerate()
        .map(|(k, c)| (k, c))
        .collect::<IndexMap<_, _>>();
    cases.reverse();

    if let Some(default_case) = stmt.2 {
        update_case_exit_map(
            codebase,
            statements_analyzer.get_interner(),
            &default_case.1 .0,
            analysis_data,
            statements_analyzer.get_file_analyzer().resolved_names,
            &mut case_action_map,
            cases.len(),
            &mut last_case_exit_type,
            &mut case_exit_types,
        );
    }

    for (i, case) in &cases {
        update_case_exit_map(
            codebase,
            statements_analyzer.get_interner(),
            &case.1 .0,
            analysis_data,
            statements_analyzer.get_file_analyzer().resolved_names,
            &mut case_action_map,
            *i,
            &mut last_case_exit_type,
            &mut case_exit_types,
        );
    }

    let mut switch_scope = SwitchScope::new();

    let mut all_options_returned = true;

    cases.reverse();

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

    let mut previous_empty_cases = vec![];

    for (i, case) in &cases {
        let case_exit_type = case_exit_types.get(&i).unwrap();

        if case_exit_type != &ControlAction::Return {
            all_options_returned = false;
        }

        let case_actions = case_action_map.get(&i).unwrap();

        if case.1.is_empty() {
            previous_empty_cases.push(*case);
            continue;
        }

        if !analyze_case(
            statements_analyzer,
            stmt,
            fake_switch_condition.as_ref().unwrap_or(stmt.0),
            condition_is_fake,
            &switch_var_id,
            Some(&case.0),
            &case.0.pos(),
            &case.1 .0,
            &previous_empty_cases,
            analysis_data,
            context,
            &original_context,
            case_exit_type,
            case_actions,
            *i == (cases.len() - 1) && stmt.2.is_none(),
            &mut switch_scope,
            loop_scope,
        ) {
            return;
        }

        previous_empty_cases = vec![];
    }

    if let Some(default_case) = stmt.2 {
        let i = cases.len();

        let case_exit_type = case_exit_types.get(&i).unwrap();

        if case_exit_type != &ControlAction::Return {
            all_options_returned = false;
        }

        let case_actions = case_action_map.get(&i).unwrap();

        if !analyze_case(
            statements_analyzer,
            stmt,
            fake_switch_condition.as_ref().unwrap_or(stmt.0),
            condition_is_fake,
            &switch_var_id,
            None,
            &default_case.0,
            &default_case.1 .0,
            &previous_empty_cases,
            analysis_data,
            context,
            &original_context,
            case_exit_type,
            case_actions,
            true,
            &mut switch_scope,
            loop_scope,
        ) {
            return;
        }
    }

    let mut possibly_redefined_vars = switch_scope
        .possibly_redefined_vars
        .unwrap_or(BTreeMap::new());
    if let Some(new_vars_in_scope) = switch_scope.new_vars_in_scope {
        possibly_redefined_vars.retain(|k, _| !new_vars_in_scope.contains_key(k));
        context.vars_in_scope.extend(new_vars_in_scope);
    }

    if let Some(redefined_vars) = &switch_scope.redefined_vars {
        possibly_redefined_vars.retain(|k, _| !redefined_vars.contains_key(k));
        context.vars_in_scope.extend(redefined_vars.clone());
    }

    for (var_id, var_type) in possibly_redefined_vars {
        if let Some(context_type) = context.vars_in_scope.get(&var_id).cloned() {
            context.vars_in_scope.insert(
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
}

fn update_case_exit_map(
    codebase: &CodebaseInfo,
    interner: &Interner,
    case_stmts: &Vec<aast::Stmt<(), ()>>,
    analysis_data: &mut FunctionAnalysisData,
    resolved_names: &FxHashMap<usize, StrId>,
    case_action_map: &mut FxHashMap<usize, FxHashSet<ControlAction>>,
    i: usize,
    last_case_exit_type: &mut ControlAction,
    case_exit_types: &mut FxHashMap<usize, ControlAction>,
) {
    let case_actions = control_analyzer::get_control_actions(
        codebase,
        interner,
        resolved_names,
        &case_stmts,
        Some(analysis_data),
        vec![BreakContext::Switch],
        true,
    );

    case_action_map.insert(i, case_actions.clone());
    *last_case_exit_type = get_last_action(case_actions).unwrap_or(last_case_exit_type.clone());
    case_exit_types.insert(i, last_case_exit_type.clone());
}

fn get_last_action(case_actions: FxHashSet<ControlAction>) -> Option<ControlAction> {
    if !case_actions.contains(&ControlAction::None) {
        if case_actions.len() == 1 && case_actions.contains(&ControlAction::End) {
            return Some(ControlAction::Return);
        } else if case_actions.len() == 1 && case_actions.contains(&ControlAction::Continue) {
            return Some(ControlAction::Continue);
        } else if case_actions.contains(&ControlAction::LeaveSwitch) {
            return Some(ControlAction::Break);
        }
    } else if case_actions.len() != 1 {
        return Some(ControlAction::Break);
    }

    return None;
}
