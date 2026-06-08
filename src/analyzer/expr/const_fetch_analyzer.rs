use std::path::Path;
use std::rc::Rc;

use hakana_code_info::issue::Issue;
use hakana_code_info::issue::IssueKind;
use hakana_code_info::ttype::get_literal_string;
use hakana_code_info::ttype::get_mixed_any;
use hakana_code_info::ttype::get_string;
use hakana_code_info::ttype::type_expander;
use hakana_code_info::ttype::type_expander::TypeExpansionOptions;
use hakana_code_info::ttype::wrap_atomic;
use hakana_str::StrId;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::stmt_analyzer::AnalysisError;

use oxidized::ast_defs;

use crate::statements_analyzer::StatementsAnalyzer;

/// Whether the given declared type references a newtype whose defining file is
/// not the file currently being analyzed — i.e. the newtype is opaque here.
pub(crate) fn is_newtype_outside_defining_file(
    provided_type: &hakana_code_info::t_union::TUnion,
    codebase: &hakana_code_info::codebase_info::CodebaseInfo,
    statements_analyzer: &StatementsAnalyzer,
) -> bool {
    provided_type.types.iter().any(|t| {
        if let hakana_code_info::t_atomic::TAtomic::TReference { name, .. }
        | hakana_code_info::t_atomic::TAtomic::TTypeAlias { name, .. } = t
            && let Some(type_definition) = codebase.type_definitions.get(name)
            && let Some(newtype_file) = &type_definition.newtype_file
        {
            return newtype_file != statements_analyzer.get_file_path();
        }

        false
    })
}

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    boxed: &ast_defs::Id,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    let name = if let Some(name) = statements_analyzer
        .file_analyzer
        .resolved_names
        .get(&(boxed.0.start_offset() as u32))
    {
        name
    } else {
        return Err(AnalysisError::InternalError(
            "unable to resolve const name".to_string(),
            statements_analyzer.get_hpos(boxed.pos()),
        ));
    };

    let mut stmt_type = if let Some(constant_storage) = codebase.constant_infos.get(name) {
        if *name == StrId::FILE_CONST {
            get_literal_string(statements_analyzer.get_file_path_actual().to_string())
        } else if *name == StrId::DIR_CONST {
            let path = Path::new(statements_analyzer.get_file_path_actual());
            if let Some(dir) = path.parent() {
                get_literal_string(dir.to_str().unwrap().to_owned())
            } else {
                get_string()
            }
        } else if *name == StrId::FUNCTION_CONST {
            get_string()
        } else if let Some(t) = &constant_storage.inferred_type {
            // if the constant is declared with a newtype, the newtype is opaque
            // outside its defining file, so the literal inferred type would be
            // wrong everywhere else
            if let Some(provided_type) = &constant_storage.provided_type
                && is_newtype_outside_defining_file(provided_type, codebase, statements_analyzer)
            {
                provided_type.clone()
            } else {
                wrap_atomic(t.clone())
            }
        } else if let Some(t) = &constant_storage.provided_type {
            t.clone()
        } else {
            get_mixed_any()
        }
    } else {
        let constant_name = statements_analyzer.interner.lookup(name);

        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentConstant,
                format!("Constant {} not recognized", constant_name),
                statements_analyzer.get_hpos(boxed.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        get_mixed_any()
    };

    type_expander::expand_union(
        codebase,
        &Some(statements_analyzer.interner),
        statements_analyzer.get_file_path(),
        &mut stmt_type,
        &TypeExpansionOptions {
            ..Default::default()
        },
        &mut analysis_data.data_flow_graph,
        &mut 0,
    );

    analysis_data.expr_types.insert(
        (boxed.0.start_offset() as u32, boxed.0.end_offset() as u32),
        Rc::new(stmt_type),
    );

    Ok(())
}
