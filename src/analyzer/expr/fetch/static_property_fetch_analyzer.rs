use super::class_constant_fetch_analyzer::get_id_name;
use super::{
    atomic_property_fetch_analyzer::add_unspecialized_property_fetch_dataflow,
    instance_property_fetch_analyzer,
};
use crate::typed_ast::TastInfo;
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::{
    combine_optional_union_types, get_named_object,
    type_expander::{self, StaticClassType},
    wrap_atomic,
};
use oxidized::{
    aast::{self, ClassGetExpr, ClassId},
    ast_defs::{self, Pos},
};
use std::rc::Rc;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ClassId<(), ()>, &ClassGetExpr<(), ()>),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let codebase = statements_analyzer.get_codebase();
    let stmt_class = expr.0;
    let stmt_name = expr.1;

    let mut stmt_name_expr = None;
    let mut stmt_name_string = None;

    match &stmt_name {
        aast::ClassGetExpr::CGexpr(expr) => {
            stmt_name_expr = Some(expr);
        }
        aast::ClassGetExpr::CGstring(str) => {
            let id = &str.1;
            stmt_name_string = Some(id)
        }
    }

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
                    tast_info,
                    context,
                );
                return true;
            }
        }
        _ => {
            panic!()
        }
    };

    tast_info.expr_effects.insert(
        (pos.start_offset(), pos.end_offset()),
        crate::typed_ast::READ_PROPS,
    );

    // TODO
    // if (count($stmt->class->parts) === 1
    //     && in_array(strtolower($stmt->class->parts[0]), ['self', 'static', 'parent'], true)
    // )

    if context.check_classes {
        // ClassLikeAnalyzer::checkFullyQualifiedClassLikeName
    }

    tast_info.set_expr_type(&stmt_class.1, get_named_object(classlike_name.clone()));

    let prop_name = if let Some(stmt_name_string) = stmt_name_string {
        Some(stmt_name_string[1..].to_string())
    } else if let Some(stmt_name_expr) = stmt_name_expr {
        if let aast::Expr_::Id(id) = &stmt_name_expr.2 {
            Some(id.1.clone())
        } else {
            if let Some(stmt_name_type) = tast_info.get_expr_type(stmt_name_expr.pos()).cloned() {
                if let TAtomic::TLiteralString { value, .. } = stmt_name_type.get_single() {
                    Some(value.clone())
                } else {
                    None
                }
            } else {
                None
            }
        }
    } else {
        None
    };

    let mut var_id = None;

    if let Some(prop_name) = &prop_name {
        var_id = Some(format!(
            "{}::${}",
            codebase.interner.lookup(classlike_name),
            prop_name
        ));
    }

    let property_id = (
        classlike_name.clone(),
        codebase.interner.get(&prop_name.unwrap()).unwrap(),
    );

    // TODO mutation handling

    // Handle scoped property fetches
    if let Some(var_id) = &var_id {
        if context.has_variable(var_id) {
            let mut stmt_type = (**context.vars_in_scope.get(var_id).unwrap()).clone();

            stmt_type = add_unspecialized_property_fetch_dataflow(
                &None,
                &property_id,
                statements_analyzer.get_hpos(pos),
                tast_info,
                false,
                stmt_type,
                &codebase.interner,
            );

            // we don't need to check anything since this variable is known in this scope
            tast_info.set_expr_type(&pos, stmt_type);

            return true;
        }
    }

    let declaring_property_class =
        codebase.get_declaring_class_for_property(&property_id.0, &property_id.1);

    if let None = declaring_property_class {
        // todo report issue
        return false;
    }

    tast_info.symbol_references.add_reference_to_class_member(
        &context.function_context,
        (property_id.0.clone(), property_id.1),
    );

    // TODO AtomicPropertyFetchAnalyzer::checkPropertyDeprecation
    // TODO ClassLikeAnalyzer::checkPropertyVisibility
    // TODO if ($codebase->alter_code) {

    // let's do getClassPropertyType
    let property_type = codebase.get_property_type(&property_id.0, &property_id.1);

    if let Some(var_id) = &var_id {
        if let Some(property_type) = property_type {
            let declaring_class_storage = codebase
                .classlike_infos
                .get(declaring_property_class.unwrap())
                .unwrap();
            let parent_class = declaring_class_storage.direct_parent_class.clone();

            let mut inserted_type = property_type.clone();
            type_expander::expand_union(
                codebase,
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
                &mut tast_info.data_flow_graph,
            );

            inserted_type = add_unspecialized_property_fetch_dataflow(
                &None,
                &property_id,
                statements_analyzer.get_hpos(pos),
                tast_info,
                false,
                inserted_type,
                &statements_analyzer.get_codebase().interner,
            );

            let rc = Rc::new(inserted_type.clone());

            context.vars_in_scope.insert(var_id.to_owned(), rc.clone());

            tast_info.set_rc_expr_type(&pos, rc)
        }
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> () {
    let stmt_class = expr.0;
    let mut stmt_class_expr = None;

    match &stmt_class.2 {
        aast::ClassId_::CIexpr(expr) => {
            stmt_class_expr = Some(expr);
        }
        _ => {}
    }

    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;

    if let Some(stmt_class_expr) = stmt_class_expr {
        expression_analyzer::analyze(
            statements_analyzer,
            stmt_class_expr,
            tast_info,
            context,
            &mut None,
        );
    }

    context.inside_general_use = was_inside_general_use;

    let codebase = statements_analyzer.get_codebase();

    let stmt_class_type = tast_info.get_expr_type(&stmt_class.1).cloned();

    let mut fake_stmt_type = None;

    if let Some(stmt_class_type) = stmt_class_type {
        for (_, class_atomic_type) in &stmt_class_type.types {
            let fake_var_name = "__fake_var_".to_string() + &pos.line().to_string();
            context.vars_in_scope.insert(
                fake_var_name.to_owned(),
                Rc::new(wrap_atomic(class_atomic_type.clone())),
            );

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

            let rhs = &aast::Expr(
                (),
                pos.clone(),
                aast::Expr_::Id(Box::new(ast_defs::Id(pos.clone(), fake_var_name.clone()))),
            );

            instance_property_fetch_analyzer::analyze(
                statements_analyzer,
                (&lhs, &rhs),
                &pos,
                tast_info,
                context,
                context.inside_assignment,
                false,
            );

            fake_stmt_type = Some(tast_info.get_expr_type(&pos).unwrap());
        }

        let stmt_type = combine_optional_union_types(fake_stmt_type, None, codebase);
        tast_info.set_expr_type(&pos, stmt_type);
    }
}
