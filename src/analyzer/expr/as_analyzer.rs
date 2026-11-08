use std::rc::Rc;

use crate::scope::BlockContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use hakana_code_info::EFFECT_IMPURE;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::t_atomic::{TAtomic, TNamedObject};
use hakana_code_info::ttype::wrap_atomic;
use hakana_code_info::ttype::{
    get_mixed_any,
    type_expander::{self, TypeExpansionOptions},
};
use hakana_code_info::var_name::VarName;
use hakana_code_info::{data_flow::graph::DataFlowGraph, t_union::populate_union_type};
use hakana_parser::aast;
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_str::StrId;

pub(crate) fn analyze<'ast, 'expr>(
    statements_analyzer: &StatementsAnalyzer<'ast>,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<'ast>,
    hint: &'expr aast::Hint<'ast>,
    null_if_false: bool,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let arena = statements_analyzer.arena();
    let mut root_expr = *left;
    let mut has_arrayget_key = false;

    analysis_data.expr_effects.insert(
        (stmt_pos.start_offset() as u32, stmt_pos.end_offset() as u32),
        EFFECT_IMPURE,
    );

    loop {
        match root_expr.2 {
            aast::Expr_::ArrayGet(boxed) => {
                root_expr = boxed.0;
                if let Some(dim) = &boxed.1
                    && let aast::Expr_::ArrayGet(..)
                    | aast::Expr_::ClassConst(..)
                    | aast::Expr_::Call(..)
                    | aast::Expr_::Cast(..)
                    | aast::Expr_::Eif(..)
                    | aast::Expr_::Binop(..)
                    | aast::Expr_::As(..)
                    | aast::Expr_::Pipe(..)
                    | aast::Expr_::Await(..)
                    | aast::Expr_::Delay(..) = dim.2
                {
                    has_arrayget_key = true;
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

    let mut replacement_left: Option<aast::Expr> = None;

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
                | aast::Expr_::Delay(..)
        )
    {
        replacement_left = get_fake_as_var(left, statements_analyzer, analysis_data, context);
    } else if let aast::Expr_::Lvar(var) = root_expr.2
        && var.1.1 == "$$"
    {
        replacement_left = get_fake_as_var(left, statements_analyzer, analysis_data, context);
    }

    let ternary = aast::Expr(
        (),
        *stmt_pos,
        aast::Expr_::Eif(&*arena.alloc((
            aast::Expr(
                (),
                *stmt_pos,
                aast::Expr_::Is(&*arena.alloc((replacement_left.unwrap_or(*left), *hint))),
            ),
            Some(replacement_left.unwrap_or(*left)),
            aast::Expr(
                (),
                *stmt_pos,
                if null_if_false {
                    aast::Expr_::Null
                } else {
                    aast::Expr_::Call(&*arena.alloc(aast::CallExpr {
                        func: aast::Expr(
                            (),
                            *stmt_pos,
                            aast::Expr_::Id(
                                &*arena.alloc(hakana_parser::aast::Id(*stmt_pos, "exit")),
                            ),
                        ),
                        targs: &[],
                        args: &[],
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
            hint.1,
            context.function_context.calling_class,
            statements_analyzer.get_type_resolution_context(),
            statements_analyzer.file_analyzer.resolved_names,
            *statements_analyzer.get_file_path(),
            hint.0.start_offset() as u32,
        )
        .unwrap();

        if hint_type.is_nonnull() && ternary_type.is_any() {
            hint_type = wrap_atomic(TAtomic::TMixedWithFlags(true, true));
        } else {
            for t in &hint_type.types {
                if let TAtomic::TReference { name, .. }
                | TAtomic::TNamedObject(TNamedObject { name, .. })
                | TAtomic::TTypeAlias { name, .. } = t
                {
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
                atomic_type.remove_placeholders(codebase);
            }
        }

        hint_type.parent_nodes = ternary_type.parent_nodes;
        ternary_type = hint_type;
    }

    analysis_data.expr_types = old_expr_types;

    analysis_data.set_expr_type(stmt_pos, ternary_type);

    Ok(())
}

fn get_fake_as_var<'ast>(
    left: &aast::Expr<'ast>,
    statements_analyzer: &StatementsAnalyzer<'ast>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<aast::Expr<'ast>> {
    let arena = statements_analyzer.arena();
    let left_var_id = format!("$<tmp coalesce var>{}", left.pos().start_offset());

    expression_analyzer::analyze(statements_analyzer, left, analysis_data, context, true).ok();

    let condition_type = analysis_data
        .get_rc_expr_type(left.pos())
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));

    context
        .locals
        .insert(VarName::new(left_var_id.clone()), condition_type);

    Some(aast::Expr(
        (),
        *left.pos(),
        aast::Expr_::Lvar(&*arena.alloc(hakana_parser::aast::Lid(
            *left.pos(),
            (5, &*arena.alloc_str(&left_var_id)),
        ))),
    ))
}
