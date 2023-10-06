use std::rc::Rc;

use hakana_type::get_mixed_any;
use hakana_type::get_string;
use hakana_type::type_expander;
use hakana_type::type_expander::TypeExpansionOptions;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::stmt_analyzer::AnalysisError;

use oxidized::ast_defs;

use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    boxed: &Box<ast_defs::Id>,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.get_codebase();

    let name = if let Some(name) = statements_analyzer
        .get_file_analyzer()
        .resolved_names
        .get(&boxed.0.start_offset())
    {
        name
    } else {
        return Err(AnalysisError::InternalError(
            "unable to resolve const name".to_string(),
            statements_analyzer.get_hpos(boxed.pos()),
        ));
    };

    let mut stmt_type = if let Some(constant_storage) = codebase.constant_infos.get(name) {
        if let Some(t) = &constant_storage.inferred_type {
            t.clone()
        } else if let Some(t) = &constant_storage.provided_type {
            t.clone()
        } else {
            get_mixed_any()
        }
    } else {
        match statements_analyzer.get_interner().lookup(name) {
            "__FILE__" | "__DIR__" => get_string(),
            _ => get_mixed_any(),
        }
    };

    type_expander::expand_union(
        codebase,
        &Some(statements_analyzer.get_interner()),
        &mut stmt_type,
        &TypeExpansionOptions {
            ..Default::default()
        },
        &mut analysis_data.data_flow_graph,
    );

    analysis_data.expr_types.insert(
        (boxed.0.start_offset() as u32, boxed.0.end_offset() as u32),
        Rc::new(stmt_type),
    );

    Ok(())
}
