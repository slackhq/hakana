use crate::scope_analyzer::ScopeAnalyzer;
use crate::{expr::call::arguments_analyzer::get_template_types_for_call, typed_ast::TastInfo};
use crate::{scope_context::ScopeContext, statements_analyzer::StatementsAnalyzer};
use hakana_reflection_info::code_location::HPos;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::{
    classlike_info::ClassLikeInfo,
    codebase_info::CodebaseInfo,
    data_flow::{
        node::DataFlowNode,
        path::{PathExpressionKind, PathKind},
    },
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_reflection_info::{Interner, StrId};
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::{
    add_optional_union_type, get_mixed_any,
    template::{inferred_type_replacer, TemplateResult},
    type_expander::{self, StaticClassType},
};
use indexmap::IndexMap;
use oxidized::{aast::Expr, ast_defs::Pos};
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (&Expr<(), ()>, &Expr<(), ()>),
    pos: &Pos,
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    in_assignment: bool,
    lhs_type_part: TAtomic,
    prop_name: &str,
    var_id: &Option<String>,
    lhs_var_id: &Option<String>,
) -> bool {
    if lhs_type_part.is_mixed() {
        tast_info.set_expr_type(&expr.0.pos(), get_mixed_any());
    }

    let codebase = statements_analyzer.get_codebase();

    let classlike_name = match &lhs_type_part {
        TAtomic::TNamedObject { name, .. } => name.clone(),
        TAtomic::TReference {
            name: classlike_name,
            ..
        } => {
            tast_info.symbol_references.add_reference_to_symbol(
                &context.function_context,
                *classlike_name,
                false,
            );

            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::NonExistentClass,
                    format!(
                        "Cannot access property on undefined class {}",
                        codebase.interner.lookup(classlike_name)
                    ),
                    statements_analyzer.get_hpos(&pos),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
            return false;
        }
        _ => {
            return false;
        }
    };

    let prop_name = if let Some(prop_name) = codebase.interner.get(prop_name) {
        prop_name
    } else {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentProperty,
                format!(
                    "Cannot access undefined property {}::${}",
                    codebase.interner.lookup(&classlike_name),
                    prop_name,
                ),
                statements_analyzer.get_hpos(&pos),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return true;
    };

    if !codebase.property_exists(&classlike_name, &prop_name) {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentProperty,
                format!(
                    "Cannot access undefined property {}::${}",
                    codebase.interner.lookup(&classlike_name),
                    codebase.interner.lookup(&prop_name)
                ),
                statements_analyzer.get_hpos(&pos),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return false;
    }

    tast_info.symbol_references.add_reference_to_class_member(
        &context.function_context,
        (classlike_name.clone(), prop_name),
        false,
    );

    let declaring_property_class =
        codebase.get_declaring_class_for_property(&classlike_name, &prop_name);

    if let None = declaring_property_class {
        return false;
    }

    // TODO: self::propertyFetchCanBeAnalyzed

    // TODO: handleNonExistentProperty

    // let's do getClassPropertyType

    let mut class_property_type = get_class_property_type(
        statements_analyzer,
        &classlike_name,
        &prop_name,
        declaring_property_class.unwrap(),
        lhs_type_part,
        tast_info,
    );

    // if (!$context->collect_mutations
    //     && !$context->collect_initializations
    //     && !($class_storage->external_mutation_free
    //         && $class_property_type->allow_mutations)
    // ) {
    //     if ($context->pure) {
    //         IssueBuffer::maybeAdd(
    //             new ImpurePropertyFetch(
    //                 'Cannot access a property on a mutable object from a pure context',
    //                 new CodeLocation($statements_analyzer, $stmt)
    //             ),
    //             $statements_analyzer->getSuppressedIssues()
    //         );
    //     } elseif ($statements_analyzer->getSource() instanceof FunctionLikeAnalyzer
    //         && $statements_analyzer->getSource()->track_mutations
    //     ) {
    //         $statements_analyzer->getSource()->inferred_impure = true;
    //     }
    // }

    let property_id = (classlike_name, prop_name.clone());

    if let Some(classlike_storage) = codebase.classlike_infos.get(&property_id.0) {
        class_property_type = add_property_dataflow(
            statements_analyzer,
            expr.0.pos(),
            pos,
            tast_info,
            classlike_storage,
            class_property_type.clone(),
            in_assignment,
            &property_id,
            lhs_var_id,
            var_id,
        );
    }

    // if ($class_storage->mutation_free) {
    //     $class_property_type->has_mutations = false;
    // }

    tast_info.set_expr_type(
        &pos,
        add_optional_union_type(class_property_type, tast_info.get_expr_type(pos), codebase),
    );

    true
}

fn get_class_property_type(
    statements_analyzer: &StatementsAnalyzer,
    classlike_name: &StrId,
    property_name: &StrId,
    declaring_property_class: &StrId,
    mut lhs_type_part: TAtomic,
    tast_info: &mut TastInfo,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();
    let class_property_type = codebase.get_property_type(&classlike_name, &property_name);

    let class_storage = codebase.classlike_infos.get(classlike_name).unwrap();
    let declaring_class_storage = codebase
        .classlike_infos
        .get(declaring_property_class)
        .unwrap();
    if let Some(mut class_property_type) = class_property_type {
        let parent_class = declaring_class_storage.direct_parent_class.clone();
        type_expander::expand_union(
            codebase,
            &mut class_property_type,
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

        if !declaring_class_storage.template_types.is_empty() {
            if let TAtomic::TNamedObject {
                name,
                type_params: None,
                ..
            } = &lhs_type_part
            {
                let mut type_params = vec![];

                for (_, type_map) in &declaring_class_storage.template_types {
                    type_params.push((**type_map.iter().next().unwrap().1).clone());
                }

                lhs_type_part = TAtomic::TNamedObject {
                    name: name.clone(),
                    type_params: Some(type_params),
                    is_this: false,
                    extra_types: None,
                    remapped_params: false,
                };
            }

            class_property_type = localize_property_type(
                codebase,
                class_property_type,
                &lhs_type_part,
                class_storage,
                declaring_class_storage,
                tast_info,
            );
        } else if let TAtomic::TNamedObject {
            type_params: Some(_),
            ..
        } = &lhs_type_part
        {
            class_property_type = localize_property_type(
                codebase,
                class_property_type,
                &lhs_type_part,
                class_storage,
                declaring_class_storage,
                tast_info,
            );
        }

        return class_property_type;
    } else {
        // send out a MissingPropertyType issue buffer error
    }

    return get_mixed_any();
}

pub(crate) fn localize_property_type(
    codebase: &CodebaseInfo,
    mut class_property_type: TUnion,
    lhs_type_part: &TAtomic,
    property_class_storage: &ClassLikeInfo,
    property_declaring_class_storage: &ClassLikeInfo,
    tast_info: &mut TastInfo,
) -> TUnion {
    if let TAtomic::TNamedObject {
        type_params: Some(lhs_type_params),
        ..
    } = lhs_type_part
    {
        let mut template_types = get_template_types_for_call(
            codebase,
            tast_info,
            Some(property_declaring_class_storage),
            Some(&property_declaring_class_storage.name),
            Some(property_class_storage),
            &property_class_storage.template_types,
            &IndexMap::new(),
        );

        let extended_types = &property_class_storage.template_extended_params;

        if !template_types.is_empty() {
            if !property_class_storage.template_types.is_empty() {
                for (param_offset, lhs_param_type) in lhs_type_params.iter().enumerate() {
                    let mut i = -1;

                    for (calling_param_name, _) in &property_class_storage.template_types {
                        i += 1;

                        if i == (param_offset as i32) {
                            template_types
                                .entry(calling_param_name.clone())
                                .or_insert_with(FxHashMap::default)
                                .insert(
                                    property_class_storage.name.clone(),
                                    lhs_param_type.clone(),
                                );
                            break;
                        }
                    }
                }
            }
        }

        let template_type_keys = template_types
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();

        for type_name in template_type_keys {
            if let Some(mapped_type) = extended_types
                .get(&property_declaring_class_storage.name)
                .unwrap_or(&IndexMap::new())
                .get(&type_name)
            {
                for mapped_type_atomic in &mapped_type.types {
                    if let TAtomic::TGenericParam { param_name, .. } = &mapped_type_atomic {
                        let position = property_class_storage
                            .template_types
                            .get_index_of(param_name);

                        if let Some(position) = position {
                            if let Some(mapped_param) = lhs_type_params.get(position) {
                                template_types
                                    .entry(type_name.clone())
                                    .or_insert_with(FxHashMap::default)
                                    .insert(
                                        property_declaring_class_storage.name.clone(),
                                        mapped_param.clone(),
                                    );
                            }
                        }
                    }
                }
            }
        }

        class_property_type = inferred_type_replacer::replace(
            &class_property_type,
            &TemplateResult::new(IndexMap::new(), template_types),
            codebase,
        );
    }

    class_property_type
}

fn add_property_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    lhs_pos: &Pos,
    pos: &Pos,
    tast_info: &mut TastInfo,
    classlike_storage: &ClassLikeInfo,
    stmt_type: TUnion,
    in_assignment: bool,
    property_id: &(StrId, StrId),
    lhs_var_id: &Option<String>,
    expr_id: &Option<String>,
) -> TUnion {
    let interner = &statements_analyzer.get_codebase().interner;

    if classlike_storage.specialize_instance {
        if let Some(lhs_var_id) = lhs_var_id {
            let var_type = tast_info
                .expr_types
                .get(&(lhs_pos.start_offset(), lhs_pos.end_offset()));

            let var_node = DataFlowNode::get_for_assignment(
                lhs_var_id.clone(),
                statements_analyzer.get_hpos(lhs_pos),
            );
            tast_info.data_flow_graph.add_node(var_node.clone());

            let property_node = DataFlowNode::get_for_assignment(
                if let Some(expr_id) = expr_id {
                    expr_id.clone()
                } else {
                    format!("{}->$property", lhs_var_id)
                },
                statements_analyzer.get_hpos(pos),
            );
            tast_info.data_flow_graph.add_node(property_node.clone());

            tast_info.data_flow_graph.add_path(
                &var_node,
                &property_node,
                PathKind::ExpressionFetch(
                    PathExpressionKind::Property,
                    interner.lookup(&property_id.1).to_string(),
                ),
                None,
                None,
            );

            if let Some(var_type) = var_type {
                for parent_node in var_type.parent_nodes.iter() {
                    tast_info.data_flow_graph.add_path(
                        parent_node,
                        &var_node,
                        PathKind::Default,
                        None,
                        None,
                    );
                }

                let mut stmt_type = stmt_type.clone();
                stmt_type.parent_nodes = FxHashSet::from_iter([property_node.clone()]);

                return stmt_type;
            }
        }

        return stmt_type;
    } else {
        let stmt_type = add_unspecialized_property_fetch_dataflow(
            expr_id,
            property_id,
            statements_analyzer.get_hpos(pos),
            tast_info,
            in_assignment,
            stmt_type,
            &statements_analyzer.get_codebase().interner,
        );

        return stmt_type;
    }
}

pub(crate) fn add_unspecialized_property_fetch_dataflow(
    expr_id: &Option<String>,
    property_id: &(StrId, StrId),
    pos: HPos,
    tast_info: &mut TastInfo,
    in_assignment: bool,
    stmt_type: TUnion,
    interner: &Interner,
) -> TUnion {
    let localized_property_node = DataFlowNode::get_for_assignment(
        if let Some(expr_id) = expr_id {
            expr_id.clone()
        } else {
            format!(
                "{}::${}",
                interner.lookup(&property_id.0),
                interner.lookup(&property_id.1)
            )
        },
        pos,
    );

    tast_info
        .data_flow_graph
        .add_node(localized_property_node.clone());

    let label = format!(
        "{}::${}",
        interner.lookup(&property_id.0),
        interner.lookup(&property_id.1)
    );

    let property_node = DataFlowNode::new(label.clone(), label, None, None);

    if in_assignment {
        tast_info.data_flow_graph.add_path(
            &property_node,
            &localized_property_node,
            PathKind::ExpressionAssignment(
                PathExpressionKind::Property,
                interner.lookup(&property_id.1).to_string(),
            ),
            None,
            None,
        );
    } else {
        tast_info.data_flow_graph.add_path(
            &property_node,
            &localized_property_node,
            PathKind::ExpressionFetch(
                PathExpressionKind::Property,
                interner.lookup(&property_id.1).to_string(),
            ),
            None,
            None,
        );
    }

    let mut stmt_type = stmt_type.clone();

    stmt_type.parent_nodes = FxHashSet::from_iter([localized_property_node.clone()]);

    stmt_type
}
