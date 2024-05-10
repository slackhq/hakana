use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_context::control_action::ControlAction;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_str::{Interner, StrId};
use oxidized::{aast, ast::CallExpr};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BreakContext {
    Switch,
    Loop,
}

pub(crate) fn get_control_actions(
    codebase: &CodebaseInfo,
    interner: &Interner,
    resolved_names: &FxHashMap<u32, StrId>,
    stmts: &Vec<aast::Stmt<(), ()>>,
    analysis_data: Option<&FunctionAnalysisData>,
    break_context: Vec<BreakContext>,
    return_is_exit: bool, // default true
) -> FxHashSet<ControlAction> {
    let mut control_actions = FxHashSet::default();

    if stmts.is_empty() {
        control_actions.insert(ControlAction::None);
        return control_actions;
    }

    'outer: for stmt in stmts {
        match &stmt.1 {
            aast::Stmt_::Expr(boxed) => {
                if let aast::Expr_::Call(call_expr) = &boxed.2 {
                    if let Some(value) = handle_call(
                        call_expr,
                        resolved_names,
                        codebase,
                        interner,
                        &control_actions,
                    ) {
                        return value;
                    }
                }

                if let Some(analysis_data) = analysis_data {
                    if let Some(t) = analysis_data.get_expr_type(boxed.pos()) {
                        if t.is_nothing() {
                            control_actions.insert(ControlAction::End);
                            return control_actions;
                        }
                    }
                }
            }
            aast::Stmt_::Break => {
                if let Some(last_context) = break_context.last() {
                    match last_context {
                        &BreakContext::Switch => {
                            if !control_actions.contains(&ControlAction::LeaveSwitch) {
                                control_actions.insert(ControlAction::LeaveSwitch);
                            }
                        }
                        BreakContext::Loop => {
                            control_actions.insert(ControlAction::BreakImmediateLoop);
                        }
                    }

                    return control_actions;
                }

                if !control_actions.contains(&ControlAction::Break) {
                    control_actions.insert(ControlAction::Break);
                }

                return control_actions;
            }
            aast::Stmt_::Continue => {
                if !control_actions.contains(&ControlAction::Continue) {
                    control_actions.insert(ControlAction::Continue);
                }

                return control_actions;
            }
            aast::Stmt_::Throw(_) | aast::Stmt_::Return(_) => {
                if !return_is_exit && stmt.1.is_return() {
                    return control_return(control_actions);
                }

                return control_end(control_actions);
            }
            aast::Stmt_::If(_) => {
                let if_stmt = stmt.1.as_if().unwrap();

                let if_statement_actions = get_control_actions(
                    codebase,
                    interner,
                    resolved_names,
                    &if_stmt.1 .0,
                    analysis_data,
                    break_context.clone(),
                    return_is_exit,
                );

                let mut all_leave = if_statement_actions
                    .iter()
                    .filter(|action| *action == &ControlAction::None)
                    .count()
                    == 0;

                let else_statement_actions = get_control_actions(
                    codebase,
                    interner,
                    resolved_names,
                    &if_stmt.2 .0,
                    analysis_data,
                    break_context.clone(),
                    return_is_exit,
                );

                all_leave = all_leave
                    && else_statement_actions
                        .iter()
                        .filter(|action| *action == &ControlAction::None)
                        .count()
                        == 0;

                control_actions.extend(if_statement_actions);
                control_actions.extend(else_statement_actions);

                if all_leave {
                    return control_actions;
                }

                control_actions.retain(|action| *action != ControlAction::None);
            }
            aast::Stmt_::Do(_)
            | aast::Stmt_::While(_)
            | aast::Stmt_::Foreach(_)
            | aast::Stmt_::For(_) => {
                let loop_stmts = if stmt.1.is_do() {
                    stmt.1.as_do().unwrap().0
                } else if stmt.1.is_while() {
                    stmt.1.as_while().unwrap().1
                } else if stmt.1.is_for() {
                    stmt.1.as_for().unwrap().3
                } else {
                    stmt.1.as_foreach().unwrap().2
                };

                let mut loop_break_context = break_context.clone();
                loop_break_context.push(BreakContext::Loop);

                let loop_actions = get_control_actions(
                    codebase,
                    interner,
                    resolved_names,
                    &loop_stmts.0,
                    analysis_data,
                    loop_break_context,
                    return_is_exit,
                );

                control_actions.extend(loop_actions);

                control_actions.retain(|action| action != &ControlAction::None);

                // check for infinite loop behaviour
                if let Some(types) = analysis_data {
                    match &stmt.1 {
                        aast::Stmt_::While(boxed) => {
                            if let Some(expr_type) = types.get_expr_type(&boxed.0 .1) {
                                if expr_type.is_always_truthy() {
                                    //infinite while loop that only return don't have an exit path
                                    let loop_only_ends = control_actions
                                        .iter()
                                        .filter(|action| {
                                            *action != &ControlAction::End
                                                && *action != &ControlAction::Return
                                        })
                                        .count()
                                        == 0;

                                    if loop_only_ends {
                                        return control_actions;
                                    }
                                }
                            }
                        }
                        aast::Stmt_::For(boxed) => {
                            let mut is_infinite_loop = true;

                            if let Some(for_cond) = &boxed.1 {
                                if let Some(expr_type) = types.get_expr_type(&for_cond.1) {
                                    if !expr_type.is_always_truthy() {
                                        is_infinite_loop = false
                                    }
                                } else {
                                    is_infinite_loop = false;
                                }
                            }

                            if is_infinite_loop {
                                let loop_only_ends = control_actions
                                    .iter()
                                    .filter(|action| {
                                        *action != &ControlAction::End
                                            && *action != &ControlAction::Return
                                    })
                                    .count()
                                    == 0;

                                if loop_only_ends {
                                    return control_actions;
                                }
                            }
                        }
                        _ => {}
                    }
                }

                control_actions.retain(|action| action != &ControlAction::BreakImmediateLoop);
            }
            aast::Stmt_::Switch(_) => {
                let mut has_ended = false;
                let mut has_default_terminator = false;

                let switch_stmt = stmt.1.as_switch().unwrap();

                let mut cases = switch_stmt.1.clone();

                cases.reverse();

                let mut switch_break_context = break_context.clone();
                switch_break_context.push(BreakContext::Switch);

                let mut all_case_actions = Vec::new();

                for case in cases {
                    let inner_case_stmts = &case.1;

                    let case_actions = get_control_actions(
                        codebase,
                        interner,
                        resolved_names,
                        &inner_case_stmts.0,
                        analysis_data,
                        switch_break_context.clone(),
                        return_is_exit,
                    );

                    if case_actions.contains(&ControlAction::LeaveSwitch)
                        || case_actions.contains(&ControlAction::Break)
                        || case_actions.contains(&ControlAction::Continue)
                    {
                        continue 'outer;
                    }

                    let case_does_end = case_actions
                        .iter()
                        .filter(|action| {
                            *action != &ControlAction::End && *action != &ControlAction::Return
                        })
                        .count()
                        == 0;

                    if case_does_end {
                        has_ended = true;
                    }

                    all_case_actions.extend(case_actions);

                    if !case_does_end && !has_ended {
                        continue 'outer;
                    }
                }

                if let Some(default_case) = switch_stmt.2 {
                    let inner_case_stmts = &default_case.1;

                    let case_actions = get_control_actions(
                        codebase,
                        interner,
                        resolved_names,
                        &inner_case_stmts.0,
                        analysis_data,
                        switch_break_context.clone(),
                        return_is_exit,
                    );

                    if case_actions.contains(&ControlAction::LeaveSwitch)
                        || case_actions.contains(&ControlAction::Break)
                        || case_actions.contains(&ControlAction::Continue)
                    {
                        continue 'outer;
                    }

                    let case_does_end = case_actions
                        .iter()
                        .filter(|action| {
                            *action != &ControlAction::End && *action != &ControlAction::Return
                        })
                        .count()
                        == 0;

                    if case_does_end {
                        has_ended = true;
                    }

                    all_case_actions.extend(case_actions);

                    if !case_does_end && !has_ended {
                        continue 'outer;
                    }

                    has_default_terminator = true;
                }

                control_actions.extend(all_case_actions);

                if has_default_terminator
                    || if let Some(analysis_data) = analysis_data {
                        analysis_data
                            .fully_matched_switch_offsets
                            .contains(&stmt.0.start_offset())
                    } else {
                        false
                    }
                {
                    return control_actions;
                }
            }
            aast::Stmt_::Try(_) => {
                let stmt = stmt.1.as_try().unwrap();

                let try_stmt_actions = get_control_actions(
                    codebase,
                    interner,
                    resolved_names,
                    &stmt.0 .0,
                    analysis_data,
                    break_context.clone(),
                    return_is_exit,
                );

                let try_leaves = try_stmt_actions
                    .iter()
                    .filter(|action| *action == &ControlAction::None)
                    .count()
                    == 0;

                let mut all_catch_actions = Vec::new();

                if !stmt.1.is_empty() {
                    let mut all_catches_leave = try_leaves;

                    for catch in stmt.1 {
                        let catch_actions = get_control_actions(
                            codebase,
                            interner,
                            resolved_names,
                            &catch.2 .0,
                            analysis_data,
                            break_context.clone(),
                            return_is_exit,
                        );

                        if all_catches_leave {
                            all_catches_leave = catch_actions
                                .iter()
                                .filter(|action| *action == &ControlAction::None)
                                .count()
                                == 0;
                        }

                        if !all_catches_leave {
                            control_actions.extend(catch_actions);
                        } else {
                            all_catch_actions.extend(catch_actions);
                        }
                    }

                    let mut none_hashset = FxHashSet::default();
                    none_hashset.insert(ControlAction::None);

                    if all_catches_leave && try_stmt_actions != none_hashset {
                        control_actions.extend(try_stmt_actions);
                        control_actions.extend(all_catch_actions);

                        return control_actions;
                    }
                } else if try_leaves {
                    control_actions.extend(try_stmt_actions);

                    return control_actions;
                }

                if !stmt.2.is_empty() {
                    let finally_actions = get_control_actions(
                        codebase,
                        interner,
                        resolved_names,
                        &stmt.2 .0,
                        analysis_data,
                        break_context.clone(),
                        return_is_exit,
                    );

                    if !finally_actions.contains(&ControlAction::None) {
                        control_actions.retain(|action| *action != ControlAction::None);
                        control_actions.extend(finally_actions);

                        return control_actions;
                    }
                }

                control_actions.extend(try_stmt_actions);

                control_actions.retain(|action| *action != ControlAction::None);
            }
            aast::Stmt_::Block(block_stmts) => {
                if handle_block(
                    codebase,
                    interner,
                    resolved_names,
                    &block_stmts.1,
                    analysis_data,
                    break_context.clone(),
                    return_is_exit,
                    &mut control_actions,
                ) {
                    return control_actions;
                }
            }
            aast::Stmt_::Concurrent(block_stmts) => {
                if handle_block(
                    codebase,
                    interner,
                    resolved_names,
                    block_stmts,
                    analysis_data,
                    break_context.clone(),
                    return_is_exit,
                    &mut control_actions,
                ) {
                    return control_actions;
                }
            }
            aast::Stmt_::Awaitall(boxed) => {
                let mut block_actions = get_control_actions(
                    codebase,
                    interner,
                    resolved_names,
                    &boxed.1 .0,
                    analysis_data,
                    break_context.clone(),
                    return_is_exit,
                );

                if !block_actions.contains(&ControlAction::None) {
                    control_actions.retain(|action| *action != ControlAction::None);
                    control_actions.extend(block_actions);

                    return control_actions;
                }

                block_actions.retain(|action| *action != ControlAction::None);

                control_actions.extend(block_actions);
            }
            aast::Stmt_::Fallthrough => {}
            aast::Stmt_::YieldBreak => {}
            aast::Stmt_::Using(_) => {}
            aast::Stmt_::Noop => {}
            aast::Stmt_::Markup(_) => {}
            aast::Stmt_::DeclareLocal(_) => {}
            aast::Stmt_::Match(_) => {}
        }
    }

    if !control_actions.contains(&ControlAction::None) {
        control_actions.insert(ControlAction::None);
    }

    control_actions
}

fn handle_block(
    codebase: &CodebaseInfo,
    interner: &Interner,
    resolved_names: &FxHashMap<u32, StrId>,
    block_stmts: &aast::Block<(), ()>,
    analysis_data: Option<&FunctionAnalysisData>,
    break_context: Vec<BreakContext>,
    return_is_exit: bool,
    control_actions: &mut FxHashSet<ControlAction>,
) -> bool {
    let mut block_actions = get_control_actions(
        codebase,
        interner,
        resolved_names,
        &block_stmts.0,
        analysis_data,
        break_context,
        return_is_exit,
    );

    if !block_actions.contains(&ControlAction::None) {
        control_actions.extend(block_actions);
        control_actions.retain(|action| *action != ControlAction::None);

        return true;
    }

    block_actions.retain(|action| *action != ControlAction::None);
    control_actions.extend(block_actions);

    false
}

fn handle_call(
    call_expr: &CallExpr,
    resolved_names: &FxHashMap<u32, StrId>,
    codebase: &CodebaseInfo,
    interner: &Interner,
    control_actions: &FxHashSet<ControlAction>,
) -> Option<FxHashSet<ControlAction>> {
    match &call_expr.func.2 {
        aast::Expr_::Id(id) => {
            if id.1.eq("exit") || id.1.eq("die") {
                return Some(control_end(control_actions.clone()));
            }

            let resolved_name = resolved_names.get(&(id.0.start_offset() as u32))?;
            if let Some(functionlike_storage) = codebase
                .functionlike_infos
                .get(&(*resolved_name, StrId::EMPTY))
            {
                if let Some(return_type) = &functionlike_storage.return_type {
                    if return_type.is_nothing() {
                        return Some(control_end(control_actions.clone()));
                    }
                }
            }
        }
        aast::Expr_::ClassConst(boxed) => {
            if let aast::ClassId_::CIexpr(lhs_expr) = &boxed.0 .2 {
                if let aast::Expr_::Id(id) = &lhs_expr.2 {
                    let name_string = &id.1;

                    match name_string.as_str() {
                        "self" | "parent" | "static" => {
                            // do nothing
                        }
                        _ => {
                            let name_string = resolved_names.get(&(id.0.start_offset() as u32))?;

                            let method_name = interner.get(&boxed.1 .1)?;

                            if let Some(functionlike_storage) = codebase
                                .functionlike_infos
                                .get(&(*name_string, method_name))
                            {
                                if let Some(return_type) = &functionlike_storage.return_type {
                                    if return_type.is_nothing() {
                                        return Some(control_end(control_actions.clone()));
                                    }
                                }
                            }
                        }
                    };
                }
            }
        }
        _ => (),
    }
    None
}

#[inline]
fn control_end(mut control_actions: FxHashSet<ControlAction>) -> FxHashSet<ControlAction> {
    control_actions.insert(ControlAction::End);

    control_actions
}

#[inline]
fn control_return(mut control_actions: FxHashSet<ControlAction>) -> FxHashSet<ControlAction> {
    control_actions.insert(ControlAction::Return);

    control_actions
}
