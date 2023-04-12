use std::rc::Rc;

use crate::statements_analyzer::StatementsAnalyzer;
use crate::{scope_analyzer::ScopeAnalyzer, scope_context::ScopeContext};

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use hakana_reflection_info::data_flow::graph::{DataFlowGraph, GraphKind};
use hakana_reflection_info::t_union::populate_union_type;
use hakana_reflection_info::EFFECT_IMPURE;
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::{get_mixed_any, type_expander};
use oxidized::aast;

pub(crate) fn analyze<'expr, 'map, 'new_expr, 'tast>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    hint: &'expr aast::Hint,
    null_if_false: bool,
    analysis_data: &'tast mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let mut root_expr = left.clone();
    let mut has_arrayget_key = false;

    analysis_data.expr_effects.insert(
        (stmt_pos.start_offset(), stmt_pos.end_offset()),
        EFFECT_IMPURE,
    );

    loop {
        match root_expr.2 {
            aast::Expr_::ArrayGet(boxed) => {
                root_expr = boxed.0;
                if let Some(dim) = &boxed.1 {
                    if let aast::Expr_::ArrayGet(..) = dim.2 {
                        has_arrayget_key = true;
                    }
                }
            }
            aast::Expr_::ObjGet(boxed) => {
                root_expr = boxed.0;
            }
            _ => {
                break;
            }
        }
    }

    let mut replacement_left: Option<aast::Expr<(), ()>> = None;

    if has_arrayget_key
        || matches!(
            root_expr.2,
            aast::Expr_::Call(..)
                | aast::Expr_::Cast(..)
                | aast::Expr_::Eif(..)
                | aast::Expr_::Binop(..)
                | aast::Expr_::As(..)
                | aast::Expr_::ClassConst(..)
                | aast::Expr_::Pipe(..)
        )
    {
        replacement_left = get_fake_as_var(
            left,
            statements_analyzer,
            analysis_data,
            context,
            if_body_context,
        );
    } else if let aast::Expr_::Lvar(var) = root_expr.2 {
        if var.1 .1 == "$$" {
            replacement_left = get_fake_as_var(
                left,
                statements_analyzer,
                analysis_data,
                context,
                if_body_context,
            );
        }
    }

    let ternary = aast::Expr(
        (),
        stmt_pos.clone(),
        aast::Expr_::Eif(Box::new((
            aast::Expr(
                (),
                stmt_pos.clone(),
                aast::Expr_::Is(Box::new((
                    replacement_left.clone().unwrap_or(left.clone()),
                    hint.clone(),
                ))),
            ),
            Some(replacement_left.unwrap_or(left.clone())),
            aast::Expr(
                (),
                stmt_pos.clone(),
                if null_if_false {
                    aast::Expr_::Null
                } else {
                    aast::Expr_::Call(Box::new((
                        aast::Expr(
                            (),
                            stmt_pos.clone(),
                            aast::Expr_::Id(Box::new(oxidized::ast_defs::Id(
                                stmt_pos.clone(),
                                "exit".to_string(),
                            ))),
                        ),
                        vec![],
                        vec![],
                        None,
                    )))
                },
            ),
        ))),
    );

    let old_expr_types = analysis_data.expr_types.clone();
    analysis_data.expr_types = analysis_data.expr_types.clone();

    expression_analyzer::analyze(
        statements_analyzer,
        &ternary,
        analysis_data,
        context,
        if_body_context,
    );

    let mut ternary_type = analysis_data
        .get_expr_type(&stmt_pos)
        .cloned()
        .unwrap_or(get_mixed_any());

    if ternary_type.is_mixed() {
        let codebase = statements_analyzer.get_codebase();
        let mut hint_type = get_type_from_hint(
            &hint.1,
            context.function_context.calling_class.as_ref(),
            &statements_analyzer.get_type_resolution_context(),
            statements_analyzer.get_file_analyzer().resolved_names,
        )
        .unwrap();

        populate_union_type(
            &mut hint_type,
            &codebase.symbols,
            &context
                .function_context
                .get_reference_source(&statements_analyzer.get_file_path().0),
            &mut analysis_data.symbol_references,
            false,
        );
        type_expander::expand_union(
            codebase,
            &Some(statements_analyzer.get_interner()),
            &mut hint_type,
            &TypeExpansionOptions {
                self_class: context.function_context.calling_class.as_ref(),
                ..Default::default()
            },
            &mut DataFlowGraph::new(GraphKind::FunctionBody),
        );
        for atomic_type in hint_type.types.iter_mut() {
            atomic_type.remove_placeholders();
        }
        hint_type.parent_nodes = ternary_type.parent_nodes;
        ternary_type = hint_type;
    }

    analysis_data.expr_types = old_expr_types;

    analysis_data.set_expr_type(&stmt_pos, ternary_type);

    true
}

fn get_fake_as_var(
    left: &aast::Expr<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> Option<aast::Expr<(), ()>> {
    let left_var_id = format!(
        "$<tmp coalesce var>{}",
        left.pos().start_offset().to_string()
    );

    expression_analyzer::analyze(
        statements_analyzer,
        left,
        analysis_data,
        context,
        if_body_context,
    );

    let condition_type = analysis_data
        .get_expr_type(left.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    context
        .vars_in_scope
        .insert(left_var_id.clone(), Rc::new(condition_type));

    return Some(aast::Expr(
        (),
        left.pos().clone(),
        aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
            left.pos().clone(),
            (5, left_var_id.clone()),
        ))),
    ));
}
