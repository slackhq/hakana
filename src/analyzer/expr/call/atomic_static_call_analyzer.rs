use hakana_code_info::{
    EFFECT_IMPURE,
    classlike_info::ClassLikeInfo,
    function_context::FunctionContext,
    issue::{Issue, IssueKind},
    t_atomic::{TAtomic, TGenericParam, TNamedObject},
    t_union::TUnion,
    ttype::{add_optional_union_type, intersect_union_types_simple},
};
use hakana_str::StrId;
use oxidized::{aast, ast_defs::Pos};

use crate::{
    function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
    stmt_analyzer::AnalysisError,
};

use super::{
    atomic_method_call_analyzer::AtomicMethodCallAnalysisResult,
    existing_atomic_method_call_analyzer,
};

/**
 * This is a bunch of complex logic to handle the potential for missing methods and intersection types.
 *
 * The happy path (i.e 99% of method calls) is handled in ExistingAtomicMethodCallAnalyzer
 *
 * @internal
 */
pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::ClassId<(), ()>,
        &(Pos, String),
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    lhs_type_part: &TAtomic,
    classlike_name: Option<StrId>,
    result: &mut AtomicMethodCallAnalysisResult,
) -> Result<(), AnalysisError> {
    // Extract classlike names from the type, handling both simple named objects and intersections
    let classlike_names: Vec<StrId> = if let Some(name) = classlike_name {
        vec![name]
    } else {
        match &lhs_type_part {
            TAtomic::TNamedObject(TNamedObject { name, .. }) => vec![*name],
            TAtomic::TObjectIntersection { types } => types
                .iter()
                .filter_map(|t| {
                    if let TAtomic::TNamedObject(TNamedObject { name, .. }) = t {
                        Some(*name)
                    } else {
                        None
                    }
                })
                .collect(),
            // During the migration from classname<T> strings to class<T> pointers,
            // support invoking `new` with an LHS expression of either type.
            // This is governed by the typechecker flag `class_pointer_ban_classname_static_meth`.
            TAtomic::TClassname { as_type, .. }
            | TAtomic::TGenericClassname { as_type, .. }
            | TAtomic::TClassPtr { as_type }
            | TAtomic::TGenericClassPtr { as_type, .. } => {
                let as_type = *as_type.clone();
                if let TAtomic::TNamedObject(TNamedObject { name, .. }) = as_type {
                    vec![name]
                } else {
                    return Ok(());
                }
            }
            TAtomic::TLiteralClassname { name } | TAtomic::TLiteralClassPtr { name } => {
                vec![*name]
            }
            TAtomic::TGenericParam(TGenericParam { as_type, .. })
            | TAtomic::TClassTypeConstant { as_type, .. } => {
                if let TAtomic::TNamedObject(TNamedObject { name, .. }) =
                    &as_type.types.first().unwrap()
                {
                    vec![*name]
                } else {
                    return Ok(());
                }
            }
            _ => {
                if lhs_type_part.is_mixed() {
                    analysis_data.maybe_add_issue(
                        Issue::new(
                            IssueKind::MixedMethodCall,
                            "Method called on unknown object".to_string(),
                            statements_analyzer.get_hpos(pos),
                            &context.function_context.calling_functionlike_id,
                        ),
                        statements_analyzer.get_config(),
                        statements_analyzer.get_file_path_actual(),
                    );
                }

                return Ok(());
            }
        }
    };

    if classlike_names.is_empty() {
        return Ok(());
    }

    handle_static_call_on_named_objects(
        statements_analyzer,
        classlike_names,
        expr,
        pos,
        analysis_data,
        context,
        lhs_type_part,
        result,
    )
}

/// Handle static method calls on named objects, supporting intersection types.
/// This mirrors handle_method_call_on_named_object for instance method calls.
fn handle_static_call_on_named_objects(
    statements_analyzer: &StatementsAnalyzer,
    mut classlike_names: Vec<StrId>,
    expr: (
        &aast::ClassId<(), ()>,
        &(Pos, String),
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    lhs_type_part: &TAtomic,
    result: &mut AtomicMethodCallAnalysisResult,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    // Verify all classes exist
    for classlike_name in &classlike_names {
        if !codebase.class_or_interface_or_enum_or_trait_exists(classlike_name) {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentClass,
                    format!(
                        "Class or interface {} does not exist",
                        statements_analyzer.interner.lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            let method_name = statements_analyzer.interner.get(&expr.1.1);

            if let Some(method_name) = method_name {
                analysis_data
                    .symbol_references
                    .add_reference_to_class_member(
                        &context.function_context,
                        (*classlike_name, method_name),
                        false,
                    );
            } else {
                analysis_data.symbol_references.add_reference_to_symbol(
                    &context.function_context,
                    *classlike_name,
                    false,
                );
            }

            return Ok(());
        }
    }

    // Use the first classlike name for error messages and goto-definition
    let first_classlike_name = classlike_names[0];

    let method_name = if let Some(method_name) = statements_analyzer.interner.get(&expr.1.1) {
        method_name
    } else {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentMethod,
                format!(
                    "Method {}::{} does not exist",
                    statements_analyzer.interner.lookup(&first_classlike_name),
                    &expr.1.1
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
        return Ok(());
    };

    let all_classlike_names = classlike_names.clone();

    // Retain only classes that have the method
    classlike_names.retain(|n| codebase.method_exists(n, &method_name));

    if classlike_names.is_empty() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentMethod,
                format!(
                    "Method {}::{} does not exist",
                    statements_analyzer.interner.lookup(&first_classlike_name),
                    &expr.1.1
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        analysis_data.expr_effects.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            EFFECT_IMPURE,
        );

        for classlike_name in all_classlike_names {
            analysis_data
                .symbol_references
                .add_reference_to_class_member(
                    &context.function_context,
                    (classlike_name, method_name),
                    false,
                );

            if let Some(classlike_info) = codebase.classlike_infos.get(&classlike_name) {
                add_missing_method_refs(
                    classlike_info,
                    analysis_data,
                    &context.function_context,
                    method_name,
                );
            }
        }

        return Ok(());
    }

    if statements_analyzer
        .get_config()
        .collect_goto_definition_locations
    {
        for classlike_name in &classlike_names {
            analysis_data.definition_locations.insert(
                (expr.0.1.start_offset() as u32, expr.0.1.end_offset() as u32),
                (*classlike_name, StrId::EMPTY),
            );
        }
    }

    // Get return types from all classes in the intersection and intersect them
    let mut return_type_candidate: Option<TUnion> = None;

    for classlike_name in &classlike_names {
        let class_return_type = existing_atomic_method_call_analyzer::analyze(
            statements_analyzer,
            *classlike_name,
            &method_name,
            None,
            (expr.2, expr.3, expr.4),
            lhs_type_part,
            pos,
            Some(&expr.1.0),
            analysis_data,
            context,
            None,
        )?;

        return_type_candidate = Some(match return_type_candidate {
            None => class_return_type,
            Some(existing) => {
                // Intersect the return types from multiple classes
                intersect_union_types_simple(&existing, &class_return_type, codebase)
                    .unwrap_or(existing)
            }
        });
    }

    if let Some(return_type_candidate) = return_type_candidate {
        result.return_type = Some(add_optional_union_type(
            return_type_candidate,
            result.return_type.as_ref(),
            codebase,
        ));
    }

    Ok(())
}

pub(crate) fn add_missing_method_refs(
    classlike_info: &ClassLikeInfo,
    analysis_data: &mut FunctionAnalysisData,
    function_context: &FunctionContext,
    method_name: StrId,
) {
    for parent_name in &classlike_info.all_parent_classes {
        analysis_data
            .symbol_references
            .add_reference_to_class_member(function_context, (*parent_name, method_name), false);
    }

    if classlike_info.is_abstract {
        for parent_name in &classlike_info.all_parent_interfaces {
            analysis_data
                .symbol_references
                .add_reference_to_class_member(
                    function_context,
                    (*parent_name, method_name),
                    false,
                );
        }
    }

    for trait_name in &classlike_info.used_traits {
        analysis_data
            .symbol_references
            .add_reference_to_class_member(function_context, (*trait_name, method_name), false);
    }
}
