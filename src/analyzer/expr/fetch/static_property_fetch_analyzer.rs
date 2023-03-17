use super::{
    atomic_property_fetch_analyzer::add_unspecialized_property_fetch_dataflow,
    instance_property_fetch_analyzer,
};
use crate::typed_ast::FunctionAnalysisData;
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::ast::get_id_name;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::EFFECT_READ_PROPS;
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::{
    get_named_object,
    type_expander::{self, StaticClassType},
};
use oxidized::ast;
use oxidized::{
    aast::{self, ClassGetExpr, ClassId},
    ast_defs::Pos,
};
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ClassId<(), ()>, &ClassGetExpr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> bool {
    let codebase = statements_analyzer.get_codebase();
    let stmt_class = expr.0;
    let stmt_name = expr.1;

    let classlike_name = match &stmt_class.2 {
        aast::ClassId_::CIexpr(lhs_expr) => {
            if let aast::Expr_::Id(id) = &lhs_expr.2 {
                let mut is_static = false;
                get_id_name(
                    id,
                    &context.function_context.calling_class,
                    codebase,
                    &mut is_static,
                    statements_analyzer.get_file_analyzer().resolved_names,
                )
                .unwrap()
            } else {
                analyze_variable_static_property_fetch(
                    statements_analyzer,
                    expr,
                    pos,
                    analysis_data,
                    context,
                );
                return true;
            }
        }
        _ => {
            panic!()
        }
    };

    if !codebase.class_exists(&classlike_name) {
        analysis_data.symbol_references.add_reference_to_symbol(
            &context.function_context,
            classlike_name,
            false,
        );

        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentClass,
                format!(
                    "Cannot access property on undefined class {}",
                    statements_analyzer.get_interner().lookup(&classlike_name)
                ),
                statements_analyzer.get_hpos(&pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return false;
    }

    analysis_data
        .expr_effects
        .insert((pos.start_offset(), pos.end_offset()), EFFECT_READ_PROPS);

    analysis_data.set_expr_type(&stmt_class.1, get_named_object(classlike_name.clone()));

    let prop_name = match &stmt_name {
        aast::ClassGetExpr::CGexpr(stmt_name_expr) => {
            if let aast::Expr_::Id(id) = &stmt_name_expr.2 {
                id.1.clone()
            } else {
                if let Some(stmt_name_type) =
                    analysis_data.get_expr_type(stmt_name_expr.pos()).cloned()
                {
                    if let TAtomic::TLiteralString { value, .. } = stmt_name_type.get_single() {
                        value.clone()
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
        aast::ClassGetExpr::CGstring(str) => {
            let id = &str.1;

            id[1..].to_string()
        }
    };

    let var_id = format!(
        "{}::${}",
        statements_analyzer.get_interner().lookup(&classlike_name),
        prop_name
    );

    let prop_name_id = statements_analyzer.get_interner().get(&prop_name);

    let property_id = if let Some(prop_name_id) = prop_name_id {
        (classlike_name.clone(), prop_name_id)
    } else {
        analysis_data.symbol_references.add_reference_to_symbol(
            &context.function_context,
            classlike_name,
            false,
        );

        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentProperty,
                format!(
                    "Property {}::${} is undefined",
                    statements_analyzer.get_interner().lookup(&classlike_name),
                    prop_name,
                ),
                statements_analyzer.get_hpos(&pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return false;
    };

    analysis_data
        .symbol_references
        .add_reference_to_class_member(
            &context.function_context,
            (property_id.0, property_id.1),
            false,
        );

    // Handle scoped property fetches
    if context.has_variable(&var_id) {
        let mut stmt_type = (**context.vars_in_scope.get(&var_id).unwrap()).clone();

        stmt_type = add_unspecialized_property_fetch_dataflow(
            &None,
            &property_id,
            statements_analyzer.get_hpos(pos),
            analysis_data,
            false,
            stmt_type,
            &statements_analyzer.get_interner(),
        );

        // we don't need to check anything since this variable is known in this scope
        analysis_data.set_expr_type(&pos, stmt_type);

        return true;
    }

    let declaring_property_class = if let Some(declaring_property_class) =
        codebase.get_declaring_class_for_property(&property_id.0, &property_id.1)
    {
        declaring_property_class
    } else {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentProperty,
                format!(
                    "Property {}::{} is undefined",
                    statements_analyzer.get_interner().lookup(&classlike_name),
                    statements_analyzer.get_interner().lookup(&property_id.1)
                ),
                statements_analyzer.get_hpos(&pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return false;
    };

    // TODO AtomicPropertyFetchAnalyzer::checkPropertyDeprecation
    // TODO ClassLikeAnalyzer::checkPropertyVisibility
    // TODO if ($codebase->alter_code) {

    // let's do getClassPropertyType
    let property_type = codebase.get_property_type(&property_id.0, &property_id.1);

    if let Some(property_type) = property_type {
        let declaring_class_storage = codebase
            .classlike_infos
            .get(declaring_property_class)
            .unwrap();
        let parent_class = declaring_class_storage.direct_parent_class.clone();

        let mut inserted_type = property_type.clone();
        type_expander::expand_union(
            codebase,
            &Some(statements_analyzer.get_interner()),
            &mut inserted_type,
            &TypeExpansionOptions {
                self_class: Some(&declaring_class_storage.name),
                static_class_type: StaticClassType::Name(&declaring_class_storage.name),
                parent_class: parent_class.as_ref(),
                file_path: Some(
                    &statements_analyzer
                        .get_file_analyzer()
                        .get_file_source()
                        .file_path,
                ),
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
        );

        inserted_type = add_unspecialized_property_fetch_dataflow(
            &None,
            &property_id,
            statements_analyzer.get_hpos(pos),
            analysis_data,
            false,
            inserted_type,
            &statements_analyzer.get_interner(),
        );

        let rc = Rc::new(inserted_type.clone());

        context.vars_in_scope.insert(var_id.to_owned(), rc.clone());

        analysis_data.set_rc_expr_type(&pos, rc)
    }

    true
}

/**
 * Handle simple cases where the value of the property can be
 * infered in the same scope as the current expression
 */
fn analyze_variable_static_property_fetch(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ClassId<(), ()>, &ClassGetExpr<(), ()>),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) {
    let stmt_class_type = if let aast::ClassId_::CIexpr(stmt_class_expr) = &expr.0 .2 {
        let was_inside_general_use = context.inside_general_use;
        context.inside_general_use = true;

        expression_analyzer::analyze(
            statements_analyzer,
            stmt_class_expr,
            analysis_data,
            context,
            &mut None,
        );

        context.inside_general_use = was_inside_general_use;
        analysis_data.get_expr_type(stmt_class_expr.pos()).cloned()
    } else {
        None
    };

    if let Some(stmt_class_type) = stmt_class_type {
        let fake_var_name = "__fake_var_".to_string() + &pos.line().to_string();
        context
            .vars_in_scope
            .insert(fake_var_name.to_owned(), Rc::new(stmt_class_type));

        let lhs = &aast::Expr(
            (),
            pos.clone(),
            aast::Expr_::Lvar(Box::new(oxidized::tast::Lid(
                pos.clone(),
                (
                    fake_var_name.len().try_into().unwrap(),
                    fake_var_name.clone(),
                ),
            ))),
        );

        let rhs = match &expr.1 {
            aast::ClassGetExpr::CGexpr(stmt_name_expr) => stmt_name_expr.clone(),
            aast::ClassGetExpr::CGstring(str) => aast::Expr(
                (),
                str.0.clone(),
                aast::Expr_::Id(Box::new(ast::Id(str.0.clone(), str.1[1..].to_string()))),
            ),
        };

        instance_property_fetch_analyzer::analyze(
            statements_analyzer,
            (&lhs, &rhs),
            &pos,
            analysis_data,
            context,
            context.inside_assignment,
            false,
        );

        let stmt_type = analysis_data.get_expr_type(&pos).unwrap();
        analysis_data.set_expr_type(&pos, stmt_type.clone());
    }
}
