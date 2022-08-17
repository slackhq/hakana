use std::rc::Rc;

use hakana_type::get_mixed_any;
use hakana_type::get_string;
use hakana_type::type_expander;
use hakana_type::type_expander::TypeExpansionOptions;

use crate::scope_analyzer::ScopeAnalyzer;
use crate::typed_ast::TastInfo;

use oxidized::aast;

use oxidized::ast_defs;

use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    boxed: &Box<ast_defs::Id>,
    expr: &aast::Expr<(), ()>,
    tast_info: &mut TastInfo,
) {
    let codebase = statements_analyzer.get_codebase();
    let mut name = &boxed.1;

    if let Some(resolved_name) = statements_analyzer
        .get_file_analyzer()
        .resolved_names
        .get(&expr.1.start_offset())
    {
        name = resolved_name;
    }

    let mut stmt_type = if let Some(constant_storage) = codebase.constant_infos.get(name) {
        if let Some(t) = &constant_storage.inferred_type {
            t.clone()
        } else if let Some(t) = &constant_storage.provided_type {
            t.clone()
        } else {
            get_mixed_any()
        }
    } else {
        if name == "__FILE__" || name == "__DIR__" {
            get_string()
        } else {
            get_mixed_any()
        }
    };

    type_expander::expand_union(
        codebase,
        &mut stmt_type,
        &TypeExpansionOptions {
            ..Default::default()
        },
        &mut tast_info.data_flow_graph,
    );

    tast_info.expr_types.insert(
        (expr.1.start_offset(), expr.1.end_offset()),
        Rc::new(stmt_type),
    );
}
