use super::expression_identifier::{get_dim_id, get_var_id};
use crate::expr::expression_identifier::get_static_functionlike_id_from_call;
use crate::{formula_generator::AssertionContext, function_analysis_data::FunctionAnalysisData};
use hakana_code_info::code_location::HPos;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::symbol_references::ReferenceSource;
use hakana_code_info::t_atomic::DictKey;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::comparison::type_comparison_result::TypeComparisonResult;
use hakana_code_info::ttype::comparison::union_type_comparator;
use hakana_code_info::ttype::type_expander::{self, TypeExpansionOptions};
use hakana_code_info::{
    assertion::Assertion,
    data_flow::graph::{DataFlowGraph, GraphKind},
    t_atomic::TAtomic,
    t_union::populate_union_type,
};
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_str::StrId;
use oxidized::{
    aast,
    aast_defs::Hint,
    ast_defs::{self, Pos},
};
use rustc_hash::FxHashMap;

pub(crate) enum OtherValuePosition {
    Left,
    Right,
}

/**
 * @internal
 * This class transform conditions in code into "assertions" that will be reconciled with the type already known of a
 * given variable to narrow the type or find paradox.
 * For example if $a is an int, if($a > 0) will be turned into an assertion to make Hakana understand that in the
 * if block, $a is a positive-int
 */
pub(crate) fn scrape_assertions(
    conditional: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    assertion_context: &AssertionContext,
    inside_negation: bool,
    cache: bool,
    inside_conditional: bool,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types = FxHashMap::default();

    match &conditional.2 {
        // matches if ($foo is Bar)
        aast::Expr_::Is(is_expr) => {
            return get_is_assertions(
                &is_expr.0,
                &is_expr.1,
                assertion_context,
                analysis_data,
                inside_negation,
            );
        }
        aast::Expr_::Call(call) => {
            let functionlike_id = if let Some((_, interner)) = assertion_context.codebase {
                get_static_functionlike_id_from_call(
                    call,
                    interner,
                    assertion_context.resolved_names,
                )
            } else {
                None
            };

            if let Some(FunctionLikeIdentifier::Function(name)) = functionlike_id {
                return scrape_function_assertions(
                    &name,
                    &call.args,
                    &conditional.1,
                    assertion_context,
                    analysis_data,
                    inside_negation,
                );
            }

            if_types.extend(process_custom_assertions(conditional.pos(), analysis_data));
        }
        _ => {}
    }

    let var_name = get_var_id(
        conditional,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );

    // matches if ($foo) {}
    if let Some(var_name) = var_name {
        if_types.insert(var_name, vec![vec![Assertion::Truthy]]);
    }

    if let aast::Expr_::Unop(unup) = &conditional.2 {
        // if (!$foo) is handled elsewhere
        if let oxidized::ast::Uop::Unot = unup.0 {
            return Vec::new();
        }
    }

    if let aast::Expr_::Binop(binop) = &conditional.2 {
        match binop.bop {
            ast_defs::Bop::Eqeq | ast_defs::Bop::Eqeqeq => {
                return scrape_equality_assertions(
                    &binop.bop,
                    &binop.lhs,
                    &binop.rhs,
                    analysis_data,
                    assertion_context,
                    cache,
                    inside_conditional,
                );
            }
            ast_defs::Bop::Diff | ast_defs::Bop::Diff2 => {
                return scrape_inequality_assertions(
                    &binop.bop,
                    &binop.lhs,
                    &binop.rhs,
                    analysis_data,
                    assertion_context,
                    cache,
                    inside_conditional,
                );
            }
            ast_defs::Bop::QuestionQuestion => {
                if let aast::Expr_::False | aast::Expr_::Null = &binop.rhs.2 {
                    let var_name = get_var_id(
                        &binop.lhs,
                        assertion_context.this_class_name,
                        assertion_context.resolved_names,
                        assertion_context.codebase,
                    );

                    if let Some(var_name) = var_name {
                        if_types.insert(var_name, vec![vec![Assertion::IsIsset]]);
                    }
                }
            }
            ast_defs::Bop::Gt | ast_defs::Bop::Gte => {
                // return scrape_greater_assertions(
                //     &binop.1,
                //     &binop.2,
                //     this_class_name,
                //     source,
                //     &analysis_data,
                //     resolved_names,
                // );
            }
            _ => {}
        }
    }

    vec![if_types]
}

fn process_custom_assertions(
    conditional_pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> FxHashMap<String, Vec<Vec<Assertion>>> {
    let mut if_true_assertions = analysis_data
        .if_true_assertions
        .get(&(
            conditional_pos.start_offset() as u32,
            conditional_pos.end_offset() as u32,
        ))
        .cloned()
        .unwrap_or(FxHashMap::default());

    let if_false_assertions = analysis_data
        .if_false_assertions
        .get(&(
            conditional_pos.start_offset() as u32,
            conditional_pos.end_offset() as u32,
        ))
        .cloned()
        .unwrap_or(FxHashMap::default());

    if if_true_assertions.is_empty() && if_false_assertions.is_empty() {
        return FxHashMap::default();
    }

    for if_false_assertion in if_false_assertions {
        if_true_assertions
            .entry(if_false_assertion.0)
            .or_insert_with(Vec::new)
            .extend(
                if_false_assertion
                    .1
                    .into_iter()
                    .map(|a| a.get_negation())
                    .collect::<Vec<_>>(),
            );
    }

    if_true_assertions
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().map(|v| vec![v]).collect()))
        .collect()
}

fn get_is_assertions(
    var_expr: &aast::Expr<(), ()>,
    hint: &Hint,
    assertion_context: &AssertionContext,
    analysis_data: &mut FunctionAnalysisData,
    _inside_negation: bool,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types: FxHashMap<String, Vec<Vec<Assertion>>> = FxHashMap::default();

    let mut is_type = if let Some(t) = get_type_from_hint(
        &hint.1,
        assertion_context.this_class_name,
        assertion_context.type_resolution_context,
        assertion_context.resolved_names,
        assertion_context.file_source.file_path,
        hint.0.start_offset() as u32,
    ) {
        t
    } else {
        return vec![];
    };

    if let Some((codebase, _)) = assertion_context.codebase {
        populate_union_type(
            &mut is_type,
            &codebase.symbols,
            &assertion_context.reference_source,
            &mut analysis_data.symbol_references,
            false,
        );
        type_expander::expand_union(
            codebase,
            &None,
            &assertion_context.file_source.file_path,
            &mut is_type,
            &TypeExpansionOptions {
                self_class: assertion_context.this_class_name,
                expand_hakana_types: false,
                ..Default::default()
            },
            &mut DataFlowGraph::new(GraphKind::FunctionBody),
            &mut 0,
        );
    }

    let var_name = get_var_id(
        var_expr,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );

    if let Some(var_name) = var_name {
        if_types.insert(
            var_name,
            vec![
                is_type
                    .types
                    .into_iter()
                    .map(Assertion::IsType)
                    .collect::<Vec<Assertion>>(),
            ],
        );
    } else {
        match is_type.get_single() {
            TAtomic::TMixedWithFlags(_, _, _, true) => {
                scrape_shapes_isset(var_expr, assertion_context, &mut if_types, false);
            }
            TAtomic::TNull => {
                scrape_shapes_isset(var_expr, assertion_context, &mut if_types, true);
            }
            _ => {}
        }

        if let (Some(lhs_type), Some((codebase, interner))) = (
            analysis_data.expr_types.get(&(
                var_expr.1.start_offset() as u32,
                var_expr.1.end_offset() as u32,
            )),
            assertion_context.codebase,
        ) {
            if !union_type_comparator::can_expression_types_be_identical(
                codebase,
                &assertion_context.file_source.file_path,
                lhs_type,
                &is_type,
                true,
            ) {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::ImpossibleTypeComparison,
                        format!(
                            "Type {} is never {}",
                            lhs_type.get_id(Some(interner)),
                            is_type.get_id(Some(interner)),
                        ),
                        HPos::new(var_expr.pos(), assertion_context.file_source.file_path),
                        &Some(match assertion_context.reference_source {
                            ReferenceSource::Symbol(_, fn_id) => {
                                FunctionLikeIdentifier::Function(fn_id)
                            }
                            ReferenceSource::ClasslikeMember(_, a, b) => {
                                FunctionLikeIdentifier::Method(a, b)
                            }
                        }),
                    ),
                    assertion_context.config,
                    &assertion_context.file_source.file_path_actual,
                );
            } else if union_type_comparator::is_contained_by(
                codebase,
                &assertion_context.file_source.file_path,
                lhs_type,
                &is_type,
                false,
                false,
                true,
                &mut TypeComparisonResult::new(),
            ) {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::RedundantTypeComparison,
                        format!(
                            "Type {} is always {}",
                            lhs_type.get_id(Some(interner)),
                            is_type.get_id(Some(interner)),
                        ),
                        HPos::new(var_expr.pos(), assertion_context.file_source.file_path),
                        &Some(match assertion_context.reference_source {
                            ReferenceSource::Symbol(_, fn_id) => {
                                FunctionLikeIdentifier::Function(fn_id)
                            }
                            ReferenceSource::ClasslikeMember(_, a, b) => {
                                FunctionLikeIdentifier::Method(a, b)
                            }
                        }),
                    ),
                    assertion_context.config,
                    &assertion_context.file_source.file_path_actual,
                );
            }
        }
    }

    vec![if_types]
}

fn scrape_shapes_isset(
    var_expr: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    if_types: &mut FxHashMap<String, Vec<Vec<Assertion>>>,
    negated: bool,
) {
    if let aast::Expr_::Call(call) = &var_expr.2 {
        let functionlike_id = if let Some((_, interner)) = assertion_context.codebase {
            get_static_functionlike_id_from_call(call, interner, assertion_context.resolved_names)
        } else {
            None
        };

        if let Some(FunctionLikeIdentifier::Method(class_name, member_name)) = functionlike_id {
            if let Some((codebase, interner)) = assertion_context.codebase {
                if class_name == StrId::SHAPES && member_name == StrId::IDX {
                    let shape_name = get_var_id(
                        &call.args[0].to_expr_ref(),
                        assertion_context.this_class_name,
                        assertion_context.resolved_names,
                        assertion_context.codebase,
                    );

                    let dim_id = get_dim_id(
                        &call.args[1].to_expr_ref(),
                        Some((codebase, interner)),
                        assertion_context.resolved_names,
                    );

                    if let (Some(shape_name), Some(dim_id)) = (shape_name, dim_id) {
                        let dict_key = if dim_id.starts_with('\'') {
                            DictKey::String(dim_id[1..(dim_id.len() - 1)].to_string())
                        } else if let Ok(arraykey_value) = dim_id.parse::<u64>() {
                            DictKey::Int(arraykey_value)
                        } else {
                            panic!("bad int key {}", dim_id);
                        };
                        if_types.insert(
                            shape_name,
                            vec![vec![if negated {
                                Assertion::DoesNotHaveNonnullEntryForKey(dict_key)
                            } else {
                                Assertion::HasNonnullEntryForKey(dict_key)
                            }]],
                        );
                    }
                }
            }
        }
    }
}

fn scrape_equality_assertions(
    bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    assertion_context: &AssertionContext,
    _cache: bool,
    _inside_conditional: bool,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let null_position = has_null_variable(bop, left, right);

    if let Some(null_position) = null_position {
        return get_null_equality_assertions(bop, left, right, assertion_context, null_position);
    }

    let true_position = has_true_variable(bop, left, right);

    if let Some(true_position) = true_position {
        return get_true_equality_assertions(bop, left, right, assertion_context, true_position);
    }

    let false_position = has_false_variable(left, right);

    if let Some(false_position) = false_position {
        return get_false_equality_assertions(bop, left, right, assertion_context, false_position);
    }

    if let Some(typed_value_position) =
        has_typed_value_comparison(left, right, analysis_data, assertion_context)
    {
        return get_typed_value_equality_assertions(
            left,
            right,
            analysis_data,
            assertion_context,
            typed_value_position,
        );
    }

    Vec::new()
}

fn scrape_inequality_assertions(
    bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    analysis_data: &FunctionAnalysisData,
    assertion_context: &AssertionContext,
    _cache: bool,
    _inside_conditional: bool,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let null_position = has_null_variable(bop, left, right);

    if let Some(null_position) = null_position {
        return get_null_inequality_assertions(bop, left, right, assertion_context, null_position);
    }

    // let true_position = has_true_variable(bop, left, right, source);

    // if let Some(true_position) = true_position {}

    // let false_position = has_false_variable(left, right);

    // if let Some(false_position) = false_position {}

    if let Some(typed_value_position) =
        has_typed_value_comparison(left, right, analysis_data, assertion_context)
    {
        return get_typed_value_inequality_assertions(
            left,
            right,
            analysis_data,
            assertion_context,
            typed_value_position,
        );
    }

    Vec::new()
}

// fn has_literal_int_comparison(
//     left: &aast::Expr<(), ()>,
//     right: &aast::Expr<(), ()>,
//     analysis_data: &TastInfo,
// ) -> Option<(OtherValuePosition, i64)> {
//     if let Some(right_type) = analysis_data.get_expr_type(right.pos()) {
//         if let Some(value) = right_type.get_single_literal_int_value() {
//             return Some((OtherValuePosition::Right, value));
//         }
//     }

//     if let Some(left_type) = analysis_data.get_expr_type(left.pos()) {
//         if let Some(value) = left_type.get_single_literal_int_value() {
//             return Some((OtherValuePosition::Left, value));
//         }
//     }

//     None
// }

fn scrape_function_assertions(
    function_name: &StrId,
    args: &[aast::Argument<(), ()>],
    pos: &Pos,
    assertion_context: &AssertionContext,
    analysis_data: &mut FunctionAnalysisData,
    _negate: bool,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let firsts = if let Some(first_arg) = args.first() {
        let expr = first_arg.to_expr_ref();
        let first_var_name = get_var_id(
            expr,
            assertion_context.this_class_name,
            assertion_context.resolved_names,
            assertion_context.codebase,
        );
        let first_var_type = analysis_data.get_expr_type(expr.pos());
        Some((expr, first_var_name, first_var_type))
    } else {
        None
    };

    let mut if_types = FxHashMap::default();

    if function_name == &StrId::ISSET {
        let (first_arg, first_var_name, first_var_type) = firsts.unwrap();
        if let Some(first_var_name) = first_var_name {
            if let Some(first_var_type) = first_var_type {
                if matches!(first_arg, aast::Expr((), _, aast::Expr_::Lvar(_)))
                    && &first_arg.1 != pos
                    && !first_var_type.is_mixed()
                    && !first_var_type.possibly_undefined_from_try
                {
                    if_types.insert(
                        first_var_name.clone(),
                        vec![vec![Assertion::IsNotType(TAtomic::TNull)]],
                    );
                } else {
                    if_types.insert(first_var_name.clone(), vec![vec![Assertion::IsIsset]]);
                }
            }
        }
    } else if function_name == &StrId::IN_ARRAY {
        let (first_arg, first_var_name, _) = firsts.unwrap();
        if let (Some(first_var_name), Some(second_arg)) = (first_var_name, args.get(1)) {
            if let aast::Expr_::ValCollection(vals) = &second_arg.to_expr_ref().2 {
                let mut in_arr_types = vec![];
                let mut has_reconcilable_type = true;
                for val_expr in &vals.2 {
                    let val_expr_type = analysis_data.get_expr_type(val_expr.pos());

                    if let Some(val_expr_type) = val_expr_type {
                        in_arr_types.extend(val_expr_type.types.clone());
                    } else {
                        has_reconcilable_type = false;
                    }
                }

                if has_reconcilable_type {
                    let union = TUnion::new(in_arr_types);
                    if matches!(first_arg, aast::Expr((), _, aast::Expr_::Lvar(_))) {
                        if_types.insert(
                            first_var_name.clone(),
                            vec![vec![Assertion::InArray(union)]],
                        );
                    }
                }
            }
        }
    }

    let custom_assertions = process_custom_assertions(pos, analysis_data);

    if_types.extend(custom_assertions);

    vec![if_types]
}

fn has_null_variable(
    _bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
) -> Option<OtherValuePosition> {
    if let aast::Expr_::Null = right.2 {
        return Some(OtherValuePosition::Right);
    }

    if let aast::Expr_::Null = left.2 {
        return Some(OtherValuePosition::Left);
    }

    None
}

fn get_null_equality_assertions(
    _bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    null_position: OtherValuePosition,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types = FxHashMap::default();
    let base_conditional = match null_position {
        OtherValuePosition::Left => right,
        OtherValuePosition::Right => left,
    };

    let var_name = get_var_id(
        base_conditional,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );

    if let Some(var_name) = var_name {
        if_types.insert(var_name, vec![vec![Assertion::IsType(TAtomic::TNull)]]);
    } else {
        scrape_shapes_isset(base_conditional, assertion_context, &mut if_types, true);
    }

    vec![if_types]
}

fn get_null_inequality_assertions(
    _bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    null_position: OtherValuePosition,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types = FxHashMap::default();
    let base_conditional = match null_position {
        OtherValuePosition::Left => right,
        OtherValuePosition::Right => left,
    };

    let var_name = get_var_id(
        base_conditional,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );

    if let Some(var_name) = var_name {
        if_types.insert(var_name, vec![vec![Assertion::IsNotType(TAtomic::TNull)]]);
    } else {
        scrape_shapes_isset(base_conditional, assertion_context, &mut if_types, false);
    }

    vec![if_types]
}

pub(crate) fn has_true_variable(
    _bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
) -> Option<OtherValuePosition> {
    if let aast::Expr_::True = right.2 {
        return Some(OtherValuePosition::Right);
    }

    if let aast::Expr_::True = left.2 {
        return Some(OtherValuePosition::Left);
    }

    None
}

fn get_true_equality_assertions(
    _bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    true_position: OtherValuePosition,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types = FxHashMap::default();
    let base_conditional = match true_position {
        OtherValuePosition::Left => right,
        OtherValuePosition::Right => left,
    };

    let var_name = get_var_id(
        base_conditional,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );

    if let Some(var_name) = var_name {
        if_types.insert(var_name, vec![vec![Assertion::IsType(TAtomic::TTrue)]]);
        return vec![if_types];
    }

    Vec::new()
}

pub(crate) fn has_false_variable(
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
) -> Option<OtherValuePosition> {
    if let aast::Expr_::False = right.2 {
        return Some(OtherValuePosition::Right);
    }

    if let aast::Expr_::False = left.2 {
        return Some(OtherValuePosition::Left);
    }

    None
}

pub(crate) fn has_typed_value_comparison(
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    analysis_data: &FunctionAnalysisData,
    assertion_context: &AssertionContext,
) -> Option<OtherValuePosition> {
    let left_var_id = get_var_id(
        left,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );
    let right_var_id = get_var_id(
        right,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );

    if let Some(right_type) = analysis_data.get_expr_type(right.pos()) {
        if (left_var_id.is_some() || right_var_id.is_none())
            && right_type.is_single()
            && !right_type.is_mixed()
        {
            return Some(OtherValuePosition::Right);
        }
    }

    if let Some(left_type) = analysis_data.get_expr_type(left.pos()) {
        if left_var_id.is_none() && left_type.is_single() && !left_type.is_mixed() {
            return Some(OtherValuePosition::Left);
        }
    }
    None
}

fn get_false_equality_assertions(
    _bop: &ast_defs::Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    false_position: OtherValuePosition,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types = FxHashMap::default();
    let base_conditional = match false_position {
        OtherValuePosition::Left => right,
        OtherValuePosition::Right => left,
    };

    let var_name = get_var_id(
        base_conditional,
        assertion_context.this_class_name,
        assertion_context.resolved_names,
        assertion_context.codebase,
    );

    if let Some(var_name) = var_name {
        if_types.insert(var_name, vec![vec![Assertion::IsType(TAtomic::TFalse)]]);
        return vec![if_types];
    }

    Vec::new()
}

fn get_typed_value_equality_assertions(
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    analysis_data: &FunctionAnalysisData,
    assertion_context: &AssertionContext,
    typed_value_position: OtherValuePosition,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types = FxHashMap::default();

    let var_name;
    let other_value_var_name;
    let var_type;
    let other_value_type;

    match typed_value_position {
        OtherValuePosition::Right => {
            var_name = get_var_id(
                left,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );
            other_value_var_name = get_var_id(
                right,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );

            var_type = analysis_data.get_expr_type(left.pos());
            other_value_type = analysis_data.get_expr_type(right.pos());
        }
        OtherValuePosition::Left => {
            var_name = get_var_id(
                right,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );
            other_value_var_name = get_var_id(
                left,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );

            var_type = analysis_data.get_expr_type(right.pos());
            other_value_type = analysis_data.get_expr_type(left.pos());
        }
    }

    if let Some(var_name) = var_name {
        if let Some(other_value_type) = other_value_type {
            if other_value_type.is_single() {
                let orred_types = vec![Assertion::IsEqual(other_value_type.get_single().clone())];

                if_types.insert(var_name, vec![orred_types]);
            }

            if let Some(other_value_var_name) = other_value_var_name {
                if let Some(var_type) = var_type {
                    if !var_type.is_mixed() && var_type.is_single() {
                        let orred_types = vec![Assertion::IsEqual(var_type.get_single().clone())];

                        if_types.insert(other_value_var_name, vec![orred_types]);
                    }
                }
            }
        }
    }

    // todo handle paradoxical equality

    if !if_types.is_empty() {
        vec![if_types]
    } else {
        vec![]
    }
}

fn get_typed_value_inequality_assertions(
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    analysis_data: &FunctionAnalysisData,
    assertion_context: &AssertionContext,
    typed_value_position: OtherValuePosition,
) -> Vec<FxHashMap<String, Vec<Vec<Assertion>>>> {
    let mut if_types = FxHashMap::default();

    let var_name;
    let other_value_var_name;
    let other_value_type;
    let var_type;

    match typed_value_position {
        OtherValuePosition::Right => {
            var_name = get_var_id(
                left,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );
            other_value_var_name = get_var_id(
                right,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );

            var_type = analysis_data.get_expr_type(left.pos());
            other_value_type = analysis_data.get_expr_type(right.pos());
        }
        OtherValuePosition::Left => {
            var_name = get_var_id(
                right,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );
            other_value_var_name = get_var_id(
                left,
                assertion_context.this_class_name,
                assertion_context.resolved_names,
                assertion_context.codebase,
            );

            var_type = analysis_data.get_expr_type(right.pos());
            other_value_type = analysis_data.get_expr_type(left.pos());
        }
    }

    if let Some(var_name) = var_name {
        if let Some(other_value_type) = other_value_type {
            if other_value_type.is_single() {
                let orred_types =
                    vec![Assertion::IsNotEqual(other_value_type.get_single().clone())];

                if_types.insert(var_name, vec![orred_types]);
            }

            if let Some(other_value_var_name) = other_value_var_name {
                if let Some(var_type) = var_type {
                    if !var_type.is_mixed() && var_type.is_single() {
                        let orred_types =
                            vec![Assertion::IsNotEqual(var_type.get_single().clone())];

                        if_types.insert(other_value_var_name, vec![orred_types]);
                    }
                }
            }
        }
    }

    // todo handle paradoxical equality

    if !if_types.is_empty() {
        vec![if_types]
    } else {
        vec![]
    }
}
