use std::rc::Rc;

use hakana_code_info::{
    ast::get_id_name,
    codebase_info::CodebaseInfo,
    issue::{Issue, IssueKind},
    t_atomic::{TAtomic, TNamedObject},
    t_union::TUnion,
};
use hakana_code_info::{
    ttype::{
        comparison::{type_comparison_result::TypeComparisonResult, union_type_comparator},
        get_mixed_any,
        type_expander::{self, StaticClassType, TypeExpansionOptions},
    },
    var_name::VarName,
};
use hakana_str::StrId;
use oxidized::{
    aast::{self, ClassId},
    ast::Id,
    ast_defs::{self, Pos},
};

use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{function_analysis_data::FunctionAnalysisData, stmt_analyzer::AnalysisError};
use crate::{scope::BlockContext, statements_analyzer::StatementsAnalyzer};

use super::instance_property_assignment_analyzer::add_unspecialized_property_assignment_dataflow;

fn resolve_id(
    id: &Id,
    context: &BlockContext,
    codebase: &CodebaseInfo,
    statements_analyzer: &StatementsAnalyzer,
) -> Result<StrId, AnalysisError> {
    let mut is_static = false;

    get_id_name(
        id,
        &context.function_context.calling_class,
        context.function_context.calling_class_final,
        codebase,
        &mut is_static,
        statements_analyzer.file_analyzer.resolved_names,
    )
    .ok_or_else(|| {
        AnalysisError::InternalError(
            "Could not resolve class name for static property assignment".to_string(),
            statements_analyzer.get_hpos(&id.0),
        )
    })
}

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ClassId<(), ()>, &ast_defs::Pstring),
    assign_value_pos: Option<&Pos>,
    assign_value_type: &TUnion,
    var_id: &Option<String>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;
    let stmt_class = expr.0;
    let stmt_name = expr.1;

    let stmt_name_pos = &stmt_name.0;
    let prop_name_str = stmt_name.1[1..].to_string();

    let prop_name = if let Some(prop_name) = Some(prop_name_str) {
        if let Some(prop_name_id) = statements_analyzer.interner.get(&prop_name) {
            prop_name_id
        } else {
            return Err(AnalysisError::UserError);
        }
    } else {
        return Err(AnalysisError::UserError);
    };

    let mut fq_class_names = Vec::new();

    match &stmt_class.2 {
        aast::ClassId_::CIexpr(expr) => {
            match &expr.2 {
                aast::Expr_::Id(id) => {
                    fq_class_names.push(resolve_id(id, context, codebase, statements_analyzer)?)
                }
                _ => {
                    // eg. $class::$foo
                    let was_inside_general_use = context.inside_general_use;
                    context.inside_general_use = true;
                    expression_analyzer::analyze(
                        statements_analyzer,
                        expr,
                        analysis_data,
                        context,
                        true,
                    )?;
                    context.inside_general_use = was_inside_general_use;

                    let lhs_type = analysis_data.get_expr_type(&expr.1.clone());

                    if let Some(lhs_type) = lhs_type {
                        for lhs_atomic_type in lhs_type.types.clone() {
                            if let TAtomic::TNamedObject(TNamedObject {
                                name,
                                type_params: None,
                                ..
                            }) = lhs_atomic_type
                            {
                                fq_class_names.push(name);
                            }
                        }
                    }
                }
            }
        }
        aast::ClassId_::CIreified(id) => {
            fq_class_names.push(resolve_id(id, context, codebase, statements_analyzer)?)
        }
        aast::ClassId_::CIself => {
            if let Some(class) = context.function_context.calling_class {
                fq_class_names.push(class);
            }
        }
        _ => {}
    }

    if fq_class_names.is_empty() {
        return Err(AnalysisError::UserError);
    }

    for fq_class_name in fq_class_names {
        // TODO if (!$prop_name instanceof PhpParser\Node\Identifier) {

        let property_id = (fq_class_name, prop_name.to_owned());

        analysis_data
            .symbol_references
            .add_reference_to_class_member(&context.function_context, property_id, false);

        // Track member definition location for go-to-definition support
        if statements_analyzer
            .get_config()
            .collect_goto_definition_locations
        {
            analysis_data.definition_locations.insert(
                (
                    stmt_name_pos.start_offset() as u32,
                    stmt_name_pos.end_offset() as u32,
                ),
                (property_id.0, property_id.1),
            );
        }

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
                analysis_data,
                assign_value_type,
                codebase,
                &fq_class_name,
                prop_name,
            );

            let declaring_class_storage = codebase.classlike_infos.get(&fq_class_name);

            if let Some(declaring_class_storage) = declaring_class_storage {
                type_expander::expand_union(
                    codebase,
                    &Some(statements_analyzer.interner),
                    &statements_analyzer.file_analyzer.file_source.file_path,
                    &mut class_property_type,
                    &TypeExpansionOptions {
                        self_class: Some(declaring_class_storage.name),
                        static_class_type: StaticClassType::Name(declaring_class_storage.name),
                        parent_class: declaring_class_storage.direct_parent_class,
                        ..Default::default()
                    },
                    &mut analysis_data.data_flow_graph,
                    &mut 0,
                );
            }

            let mut union_comparison_result = TypeComparisonResult::new();

            let type_match_found = union_type_comparator::is_contained_by(
                codebase,
                statements_analyzer.get_file_path(),
                assign_value_type,
                &class_property_type,
                false,
                assign_value_type.ignore_falsable_issues
                    && !statements_analyzer.get_config().strict_falsable_types,
                false,
                &mut union_comparison_result,
            );

            if type_match_found
                && union_comparison_result.replacement_union_type.is_some()
                && let Some(union_type) = union_comparison_result.replacement_union_type
                && let Some(var_id) = var_id.clone()
            {
                context
                    .locals
                    .insert(VarName::new(var_id), Rc::new(union_type));
            }

            if !type_match_found && union_comparison_result.type_coerced.is_none() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidPropertyAssignmentValue,
                        format!(
                            "{}::${} with declared type {}, cannot be assigned type {}",
                            statements_analyzer
                                .interner
                                .lookup(&declaring_property_class),
                            statements_analyzer.interner.lookup(&property_id.1),
                            class_property_type.get_id(Some(statements_analyzer.interner)),
                            assign_value_type.get_id(Some(statements_analyzer.interner)),
                        ),
                        statements_analyzer.get_hpos(&stmt_class.1),
                        &context.function_context.calling_functionlike_id,
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
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::MixedPropertyTypeCoercion,
                            format!(
                                "{} expects {}, parent type {} provided",
                                var_id.clone().unwrap_or("This property".to_string()),
                                class_property_type.get_id(Some(statements_analyzer.interner)),
                                assign_value_type.get_id(Some(statements_analyzer.interner)),
                            ),
                            statements_analyzer.get_hpos(&stmt_class.1),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                } else {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::PropertyTypeCoercion,
                            format!(
                                "{} expects {}, parent type {} provided",
                                var_id.clone().unwrap_or("This property".to_string()),
                                class_property_type.get_id(Some(statements_analyzer.interner)),
                                assign_value_type.get_id(Some(statements_analyzer.interner)),
                            ),
                            statements_analyzer.get_hpos(&stmt_class.1),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }
            }

            if let Some(var_id) = var_id.clone() {
                context
                    .locals
                    .insert(VarName::new(var_id), Rc::new(assign_value_type.clone()));
            }
        }
    }

    Ok(())
}
