use super::atomic_property_fetch_analyzer;
use crate::{expr::expression_identifier, typed_ast::TastInfo};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::{add_union_type, get_mixed_any, get_null};
use oxidized::{
    aast::{self, Expr},
    ast_defs::Pos,
};
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, &Expr<(), ()>),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    in_assignment: bool,
    nullsafe: bool,
) -> bool {
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;

    let prop_name = if let aast::Expr_::Id(id) = &expr.1 .2 {
        Some(id.1.clone())
    } else {
        if !expression_analyzer::analyze(
            statements_analyzer,
            &expr.1,
            tast_info,
            context,
            &mut None,
        ) {
            return false;
        }

        if let Some(stmt_name_type) = tast_info.get_expr_type(expr.1.pos()).cloned() {
            if let TAtomic::TLiteralString { value, .. } = stmt_name_type.get_single() {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        }
    };

    if !expression_analyzer::analyze(statements_analyzer, &expr.0, tast_info, context, &mut None) {
        return false;
    }

    tast_info.combine_effects_with(
        expr.0.pos(),
        expr.1.pos(),
        pos,
        crate::typed_ast::READ_PROPS,
    );

    context.inside_general_use = was_inside_general_use;

    let stmt_var_id = expression_identifier::get_var_id(
        &expr.0,
        context.function_context.calling_class.as_ref(),
        statements_analyzer.get_file_analyzer().get_file_source(),
        statements_analyzer.get_file_analyzer().resolved_names,
        Some(statements_analyzer.get_codebase()),
    );

    let var_id = if let Some(stmt_var_id) = stmt_var_id.clone() {
        if let Some(prop_name) = &prop_name {
            Some(stmt_var_id + "->" + prop_name.as_str())
        } else {
            None
        }
    } else {
        None
    };

    if let Some(var_id) = &var_id {
        if context.has_variable(&var_id) {
            // short circuit if the type is known in scope
            handle_scoped_property(context, tast_info, pos, var_id);

            return true;
        }
    }

    let stmt_var_type = if let Some(stmt_var_id) = &stmt_var_id {
        if context.has_variable(&stmt_var_id) {
            Some((**context.vars_in_scope.get(stmt_var_id).unwrap()).clone())
        } else {
            tast_info.get_expr_type(expr.0.pos()).cloned()
        }
    } else {
        tast_info.get_expr_type(expr.0.pos()).cloned()
    }
    .unwrap_or(get_mixed_any());

    // TODO $stmt_var_type->isNull()
    // TODO $stmt_var_type->isEmpty()
    // TODO $stmt_var_type->hasMixed()
    // TODO $stmt_var_type->isNullable()
    // TODO mixed count

    let mut has_nullsafe_null = false;

    if let Some(prop_name) = prop_name {
        let var_atomic_types = &stmt_var_type.types;
        for lhs_type_part in var_atomic_types {
            if let TAtomic::TNull = lhs_type_part {
                if nullsafe {
                    has_nullsafe_null = true;
                    continue;
                }

                if !context.inside_isset {
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::PossiblyNullPropertyFetch,
                            "Unsafe property access on null".to_string(),
                            statements_analyzer.get_hpos(&expr.0.pos()),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }

            // TODO $lhs_type_part instanceof TTemplateParam
            atomic_property_fetch_analyzer::analyze(
                statements_analyzer,
                expr,
                pos,
                tast_info,
                context,
                in_assignment,
                lhs_type_part.clone(),
                &prop_name,
                &var_id,
                &stmt_var_id,
            );
        }
    }

    let mut stmt_type = tast_info.get_rc_expr_type(&pos).cloned();

    if has_nullsafe_null {
        if let Some(ref mut stmt_type) = stmt_type {
            if !stmt_type.is_nullable_mixed() {
                let mut stmt_type_inner = (**stmt_type).clone();
                stmt_type_inner = add_union_type(
                    stmt_type_inner,
                    &get_null(),
                    statements_analyzer.get_codebase(),
                    false,
                );

                *stmt_type = Rc::new(stmt_type_inner);

                tast_info.set_rc_expr_type(pos, stmt_type.clone());
            }
        }
    } else if nullsafe {
        // todo emit issue
    }

    // TODO $stmt_var_type->isNullable(

    // TODO  if ($invalid_fetch_types) {

    if let Some(var_id) = &var_id {
        context.vars_in_scope.insert(
            var_id.to_owned(),
            stmt_type.unwrap_or(Rc::new(get_mixed_any())),
        );
    }

    true
}

/**
 * Handle simple cases where the value of the property can be
 * infered in the same scope as the current expression
 */
pub(crate) fn handle_scoped_property(
    context: &mut ScopeContext,
    tast_info: &mut TastInfo,
    pos: &Pos,
    var_id: &String,
) -> () {
    let stmt_type = context.vars_in_scope.get(var_id);

    // we don't need to check anything since this variable is known in this scope
    tast_info.set_rc_expr_type(
        &pos,
        if let Some(stmt_type) = stmt_type {
            stmt_type.clone()
        } else {
            Rc::new(get_mixed_any())
        },
    );

    // TODO see original handleScopedProperty, lots of special case handling which we might not need, but we will need to handle taints.
}
