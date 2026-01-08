use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::AnalysisError;
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope::BlockContext, statements_analyzer::StatementsAnalyzer};
use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::ast::get_id_name;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TNamedObject;
use hakana_code_info::ttype::type_expander::{StaticClassType, TypeExpansionOptions};
use hakana_code_info::ttype::{
    add_optional_union_type, get_mixed_any,
    type_expander::{self},
    wrap_atomic,
};
use hakana_code_info::{t_atomic::TAtomic, t_union::TUnion};
use hakana_str::StrId;
use oxidized::{
    aast::{self, ClassId},
    ast_defs::Pos,
};
use rustc_hash::FxHashSet;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ClassId<(), ()>, (&Pos, &String)),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    let const_name = expr.1.1;
    let mut is_static = false;
    let classlike_name = match &expr.0.2 {
        aast::ClassId_::CIexpr(lhs_expr) => {
            if let aast::Expr_::Id(id) = &lhs_expr.2 {
                match get_id_name(
                    id,
                    &context.function_context.calling_class,
                    context.function_context.calling_class_final,
                    codebase,
                    &mut is_static,
                    statements_analyzer.file_analyzer.resolved_names,
                ) {
                    Some(value) => value,
                    None => return Err(AnalysisError::UserError),
                }
            } else {
                let was_inside_general_use = context.inside_general_use;
                context.inside_general_use = true;

                expression_analyzer::analyze(
                    statements_analyzer,
                    lhs_expr,
                    analysis_data,
                    context,
                    true,
                )?;

                context.inside_general_use = was_inside_general_use;

                let mut stmt_type = None;

                if let Some(lhs_type) = analysis_data.get_rc_expr_type(lhs_expr.pos()).cloned() {
                    for atomic_type in &lhs_type.types {
                        match atomic_type {
                            TAtomic::TNamedObject(TNamedObject { name, is_this, .. }) => {
                                stmt_type = Some(add_optional_union_type(
                                    analyse_known_class_constant(
                                        codebase,
                                        analysis_data,
                                        context,
                                        name,
                                        const_name,
                                        *is_this,
                                        statements_analyzer,
                                        pos,
                                    )
                                    .unwrap_or(get_mixed_any()),
                                    stmt_type.as_ref(),
                                    codebase,
                                ));
                            }
                            TAtomic::TClassPtr { as_type }
                            | TAtomic::TGenericClassPtr { as_type, .. } => {
                                if let TAtomic::TNamedObject(TNamedObject {
                                    name, is_this, ..
                                }) = &**as_type
                                {
                                    // When checking the type of a class constant accessed on a class pointer,
                                    // check its potential types across all descendants.
                                    let descendants = codebase
                                        .all_classlike_descendants
                                        .get(name)
                                        .map(|i| i.clone())
                                        .unwrap_or_default();

                                    let mut potential_constant_types = add_optional_union_type(
                                        analyse_known_class_constant(
                                            codebase,
                                            analysis_data,
                                            context,
                                            name,
                                            const_name,
                                            *is_this,
                                            statements_analyzer,
                                            pos,
                                        )
                                        .unwrap_or(get_mixed_any()),
                                        stmt_type.as_ref(),
                                        codebase,
                                    );

                                    for class_name in descendants {
                                        potential_constant_types = add_optional_union_type(
                                            potential_constant_types,
                                            analyse_known_class_constant(
                                                codebase,
                                                analysis_data,
                                                context,
                                                &class_name,
                                                const_name,
                                                false,
                                                statements_analyzer,
                                                pos,
                                            )
                                            .as_ref(),
                                            codebase,
                                        );
                                    }

                                    stmt_type = Some(potential_constant_types);
                                }
                            }
                            TAtomic::TReference {
                                name: classlike_name,
                                ..
                            } => {
                                analysis_data.symbol_references.add_reference_to_symbol(
                                    &context.function_context,
                                    *classlike_name,
                                    false,
                                );

                                analysis_data.maybe_add_issue(
                                    Issue::new(
                                        IssueKind::NonExistentClasslike,
                                        format!(
                                            "Unknown classlike {}",
                                            statements_analyzer.interner.lookup(classlike_name)
                                        ),
                                        statements_analyzer.get_hpos(pos),
                                        &context.function_context.calling_functionlike_id,
                                    ),
                                    statements_analyzer.get_config(),
                                    statements_analyzer.get_file_path_actual(),
                                );
                            }
                            _ => (),
                        }
                    }
                }

                analysis_data.set_expr_type(pos, stmt_type.unwrap_or(get_mixed_any()));

                return Ok(());
            }
        }
        _ => {
            panic!()
        }
    };

    let stmt_type = analyse_known_class_constant(
        codebase,
        analysis_data,
        context,
        &classlike_name,
        const_name,
        is_static,
        statements_analyzer,
        pos,
    )
    .unwrap_or(get_mixed_any());
    analysis_data.set_expr_type(pos, stmt_type);

    Ok(())
}

fn analyse_known_class_constant(
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    classlike_name: &StrId,
    const_name: &String,
    is_this: bool,
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
) -> Option<TUnion> {
    if !codebase.class_or_interface_or_enum_or_trait_exists(classlike_name) {
        analysis_data.symbol_references.add_reference_to_symbol(
            &context.function_context,
            *classlike_name,
            false,
        );

        if const_name == "class" && codebase.type_definitions.contains_key(classlike_name) {
            return Some(wrap_atomic(TAtomic::TLiteralClassPtr {
                name: *classlike_name,
            }));
        }

        analysis_data.maybe_add_issue(
            if const_name == "class" {
                Issue::new(
                    IssueKind::NonExistentType,
                    format!(
                        "Unknown class {}",
                        statements_analyzer.interner.lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                )
            } else {
                Issue::new(
                    IssueKind::NonExistentClasslike,
                    format!(
                        "Unknown classlike {}",
                        statements_analyzer.interner.lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(pos),
                    &context.function_context.calling_functionlike_id,
                )
            },
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return None;
    }

    if const_name == "class" {
        let inner_object = if is_this {
            let named_object = TAtomic::TNamedObject(TNamedObject {
                name: *classlike_name,
                type_params: None,
                is_this,
                remapped_params: false,
            });
            TAtomic::TClassPtr {
                as_type: Box::new(named_object),
            }
        } else {
            TAtomic::TLiteralClassPtr {
                name: *classlike_name,
            }
        };

        analysis_data.symbol_references.add_reference_to_symbol(
            &context.function_context,
            *classlike_name,
            false,
        );

        if statements_analyzer
            .get_config()
            .collect_goto_definition_locations
        {
            analysis_data.definition_locations.insert(
                (pos.start_offset() as u32, pos.end_offset() as u32),
                (*classlike_name, StrId::EMPTY),
            );
        }

        return Some(wrap_atomic(inner_object));
    }

    let const_name = if let Some(const_name) = statements_analyzer.interner.get(const_name) {
        const_name
    } else {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentClassConstant,
                format!(
                    "Unknown class constant {}::{}",
                    statements_analyzer.interner.lookup(classlike_name),
                    const_name
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return None;
    };

    analysis_data
        .symbol_references
        .add_reference_to_class_member(
            &context.function_context,
            (*classlike_name, const_name),
            false,
        );

    // Track member definition location for go-to-definition support
    if statements_analyzer
        .get_config()
        .collect_goto_definition_locations
    {
        analysis_data.definition_locations.insert(
            (pos.start_offset() as u32, pos.end_offset() as u32),
            (*classlike_name, const_name),
        );
    }

    let classlike_storage = codebase.classlike_infos.get(classlike_name).unwrap();

    if !classlike_storage.constants.contains_key(&const_name) {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentClassConstant,
                format!(
                    "Unknown class constant {}::{}",
                    statements_analyzer.interner.lookup(classlike_name),
                    statements_analyzer.interner.lookup(&const_name),
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let mut class_constant_type = codebase.get_class_constant_type(
        classlike_name,
        is_this,
        &const_name,
        FxHashSet::default(),
    );

    if let Some(ref mut class_constant_type) = class_constant_type {
        let this_class = TAtomic::TNamedObject(TNamedObject {
            name: *classlike_name,
            type_params: None,
            is_this,
            remapped_params: false,
        });
        type_expander::expand_union(
            codebase,
            &Some(statements_analyzer.interner),
            statements_analyzer.get_file_path(),
            class_constant_type,
            &TypeExpansionOptions {
                evaluate_conditional_types: true,
                expand_generic: true,
                self_class: Some(*classlike_name),
                static_class_type: StaticClassType::Object(&this_class),
                parent_class: None,
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
            &mut 0,
        );
    }

    class_constant_type
}

/// Check whether the given expression is a class<T> pointer literal and raise an issue if so.
pub(crate) fn check_class_ptr_used_as_string(
    statements_analyzer: &StatementsAnalyzer,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    expr_type: &TUnion,
    expr: &aast::Expr<(), ()>,
) {
    let is_literal_class_ptr = expr_type
        .types
        .iter()
        .all(|t| matches!(t, TAtomic::TLiteralClassPtr { name: _ }));

    if is_literal_class_ptr
        && let Some(class_name) = get_class_name_from_class_ptr_literal_expr(expr)
    {
        emit_class_pointer_used_as_string(
            statements_analyzer,
            context,
            analysis_data,
            expr,
            class_name,
        );
    }
}

pub(crate) fn emit_class_pointer_used_as_string(
    statements_analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    expr: &aast::Expr<(), ()>,
    class_name: &str,
) {
    let pos = expr.pos();
    let issue = Issue::new(
        IssueKind::ClassnameUsedAsString,
        format!(
            "Using {} in this position will lead to an implicit runtime conversion to string, please use \"nameof {}\" instead",
            class_name, class_name
        ),
        statements_analyzer.get_hpos(pos),
        &context.function_context.calling_functionlike_id,
    );

    let config = statements_analyzer.get_config();

    if config.issues_to_fix.contains(&issue.kind) && !config.add_fixmes {
        // Only replace code that's not already covered by a FIXME
        if !context
            .function_context
            .is_production(statements_analyzer.codebase)
            || analysis_data.get_matching_hakana_fixme(&issue).is_none()
        {
            let nameof_expr = format!("nameof {}", class_name);
            analysis_data.add_replacement(
                (pos.start_offset() as u32, pos.end_offset() as u32),
                Replacement::Substitute(nameof_expr),
            );
        }
    } else {
        analysis_data.maybe_add_issue(issue, config, statements_analyzer.get_file_path_actual());
    }
}

/// Extract the referenced class name from a class<T> pointer literal expression in the form of C::class.
/// Returns `None` if the class name cannot be determined.
pub(crate) fn get_class_name_from_class_ptr_literal_expr(
    expr: &aast::Expr<(), ()>,
) -> Option<&str> {
    let aast::Expr_::ClassConst(class_id) = &expr.2 else {
        return None;
    };

    let aast::ClassId_::CIexpr(ci_expr) = &(**class_id).0.2 else {
        return None;
    };

    if let aast::Expr_::Id(inner_class_id) = &ci_expr.2 {
        Some(inner_class_id.name())
    } else {
        None
    }
}
