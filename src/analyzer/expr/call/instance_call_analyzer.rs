use std::rc::Rc;

use crate::expr::expression_identifier;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::control_action::ControlAction;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::{add_union_type, get_mixed_any, get_null};
use hakana_code_info::EFFECT_WRITE_PROPS;
use oxidized::aast;
use oxidized::pos::Pos;

use super::atomic_method_call_analyzer::{self, AtomicMethodCallAnalysisResult};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::Expr<(), ()>,
        &aast::Expr<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    nullsafe: bool,
) -> Result<(), AnalysisError> {
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    expression_analyzer::analyze(statements_analyzer, expr.0, analysis_data, context)?;
    context.inside_general_use = was_inside_general_use;

    if let aast::Expr_::Id(_) = &expr.1 .2 {
        // do nothing
    } else {
        expression_analyzer::analyze(statements_analyzer, expr.1, analysis_data, context)?;
    }

    let lhs_var_id = expression_identifier::get_var_id(
        expr.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.file_analyzer.resolved_names,
        Some((statements_analyzer.codebase, statements_analyzer.interner)),
    );

    let class_type = analysis_data
        .get_rc_expr_type(expr.0.pos())
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));

    let mut analysis_result = AtomicMethodCallAnalysisResult::new();

    let mut has_nullsafe_null = false;

    if class_type.is_null() || class_type.is_void() {
        if nullsafe {
            has_nullsafe_null = true;
        }
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::MethodCallOnNull,
                "Cannot call method on null value".to_string(),
                statements_analyzer.get_hpos(expr.1.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    } else {
        if class_type.is_mixed() {
            for origin in &class_type.parent_nodes {
                analysis_data.data_flow_graph.add_mixed_data(origin, pos);
            }
        }

        let mut class_types = class_type.types.iter().collect::<Vec<_>>();

        let type_variable_bounds = analysis_data.type_variable_bounds.clone();

        while let Some(lhs_atomic_type) = class_types.pop() {
            match lhs_atomic_type {
                TAtomic::TNull => {
                    if nullsafe {
                        has_nullsafe_null = true;
                    } else {
                        analysis_data.maybe_add_issue(
                            Issue::new(
                                IssueKind::PossibleMethodCallOnNull,
                                "Cannot call method on null value".to_string(),
                                statements_analyzer.get_hpos(expr.1.pos()),
                                &context.function_context.calling_functionlike_id,
                            ),
                            statements_analyzer.get_config(),
                            statements_analyzer.get_file_path_actual(),
                        )
                    }

                    continue;
                }
                TAtomic::TFalse => {
                    if class_type.ignore_falsable_issues {
                        continue;
                    }
                }
                TAtomic::TGenericParam { as_type, .. }
                | TAtomic::TClassTypeConstant { as_type, .. } => {
                    class_types.extend(&as_type.types);
                    continue;
                }
                TAtomic::TTypeAlias {
                    as_type: Some(as_type),
                    ..
                } => {
                    class_types.extend(&as_type.types);
                    continue;
                }
                TAtomic::TTypeVariable { name } => {
                    if let Some(bounds) = type_variable_bounds.get(name) {
                        for lower_bound_info in &bounds.0 {
                            class_types.extend(&lower_bound_info.bound_type.types);
                        }
                    }
                    continue;
                }
                _ => (),
            }

            atomic_method_call_analyzer::analyze(
                statements_analyzer,
                expr,
                pos,
                analysis_data,
                context,
                lhs_atomic_type,
                &lhs_var_id,
                &mut analysis_result,
            )?;
        }
    }

    if analysis_data
        .expr_effects
        .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
        .unwrap_or(&0)
        >= &EFFECT_WRITE_PROPS
    {
        context.remove_mutable_object_vars();
    }

    if let Some(mut stmt_type) = analysis_result.return_type {
        if has_nullsafe_null && !stmt_type.is_mixed() {
            stmt_type = add_union_type(stmt_type, &get_null(), statements_analyzer.codebase, false);
        }

        if stmt_type.is_nothing() && !context.inside_loop {
            context.has_returned = true;
            context.control_actions.insert(ControlAction::End);
        }

        analysis_data.set_expr_type(pos, stmt_type);
    }

    Ok(())
}
