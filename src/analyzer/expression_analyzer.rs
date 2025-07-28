use std::collections::BTreeMap;
use std::rc::Rc;

use crate::custom_hook::AfterExprAnalysisData;
use crate::expr::binop::concat_analyzer::analyze_concat_nodes;
use crate::expr::call::new_analyzer;
use crate::expr::fetch::{
    array_fetch_analyzer, class_constant_fetch_analyzer, instance_property_fetch_analyzer,
    static_property_fetch_analyzer,
};
use crate::expr::{
    as_analyzer, await_analyzer, binop_analyzer, call_analyzer, cast_analyzer, closure_analyzer,
    collection_analyzer, const_fetch_analyzer, expression_identifier, include_analyzer,
    pipe_analyzer, prefixed_string_analyzer, shape_analyzer, ternary_analyzer, tuple_analyzer,
    unop_analyzer, variable_fetch_analyzer, xml_analyzer, yield_analyzer,
};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::{var_has_root, BlockContext};
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{algebra_analyzer, expression_analyzer, formula_generator};
use hakana_algebra::clause::ClauseKey;
use hakana_algebra::Clause;
use hakana_code_info::ast::get_id_name;
use hakana_code_info::code_location::StmtStart;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::data_flow::node::DataFlowNode;
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::method_identifier::MethodIdentifier;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::type_expander::get_closure_from_id;
use hakana_code_info::ttype::{
    get_bool, get_false, get_float, get_int, get_literal_int, get_literal_string, get_mixed_any,
    get_null, get_true, wrap_atomic,
};
use hakana_code_info::var_name::VarName;
use hakana_reflector::simple_type_inferer::int_from_string;
use hakana_str::StrId;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
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
                Rc::new(if let Ok(value) = int_from_string(value) {
                    get_literal_int(value)
                } else {
                    // should never happen
                    get_int()
                }),
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

            expression_analyzer::analyze(statements_analyzer, lhs_expr, analysis_data, context)?;

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
                &boxed.expr,
                &boxed.hint,
                boxed.is_nullable,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::Call(boxed) => {
            call_analyzer::analyze(statements_analyzer, boxed, &expr.1, analysis_data, context)?;
        }
        aast::Expr_::ArrayGet(boxed) => {
            let keyed_array_var_id = expression_identifier::get_var_id(
                expr,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.file_analyzer.resolved_names,
                Some((statements_analyzer.codebase, statements_analyzer.interner)),
            );

            array_fetch_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, boxed.1.as_ref()),
                &expr.1,
                analysis_data,
                context,
                keyed_array_var_id.map(|t| VarName::new(t)),
            )?;
        }
        aast::Expr_::Eif(boxed) => {
            ternary_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2),
                &expr.1,
                analysis_data,
                context,
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
            )?;
        }
        aast::Expr_::ObjGet(boxed) => {
            let (lhs_expr, rhs_expr, nullfetch, prop_or_method) =
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

            match prop_or_method {
                ast_defs::PropOrMethod::IsProp => {
                    instance_property_fetch_analyzer::analyze(
                        statements_analyzer,
                        (lhs_expr, rhs_expr),
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
            }
        }
        aast::Expr_::New(boxed) => {
            new_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3),
                &expr.1,
                analysis_data,
                context,
            )?;
        }
        aast::Expr_::ClassGet(boxed) => {
            let (lhs, rhs, prop_or_method) = (&boxed.0, &boxed.1, &boxed.2);

            match prop_or_method {
                ast_defs::PropOrMethod::IsProp => {
                    static_property_fetch_analyzer::analyze(
                        statements_analyzer,
                        (lhs, rhs),
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
            )?;
        }
        aast::Expr_::Clone(boxed) => {
            expression_analyzer::analyze(statements_analyzer, boxed, analysis_data, context)?;

            if let Some(stmt_type) = analysis_data
                .expr_types
                .get(&(
                    boxed.pos().start_offset() as u32,
                    boxed.pos().end_offset() as u32,
                ))
                .cloned()
            {
                let mut stmt_type = (*stmt_type).clone();
                stmt_type.reference_free = true;
                analysis_data.expr_types.insert(
                    (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                    Rc::new(stmt_type),
                );
            }
        }
        aast::Expr_::String2(exprs) => {
            for concat_node in exprs {
                expression_analyzer::analyze(
                    statements_analyzer,
                    concat_node,
                    analysis_data,
                    context,
                )?;
            }

            let result_type = analyze_concat_nodes(
                exprs.iter().collect(),
                statements_analyzer,
                analysis_data,
                expr.pos(),
            );

            analysis_data.expr_types.insert(
                (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
                Rc::new(result_type),
            );
        }
        aast::Expr_::PrefixedString(boxed) => {
            prefixed_string_analyzer::analyze(
                statements_analyzer,
                boxed,
                analysis_data,
                context,
                expr,
            )?;
        }
        aast::Expr_::Id(boxed) => {
            const_fetch_analyzer::analyze(statements_analyzer, boxed, analysis_data, context)?;
        }
        aast::Expr_::Xml(boxed) => {
            xml_analyzer::analyze(
                context,
                boxed,
                expr.pos(),
                statements_analyzer,
                analysis_data,
            )?;
        }
        aast::Expr_::Await(boxed) => {
            await_analyzer::analyze(statements_analyzer, expr, boxed, analysis_data, context)?;
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
            );
        }
        aast::Expr_::Yield(boxed) => {
            yield_analyzer::analyze(&expr.1, boxed, statements_analyzer, analysis_data, context)?;
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
                let resolved_names = statements_analyzer.file_analyzer.resolved_names;

                Some(
                    resolved_names
                        .get(&(id.0.start_offset() as u32))
                        .cloned()
                        .unwrap(),
                )
            } else {
                None
            };
            if let Some(member_name) = statements_analyzer.interner.get(&boxed.1) {
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
        aast::Expr_::Assign(boxed) => {
            crate::expr::binop::assignment_analyzer::analyze(
                statements_analyzer,
                (&boxed.0, boxed.1, Some(&boxed.2)),
                expr.pos(),
                None,
                analysis_data,
                context,
                None,
            )?;
        }
    }

    let newly_called = analysis_data.after_expr_hook_called.insert((
        expr.pos().start_offset() as u32,
        expr.pos().end_offset() as u32,
    ));

    for hook in &statements_analyzer.get_config().hooks {
        hook.after_expr_analysis(
            analysis_data,
            AfterExprAnalysisData {
                statements_analyzer,
                expr,
                context,
                already_called: !newly_called,
            },
        );
    }

    analysis_data.applicable_fixme_start = expr.pos().end_offset() as u32;

    Ok(())
}

pub(crate) fn expr_has_logic(expr: &aast::Expr<(), ()>) -> bool {
    match &expr.2 {
        aast::Expr_::Binop(boxed) => matches!(
            boxed.bop,
            oxidized::nast::Bop::Eqeq
                | oxidized::nast::Bop::Eqeqeq
                | oxidized::nast::Bop::Diff
                | oxidized::nast::Bop::Diff2
                | oxidized::nast::Bop::Ampamp
                | oxidized::nast::Bop::Barbar
                | oxidized::nast::Bop::QuestionQuestion
        ),
        aast::Expr_::Is(_) => true,
        _ => false,
    }
}

pub(crate) fn find_expr_logic_issues(
    statements_analyzer: &StatementsAnalyzer,
    context: &BlockContext,
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

    for (var_id, var_type) in &context.locals {
        if var_type.is_mixed() && context.locals.contains_key(var_id) {
            mixed_var_ids.push(var_id);
        }
    }

    expr_clauses = expr_clauses
        .into_iter()
        .map(|c| {
            let mut keys = vec![];
            for k in c.possibilities.keys() {
                if let ClauseKey::Name(var_name) = k {
                    keys.push(var_name);
                }
            }

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

            c
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
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    expr: &aast::Expr<(), ()>,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;
    let id = match &boxed.0 {
        aast::FunctionPtrId::FPId(id) => FunctionLikeIdentifier::Function({
            let resolved_names = statements_analyzer.file_analyzer.resolved_names;

            if let Some(name) = resolved_names.get(&(id.0.start_offset() as u32)).cloned() {
                name
            } else {
                return Err(AnalysisError::InternalError(
                    "Cannot resolve name for function pointer".to_string(),
                    statements_analyzer.get_hpos(&id.0),
                ));
            }
        }),
        aast::FunctionPtrId::FPClassConst(class_id, method_name) => {
            let resolved_names = statements_analyzer.file_analyzer.resolved_names;
            let calling_class = &context.function_context.calling_class;

            let class_name = match &class_id.2 {
                aast::ClassId_::CIexpr(inner_expr) => {
                    if let aast::Expr_::Id(id) = &inner_expr.2 {
                        if let Some(name) = get_id_name(
                            id,
                            calling_class,
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

            let method_name = statements_analyzer.interner.get(&method_name.1);

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
                *name,
                false,
            );

            if !codebase
                .functionlike_infos
                .contains_key(&(*name, StrId::EMPTY))
            {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::NonExistentFunction,
                        format!(
                            "Unknown function {}",
                            statements_analyzer.interner.lookup(name)
                        ),
                        statements_analyzer.get_hpos(expr.pos()),
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
                    (*class_name, *method_name),
                    false,
                );

            if let Some(classlike_storage) = codebase.classlike_infos.get(class_name) {
                let declaring_method_id =
                    codebase.get_declaring_method_id(&MethodIdentifier(*class_name, *method_name));

                if let Some(overridden_classlikes) = classlike_storage
                    .overridden_method_ids
                    .get(&declaring_method_id.1)
                {
                    for overridden_classlike in overridden_classlikes {
                        analysis_data
                            .symbol_references
                            .add_reference_to_overridden_class_member(
                                &context.function_context,
                                (*overridden_classlike, declaring_method_id.1),
                            );
                    }
                }
            } else {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::NonExistentClasslike,
                        format!(
                            "Unknown classlike {}",
                            statements_analyzer.interner.lookup(class_name)
                        ),
                        statements_analyzer.get_hpos(expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                return Ok(());
            }
        }
        _ => {
            panic!()
        }
    }

    if let Some(closure) = get_closure_from_id(
        &id,
        codebase,
        &Some(statements_analyzer.interner),
        statements_analyzer.get_file_path(),
        &mut analysis_data.data_flow_graph,
        &mut 0,
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

    let decision_node =
        DataFlowNode::get_for_unlabelled_sink(statements_analyzer.get_hpos(expr_pos));

    if let Some(lhs_type) = analysis_data.expr_types.get(&(
        lhs_expr.1.start_offset() as u32,
        lhs_expr.1.end_offset() as u32,
    )) {
        cond_type.parent_nodes.push(decision_node.clone());

        for old_parent_node in &lhs_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &old_parent_node.id,
                &decision_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
    }

    if let Some(rhs_expr) = rhs_expr {
        if let Some(rhs_type) = analysis_data.expr_types.get(&(
            rhs_expr.1.start_offset() as u32,
            rhs_expr.1.end_offset() as u32,
        )) {
            cond_type.parent_nodes.push(decision_node.clone());

            for old_parent_node in &rhs_type.parent_nodes {
                analysis_data.data_flow_graph.add_path(
                    &old_parent_node.id,
                    &decision_node.id,
                    PathKind::Default,
                    vec![],
                    vec![],
                );
            }
        }
    }
    analysis_data.expr_types.insert(
        (expr_pos.start_offset() as u32, expr_pos.end_offset() as u32),
        Rc::new(cond_type),
    );

    analysis_data.data_flow_graph.add_node(decision_node);
}
