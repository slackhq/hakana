use std::rc::Rc;

use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::ttype::template::TemplateBound;
use rustc_hash::FxHashMap;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{expr::expression_identifier, expression_analyzer};
use hakana_code_info::ttype::{get_mixed_any, get_num, type_combiner};
use hakana_code_info::{
    data_flow::{node::DataFlowNode, path::PathKind},
    t_atomic::TAtomic,
    t_union::TUnion,
    taint::SinkType,
};
use oxidized::{aast, ast, ast_defs::Pos};

pub(crate) fn analyze<'expr: 'tast, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    operator: &'expr ast::Bop,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    analysis_data: &'tast mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    expression_analyzer::analyze(statements_analyzer, left, analysis_data, context)?;
    expression_analyzer::analyze(statements_analyzer, right, analysis_data, context)?;

    let fallback = get_mixed_any();
    let e1_type = match analysis_data.get_rc_expr_type(&left.1).cloned() {
        Some(var_type) => var_type,
        None => Rc::new(fallback.clone()),
    };

    if e1_type.is_mixed() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::MixedOperand,
                "Operand has a mixed type".to_string(),
                statements_analyzer.get_hpos(&left.1),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let e1_var_id = if context.inside_loop {
        expression_identifier::get_var_id(left, None, &FxHashMap::default(), None)
    } else {
        None
    };

    let e2_type = match analysis_data.get_rc_expr_type(&right.1).cloned() {
        Some(var_type) => var_type,
        None => Rc::new(fallback),
    };

    if e2_type.is_mixed() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::MixedOperand,
                "Operand has a mixed type".to_string(),
                statements_analyzer.get_hpos(&right.1),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let e2_var_id = if context.inside_loop {
        expression_identifier::get_var_id(right, None, &FxHashMap::default(), None)
    } else {
        None
    };

    let has_loop_variable = e1_var_id.is_some() || e2_var_id.is_some();

    let zero = TAtomic::TLiteralInt { value: 0 };

    let mut results = vec![];

    let mut e1_types = e1_type.types.clone();

    while let Some(mut e1_type_atomic) = e1_types.pop() {
        if let TAtomic::TGenericParam { as_type, .. }
        | TAtomic::TClassTypeConstant { as_type, .. }
        | TAtomic::TTypeAlias {
            as_type: Some(as_type),
            ..
        } = e1_type_atomic
        {
            e1_types.extend(as_type.types);
            continue;
        }

        if let TAtomic::TEnumLiteralCase {
            as_type: Some(constraint),
            ..
        } = &e1_type_atomic
        {
            e1_type_atomic = (**constraint).clone();
        }

        if let TAtomic::TEnum {
            as_type: Some(base_type),
            ..
        } = &e1_type_atomic
        {
            e1_type_atomic = (**base_type).clone();
        }

        if let TAtomic::TFalse = e1_type_atomic {
            if e1_type.ignore_falsable_issues {
                continue;
            }
            e1_type_atomic = zero.clone();
        }

        if let TAtomic::TTypeVariable { name } = &e1_type_atomic {
            results.push(e1_type_atomic.clone());
            if let Some((_, upper_bounds)) = analysis_data.type_variable_bounds.get_mut(name) {
                let mut bound = TemplateBound::new(get_num(), 0, None, None);
                bound.pos = Some(statements_analyzer.get_hpos(left.pos()));
                upper_bounds.push(bound);
            }

            continue;
        }

        let mut e2_types = e2_type.types.clone();

        while let Some(mut e2_type_atomic) = e2_types.pop() {
            if let TAtomic::TGenericParam { ref as_type, .. }
            | TAtomic::TClassTypeConstant { ref as_type, .. }
            | TAtomic::TTypeAlias {
                as_type: Some(ref as_type),
                ..
            } = e1_type_atomic
            {
                e1_types.extend(as_type.types.clone());
                continue;
            }

            if let TAtomic::TEnumLiteralCase {
                as_type: Some(constraint),
                ..
            } = &e2_type_atomic
            {
                e2_type_atomic = (**constraint).clone();
            }

            if let TAtomic::TEnum {
                as_type: Some(base_type),
                ..
            } = &e2_type_atomic
            {
                e2_type_atomic = (**base_type).clone();
            }

            if let TAtomic::TFalse = e2_type_atomic {
                if e2_type.ignore_falsable_issues {
                    continue;
                }
                e2_type_atomic = zero.clone();
            }

            if let TAtomic::TTypeVariable { name } = &e2_type_atomic {
                results.push(e2_type_atomic.clone());

                if let Some((_, upper_bounds)) = analysis_data.type_variable_bounds.get_mut(name) {
                    let mut bound = TemplateBound::new(get_num(), 0, None, None);
                    bound.pos = Some(statements_analyzer.get_hpos(left.pos()));
                    upper_bounds.push(bound);
                }

                continue;
            }

            results.push(if has_loop_variable {
                match (&e1_type_atomic, &e2_type_atomic) {
                    (
                        TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TNothing,
                        TAtomic::TInt | TAtomic::TLiteralInt { .. } | TAtomic::TNothing,
                    ) => match operator {
                        oxidized::ast_defs::Bop::Slash => TAtomic::TNum,
                        _ => TAtomic::TInt,
                    },
                    _ => TAtomic::TFloat,
                }
            } else {
                match (&e1_type_atomic, &e2_type_atomic) {
                    (
                        TAtomic::TLiteralInt { value: e1_value },
                        TAtomic::TLiteralInt { value: e2_value },
                    ) => match operator {
                        oxidized::ast_defs::Bop::Plus => TAtomic::TLiteralInt {
                            value: e1_value + e2_value,
                        },
                        oxidized::ast_defs::Bop::Minus => TAtomic::TLiteralInt {
                            value: e1_value - e2_value,
                        },
                        oxidized::ast_defs::Bop::Amp => TAtomic::TLiteralInt {
                            value: e1_value & e2_value,
                        },
                        oxidized::ast_defs::Bop::Bar => TAtomic::TLiteralInt {
                            value: e1_value | e2_value,
                        },
                        oxidized::ast_defs::Bop::Ltlt => TAtomic::TLiteralInt {
                            value: e1_value.wrapping_shl(
                                if let Ok(result) = (*e2_value).try_into() {
                                    result
                                } else {
                                    return Ok(());
                                },
                            ),
                        },
                        oxidized::ast_defs::Bop::Gtgt => TAtomic::TLiteralInt {
                            value: e1_value.wrapping_shr(
                                if let Ok(result) = (*e2_value).try_into() {
                                    result
                                } else {
                                    return Ok(());
                                },
                            ),
                        },
                        oxidized::ast_defs::Bop::Percent => TAtomic::TLiteralInt {
                            value: e1_value % e2_value,
                        },
                        oxidized::ast_defs::Bop::Slash => TAtomic::TNum,
                        _ => TAtomic::TInt,
                    },
                    (
                        TAtomic::TInt | TAtomic::TLiteralInt { .. },
                        TAtomic::TInt | TAtomic::TLiteralInt { .. },
                    ) => match operator {
                        oxidized::ast_defs::Bop::Slash => TAtomic::TNum,
                        _ => TAtomic::TInt,
                    },
                    _ => TAtomic::TFloat,
                }
            });
        }
    }

    let result_type = TUnion::new(if results.len() == 1 {
        results
    } else {
        type_combiner::combine(results, statements_analyzer.codebase, false)
    });

    assign_arithmetic_type(
        statements_analyzer,
        analysis_data,
        result_type,
        left,
        right,
        stmt_pos,
    );

    Ok(())
}

pub(crate) fn assign_arithmetic_type(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    cond_type: TUnion,
    lhs_expr: &aast::Expr<(), ()>,
    rhs_expr: &aast::Expr<(), ()>,
    expr_pos: &Pos,
) {
    let mut cond_type = cond_type;
    let decision_node = DataFlowNode::get_for_composition(statements_analyzer.get_hpos(expr_pos));

    analysis_data
        .data_flow_graph
        .add_node(decision_node.clone());

    if let Some(lhs_type) = analysis_data.expr_types.get(&(
        lhs_expr.1.start_offset() as u32,
        lhs_expr.1.end_offset() as u32,
    )) {
        cond_type.parent_nodes.push(decision_node.clone());

        for old_parent_node in &lhs_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                old_parent_node,
                &decision_node,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
    }

    if let Some(rhs_type) = analysis_data.expr_types.get(&(
        rhs_expr.1.start_offset() as u32,
        rhs_expr.1.end_offset() as u32,
    )) {
        cond_type.parent_nodes.push(decision_node.clone());

        for old_parent_node in &rhs_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                old_parent_node,
                &decision_node,
                PathKind::Default,
                vec![],
                if cond_type.has_string() {
                    vec![
                        SinkType::HtmlAttributeUri,
                        SinkType::CurlUri,
                        SinkType::RedirectUri,
                    ]
                } else {
                    vec![]
                },
            );
        }
    }

    analysis_data.set_expr_type(expr_pos, cond_type);
}
