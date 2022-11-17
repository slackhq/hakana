use std::rc::Rc;

use hakana_reflection_info::{
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_type::{
    get_mixed_any,
    type_comparator::{type_comparison_result::TypeComparisonResult, union_type_comparator},
    type_expander::{self, StaticClassType, TypeExpansionOptions},
};
use oxidized::{
    aast::{self, ClassGetExpr, ClassId},
    tast::Pos,
};

use crate::{expr::fetch::class_constant_fetch_analyzer::get_id_name, typed_ast::TastInfo};
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};

use super::instance_property_assignment_analyzer::add_unspecialized_property_assignment_dataflow;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ClassId<(), ()>, &ClassGetExpr<(), ()>),
    assign_value_pos: Option<&Pos>,
    assign_value_type: &TUnion,
    var_id: &Option<String>,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
) -> bool {
    let codebase = statements_analyzer.get_codebase();
    let stmt_class = expr.0;
    let stmt_name = expr.1;

    let mut stmt_name_expr = None;
    let mut stmt_name_string = None;
    let stmt_name_pos;

    match &stmt_name {
        aast::ClassGetExpr::CGexpr(expr) => {
            stmt_name_expr = Some(expr);
            stmt_name_pos = expr.pos();
        }
        aast::ClassGetExpr::CGstring(str) => {
            let id = &str.1;
            stmt_name_string = Some(id);
            stmt_name_pos = &str.0;
        }
    }

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

    let prop_name = if let Some(prop_name) = prop_name {
        if let Some(prop_name_id) = codebase.interner.get(&prop_name) {
            prop_name_id
        } else {
            return false;
        }
    } else {
        return false;
    };

    let mut fq_class_names = Vec::new();

    match &stmt_class.2 {
        aast::ClassId_::CIexpr(expr) => {
            match &expr.2 {
                aast::Expr_::Id(id) => {
                    let mut is_static = false;
                    let classlike_name = get_id_name(
                        id,
                        &context.function_context.calling_class,
                        codebase,
                        &mut is_static,
                        statements_analyzer.get_file_analyzer().resolved_names,
                    )
                    .unwrap();

                    fq_class_names.push(classlike_name);
                }
                _ => {
                    // eg. $class::$foo
                    let was_inside_general_use = context.inside_general_use;
                    context.inside_general_use = true;
                    expression_analyzer::analyze(
                        statements_analyzer,
                        expr,
                        tast_info,
                        context,
                        &mut None,
                    );
                    context.inside_general_use = was_inside_general_use;

                    let lhs_type = tast_info.get_expr_type(&expr.1.clone());

                    if let Some(lhs_type) = lhs_type {
                        for lhs_atomic_type in lhs_type.types.clone() {
                            if let TAtomic::TNamedObject {
                                name,
                                type_params: None,
                                ..
                            } = lhs_atomic_type
                            {
                                fq_class_names.push(name);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    if fq_class_names.is_empty() {
        return false;
    }

    for fq_class_name in fq_class_names {
        // TODO if (!$prop_name instanceof PhpParser\Node\Identifier) {

        let property_id = (fq_class_name.clone(), prop_name.to_owned());

        // TODO if (ClassLikeAnalyzer::checkPropertyVisibility(

        let declaring_property_class =
            codebase.get_declaring_class_for_property(&fq_class_name, &property_id.1);

        if let Some(declaring_property_class) = declaring_property_class {
            let mut class_property_type = if let Some(prop_type) =
                codebase.get_property_type(&fq_class_name, &property_id.1)
            {
                prop_type
            } else {
                get_mixed_any()
            };

            add_unspecialized_property_assignment_dataflow(
                statements_analyzer,
                &property_id,
                stmt_name_pos,
                assign_value_pos,
                tast_info,
                assign_value_type,
                codebase,
                &fq_class_name,
                prop_name,
            );

            let declaring_class_storage = codebase.classlike_infos.get(&fq_class_name);

            if let Some(declaring_class_storage) = declaring_class_storage {
                type_expander::expand_union(
                    codebase,
                    &mut class_property_type,
                    &TypeExpansionOptions {
                        self_class: Some(&declaring_class_storage.name),
                        static_class_type: StaticClassType::Name(&declaring_class_storage.name),
                        parent_class: declaring_class_storage.direct_parent_class.as_ref(),
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
            }

            let mut union_comparison_result = TypeComparisonResult::new();

            let type_match_found = union_type_comparator::is_contained_by(
                codebase,
                assign_value_type,
                &class_property_type,
                false,
                assign_value_type.ignore_falsable_issues,
                false,
                &mut union_comparison_result,
            );

            if type_match_found && union_comparison_result.replacement_union_type.is_some() {
                if let Some(union_type) = union_comparison_result.replacement_union_type {
                    if let Some(var_id) = var_id.clone() {
                        context.vars_in_scope.insert(var_id, Rc::new(union_type));
                    }
                }
            }

            if !type_match_found && union_comparison_result.type_coerced.is_none() {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidPropertyAssignmentValue,
                        format!(
                            "{}::${} with declared type {}, cannot be assigned type {}",
                            codebase.interner.lookup(*declaring_property_class),
                            codebase.interner.lookup(property_id.1),
                            class_property_type.get_id(Some(&codebase.interner)),
                            assign_value_type.get_id(Some(&codebase.interner)),
                        ),
                        statements_analyzer.get_hpos(&stmt_class.1),
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }

            if union_comparison_result.type_coerced.is_some() {
                if union_comparison_result
                    .type_coerced_from_nested_mixed
                    .is_some()
                {
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::MixedPropertyTypeCoercion,
                            format!(
                                "{} expects {}, parent type {} provided",
                                var_id.clone().unwrap_or("This property".to_string()),
                                class_property_type.get_id(Some(&codebase.interner)),
                                assign_value_type.get_id(Some(&codebase.interner)),
                            ),
                            statements_analyzer.get_hpos(&stmt_class.1),
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                } else {
                    tast_info.maybe_add_issue(
                        Issue::new(
                            IssueKind::PropertyTypeCoercion,
                            format!(
                                "{} expects {}, parent type {} provided",
                                var_id.clone().unwrap_or("This property".to_string()),
                                class_property_type.get_id(Some(&codebase.interner)),
                                assign_value_type.get_id(Some(&codebase.interner)),
                            ),
                            statements_analyzer.get_hpos(&stmt_class.1),
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }

            if let Some(var_id) = var_id.clone() {
                context
                    .vars_in_scope
                    .insert(var_id, Rc::new(assign_value_type.clone()));
            }
        }
    }

    true
}
