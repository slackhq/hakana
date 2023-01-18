use crate::typed_ast::TastInfo;
use crate::{expression_analyzer, scope_analyzer::ScopeAnalyzer};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::StrId;
use hakana_reflection_info::{t_atomic::TAtomic, t_union::TUnion};
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::{
    add_optional_union_type, get_mixed_any,
    type_expander::{self},
    wrap_atomic,
};
use oxidized::{
    aast::{self, ClassId},
    ast_defs::Pos,
};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&ClassId<(), ()>, (&Pos, &String)),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    let codebase = statements_analyzer.get_codebase();

    let const_name = expr.1 .1;
    let mut is_static = false;
    let classlike_name = match &expr.0 .2 {
        aast::ClassId_::CIexpr(lhs_expr) => {
            if let aast::Expr_::Id(id) = &lhs_expr.2 {
                match get_id_name(
                    id,
                    &context.function_context.calling_class,
                    codebase,
                    &mut is_static,
                    statements_analyzer.get_file_analyzer().resolved_names,
                ) {
                    Some(value) => value,
                    None => return false,
                }
            } else {
                let was_inside_general_use = context.inside_general_use;
                context.inside_general_use = true;

                if !expression_analyzer::analyze(
                    statements_analyzer,
                    lhs_expr,
                    tast_info,
                    context,
                    if_body_context,
                ) {
                    context.inside_general_use = was_inside_general_use;
                    return false;
                }

                context.inside_general_use = was_inside_general_use;

                let mut stmt_type = None;

                if let Some(lhs_type) = tast_info.get_expr_type(lhs_expr.pos()).cloned() {
                    for atomic_type in &lhs_type.types {
                        match atomic_type {
                            TAtomic::TNamedObject { name, is_this, .. } => {
                                stmt_type = Some(add_optional_union_type(
                                    analyse_known_class_constant(
                                        codebase,
                                        tast_info,
                                        context,
                                        &name,
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
                            TAtomic::TReference {
                                name: classlike_name,
                                ..
                            } => {
                                tast_info.maybe_add_issue(
                                    Issue::new(
                                        IssueKind::NonExistentClasslike,
                                        format!(
                                            "Unknown classlike {}",
                                            codebase.interner.lookup(classlike_name)
                                        ),
                                        statements_analyzer.get_hpos(&pos),
                                    ),
                                    statements_analyzer.get_config(),
                                    statements_analyzer.get_file_path_actual(),
                                );
                            }
                            _ => (),
                        }
                    }
                }

                tast_info.set_expr_type(&pos, stmt_type.unwrap_or(get_mixed_any()));

                return true;
            }
        }
        _ => {
            panic!()
        }
    };

    let stmt_type = analyse_known_class_constant(
        codebase,
        tast_info,
        context,
        &classlike_name,
        const_name,
        is_static,
        statements_analyzer,
        pos,
    )
    .unwrap_or(get_mixed_any());
    tast_info.set_expr_type(&pos, stmt_type);

    return true;
}

pub(crate) fn get_id_name(
    id: &Box<oxidized::ast_defs::Id>,
    calling_class: &Option<StrId>,
    codebase: &CodebaseInfo,
    is_static: &mut bool,
    resolved_names: &FxHashMap<usize, StrId>,
) -> Option<StrId> {
    Some(match id.1.as_str() {
        "self" => {
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            self_name.clone()
        }
        "parent" => {
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();
            classlike_storage.direct_parent_class.clone().unwrap()
        }
        "static" => {
            *is_static = true;
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            self_name.clone()
        }
        _ => resolved_names.get(&id.0.start_offset()).unwrap().clone(),
    })
}

fn analyse_known_class_constant(
    codebase: &CodebaseInfo,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    classlike_name: &StrId,
    const_name: &String,
    is_this: bool,
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
) -> Option<TUnion> {
    if !codebase.class_or_interface_or_enum_or_trait_exists(&classlike_name) {
        if const_name == "class" && codebase.type_definitions.contains_key(classlike_name) {
            return Some(wrap_atomic(TAtomic::TLiteralClassname {
                name: classlike_name.clone(),
            }));
        }

        tast_info.maybe_add_issue(
            if const_name == "class" {
                Issue::new(
                    IssueKind::NonExistentType,
                    format!(
                        "Unknown class {}",
                        codebase.interner.lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(&pos),
                )
            } else {
                Issue::new(
                    IssueKind::NonExistentClasslike,
                    format!(
                        "Unknown classlike {}",
                        codebase.interner.lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(&pos),
                )
            },
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return None;
    }

    if const_name == "class" {
        let inner_object = if is_this {
            let named_object = TAtomic::TNamedObject {
                name: classlike_name.clone(),
                type_params: None,
                is_this,
                extra_types: None,
                remapped_params: false,
            };
            TAtomic::TClassname {
                as_type: Box::new(named_object),
            }
        } else {
            TAtomic::TLiteralClassname {
                name: classlike_name.clone(),
            }
        };

        tast_info.symbol_references.add_reference_to_symbol(
            &context.function_context,
            classlike_name.clone(),
            false,
        );

        return Some(wrap_atomic(inner_object));
    }

    let const_name = if let Some(const_name) = codebase.interner.get(&const_name) {
        const_name
    } else {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentClassConstant,
                format!(
                    "Unknown class constant {}::{}",
                    codebase.interner.lookup(classlike_name),
                    const_name
                ),
                statements_analyzer.get_hpos(&pos),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return None;
    };

    tast_info.symbol_references.add_reference_to_class_member(
        &context.function_context,
        (classlike_name.clone(), const_name),
        false,
    );

    let classlike_storage = codebase.classlike_infos.get(classlike_name).unwrap();

    if !classlike_storage.constants.contains_key(&const_name) {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentClassConstant,
                format!(
                    "Unknown class constant {}::{}",
                    codebase.interner.lookup(classlike_name),
                    codebase.interner.lookup(&const_name),
                ),
                statements_analyzer.get_hpos(&pos),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    let mut class_constant_type =
        codebase.get_class_constant_type(&classlike_name, &const_name, FxHashSet::default());

    if let Some(ref mut class_constant_type) = class_constant_type {
        type_expander::expand_union(
            codebase,
            class_constant_type,
            &TypeExpansionOptions {
                evaluate_conditional_types: true,
                expand_generic: true,
                ..Default::default()
            },
            &mut tast_info.data_flow_graph,
        );
    }

    class_constant_type
}
