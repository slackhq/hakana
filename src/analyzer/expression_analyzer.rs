use std::collections::BTreeMap;
use std::rc::Rc;

use crate::custom_hook::AfterExprAnalysisData;
use crate::expr::call::new_analyzer;
use crate::expr::fetch::{
    array_fetch_analyzer, class_constant_fetch_analyzer, instance_property_fetch_analyzer,
    static_property_fetch_analyzer,
};
use crate::expr::{
    as_analyzer, binop_analyzer, call_analyzer, cast_analyzer, closure_analyzer,
    collection_analyzer, const_fetch_analyzer, expression_identifier, include_analyzer,
    pipe_analyzer, prefixed_string_analyzer, shape_analyzer, ternary_analyzer, tuple_analyzer,
    unop_analyzer, variable_fetch_analyzer, xml_analyzer, yield_analyzer,
};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler::reconciler;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::{var_has_root, ScopeContext};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{algebra_analyzer, expression_analyzer, formula_generator};
use hakana_algebra::Clause;
use hakana_reflection_info::ast::get_id_name;
use hakana_reflection_info::code_location::StmtStart;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::taint::SinkType;
use hakana_reflection_info::{EFFECT_IMPURE, STR_AWAITABLE, STR_EMPTY};
use hakana_type::type_expander::get_closure_from_id;
use hakana_type::{
    get_bool, get_false, get_float, get_int, get_literal_int, get_literal_string, get_mixed_any,
    get_null, get_string, get_true, wrap_atomic,
};
use oxidized::ast::Field;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> Result<(), AnalysisError> {
    if statements_analyzer.get_config().add_fixmes {
        if let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset {
            if current_stmt_offset.line != expr.1.line() as u32 {
                if !matches!(expr.2, aast::Expr_::Xml(..)) {
                    *current_stmt_offset = StmtStart {
                        offset: expr.1.start_offset() as u32,
                        line: expr.1.line() as u32,
                        column: expr.1.to_raw_span().start.column() as u16,
                        add_newline: true,
                    };
                } else {
                    current_stmt_offset.line = expr.1.line() as u32;
                }
            }

            analysis_data.expr_fixme_positions.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                *current_stmt_offset,
            );
        }
    }

    match &expr.2 {
        aast::Expr_::Binop(x) => {
            let (binop, e1, e2) = (&x.bop, &x.lhs, &x.rhs);

            binop_analyzer::analyze(
                statements_analyzer,
                (binop, e1, e2),
                &expr.1,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::Lvar(lid) => {
            variable_fetch_analyzer::analyze(
                statements_analyzer,
                lid,
                &expr.1,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::Int(value) => {
            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
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
            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(get_literal_string(value.to_string())),
            );
        }
        aast::Expr_::Float(_) => {
            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(get_float()),
            );
        }
        aast::Expr_::Is(boxed) => {
            let (lhs_expr, _) = (&boxed.0, &boxed.1);

            expression_analyzer::analyze(
                statements_analyzer,
                lhs_expr,
                analysis_data,
                context,
                if_body_context,
            )?;

            add_decision_dataflow(
                statements_analyzer,
                analysis_data,
                lhs_expr,
                None,
                expr.pos(),
                get_bool(),
            );
        }
        aast::Expr_::As(boxed) => {
            as_analyzer::analyze(
                statements_analyzer,
                expr.pos(),
                &boxed.0,
                &boxed.1,
                boxed.2,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::Call(boxed) => {
            call_analyzer::analyze(
                statements_analyzer,
                boxed,
                &expr.1,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::ArrayGet(boxed) => {
            let keyed_array_var_id = expression_identifier::get_var_id(
                &expr,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_file_analyzer().get_file_source(),
                statements_analyzer.get_file_analyzer().resolved_names,
                Some((
                    statements_analyzer.get_codebase(),
                    statements_analyzer.get_interner(),
                )),
            );

            array_fetch_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, boxed.1.as_ref()),
                &expr.1,
                analysis_data,
                context,
                keyed_array_var_id,
            )?;
        }
        aast::Expr_::Eif(boxed) => {
            ternary_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &expr.1,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::Shape(shape_fields) => {
            shape_analyzer::analyze(
                statements_analyzer,
                shape_fields,
                &expr.1,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::Tuple(shape_fields) => {
            tuple_analyzer::analyze(
                statements_analyzer,
                shape_fields,
                &expr.1,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::Pipe(boxed) => {
            pipe_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &expr.1,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::ObjGet(boxed) => {
            let (lhs_expr, rhs_expr, nullfetch, prop_or_method) =
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

            match prop_or_method {
                ast_defs::PropOrMethod::IsProp => {
                    instance_property_fetch_analyzer::analyze(
                        statements_analyzer,
                        (&lhs_expr, &rhs_expr),
                        &expr.1,
                        analysis_data,
                        context,
                        context.inside_assignment,
                        matches!(nullfetch, ast_defs::OgNullFlavor::OGNullsafe),
                    )?;
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
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::ClassGet(boxed) => {
            let (lhs, rhs, prop_or_method) = (&boxed.0, &boxed.1, &boxed.2);

            match prop_or_method {
                ast_defs::PropOrMethod::IsProp => {
                    static_property_fetch_analyzer::analyze(
                        statements_analyzer,
                        (lhs, &rhs),
                        &expr.1,
                        analysis_data,
                        context,
                    )?;
                }
                ast_defs::PropOrMethod::IsMethod => {
                    panic!("should be handled in call_analyzer")
                }
            }
        }
        aast::Expr_::Null => {
            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(get_null()),
            );
        }
        aast::Expr_::True => {
            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(get_true()),
            );
        }
        aast::Expr_::False => {
            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(get_false()),
            );
        }
        aast::Expr_::Unop(x) => {
            let (unop, inner_expr) = (&x.0, &x.1);

            unop_analyzer::analyze(
                statements_analyzer,
                (unop, inner_expr),
                &expr.1,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::Lfun(boxed) => {
            closure_analyzer::analyze(statements_analyzer, context, analysis_data, &boxed.0, expr)?;
        }
        aast::Expr_::Efun(boxed) => {
            closure_analyzer::analyze(
                statements_analyzer,
                context,
                analysis_data,
                &boxed.fun,
                expr,
            )?;
        }
        aast::Expr_::ClassConst(boxed) => {
            class_constant_fetch_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, (&boxed.1 .0, &boxed.1 .1)),
                &expr.1,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::Clone(boxed) => {
            expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                analysis_data,
                context,
                if_body_context,
            )?;

            if let Some(stmt_type) = analysis_data
                .expr_types
                .get(&(
                    boxed.pos().start_offset() as u32,
                    boxed.pos().end_offset() as u32,
                ))
                .cloned()
            {
                analysis_data.expr_types.insert(
                    (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                    stmt_type.clone(),
                );
            }
        }
        aast::Expr_::String2(exprs) => {
            let mut all_literals = true;

            let mut parent_nodes = FxHashSet::default();

            for (offset, inner_expr) in exprs.iter().enumerate() {
                expression_analyzer::analyze(
                    statements_analyzer,
                    inner_expr,
                    analysis_data,
                    context,
                    if_body_context,
                )?;

                let expr_part_type = analysis_data.expr_types.get(&(
                    inner_expr.pos().start_offset() as u32,
                    inner_expr.pos().end_offset() as u32,
                ));

                let new_parent_node = DataFlowNode::get_for_assignment(
                    "concat".to_string(),
                    statements_analyzer.get_hpos(inner_expr.pos()),
                );

                analysis_data
                    .data_flow_graph
                    .add_node(new_parent_node.clone());

                if let Some(expr_part_type) = expr_part_type {
                    if !expr_part_type.all_literals() {
                        all_literals = false;
                    }

                    for parent_node in &expr_part_type.parent_nodes {
                        analysis_data.data_flow_graph.add_path(
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

                parent_nodes.insert(new_parent_node);
            }

            let mut string_type = if all_literals {
                wrap_atomic(TAtomic::TStringWithFlags(true, false, true))
            } else {
                get_string()
            };

            string_type.parent_nodes = parent_nodes;

            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(string_type),
            );
        }
        aast::Expr_::PrefixedString(boxed) => {
            prefixed_string_analyzer::analyze(
                statements_analyzer,
                boxed,
                analysis_data,
                context,
                if_body_context,
                expr,
            )?;
        }
        aast::Expr_::Id(boxed) => {
            const_fetch_analyzer::analyze(statements_analyzer, boxed, analysis_data)?;
        }
        aast::Expr_::Xml(boxed) => {
            xml_analyzer::analyze(
                context,
                boxed,
                expr.pos(),
                statements_analyzer,
                analysis_data,
                if_body_context,
            )?;
        }
        aast::Expr_::Await(boxed) => {
            expression_analyzer::analyze(
                statements_analyzer,
                &boxed,
                analysis_data,
                context,
                if_body_context,
            )?;

            let mut awaited_stmt_type = analysis_data
                .get_expr_type(boxed.pos())
                .cloned()
                .unwrap_or(get_mixed_any());

            analysis_data.expr_effects.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                EFFECT_IMPURE,
            );

            let awaited_types = awaited_stmt_type.types.drain(..).collect::<Vec<_>>();

            let mut new_types = vec![];

            for atomic_type in awaited_types {
                if let TAtomic::TNamedObject {
                    name: STR_AWAITABLE,
                    type_params: Some(ref type_params),
                    ..
                } = atomic_type
                {
                    if type_params.len() == 1 {
                        let inside_type = type_params.first().unwrap().clone();
                        awaited_stmt_type
                            .parent_nodes
                            .extend(inside_type.parent_nodes);
                        new_types.extend(inside_type.types);
                    } else {
                        new_types.push(atomic_type);
                    }
                } else {
                    new_types.push(atomic_type);
                }
            }

            awaited_stmt_type.types = new_types;

            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(awaited_stmt_type),
            );
        }
        aast::Expr_::FunctionPointer(boxed) => {
            analyze_function_pointer(statements_analyzer, boxed, context, analysis_data, expr)?;
        }
        aast::Expr_::Cast(boxed) => {
            return cast_analyzer::analyze(
                statements_analyzer,
                &expr.1,
                &boxed.0,
                &boxed.1,
                analysis_data,
                context,
                if_body_context,
            );
        }
        aast::Expr_::Yield(boxed) => {
            yield_analyzer::analyze(
                &expr.1,
                boxed,
                statements_analyzer,
                analysis_data,
                context,
                if_body_context,
            )?;
        }
        aast::Expr_::List(_) => {
            panic!("should not happen")
        }
        aast::Expr_::Import(boxed) => {
            include_analyzer::analyze(
                statements_analyzer,
                &boxed.1,
                expr.pos(),
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::EnumClassLabel(boxed) => {
            let class_name = if let Some(id) = &boxed.0 {
                let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

                Some(resolved_names.get(&id.0.start_offset()).cloned().unwrap())
            } else {
                None
            };
            if let Some(member_name) = statements_analyzer.get_interner().get(&boxed.1) {
                analysis_data.expr_types.insert(
                    (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                    Rc::new(wrap_atomic(TAtomic::TEnumClassLabel {
                        class_name,
                        member_name,
                    })),
                );
            } else {
                analysis_data.expr_types.insert(
                    (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                    Rc::new(get_mixed_any()),
                );
            }
        }
        aast::Expr_::Darray(boxed) => {
            let fields = boxed
                .1
                .iter()
                .map(|(key_expr, value_expr)| Field(key_expr.clone(), value_expr.clone()))
                .collect::<Vec<_>>();

            collection_analyzer::analyze_keyvals(
                statements_analyzer,
                &oxidized::tast::KvcKind::Dict,
                &fields,
                expr.pos(),
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::Varray(boxed) => {
            collection_analyzer::analyze_vals(
                statements_analyzer,
                &oxidized::tast::VcKind::Vec,
                &boxed.1,
                expr.pos(),
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::ValCollection(boxed) => {
            collection_analyzer::analyze_vals(
                statements_analyzer,
                &boxed.0 .1,
                &boxed.2,
                expr.pos(),
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::KeyValCollection(boxed) => {
            collection_analyzer::analyze_keyvals(
                statements_analyzer,
                &boxed.0 .1,
                &boxed.2,
                expr.pos(),
                analysis_data,
                context,
            )?;
        }

        aast::Expr_::Collection(_)
        | aast::Expr_::This
        | aast::Expr_::Omitted
        | aast::Expr_::Dollardollar(_)
        | aast::Expr_::ReadonlyExpr(_)
        | aast::Expr_::Upcast(_)
        | aast::Expr_::ExpressionTree(_)
        | aast::Expr_::Lplaceholder(_)
        | aast::Expr_::MethodCaller(_)
        | aast::Expr_::Pair(_)
        | aast::Expr_::ETSplice(_)
        | aast::Expr_::Hole(_)
        | aast::Expr_::Nameof(_)
        | aast::Expr_::Invalid(_) => {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::UnrecognizedExpression,
                    "Unrecognized expression".to_string(),
                    statements_analyzer.get_hpos(&expr.1),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
            //return Err(AnalysisError::UserError);
        }
        aast::Expr_::Package(_) => todo!(),
    }

    for hook in &statements_analyzer.get_config().hooks {
        hook.after_expr_analysis(
            analysis_data,
            AfterExprAnalysisData {
                statements_analyzer,
                expr,
                context,
            },
        );
    }

    Ok(())
}

pub(crate) fn expr_has_logic(expr: &aast::Expr<(), ()>) -> bool {
    match &expr.2 {
        aast::Expr_::Binop(boxed) => match boxed.bop {
            oxidized::nast::Bop::Eqeq
            | oxidized::nast::Bop::Eqeqeq
            | oxidized::nast::Bop::Diff
            | oxidized::nast::Bop::Diff2
            | oxidized::nast::Bop::Ampamp
            | oxidized::nast::Bop::Barbar
            | oxidized::nast::Bop::QuestionQuestion => true,
            _ => false,
        },
        aast::Expr_::Is(_) => true,
        _ => false,
    }
}

pub(crate) fn find_expr_logic_issues(
    statements_analyzer: &StatementsAnalyzer,
    context: &ScopeContext,
    expr: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
) {
    let assertion_context = statements_analyzer.get_assertion_context(
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
    );

    let mut if_context = context.clone();
    let mut cond_referenced_var_ids = if_context.cond_referenced_var_ids.clone();

    let cond_object_id = (
        expr.pos().start_offset() as u32,
        expr.pos().end_offset() as u32,
    );

    let if_clauses = formula_generator::get_formula(
        cond_object_id,
        cond_object_id,
        expr,
        &assertion_context,
        analysis_data,
        false,
        false,
    );

    let mut expr_clauses = if let Ok(if_clauses) = if_clauses {
        if if_clauses.len() > 200 {
            vec![]
        } else {
            if_clauses
        }
    } else {
        vec![]
    };

    let mut mixed_var_ids = Vec::new();

    for (var_id, var_type) in &context.vars_in_scope {
        if var_type.is_mixed() && context.vars_in_scope.contains_key(var_id) {
            mixed_var_ids.push(var_id);
        }
    }

    expr_clauses = expr_clauses
        .into_iter()
        .map(|c| {
            let keys = &c
                .possibilities
                .iter()
                .map(|(k, _)| k)
                .collect::<Vec<&String>>();

            let mut new_mixed_var_ids = vec![];
            for i in mixed_var_ids.clone() {
                if !keys.contains(&i) {
                    new_mixed_var_ids.push(i);
                }
            }
            mixed_var_ids = new_mixed_var_ids;

            for key in keys {
                for mixed_var_id in &mixed_var_ids {
                    if var_has_root(key, mixed_var_id) {
                        return Clause::new(
                            BTreeMap::new(),
                            cond_object_id,
                            cond_object_id,
                            Some(true),
                            None,
                            None,
                        );
                    }
                }
            }

            return c;
        })
        .collect::<Vec<Clause>>();

    // this will see whether any of the clauses in set A conflict with the clauses in set B
    algebra_analyzer::check_for_paradox(
        statements_analyzer,
        &context.clauses,
        &expr_clauses,
        analysis_data,
        expr.pos(),
        &context.function_context.calling_functionlike_id,
    );

    expr_clauses.extend(
        context
            .clauses
            .iter()
            .map(|v| (**v).clone())
            .collect::<Vec<_>>(),
    );

    let (reconcilable_if_types, active_if_types) = hakana_algebra::get_truths_from_formula(
        expr_clauses.iter().collect(),
        Some(cond_object_id),
        &mut cond_referenced_var_ids,
    );

    reconciler::reconcile_keyed_types(
        &reconcilable_if_types,
        active_if_types,
        &mut if_context,
        &mut FxHashSet::default(),
        &cond_referenced_var_ids,
        statements_analyzer,
        analysis_data,
        expr.pos(),
        true,
        false,
        &FxHashMap::default(),
    );
}

fn analyze_function_pointer(
    statements_analyzer: &StatementsAnalyzer,
    boxed: &Box<(aast::FunctionPtrId<(), ()>, Vec<aast::Targ<()>>)>,
    context: &mut ScopeContext,
    analysis_data: &mut FunctionAnalysisData,
    expr: &aast::Expr<(), ()>,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.get_codebase();
    let id = match &boxed.0 {
        aast::FunctionPtrId::FPId(id) => FunctionLikeIdentifier::Function({
            let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

            if let Some(name) = resolved_names.get(&id.0.start_offset()).cloned() {
                name
            } else {
                return Err(AnalysisError::InternalError(
                    "Cannot resolve name for function pointer".to_string(),
                    statements_analyzer.get_hpos(&id.0),
                ));
            }
        }),
        aast::FunctionPtrId::FPClassConst(class_id, method_name) => {
            let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;
            let calling_class = &context.function_context.calling_class;

            let class_name = match &class_id.2 {
                aast::ClassId_::CIexpr(inner_expr) => {
                    if let aast::Expr_::Id(id) = &inner_expr.2 {
                        if let Some(name) = get_id_name(
                            id,
                            &calling_class,
                            context.function_context.calling_class_final,
                            codebase,
                            &mut false,
                            resolved_names,
                        ) {
                            name
                        } else {
                            return Err(AnalysisError::InternalError(
                                "Cannot resolve function pointer class constant".to_string(),
                                statements_analyzer.get_hpos(&id.0),
                            ));
                        }
                    } else {
                        panic!("Unrecognised expression type for class constant reference");
                    }
                }
                _ => panic!("Unrecognised expression type for class constant reference"),
            };

            let method_name = statements_analyzer.get_interner().get(&method_name.1);

            if let Some(method_name) = method_name {
                FunctionLikeIdentifier::Method(class_name, method_name)
            } else {
                return Ok(());
            }
        }
    };

    match &id {
        FunctionLikeIdentifier::Function(name) => {
            analysis_data.symbol_references.add_reference_to_symbol(
                &context.function_context,
                name.clone(),
                false,
            );

            if !codebase
                .functionlike_infos
                .contains_key(&(*name, STR_EMPTY))
            {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::NonExistentFunction,
                        format!(
                            "Unknown function {}",
                            statements_analyzer.get_interner().lookup(name)
                        ),
                        statements_analyzer.get_hpos(&expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                return Ok(());
            }
        }
        FunctionLikeIdentifier::Method(class_name, method_name) => {
            analysis_data
                .symbol_references
                .add_reference_to_class_member(
                    &context.function_context,
                    (class_name.clone(), *method_name),
                    false,
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
                        analysis_data
                            .symbol_references
                            .add_reference_to_overridden_class_member(
                                &context.function_context,
                                (overridden_classlike.clone(), declaring_method_id.1),
                            );
                    }
                }
            } else {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::NonExistentClasslike,
                        format!(
                            "Unknown classlike {}",
                            statements_analyzer.get_interner().lookup(class_name)
                        ),
                        statements_analyzer.get_hpos(&expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                return Ok(());
            }
        }
    }

    if let Some(closure) = get_closure_from_id(
        &id,
        codebase,
        &Some(statements_analyzer.get_interner()),
        &mut analysis_data.data_flow_graph,
    ) {
        analysis_data.expr_types.insert(
            (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
            Rc::new(wrap_atomic(closure)),
        );
    }

    Ok(())
}

pub(crate) fn add_decision_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    lhs_expr: &aast::Expr<(), ()>,
    rhs_expr: Option<&aast::Expr<(), ()>>,
    expr_pos: &Pos,
    mut cond_type: TUnion,
) {
    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        return;
    }

    let decision_node = DataFlowNode::get_for_variable_sink(
        "is decision".to_string(),
        statements_analyzer.get_hpos(expr_pos),
    );

    if let Some(lhs_type) = analysis_data.expr_types.get(&(
        lhs_expr.1.start_offset() as u32,
        lhs_expr.1.end_offset() as u32,
    )) {
        cond_type.parent_nodes.insert(decision_node.clone());

        for old_parent_node in &lhs_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                old_parent_node,
                &decision_node,
                PathKind::Default,
                None,
                None,
            );
        }
    }

    if let Some(rhs_expr) = rhs_expr {
        if let Some(rhs_type) = analysis_data.expr_types.get(&(
            rhs_expr.1.start_offset() as u32,
            rhs_expr.1.end_offset() as u32,
        )) {
            cond_type.parent_nodes.insert(decision_node.clone());

            for old_parent_node in &rhs_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    old_parent_node,
                    &decision_node,
                    PathKind::Default,
                    None,
                    None,
                );
            }
        }
    }
    analysis_data.expr_types.insert(
        (expr_pos.start_offset() as u32, expr_pos.end_offset() as u32),
        Rc::new(cond_type),
    );
}
