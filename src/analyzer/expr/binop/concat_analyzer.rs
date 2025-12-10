use crate::expr::fetch::class_constant_fetch_analyzer::{
    emit_class_pointer_used_as_string, get_class_name_from_class_ptr_literal_expr,
};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::{expression_analyzer, stmt_analyzer::AnalysisError};
use bstr::ByteSlice;
use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TGenericParam;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::template::TemplateBound;
use hakana_code_info::ttype::{get_arraykey, get_string, wrap_atomic};
use hakana_code_info::{
    data_flow::{node::DataFlowNode, path::PathKind},
    t_atomic::TAtomic,
    taint::SinkType,
};
use oxidized::aast;

pub(crate) fn analyze<'expr>(
    statements_analyzer: &StatementsAnalyzer,
    stmt_pos: &aast::Pos,
    left: &'expr aast::Expr<(), ()>,
    right: &'expr aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let mut concat_nodes = get_concat_nodes(left);
    concat_nodes.push(right);

    for concat_node in &concat_nodes {
        expression_analyzer::analyze(
            statements_analyzer,
            concat_node,
            analysis_data,
            context,
            true,
        )?;
    }

    let result_type = analyze_concat_nodes(
        concat_nodes,
        statements_analyzer,
        analysis_data,
        context,
        stmt_pos,
    );

    // todo handle more string type combinations

    analysis_data.set_expr_type(stmt_pos, result_type);

    Ok(())
}

pub(crate) fn analyze_concat_nodes(
    concat_nodes: Vec<&aast::Expr<(), ()>>,
    statements_analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    stmt_pos: &aast::Pos,
) -> TUnion {
    let mut all_literals = true;

    let decision_node = DataFlowNode::get_for_composition(statements_analyzer.get_hpos(stmt_pos));

    let mut has_slash = false;
    let mut has_query = false;
    let mut nonempty_string = false;

    let mut existing_literal_string_values: Option<Vec<String>> = Some(vec!["".to_string()]);

    for (i, concat_node) in concat_nodes.iter().enumerate() {
        let mut new_literal_string_values = vec![];

        if let aast::Expr_::String(simple_string) = &concat_node.2 {
            if let Some(existing_literal_string_values) = &existing_literal_string_values {
                for val in existing_literal_string_values {
                    new_literal_string_values.push(val.clone() + simple_string.to_str().unwrap());
                }
            }

            if simple_string != "" {
                nonempty_string = true;
            }

            if simple_string.contains(&b'/') {
                has_slash = true;
            }
            if simple_string.contains(&b'?') {
                has_query = true;
            }
        } else {
            let expr_type = analysis_data
                .expr_types
                .get(&(
                    concat_node.pos().start_offset() as u32,
                    concat_node.pos().end_offset() as u32,
                ))
                .map(|t| t.clone());

            if let Some(expr_type) = expr_type {
                let mut local_nonempty_string = true;
                for t in &expr_type.types {
                    match t {
                        TAtomic::TLiteralString { value, .. } => {
                            if value.contains('/') {
                                has_slash = true;
                            }
                            if value.contains('?') {
                                has_query = true;
                            }

                            if value == "" {
                                local_nonempty_string = false;
                            }

                            if let Some(existing_literal_string_values) =
                                &existing_literal_string_values
                            {
                                for val in existing_literal_string_values {
                                    new_literal_string_values.push(val.clone() + value);
                                }
                            }
                        }
                        TAtomic::TStringWithFlags(is_truthy, _, true) => {
                            if !is_truthy {
                                local_nonempty_string = false;
                            }
                        }
                        TAtomic::TLiteralInt { .. }
                        | TAtomic::TEnumLiteralCase { .. }
                        | TAtomic::TEnum { .. } => {
                            existing_literal_string_values = None;
                            local_nonempty_string = false;
                        }
                        TAtomic::TLiteralClassPtr { name } => {
                            if let Some(class_name) =
                                get_class_name_from_class_ptr_literal_expr(concat_node)
                            {
                                emit_class_pointer_used_as_string(
                                    statements_analyzer,
                                    context,
                                    analysis_data,
                                    concat_node,
                                    class_name,
                                );
                            } else {
                                emit_class_pointer_used_as_string(
                                    statements_analyzer,
                                    context,
                                    analysis_data,
                                    concat_node,
                                    &("\\".to_string() + statements_analyzer.interner.lookup(name)),
                                );
                            }

                            existing_literal_string_values = None;
                        }
                        _ => {
                            if !can_be_coerced_to_string(
                                statements_analyzer,
                                context,
                                analysis_data,
                                concat_node,
                                t,
                            ) {
                                concat_non_string(
                                    statements_analyzer,
                                    context,
                                    analysis_data,
                                    concat_node,
                                    t,
                                );
                            }
                            local_nonempty_string = false;
                            all_literals = false;
                            existing_literal_string_values = None;
                            break;
                        }
                    }
                }

                if local_nonempty_string {
                    nonempty_string = true;
                }

                for old_parent_node in &expr_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        &old_parent_node.id,
                        &decision_node.id,
                        PathKind::Default,
                        vec![],
                        if i > 0 && (has_slash || has_query) {
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
            } else {
                nonempty_string = false;
                all_literals = false;
                existing_literal_string_values = None;
            }
        }

        if existing_literal_string_values.is_some() && !new_literal_string_values.is_empty() {
            existing_literal_string_values = Some(new_literal_string_values);
        } else {
            existing_literal_string_values = None;
        }
    }

    let mut result_type = if all_literals {
        if let Some(existing_literal_string_values) = existing_literal_string_values {
            TUnion::new(
                existing_literal_string_values
                    .into_iter()
                    .map(|s| TAtomic::TLiteralString { value: s })
                    .collect(),
            )
        } else {
            wrap_atomic(TAtomic::TStringWithFlags(
                nonempty_string,
                nonempty_string,
                true,
            ))
        }
    } else {
        get_string()
    };

    result_type.parent_nodes.push(decision_node.clone());

    analysis_data.data_flow_graph.add_node(decision_node);

    result_type
}

pub(crate) fn get_concat_nodes(expr: &aast::Expr<(), ()>) -> Vec<&aast::Expr<(), ()>> {
    match &expr.2 {
        aast::Expr_::Binop(x) => {
            let (binop, e1, e2) = (&x.bop, &x.lhs, &x.rhs);
            match binop {
                oxidized::ast_defs::Bop::Dot => {
                    let mut concat_nodes = get_concat_nodes(e1);
                    concat_nodes.push(e2);
                    concat_nodes
                }
                _ => vec![expr],
            }
        }
        _ => vec![expr],
    }
}

fn can_be_coerced_to_string(
    statements_analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    expr: &aast::Expr<(), ()>,
    t: &TAtomic,
) -> bool {
    match t {
        TAtomic::TStringWithFlags(..)
        | TAtomic::TString
        | TAtomic::TArraykey { .. }
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TEnum { .. }
        | TAtomic::TEnumLiteralCase { .. }
        | TAtomic::TClassname { .. }
        | TAtomic::TGenericClassname { .. }
        | TAtomic::TTypename { .. }
        | TAtomic::TGenericTypename { .. }
        | TAtomic::TLiteralClassname { .. }
        | TAtomic::TInt => true,

        TAtomic::TTypeVariable { name } => {
            analysis_data
                .type_variable_bounds
                .entry(name.clone())
                .and_modify(|v| {
                    v.upper_bounds.push(TemplateBound {
                        bound_type: get_arraykey(false),
                        appearance_depth: 0,
                        arg_offset: None,
                        equality_bound_classlike: None,
                        pos: Some(statements_analyzer.get_hpos(expr.pos())),
                    })
                });
            true
        }

        TAtomic::TMixedWithFlags(true, _, _, _) => true,

        // This generally comes from json_encode and is unlikely
        TAtomic::TFalse => true,

        TAtomic::TTypeAlias {
            as_type: Some(as_type),
            ..
        }
        | TAtomic::TGenericParam(TGenericParam { as_type, .. }) => as_type.types.iter().all(|t| {
            can_be_coerced_to_string(statements_analyzer, context, analysis_data, expr, t)
        }),

        _ => false,
    }
}

fn concat_non_string(
    statements_analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    expr: &aast::Expr<(), ()>,
    t: &TAtomic,
) {
    let pos = expr.pos();
    let issue = Issue::new(
        IssueKind::ImplicitStringCast,
        format!(
            "Cannot convert {} to type string implicitly",
            t.get_id(Some(statements_analyzer.interner))
        ),
        statements_analyzer.get_hpos(pos),
        &context.function_context.calling_functionlike_id,
    );

    let config = statements_analyzer.get_config();

    if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
        // Only replace code that's not already covered by a FIXME
        if !context
            .function_context
            .is_production(statements_analyzer.codebase)
            || analysis_data.get_matching_hakana_fixme(&issue).is_none()
        {
            analysis_data.add_replacement(
                (pos.start_offset() as u32, pos.start_offset() as u32),
                Replacement::Substitute("((string)".to_string()),
            );
            analysis_data.add_replacement(
                (pos.end_offset() as u32, pos.end_offset() as u32),
                Replacement::Substitute(")".to_string()),
            );
        }
    } else {
        analysis_data.maybe_add_issue(issue, config, statements_analyzer.get_file_path_actual());
    }
}
