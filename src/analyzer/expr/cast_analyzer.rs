use rustc_hash::FxHashMap;

use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;

use crate::expression_analyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_type::get_mixed_any;
use oxidized::aast;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr_pos: &aast::Pos,
    hint: &aast::Hint,
    inner_expr: &aast::Expr<(), ()>,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    if expression_analyzer::analyze(
        statements_analyzer,
        inner_expr,
        tast_info,
        context,
        if_body_context,
    ) == false
    {
        return false;
    }

    let expr_type = tast_info
        .get_expr_type(inner_expr.pos())
        .cloned()
        .unwrap_or(get_mixed_any());

    let mut hint_type = get_type_from_hint(
        &hint.1,
        None,
        &statements_analyzer.get_type_resolution_context(),
        &FxHashMap::default(),
    );

    // todo emit issues about redundant casts

    if hint_type.has_taintable_value() || tast_info.data_flow_graph.kind == GraphKind::Variable {
        hint_type.parent_nodes = expr_type.parent_nodes;
    }

    tast_info.set_expr_type(&expr_pos, hint_type);

    if tast_info.pure_exprs.contains(&(
        inner_expr.pos().start_offset(),
        inner_expr.pos().end_offset(),
    )) {
        tast_info
            .pure_exprs
            .insert((expr_pos.start_offset(), expr_pos.end_offset()));
    }

    true
}
