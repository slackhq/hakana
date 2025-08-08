use super::atomic_property_fetch_analyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{expr::expression_identifier, function_analysis_data::FunctionAnalysisData};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope::BlockContext, statements_analyzer::StatementsAnalyzer};
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::{add_union_type, get_mixed_any, get_null};
use hakana_code_info::var_name::VarName;
use hakana_code_info::EFFECT_READ_PROPS;
use itertools::Itertools;
use oxidized::{
    aast::{self, Expr},
    ast_defs::Pos,
};
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, &Expr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    in_assignment: bool,
    nullsafe: bool,
) -> Result<(), AnalysisError> {
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;

    let prop_name = if let aast::Expr_::Id(id) = &expr.1 .2 {
        Some(id.1.clone())
    } else {
        expression_analyzer::analyze(statements_analyzer, expr.1, analysis_data, context, true,)?;

        if let Some(stmt_name_type) = analysis_data.get_rc_expr_type(expr.1.pos()) {
            if let TAtomic::TLiteralString { value, .. } = stmt_name_type.get_single() {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        }
    };

    expression_analyzer::analyze(statements_analyzer, expr.0, analysis_data, context, true,)?;

    analysis_data.combine_effects_with(expr.0.pos(), expr.1.pos(), pos, EFFECT_READ_PROPS);

    context.inside_general_use = was_inside_general_use;

    let stmt_var_id = expression_identifier::get_var_id(
        expr.0,
        context.function_context.calling_class,
        statements_analyzer.file_analyzer.resolved_names,
        Some((statements_analyzer.codebase, statements_analyzer.interner)),
    );

    let var_id = if let Some(stmt_var_id) = stmt_var_id.clone() {
        prop_name
            .as_ref()
            .map(|prop_name| stmt_var_id + "->" + prop_name.as_str())
    } else {
        None
    };

    if let Some(var_id) = &var_id {
        if context.has_variable(var_id) {
            // short circuit if the type is known in scope
            handle_scoped_property(context, analysis_data, pos, var_id);

            return Ok(());
        }
    }

    let stmt_var_type = if let Some(stmt_var_id) = &stmt_var_id {
        if context.has_variable(stmt_var_id) {
            Some(context.locals.get(stmt_var_id.as_str()).unwrap().clone())
        } else {
            analysis_data.get_rc_expr_type(expr.0.pos()).cloned()
        }
    } else {
        analysis_data.get_rc_expr_type(expr.0.pos()).cloned()
    }
    .unwrap_or(Rc::new(get_mixed_any()));

    // TODO $stmt_var_type->isNull()
    // TODO $stmt_var_type->isEmpty()
    // TODO $stmt_var_type->hasMixed()
    // TODO $stmt_var_type->isNullable()
    // TODO mixed count

    let mut has_nullsafe_null = false;

    if let Some(prop_name) = prop_name {
        let mut var_atomic_types = stmt_var_type.types.iter().collect_vec();
        while let Some(mut var_atomic_type) = var_atomic_types.pop() {
            if let TAtomic::TGenericParam { as_type, .. }
            | TAtomic::TClassTypeConstant { as_type, .. } = var_atomic_type
            {
                var_atomic_types.extend(&as_type.types);
                continue;
            }

            if let TAtomic::TTypeAlias {
                as_type: Some(as_type),
                ..
            } = var_atomic_type
            {
                var_atomic_type = as_type.get_single();
            }

            if let TAtomic::TNull = var_atomic_type {
                if nullsafe {
                    has_nullsafe_null = true;
                    continue;
                }

                if !context.inside_isset {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::PossiblyNullPropertyFetch,
                            "Unsafe property access on null".to_string(),
                            statements_analyzer.get_hpos(expr.0.pos()),
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
                analysis_data,
                context,
                in_assignment,
                var_atomic_type.clone(),
                &prop_name,
                &stmt_var_id,
            )?;
        }
    }

    let mut stmt_type = analysis_data.get_rc_expr_type(pos).cloned();

    if has_nullsafe_null {
        if let Some(ref mut stmt_type) = stmt_type {
            if !stmt_type.is_nullable_mixed() {
                let mut stmt_type_inner = (**stmt_type).clone();
                stmt_type_inner = add_union_type(
                    stmt_type_inner,
                    &get_null(),
                    statements_analyzer.codebase,
                    false,
                );

                *stmt_type = Rc::new(stmt_type_inner);

                analysis_data.set_rc_expr_type(pos, stmt_type.clone());
            }
        }
    } else if nullsafe {
        // todo emit issue
    }

    // TODO $stmt_var_type->isNullable(

    // TODO  if ($invalid_fetch_types) {

    if let Some(var_id) = &var_id {
        context.locals.insert(
            VarName::new(var_id.clone()),
            stmt_type.unwrap_or(Rc::new(get_mixed_any())),
        );
    }

    Ok(())
}

/**
 * Handle simple cases where the value of the property can be
 * infered in the same scope as the current expression
 */
pub(crate) fn handle_scoped_property(
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    var_id: &str,
) {
    let stmt_type = context.locals.get(var_id);

    // we don't need to check anything since this variable is known in this scope
    analysis_data.set_rc_expr_type(
        pos,
        if let Some(stmt_type) = stmt_type {
            stmt_type.clone()
        } else {
            Rc::new(get_mixed_any())
        },
    );

    // TODO see original handleScopedProperty, lots of special case handling which we might not need, but we will need to handle taints.
}
