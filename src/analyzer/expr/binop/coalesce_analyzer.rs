use std::rc::Rc;

use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;

use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::{TAtomic, TDict, TVec};
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::{add_union_type, combine_union_types, get_mixed_any, get_null};
use hakana_code_info::var_name::VarName;
use hakana_parser::aast;
use hakana_parser::aast::Argument;
use hakana_parser::aast::CallExpr;
use rustc_hash::FxHashSet;

pub(crate) fn analyze<'ast, 'expr>(
    statements_analyzer: &StatementsAnalyzer<'ast>,
    pos: &aast::Pos,
    left: &'expr aast::Expr<'ast>,
    right: &'expr aast::Expr<'ast>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let arena = statements_analyzer.arena();
    let mut root_expr = left;
    let mut root_not_left = false;
    let mut has_arrayget_key = false;
    let mut has_array_access = false;

    loop {
        match &root_expr.2 {
            aast::Expr_::ArrayGet(boxed) => {
                root_expr = &boxed.0;
                root_not_left = true;
                has_array_access = true;

                if let Some(dim) = &boxed.1
                    && let aast::Expr_::ArrayGet(..)
                    | aast::Expr_::ClassConst(..)
                    | aast::Expr_::Call(..)
                    | aast::Expr_::Cast(..)
                    | aast::Expr_::Eif(..)
                    | aast::Expr_::Binop(..)
                    | aast::Expr_::As(..)
                    | aast::Expr_::Pipe(..)
                    | aast::Expr_::Await(..)
                    | aast::Expr_::Delay(..) = dim.2
                {
                    has_arrayget_key = true;
                }
            }
            aast::Expr_::ObjGet(boxed) => {
                root_expr = &boxed.0;
                root_not_left = true;
            }
            _ => {
                break;
            }
        }
    }

    // Check for tautological `$foo ?? null` checks where the LHS is always known to be defined.
    // Do this only for the null literal rather than any nullable RHS since implementing the latter logic
    // would have diminishing returns.
    if !has_array_access && matches!(right.2, aast::Expr_::Null) {
        let issue = Issue::new(
            IssueKind::RedundantIssetCheck,
            "Unnecessary isset check".to_string(),
            statements_analyzer.get_hpos(pos),
            &context.function_context.calling_functionlike_id,
        );

        if statements_analyzer.should_autofix(context, analysis_data, &issue) {
            analysis_data.add_replacement(
                (
                    left.pos().end_offset() as u32,
                    right.pos().end_offset() as u32,
                ),
                Replacement::Remove,
            );
        } else {
            analysis_data.maybe_add_issue(
                issue,
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    let mut replacement_left = None;

    if has_arrayget_key {
        replacement_left = Some(get_left_expr(
            context,
            statements_analyzer,
            left,
            analysis_data,
            left,
            &None,
            true,
        ));
    } else {
        let root_type = if root_not_left {
            let mut isset_context = context.clone();
            isset_context.inside_isset = true;
            expression_analyzer::analyze(
                statements_analyzer,
                root_expr,
                analysis_data,
                &mut isset_context,
                true,
            )
            .ok();

            context.response = isset_context.response;
            analysis_data.get_rc_expr_type(root_expr.pos()).cloned()
        } else {
            None
        };

        if matches!(
            root_expr.2,
            aast::Expr_::Call(..)
                | aast::Expr_::Cast(..)
                | aast::Expr_::Eif(..)
                | aast::Expr_::Binop(..)
                | aast::Expr_::As(..)
                | aast::Expr_::ClassConst(..)
                | aast::Expr_::Pipe(..)
                | aast::Expr_::Await(..)
                | aast::Expr_::Delay(..)
        ) {
            replacement_left = Some(get_left_expr(
                context,
                statements_analyzer,
                left,
                analysis_data,
                root_expr,
                &root_type,
                false,
            ));
        } else if let Some(root_type) = root_type
            && (root_type.has_typealias()
                || hakana_code_info::ttype::type_variable::contains(&root_type)
                || root_type
                    .types
                    .iter()
                    .all(|atomic| has_no_arrayish_value(atomic, statements_analyzer.codebase)))
        {
            replacement_left = Some(get_left_expr(
                context,
                statements_analyzer,
                left,
                analysis_data,
                left,
                &None,
                true,
            ));
        }
    }

    if let Some(aast::Expr(_, _, aast::Expr_::Lvar(var))) = &replacement_left
        && context.locals.get(var.1.1).is_some_and(|ty| ty.is_null())
    {
        // The evaluated fetch has no possible value, as on the first
        // iteration of a reducer with an empty accumulator.
        expression_analyzer::analyze(statements_analyzer, right, analysis_data, context, true)?;
        let right_type = analysis_data
            .get_expr_type(right.pos())
            .cloned()
            .unwrap_or_else(get_mixed_any);
        analysis_data.set_expr_type(pos, right_type);
        analysis_data.combine_effects(left.pos(), right.pos(), pos);
        return Ok(());
    }

    let ternary = aast::Expr(
        (),
        *pos,
        aast::Expr_::Eif(&*arena.alloc((
            aast::Expr(
                (),
                *left.pos(),
                aast::Expr_::Call(&*arena.alloc(CallExpr {
                    func: aast::Expr(
                        (),
                        *left.pos(),
                        aast::Expr_::Id(
                            &*arena.alloc(hakana_parser::aast::Id(*left.pos(), "isset")),
                        ),
                    ),
                    targs: &[],
                    args: &*arena.alloc([Argument::Anormal(replacement_left.unwrap_or(*left))]),
                    unpacked_arg: None,
                })),
            ),
            Some(replacement_left.unwrap_or(*left)),
            *right,
        ))),
    );

    let old_expr_types = analysis_data.expr_types.clone();
    analysis_data.expr_types.clone_from(&old_expr_types);

    expression_analyzer::analyze(statements_analyzer, &ternary, analysis_data, context, true).ok();

    let ternary_type = analysis_data
        .get_rc_expr_type(pos)
        .cloned()
        .unwrap_or(Rc::new(get_mixed_any()));
    analysis_data.expr_types = old_expr_types;

    analysis_data.set_rc_expr_type(pos, ternary_type);

    analysis_data.combine_effects(left.pos(), right.pos(), pos);

    Ok(())
}

fn get_left_expr<'ast>(
    context: &mut BlockContext,
    statements_analyzer: &StatementsAnalyzer<'ast>,
    left: &aast::Expr<'ast>,
    analysis_data: &mut FunctionAnalysisData,
    root_expr: &aast::Expr<'ast>,
    root_type: &Option<Rc<TUnion>>,
    make_nullable: bool,
) -> aast::Expr<'ast> {
    let arena = statements_analyzer.arena();
    let mut isset_context = context.clone();
    isset_context.inside_isset = true;

    let mut condition_type = if let Some(root_type) = root_type {
        root_type.clone()
    } else {
        expression_analyzer::analyze(
            statements_analyzer,
            root_expr,
            analysis_data,
            &mut isset_context,
            true,
        )
        .ok();

        analysis_data
            .get_rc_expr_type(root_expr.pos())
            .cloned()
            .unwrap_or(Rc::new(get_mixed_any()))
    };

    let root_expr_var_id = format!("$tmp_coalesce_var {}", left.pos().start_offset());

    if make_nullable && !condition_type.is_nullable_mixed() {
        condition_type = Rc::new(add_union_type(
            (*condition_type).clone(),
            &get_null(),
            statements_analyzer.codebase,
            false,
        ));
    }

    context.response = isset_context.response;

    let redefined_vars = isset_context
        .get_redefined_locals(&context.locals, false, &mut FxHashSet::default())
        .into_keys()
        .collect::<FxHashSet<_>>();

    //these vars were changed in both branches
    for redef_var_id in &redefined_vars {
        context.locals.insert(
            redef_var_id.clone(),
            Rc::new(combine_union_types(
                &isset_context.locals[redef_var_id],
                &context.locals[redef_var_id],
                statements_analyzer.codebase,
                false,
            )),
        );
    }

    context
        .locals
        .insert(VarName::new(root_expr_var_id.clone()), condition_type);

    if root_expr.1 != left.1 {
        let mut left = *left;

        let new_root_expr = aast::Expr(
            (),
            *root_expr.pos(),
            aast::Expr_::Lvar(&*arena.alloc(hakana_parser::aast::Lid(
                *root_expr.pos(),
                (5, &*arena.alloc_str(&root_expr_var_id)),
            ))),
        );

        replace_expr_with_root(arena, &mut left, new_root_expr);
        left
    } else {
        aast::Expr(
            (),
            *root_expr.pos(),
            aast::Expr_::Lvar(&*arena.alloc(hakana_parser::aast::Lid(
                *root_expr.pos(),
                (5, &*arena.alloc_str(&root_expr_var_id)),
            ))),
        )
    }
}

fn replace_expr_with_root<'ast>(
    arena: &'ast hakana_parser::Bump,
    expr: &mut aast::Expr<'ast>,
    root: aast::Expr<'ast>,
) {
    match &expr.2 {
        aast::Expr_::ArrayGet(boxed) => {
            let mut left = boxed.0;
            replace_expr_with_root(arena, &mut left, root);
            *expr = aast::Expr(
                (),
                *expr.pos(),
                aast::Expr_::ArrayGet(&*arena.alloc((left, boxed.1))),
            );
        }
        aast::Expr_::ObjGet(boxed) => {
            let mut left = boxed.0;
            replace_expr_with_root(arena, &mut left, root);
            *expr = aast::Expr(
                (),
                *expr.pos(),
                aast::Expr_::ObjGet(&*arena.alloc((left, boxed.1, boxed.2, boxed.3))),
            );
        }
        _ => {
            *expr = root;
        }
    }
}

/// Whether `get_arrayish_params` gives this type a value type of `nothing`.
fn has_no_arrayish_value(atomic: &TAtomic, codebase: &CodebaseInfo) -> bool {
    // The value type is the union of the value param and of every known item,
    // and a union with a member other than `nothing` is not `nothing`. Asking
    // that first saves building the union, which for a big shape is costly.
    let (value_param, has_item) = match atomic {
        TAtomic::TDict(TDict {
            params,
            known_items,
            ..
        }) => (
            params.as_ref().map(|params| &*params.1),
            known_items.as_ref().is_some_and(|known_items| {
                known_items
                    .values()
                    .any(|(_, item_type)| !item_type.is_nothing())
            }),
        ),
        TAtomic::TVec(TVec {
            type_param,
            known_items,
            ..
        }) => (
            Some(&**type_param),
            known_items.as_ref().is_some_and(|known_items| {
                known_items
                    .values()
                    .any(|(_, item_type)| !item_type.is_nothing())
            }),
        ),
        _ => (None, false),
    };

    if has_item || value_param.is_some_and(|value_param| !value_param.is_nothing()) {
        return false;
    }

    hakana_code_info::ttype::get_arrayish_params(atomic, codebase)
        .is_some_and(|(_, value)| value.is_nothing())
}
