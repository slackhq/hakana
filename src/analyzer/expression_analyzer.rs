use std::rc::Rc;

use crate::custom_hook::AfterExprAnalysisData;
use crate::expr::call::new_analyzer;
use crate::expr::fetch::class_constant_fetch_analyzer::get_id_name;
use crate::expr::fetch::{
    array_fetch_analyzer, class_constant_fetch_analyzer, instance_property_fetch_analyzer,
    static_property_fetch_analyzer,
};
use crate::expr::{
    as_analyzer, binop_analyzer, call_analyzer, cast_analyzer, collection_analyzer,
    const_fetch_analyzer, expression_identifier, pipe_analyzer, prefixed_string_analyzer,
    shape_analyzer, ternary_analyzer, tuple_analyzer, unop_analyzer, variable_fetch_analyzer,
    xml_analyzer, yield_analyzer,
};
use crate::expression_analyzer;
use crate::functionlike_analyzer::FunctionLikeAnalyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::taint::SinkType;
use hakana_type::type_expander::{self, get_closure_from_id, TypeExpansionOptions};
use hakana_type::{
    get_bool, get_false, get_float, get_int, get_literal_int, get_literal_string, get_mixed_any,
    get_null, get_string, get_true, wrap_atomic,
};
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    match &expr.2 {
        aast::Expr_::Binop(x) => {
            let (binop, e1, e2) = (&x.0, &x.1, &x.2);

            if !binop_analyzer::analyze(
                statements_analyzer,
                (binop, e1, e2),
                &expr.1,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }
        }
        aast::Expr_::Lvar(lid) => {
            variable_fetch_analyzer::analyze(statements_analyzer, lid, &expr.1, tast_info, context);
        }
        aast::Expr_::Int(value) => {
            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(
                    if let Ok(value) = if value.starts_with("0x") {
                        i64::from_str_radix(value.trim_start_matches("0x"), 16)
                    } else if value.starts_with("0b") {
                        i64::from_str_radix(value.trim_start_matches("0b"), 2)
                    } else {
                        value.parse::<i64>()
                    } {
                        get_literal_int(value)
                    } else {
                        // should never happen
                        get_int()
                    },
                ),
            );
        }
        aast::Expr_::String(value) => {
            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(get_literal_string(value.to_string())),
            );
        }
        aast::Expr_::Float(_) => {
            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(get_float()),
            );
        }
        aast::Expr_::Is(boxed) => {
            let (lhs_expr, _) = (&boxed.0, &boxed.1);

            if !expression_analyzer::analyze(
                statements_analyzer,
                lhs_expr,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }

            add_decision_dataflow(
                statements_analyzer,
                tast_info,
                lhs_expr,
                None,
                expr.pos(),
                get_bool(),
            );
        }
        aast::Expr_::As(boxed) => {
            if !as_analyzer::analyze(
                statements_analyzer,
                expr.pos(),
                &boxed.0,
                &boxed.1,
                boxed.2,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }
        }
        aast::Expr_::Call(boxed) => {
            if !call_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3),
                &expr.1,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }
        }
        aast::Expr_::ArrayGet(boxed) => {
            let keyed_array_var_id = expression_identifier::get_var_id(
                &expr,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                statements_analyzer.get_file_analyzer().resolved_names,
                Some(statements_analyzer.get_codebase()),
            );

            if !array_fetch_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, boxed.1.as_ref()),
                &expr.1,
                tast_info,
                context,
                keyed_array_var_id,
            ) {
                return false;
            }
        }
        aast::Expr_::Eif(boxed) => {
            if !ternary_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &expr.1,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }
        }
        aast::Expr_::Collection(boxed) => {
            if !collection_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &expr.1,
                tast_info,
                context,
            ) {
                return false;
            }
        }
        aast::Expr_::Shape(shape_fields) => {
            if !shape_analyzer::analyze(
                statements_analyzer,
                shape_fields,
                &expr.1,
                tast_info,
                context,
            ) {
                return false;
            }
        }
        aast::Expr_::Tuple(shape_fields) => {
            if !tuple_analyzer::analyze(
                statements_analyzer,
                shape_fields,
                &expr.1,
                tast_info,
                context,
            ) {
                return false;
            }
        }
        aast::Expr_::Pipe(boxed) => {
            if !pipe_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &expr.1,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }
        }
        aast::Expr_::ObjGet(boxed) => {
            let (lhs_expr, rhs_expr, nullfetch, prop_or_method) =
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

            match prop_or_method {
                ast_defs::PropOrMethod::IsProp => {
                    if !instance_property_fetch_analyzer::analyze(
                        statements_analyzer,
                        (&lhs_expr, &rhs_expr),
                        &expr.1,
                        tast_info,
                        context,
                        context.inside_assignment,
                        matches!(nullfetch, ast_defs::OgNullFlavor::OGNullsafe),
                    ) {
                        return false;
                    }
                }
                ast_defs::PropOrMethod::IsMethod => {
                    panic!("should be handled in call_analyzer")
                }
            }

            if let ast_defs::OgNullFlavor::OGNullsafe = nullfetch {
                // handle nullsafe calls
            } else {
            }
        }
        aast::Expr_::New(boxed) => {
            new_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3),
                &expr.1,
                tast_info,
                context,
                if_body_context,
            );
        }
        aast::Expr_::ClassGet(boxed) => {
            let (lhs, rhs, prop_or_method) = (&boxed.0, &boxed.1, &boxed.2);

            match prop_or_method {
                ast_defs::PropOrMethod::IsProp => {
                    if !static_property_fetch_analyzer::analyze(
                        statements_analyzer,
                        (lhs, &rhs),
                        &expr.1,
                        tast_info,
                        context,
                    ) {
                        return false;
                    }
                }
                ast_defs::PropOrMethod::IsMethod => {
                    panic!("should be handled in call_analyzer")
                }
            }
        }
        aast::Expr_::Null => {
            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(get_null()),
            );
        }
        aast::Expr_::True => {
            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(get_true()),
            );
        }
        aast::Expr_::False => {
            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(get_false()),
            );
        }
        aast::Expr_::Unop(x) => {
            let (unop, inner_expr) = (&x.0, &x.1);

            if !unop_analyzer::analyze(
                statements_analyzer,
                (unop, inner_expr),
                &expr.1,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }
        }
        aast::Expr_::Lfun(boxed) | aast::Expr_::Efun(boxed) => {
            let mut function_analyzer =
                FunctionLikeAnalyzer::new(statements_analyzer.get_file_analyzer());
            let mut lambda_context = context.clone();
            let mut analysis_result = AnalysisResult::new(tast_info.data_flow_graph.kind);
            let mut lambda_storage = if let Some(lambda_storage) = function_analyzer.analyze_lambda(
                &boxed.0,
                &mut lambda_context,
                tast_info,
                &mut analysis_result,
                expr.pos(),
            ) {
                lambda_storage
            } else {
                return false;
            };
            for param in lambda_storage.params.iter_mut() {
                if let Some(ref mut param_type) = param.signature_type {
                    type_expander::expand_union(
                        statements_analyzer.get_codebase(),
                        param_type,
                        &TypeExpansionOptions {
                            evaluate_conditional_types: true,
                            expand_generic: true,
                            ..Default::default()
                        },
                        &mut tast_info.data_flow_graph,
                    )
                }
            }
            let issues = analysis_result.emitted_issues.into_iter().next();
            if let Some(issues) = issues {
                for issue in issues.1 {
                    tast_info.maybe_add_issue(issue, statements_analyzer.get_config());
                }
            }
            let replacements = analysis_result.replacements.into_iter().next();
            if let Some((_, replacements)) = replacements {
                tast_info.replacements.extend(replacements);
            }

            let mut closure_type = wrap_atomic(TAtomic::TClosure {
                params: lambda_storage.params,
                return_type: lambda_storage.return_type,
                effects: lambda_storage.effects.to_u8(),
            });

            let closure_id = format!(
                "{}:{}",
                boxed.0.name.pos().filename(),
                boxed.0.name.pos().start_offset()
            );

            if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
                let application_node = DataFlowNode::get_for_method_reference(
                    closure_id.clone(),
                    statements_analyzer.get_hpos(expr.pos()),
                );

                let closure_return_node = DataFlowNode::get_for_method_return(
                    closure_id.clone(),
                    Some(statements_analyzer.get_hpos(expr.pos())),
                    None,
                );

                tast_info.data_flow_graph.add_path(
                    &closure_return_node,
                    &application_node,
                    PathKind::Default,
                    None,
                    None,
                );

                tast_info.data_flow_graph.add_node(application_node.clone());

                closure_type.parent_nodes =
                    FxHashMap::from_iter([(application_node.get_id().clone(), application_node)]);
            }

            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(closure_type),
            );
        }
        aast::Expr_::ClassConst(boxed) => {
            if !class_constant_fetch_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, (&boxed.1 .0, &boxed.1 .1)),
                &expr.1,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }
        }
        aast::Expr_::Clone(boxed) => {
            if !expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }

            if let Some(stmt_type) = tast_info
                .expr_types
                .get(&(boxed.pos().start_offset(), boxed.pos().end_offset()))
                .cloned()
            {
                tast_info.expr_types.insert(
                    (expr.1.start_offset(), expr.1.end_offset()),
                    stmt_type.clone(),
                );
            }
        }
        aast::Expr_::String2(exprs) => {
            let mut all_literals = true;

            let mut parent_nodes = FxHashMap::default();

            for (offset, inner_expr) in exprs.iter().enumerate() {
                if !expression_analyzer::analyze(
                    statements_analyzer,
                    inner_expr,
                    tast_info,
                    context,
                    if_body_context,
                ) {
                    return false;
                }

                let expr_part_type = tast_info.expr_types.get(&(
                    inner_expr.pos().start_offset(),
                    inner_expr.pos().end_offset(),
                ));

                let new_parent_node = DataFlowNode::get_for_assignment(
                    "concat".to_string(),
                    statements_analyzer.get_hpos(inner_expr.pos()),
                );

                tast_info.data_flow_graph.add_node(new_parent_node.clone());

                if let Some(expr_part_type) = expr_part_type {
                    if !expr_part_type.all_literals() {
                        all_literals = false;
                    }

                    for (_, parent_node) in &expr_part_type.parent_nodes {
                        tast_info.data_flow_graph.add_path(
                            parent_node,
                            &new_parent_node,
                            PathKind::Default,
                            None,
                            if offset > 0 {
                                Some(FxHashSet::from_iter([
                                    SinkType::HtmlAttributeUri,
                                    SinkType::CurlUri,
                                    SinkType::RedirectUri,
                                ]))
                            } else {
                                None
                            },
                        );
                    }
                } else {
                    all_literals = false;
                }

                parent_nodes.insert(new_parent_node.get_id().clone(), new_parent_node);
            }

            let mut string_type = if all_literals {
                wrap_atomic(TAtomic::TStringWithFlags(true, false, true))
            } else {
                get_string()
            };

            string_type.parent_nodes = parent_nodes;

            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(string_type),
            );
        }
        aast::Expr_::PrefixedString(boxed) => {
            if let Some(value) = prefixed_string_analyzer::analyze(
                statements_analyzer,
                boxed,
                tast_info,
                context,
                if_body_context,
                expr,
            ) {
                return value;
            }
        }
        aast::Expr_::Id(boxed) => {
            const_fetch_analyzer::analyze(statements_analyzer, boxed, tast_info);
        }
        aast::Expr_::Xml(boxed) => {
            xml_analyzer::analyze(
                context,
                boxed,
                statements_analyzer,
                tast_info,
                if_body_context,
                expr,
            );
        }
        aast::Expr_::Await(boxed) => {
            if !expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                tast_info,
                context,
                if_body_context,
            ) {
                return false;
            }

            let awaited_stmt_type = tast_info
                .get_expr_type(boxed.pos())
                .cloned()
                .unwrap_or(get_mixed_any());

            tast_info
                .expr_effects
                .insert((expr.1.start_offset(), expr.1.end_offset()), 7);

            for (_, atomic_type) in awaited_stmt_type.types {
                if let TAtomic::TNamedObject {
                    name,
                    type_params: Some(type_params),
                    ..
                } = atomic_type
                {
                    if *name == "HH\\Awaitable" && type_params.len() == 1 {
                        let mut inside_type = type_params.get(0).unwrap().clone();
                        inside_type.parent_nodes = awaited_stmt_type.parent_nodes.clone();

                        tast_info.expr_types.insert(
                            (expr.1.start_offset(), expr.1.end_offset()),
                            Rc::new(inside_type),
                        );
                        break;
                    }
                }
            }
        }
        aast::Expr_::FunctionPointer(boxed) => {
            analyze_function_pointer(statements_analyzer, boxed, context, tast_info, expr);
        }
        aast::Expr_::Cast(boxed) => {
            return cast_analyzer::analyze(
                statements_analyzer,
                &expr.1,
                &boxed.0,
                &boxed.1,
                tast_info,
                context,
                if_body_context,
            );
        }
        aast::Expr_::Yield(boxed) => {
            yield_analyzer::analyze(
                &expr.1,
                boxed,
                statements_analyzer,
                tast_info,
                context,
                if_body_context,
            );
        }
        aast::Expr_::List(_) => {
            panic!("should not happen")
        }
        aast::Expr_::Import(_) => {
            // do nothing with require/include
        }
        aast::Expr_::EnumClassLabel(boxed) => {
            let class_name = if let Some(id) = &boxed.0 {
                let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

                Some(resolved_names.get(&id.0.start_offset()).cloned().unwrap())
            } else {
                None
            };
            tast_info.expr_types.insert(
                (expr.1.start_offset(), expr.1.end_offset()),
                Rc::new(wrap_atomic(TAtomic::TEnumClassLabel {
                    class_name,
                    member_name: boxed.1.clone(),
                })),
            );
        }
        aast::Expr_::Darray(_)
        | aast::Expr_::Varray(_)
        | aast::Expr_::ValCollection(_)
        | aast::Expr_::KeyValCollection(_)
        | aast::Expr_::This
        | aast::Expr_::Omitted
        | aast::Expr_::Dollardollar(_)
        | aast::Expr_::ReadonlyExpr(_)
        | aast::Expr_::Upcast(_)
        | aast::Expr_::ExpressionTree(_)
        | aast::Expr_::Lplaceholder(_)
        | aast::Expr_::FunId(_)
        | aast::Expr_::MethodId(_)
        | aast::Expr_::MethodCaller(_)
        | aast::Expr_::SmethodId(_)
        | aast::Expr_::Pair(_)
        | aast::Expr_::ETSplice(_)
        | aast::Expr_::Hole(_) => {
            //println!("{:#?}", expr);
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::UnrecognizedExpression,
                    "Unrecognized expression".to_string(),
                    statements_analyzer.get_hpos(&expr.1),
                ),
                statements_analyzer.get_config(),
            );
            return false;
        }
    }

    for hook in &statements_analyzer.get_config().hooks {
        hook.after_expr_analysis(
            tast_info,
            AfterExprAnalysisData {
                statements_analyzer,
                expr,
                context,
            },
        );
    }

    true
}

fn analyze_function_pointer(
    statements_analyzer: &StatementsAnalyzer,
    boxed: &Box<(aast::FunctionPtrId<(), ()>, Vec<aast::Targ<()>>)>,
    context: &mut ScopeContext,
    tast_info: &mut TastInfo,
    expr: &aast::Expr<(), ()>,
) {
    let codebase = statements_analyzer.get_codebase();
    let id = match &boxed.0 {
        aast::FunctionPtrId::FPId(id) => FunctionLikeIdentifier::Function({
            let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

            resolved_names.get(&id.0.start_offset()).cloned().unwrap()
        }),
        aast::FunctionPtrId::FPClassConst(class_id, method_name) => {
            let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;
            let calling_class = &context.function_context.calling_class;

            let class_name = match &class_id.2 {
                aast::ClassId_::CIexpr(inner_expr) => {
                    if let aast::Expr_::Id(id) = &inner_expr.2 {
                        get_id_name(id, &calling_class, codebase, &mut false, resolved_names)
                    } else {
                        None
                    }
                }
                _ => None,
            }
            .unwrap();

            FunctionLikeIdentifier::Method(class_name, method_name.1.clone())
        }
    };

    match &id {
        FunctionLikeIdentifier::Function(name) => {
            tast_info
                .symbol_references
                .add_reference_to_symbol(&context.function_context, name.clone());
        }
        FunctionLikeIdentifier::Method(class_name, method_name) => {
            tast_info.symbol_references.add_reference_to_class_member(
                &context.function_context,
                (class_name.clone(), format!("{}()", method_name)),
            );

            if let Some(classlike_storage) = codebase.classlike_infos.get(class_name) {
                let declaring_method_id = codebase.get_declaring_method_id(&MethodIdentifier(
                    class_name.clone(),
                    method_name.clone(),
                ));

                if let Some(overridden_classlikes) = classlike_storage
                    .overridden_method_ids
                    .get(&declaring_method_id.1)
                {
                    for overridden_classlike in overridden_classlikes {
                        tast_info
                            .symbol_references
                            .add_reference_to_overridden_class_member(
                                &context.function_context,
                                (
                                    overridden_classlike.clone(),
                                    format!("{}()", declaring_method_id.1),
                                ),
                            );
                    }
                }
            }
        }
    }

    if let Some(closure) = get_closure_from_id(&id, codebase, &mut tast_info.data_flow_graph) {
        tast_info.expr_types.insert(
            (expr.1.start_offset(), expr.1.end_offset()),
            Rc::new(wrap_atomic(closure)),
        );
    }
}

pub(crate) fn add_decision_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    lhs_expr: &aast::Expr<(), ()>,
    rhs_expr: Option<&aast::Expr<(), ()>>,
    expr_pos: &Pos,
    mut cond_type: TUnion,
) {
    if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
        return;
    }

    let decision_node = DataFlowNode::get_for_variable_sink(
        "is decision".to_string(),
        statements_analyzer.get_hpos(expr_pos),
    );

    if let Some(lhs_type) = tast_info
        .expr_types
        .get(&(lhs_expr.1.start_offset(), lhs_expr.1.end_offset()))
    {
        cond_type
            .parent_nodes
            .insert(decision_node.get_id().clone(), decision_node.clone());

        for (_, old_parent_node) in &lhs_type.parent_nodes {
            tast_info.data_flow_graph.add_path(
                old_parent_node,
                &decision_node,
                PathKind::Default,
                None,
                None,
            );
        }
    }

    if let Some(rhs_expr) = rhs_expr {
        if let Some(rhs_type) = tast_info
            .expr_types
            .get(&(rhs_expr.1.start_offset(), rhs_expr.1.end_offset()))
        {
            cond_type
                .parent_nodes
                .insert(decision_node.get_id().clone(), decision_node.clone());

            for (_, old_parent_node) in &rhs_type.parent_nodes {
                tast_info.data_flow_graph.add_path(
                    old_parent_node,
                    &decision_node,
                    PathKind::Default,
                    None,
                    None,
                );
            }
        }
    }
    tast_info.expr_types.insert(
        (expr_pos.start_offset(), expr_pos.end_offset()),
        Rc::new(cond_type),
    );
}
