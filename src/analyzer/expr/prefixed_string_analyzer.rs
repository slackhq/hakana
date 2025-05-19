use hakana_reflector::simple_type_inferer::get_atomic_for_prefix_regex_string;
use std::rc::Rc;

use hakana_code_info::ttype::{get_string, wrap_atomic};

use crate::expression_analyzer;

use crate::scope::BlockContext;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::AnalysisError;

use oxidized::aast;

use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    boxed: &(String, aast::Expr<(), ()>),
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    expr: &aast::Expr<(), ()>,
) -> Result<(), AnalysisError> {
    expression_analyzer::analyze(statements_analyzer, &boxed.1, analysis_data, context)?;

    let inner_type = if let Some(t) = analysis_data.expr_types.get(&(
        boxed.1.pos().start_offset() as u32,
        boxed.1.pos().end_offset() as u32,
    )) {
        (**t).clone()
    } else {
        get_string()
    };

    analysis_data.expr_types.insert(
        (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
        Rc::new(if boxed.0 == "re" {
            let inner_text = inner_type.get_single_literal_string_value().unwrap();
            wrap_atomic(get_atomic_for_prefix_regex_string(inner_text))
        } else {
            inner_type
        }),
    );

    Ok(())
}
