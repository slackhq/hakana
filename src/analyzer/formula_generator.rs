use std::collections::BTreeMap;

use hakana_reflection_info::symbol_references::ReferenceSource;
use indexmap::IndexMap;
use rustc_hash::FxHashMap;

use hakana_algebra::Clause;
use hakana_reflection_info::{
    assertion::Assertion, codebase_info::CodebaseInfo, type_resolution::TypeResolutionContext,
};
use hakana_reflection_info::{FileSource, Interner, StrId};
use oxidized::{
    aast,
    ast::{Bop, Uop},
};

use crate::config::Config;
use crate::{expr::assertion_finder, function_analysis_data::FunctionAnalysisData};

pub(crate) struct AssertionContext<'a> {
    pub file_source: &'a FileSource<'a>,
    pub resolved_names: &'a FxHashMap<usize, StrId>,
    pub codebase: Option<(&'a CodebaseInfo, &'a Interner)>,
    pub this_class_name: Option<&'a StrId>,
    pub type_resolution_context: &'a TypeResolutionContext,
    pub config: &'a Config,
    pub reference_source: ReferenceSource,
}

pub(crate) fn get_formula(
    conditional_object_id: (usize, usize),
    creating_object_id: (usize, usize),
    conditional: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    analysis_data: &mut FunctionAnalysisData,
    cache: bool,
    inside_negation: bool,
) -> Result<Vec<Clause>, String> {
    if let aast::Expr_::Binop(expr) = &conditional.2 {
        if let Some(clauses) = handle_binop(
            conditional_object_id,
            &expr.bop,
            &expr.lhs,
            &expr.rhs,
            assertion_context,
            analysis_data,
            cache,
            inside_negation,
        ) {
            return clauses;
        }
    }

    if let aast::Expr_::Unop(expr) = &conditional.2 {
        if let Some(clauses) = handle_uop(
            conditional_object_id,
            &expr.0,
            &expr.1,
            assertion_context,
            analysis_data,
            cache,
            inside_negation,
        ) {
            return clauses;
        }
    }

    let anded_assertions = assertion_finder::scrape_assertions(
        conditional,
        analysis_data,
        &assertion_context,
        inside_negation,
        cache,
        true,
    );

    let mut clauses = Vec::new();

    for assertions in anded_assertions {
        for (var_id, anded_types) in assertions {
            for orred_types in anded_types {
                let has_equality = orred_types.get(0).unwrap().has_equality();
                clauses.push(Clause::new(
                    {
                        let mut map = BTreeMap::new();
                        map.insert(
                            var_id.clone(),
                            orred_types
                                .into_iter()
                                .map(|a| (a.to_hash(), a))
                                .collect::<IndexMap<_, _>>(),
                        );
                        map
                    },
                    conditional_object_id,
                    creating_object_id,
                    Some(false),
                    Some(true),
                    Some(has_equality),
                ))
            }
        }
    }

    if !clauses.is_empty() {
        return Ok(clauses);
    }

    let mut conditional_ref = String::new();
    conditional_ref += "*";
    conditional_ref += conditional.1.start_offset().to_string().as_str();
    conditional_ref += "-";
    conditional_ref += conditional.1.end_offset().to_string().as_str();

    Ok(vec![Clause::new(
        {
            let mut map = BTreeMap::new();
            map.insert(
                conditional_ref,
                IndexMap::from([(Assertion::Truthy.to_hash(), Assertion::Truthy)]),
            );
            map
        },
        conditional_object_id,
        creating_object_id,
        None,
        None,
        None,
    )])
}

#[inline]
fn handle_binop(
    conditional_object_id: (usize, usize),
    bop: &Bop,
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    analysis_data: &mut FunctionAnalysisData,
    cache: bool,
    inside_negation: bool,
) -> Option<Result<Vec<Clause>, String>> {
    if let oxidized::ast::Bop::Ampamp = bop {
        return Some(handle_and(
            conditional_object_id,
            left,
            right,
            assertion_context,
            analysis_data,
            cache,
            inside_negation,
        ));
    }

    if let oxidized::ast::Bop::Barbar = bop {
        return Some(handle_or(
            conditional_object_id,
            left,
            right,
            assertion_context,
            analysis_data,
            cache,
            inside_negation,
        ));
    }

    // TODO: shortcuts for
    // if (($a || $b) === false) {}
    // if (($a || $b) !== false) {}
    // if (!$a === true) {}
    // if (!$a === false) {}
    // OR we just remove that pattern with a lint (because it's redundant)

    None
}

#[inline]
fn handle_or(
    conditional_object_id: (usize, usize),
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    analysis_data: &mut FunctionAnalysisData,
    cache: bool,
    inside_negation: bool,
) -> Result<Vec<Clause>, String> {
    let left_clauses = get_formula(
        conditional_object_id,
        (left.pos().start_offset(), left.pos().end_offset()),
        left,
        assertion_context,
        analysis_data,
        cache,
        inside_negation,
    );

    if let Err(_) = left_clauses {
        return left_clauses;
    }

    let right_clauses = get_formula(
        conditional_object_id,
        (right.pos().start_offset(), right.pos().end_offset()),
        right,
        assertion_context,
        analysis_data,
        cache,
        inside_negation,
    );

    if let Err(_) = right_clauses {
        return right_clauses;
    }

    hakana_algebra::combine_ored_clauses(
        left_clauses.unwrap(),
        right_clauses.unwrap(),
        conditional_object_id,
    )
}

#[inline]
fn handle_and(
    conditional_object_id: (usize, usize),
    left: &aast::Expr<(), ()>,
    right: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    analysis_data: &mut FunctionAnalysisData,
    cache: bool,
    inside_negation: bool,
) -> Result<Vec<Clause>, String> {
    let left_clauses = get_formula(
        conditional_object_id,
        (left.pos().start_offset(), left.pos().end_offset()),
        left,
        assertion_context,
        analysis_data,
        cache,
        inside_negation,
    );

    if let Err(_) = left_clauses {
        return left_clauses;
    }

    let right_clauses = get_formula(
        conditional_object_id,
        (right.pos().start_offset(), right.pos().end_offset()),
        right,
        assertion_context,
        analysis_data,
        cache,
        inside_negation,
    );

    if let Err(_) = right_clauses {
        return right_clauses;
    }

    let mut left_clauses = left_clauses.unwrap();
    left_clauses.extend(right_clauses.unwrap());

    return Ok(left_clauses);
}

#[inline]
fn handle_uop(
    conditional_object_id: (usize, usize),
    uop: &oxidized::ast::Uop,
    expr: &aast::Expr<(), ()>,
    assertion_context: &AssertionContext,
    analysis_data: &mut FunctionAnalysisData,
    cache: bool,
    inside_negation: bool,
) -> Option<Result<Vec<Clause>, String>> {
    if let oxidized::ast::Uop::Unot = uop {
        if let aast::Expr_::Binop(inner_expr) = &expr.2 {
            if let oxidized::ast::Bop::Barbar = inner_expr.bop {
                return Some(self::handle_and(
                    conditional_object_id,
                    &aast::Expr(
                        (),
                        expr.pos().clone(),
                        aast::Expr_::Unop(Box::new((Uop::Unot, inner_expr.lhs.clone()))),
                    ),
                    &aast::Expr(
                        (),
                        expr.pos().clone(),
                        aast::Expr_::Unop(Box::new((Uop::Unot, inner_expr.rhs.clone()))),
                    ),
                    assertion_context,
                    analysis_data,
                    cache,
                    inside_negation,
                ));
            }

            if let oxidized::ast::Bop::Ampamp = inner_expr.bop {
                return Some(self::handle_or(
                    conditional_object_id,
                    &aast::Expr(
                        (),
                        expr.pos().clone(),
                        aast::Expr_::Unop(Box::new((Uop::Unot, inner_expr.lhs.clone()))),
                    ),
                    &aast::Expr(
                        (),
                        expr.pos().clone(),
                        aast::Expr_::Unop(Box::new((Uop::Unot, inner_expr.rhs.clone()))),
                    ),
                    assertion_context,
                    analysis_data,
                    cache,
                    inside_negation,
                ));
            }
        }

        let original_clauses = self::get_formula(
            conditional_object_id,
            (expr.pos().start_offset(), expr.pos().end_offset()),
            expr,
            assertion_context,
            analysis_data,
            cache,
            inside_negation,
        );

        if let Err(_) = original_clauses {
            return Some(original_clauses);
        }

        return Some(hakana_algebra::negate_formula(original_clauses.unwrap()));
    }

    None
}
