use std::rc::Rc;

use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::wrap_atomic;
use hakana_code_info::ttype::{
    get_mixed_any,
    type_expander::{self, TypeExpansionOptions},
};
use hakana_code_info::var_name::VarName;
use hakana_code_info::EFFECT_IMPURE;
use hakana_code_info::{data_flow::graph::DataFlowGraph, t_union::populate_union_type};
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_str::StrId;
use oxidized::aast;

pub(crate) fn analyze<'expr>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    hint: &'expr aast::Hint,
    null_if_false: bool,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let mut root_expr = left.clone();
    let mut has_arrayget_key = false;

    analysis_data.expr_effects.insert(
        (stmt_pos.start_offset() as u32, stmt_pos.end_offset() as u32),
        EFFECT_IMPURE,
    );

    loop {
        match root_expr.2 {
            aast::Expr_::ArrayGet(boxed) => {
                root_expr = boxed.0;
                if let Some(dim) = &boxed.1 {
                    if let aast::Expr_::ArrayGet(..)
                    | aast::Expr_::ClassConst(..)
                    | aast::Expr_::Call(..)
                    | aast::Expr_::Cast(..)
                    | aast::Expr_::Eif(..)
                    | aast::Expr_::Binop(..)
                    | aast::Expr_::As(..)
                    | aast::Expr_::Pipe(..)
                    | aast::Expr_::Await(..) = dim.2
                    {
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
                | aast::Expr_::Await(..)
        )
    {
        replacement_left = get_fake_as_var(left, statements_analyzer, analysis_data, context);
    } else if let aast::Expr_::Lvar(var) = root_expr.2 {
        if var.1 .1 == "$$" {
            replacement_left = get_fake_as_var(left, statements_analyzer, analysis_data, context);
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
                    aast::Expr_::Call(Box::new(aast::CallExpr {
                        func: aast::Expr(
                            (),
                            stmt_pos.clone(),
                            aast::Expr_::Id(Box::new(oxidized::ast_defs::Id(
                                stmt_pos.clone(),
                                "exit".to_string(),
                            ))),
                        ),
                        targs: vec![],
                        args: vec![],
                        unpacked_arg: None,
                    }))
                },
            ),
        ))),
    );

    let old_expr_types = analysis_data.expr_types.clone();
    analysis_data.expr_types.clone_from(&old_expr_types);

    expression_analyzer::analyze(statements_analyzer, &ternary, analysis_data, context, true)?;

    let mut ternary_type = analysis_data
        .get_expr_type(stmt_pos)
        .cloned()
        .unwrap_or(get_mixed_any());

    if ternary_type.is_mixed() {
        let codebase = statements_analyzer.codebase;
        let mut hint_type = get_type_from_hint(
            &hint.1,
            context.function_context.calling_class,
            statements_analyzer.get_type_resolution_context(),
            statements_analyzer.file_analyzer.resolved_names,
            *statements_analyzer.get_file_path(),
            hint.0.start_offset() as u32,
        )
        .unwrap();

        if hint_type.is_nonnull() && ternary_type.is_any() {
            hint_type = wrap_atomic(TAtomic::TMixedWithFlags(true, false, false, true));
        } else {
            for t in &hint_type.types {
                if let TAtomic::TReference { name, .. } = t {
                    analysis_data.definition_locations.insert(
                        (hint.0.start_offset() as u32, hint.0.end_offset() as u32),
                        (*name, StrId::EMPTY),
                    );
                }
            }

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
                &Some(statements_analyzer.interner),
                statements_analyzer.get_file_path(),
                &mut hint_type,
                &TypeExpansionOptions {
                    self_class: context.function_context.calling_class,
                    ..Default::default()
                },
                &mut DataFlowGraph::new(GraphKind::FunctionBody),
                &mut 0,
            );
            for atomic_type in hint_type.types.iter_mut() {
                atomic_type.remove_placeholders();
            }
        }

        hint_type.parent_nodes = ternary_type.parent_nodes;
        ternary_type = hint_type;
    }

    analysis_data.expr_types = old_expr_types;

    analysis_data.set_expr_type(stmt_pos, ternary_type);

    Ok(())
}

fn get_fake_as_var(
    left: &aast::Expr<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<aast::Expr<(), ()>> {
    let left_var_id = format!("$<tmp coalesce var>{}", left.pos().start_offset());

    expression_analyzer::analyze(statements_analyzer, left, analysis_data, context, true).ok();

    let condition_type = analysis_data
        .get_rc_expr_type(left.pos())
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));

    context
        .locals
        .insert(VarName::new(left_var_id.clone()), condition_type);

    return Some(aast::Expr(
        (),
        left.pos().clone(),
        aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
            left.pos().clone(),
            (5, left_var_id.clone()),
        ))),
    ));
}
