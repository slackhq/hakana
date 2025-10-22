use std::rc::Rc;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::AnalysisError;
use crate::{scope::BlockContext, statements_analyzer::StatementsAnalyzer};
use hakana_code_info::ast::get_id_name;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::wrap_atomic;
use oxidized::aast;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    expr: &aast::Expr<(), ()>,
    class_id: &Box<aast::ClassId<(), ()>>,
) -> Result<(), AnalysisError> {
    let resolved_nameof_type =
        resolve_class_name(statements_analyzer, analysis_data, context, class_id).ok_or_else(
            || {
                AnalysisError::InternalError(
                    "invalid nameof operand".to_string(),
                    statements_analyzer.get_hpos(expr.pos()),
                )
            },
        )?;

    analysis_data.expr_types.insert(
        (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
        Rc::new(wrap_atomic(resolved_nameof_type)),
    );

    Ok(())
}

fn resolve_class_name(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    class_id: &Box<aast::ClassId<(), ()>>,
) -> Option<TAtomic> {
    // The RHS of a nameof expression always seems to be a CIexpr,
    // even if a classname literal or a keyword like self/static/parent is passed.
    let aast::ClassId_::CIexpr(ci_expr) = &class_id.2 else {
        return None;
    };
    let aast::Expr_::Id(inner_class_id) = &ci_expr.2 else {
        return None;
    };

    let mut is_static = false;
    let resolved_class_id = get_id_name(
        &inner_class_id,
        &context.function_context.calling_class,
        context.function_context.calling_class_final,
        statements_analyzer.codebase,
        &mut is_static,
        statements_analyzer.file_analyzer.resolved_names,
    )?;

    analysis_data.symbol_references.add_reference_to_symbol(
        &context.function_context,
        resolved_class_id,
        false,
    );

    Some(TAtomic::TLiteralClassname {
        name: resolved_class_id,
    })
}
