use std::sync::Arc;

use hakana_code_info::classlike_info::Variance;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::ttype::type_expander::TypeExpansionOptions;
use hakana_code_info::{GenericParent, EFFECT_WRITE_GLOBALS};

use hakana_code_info::data_flow::node::DataFlowNode;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::ttype::template::standin_type_replacer::get_most_specific_type_from_bounds;
use hakana_str::StrId;
use rustc_hash::FxHashMap;

use crate::expr::call_analyzer::{check_method_args, get_generic_param_for_offset};
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::method_identifier::MethodIdentifier;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::{populate_union_type, TUnion};
use hakana_code_info::ttype::template::{self, TemplateBound, TemplateResult};
use hakana_code_info::ttype::{
    add_optional_union_type, get_mixed_any, get_named_object, get_nothing, get_placeholder,
    type_expander, wrap_atomic,
};
use hakana_reflector::typehint_resolver::get_type_from_hint;
use indexmap::IndexMap;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::atomic_method_call_analyzer::AtomicMethodCallAnalysisResult;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::ClassId<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<aast::Expr<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    //let method_id = None;

    let codebase = statements_analyzer.codebase;

    let mut can_extend = false;

    let lhs_type = match &expr.0 .2 {
        aast::ClassId_::CIexpr(lhs_expr) => {
            if let aast::Expr_::Id(id) = &lhs_expr.2 {
                let name_string = id.1.clone();
                match name_string.as_str() {
                    "self" => {
                        let self_name = &context.function_context.calling_class.unwrap();

                        get_named_object(*self_name, None)
                    }
                    "parent" => {
                        let self_name = &context.function_context.calling_class.unwrap();

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        get_named_object(classlike_storage.direct_parent_class.unwrap(), None)
                    }
                    "static" => {
                        let self_name = &context.function_context.calling_class.unwrap();

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        if !classlike_storage.is_final {
                            can_extend = true;
                        }

                        wrap_atomic(TAtomic::TNamedObject {
                            name: *self_name,
                            type_params: None,
                            is_this: !classlike_storage.is_final,
                            extra_types: None,
                            remapped_params: false,
                        })
                    }
                    _ => {
                        let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

                        let name_string = if let Some(resolved_name) =
                            resolved_names.get(&(id.0.start_offset() as u32))
                        {
                            *resolved_name
                        } else {
                            return Err(AnalysisError::InternalError(
                                "Unable to resolve new constructor class name".to_string(),
                                statements_analyzer.get_hpos(pos),
                            ));
                        };

                        let type_resolution_context =
                            statements_analyzer.get_type_resolution_context();

                        get_named_object(name_string, Some(type_resolution_context))
                    }
                }
            } else {
                let was_inside_general_use = context.inside_general_use;
                context.inside_general_use = true;
                expression_analyzer::analyze(
                    statements_analyzer,
                    lhs_expr,
                    analysis_data,
                    context,
                )?;
                context.inside_general_use = was_inside_general_use;
                analysis_data
                    .get_expr_type(&lhs_expr.1)
                    .cloned()
                    .unwrap_or(get_mixed_any())
            }
        }
        _ => {
            panic!("cannot get here")
        }
    };

    let mut result = AtomicMethodCallAnalysisResult::new();

    for lhs_type_part in &lhs_type.types {
        analyze_atomic(
            statements_analyzer,
            expr,
            pos,
            analysis_data,
            context,
            lhs_type_part,
            can_extend,
            &mut result,
        )?;
    }

    let mut return_type = result.return_type.unwrap_or(get_mixed_any());
    return_type.reference_free = true;

    analysis_data.set_expr_type(pos, return_type);

    Ok(())
}

fn analyze_atomic(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::ClassId<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<aast::Expr<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    lhs_type_part: &TAtomic,
    can_extend: bool,
    result: &mut AtomicMethodCallAnalysisResult,
) -> Result<(), AnalysisError> {
    let mut from_static = false;
    let mut from_classname = false;

    let classlike_name = match &lhs_type_part {
        TAtomic::TNamedObject { name, is_this, .. } => {
            from_static = *is_this;
            // todo check class name and register usage
            *name
        }
        TAtomic::TClassname { as_type, .. } | TAtomic::TGenericClassname { as_type, .. } => {
            let as_type = *as_type.clone();
            if let TAtomic::TNamedObject { name, .. } = as_type {
                from_classname = true;

                name
            } else {
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

                return Ok(());
            }
        }
        TAtomic::TLiteralClassname { name } => *name,
        TAtomic::TGenericParam { as_type, .. } | TAtomic::TClassTypeConstant { as_type, .. } => {
            let generic_param_type = &as_type.types[0];
            if let TAtomic::TNamedObject { name, .. } = generic_param_type {
                *name
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

            // todo handle nonobject call
            return Ok(());
        }
    };

    match classlike_name {
        StrId::REFLECTION_CLASS | StrId::REFLECTION_FUNCTION | StrId::REFLECTION_TYPE_ALIAS => {
            analysis_data.expr_effects.insert(
                (pos.start_offset() as u32, pos.end_offset() as u32),
                EFFECT_WRITE_GLOBALS,
            );
        }
        _ => {}
    }

    analyze_named_constructor(
        statements_analyzer,
        expr,
        pos,
        analysis_data,
        context,
        classlike_name,
        from_static,
        from_classname,
        can_extend,
        result,
    )
}

fn analyze_named_constructor(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::ClassId<(), ()>,
        &Vec<aast::Targ<()>>,
        &Vec<aast::Expr<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    classlike_name: StrId,
    from_static: bool,
    from_classname: bool,
    can_extend: bool,
    result: &mut AtomicMethodCallAnalysisResult,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;
    let storage = if let Some(storage) = codebase.classlike_infos.get(&classlike_name) {
        storage
    } else {
        analysis_data.symbol_references.add_reference_to_symbol(
            &context.function_context,
            classlike_name,
            false,
        );

        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NonExistentClass,
                format!(
                    "Cannot call new on undefined class {}",
                    statements_analyzer.interner.lookup(&classlike_name)
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return Ok(());
    };

    if from_static {
        // todo check for unsafe instantiation
    }

    if storage.is_abstract && !can_extend && !from_classname {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::AbstractInstantiation,
                format!(
                    "Cannot call new on abstract class {}",
                    statements_analyzer.interner.lookup(&classlike_name)
                ),
                statements_analyzer.get_hpos(pos),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    if storage.is_deprecated
        && if let Some(calling_class) = &context.function_context.calling_class {
            calling_class != &classlike_name
        } else {
            true
        }
    {
        // todo complain about deprecated class
    }

    let mut generic_type_params = None;

    let method_name = StrId::CONSTRUCT;
    let method_id = MethodIdentifier(classlike_name, method_name);
    let declaring_method_id = codebase.get_declaring_method_id(&method_id);

    analysis_data
        .symbol_references
        .add_reference_to_class_member(
            &context.function_context,
            (classlike_name, method_id.1),
            false,
        );

    if codebase.method_exists(&method_id.0, &method_id.1) {
        let declaring_method_id = codebase.get_declaring_method_id(&method_id);

        analysis_data
            .symbol_references
            .add_reference_to_class_member(
                &context.function_context,
                (declaring_method_id.0, declaring_method_id.1),
                false,
            );

        let mut template_result = TemplateResult::new(
            if expr.1.is_empty() {
                IndexMap::new()
            } else {
                IndexMap::from_iter(storage.template_types.clone())
            },
            IndexMap::new(),
        );

        let Some(method_storage) = codebase.get_method(&declaring_method_id) else {
            return Err(AnalysisError::InternalError(
                "Could not load method storage".to_string(),
                statements_analyzer.get_hpos(pos),
            ));
        };

        check_method_args(
            statements_analyzer,
            analysis_data,
            &method_id,
            method_storage,
            (
                expr.1,
                &expr
                    .2
                    .iter()
                    .map(|arg_expr| (ast_defs::ParamKind::Pnormal, arg_expr.clone()))
                    .collect::<Vec<_>>(),
                expr.3,
            ),
            None,
            &mut template_result,
            context,
            pos,
            None,
        )?;

        // todo check method visibility

        // todo check purity

        if !storage.template_types.is_empty() {
            let mut v = vec![];

            for (i, (template_name, base_type_map)) in storage.template_types.iter().enumerate() {
                let mut param_type = if let Some(type_arg) = expr.1.get(i) {
                    get_type_from_hint(
                        &type_arg.1 .1,
                        context.function_context.calling_class.as_ref(),
                        statements_analyzer.get_type_resolution_context(),
                        statements_analyzer.get_file_analyzer().resolved_names,
                        *statements_analyzer.get_file_path(),
                        type_arg.1 .0.start_offset() as u32,
                    )
                    .unwrap()
                } else {
                    get_placeholder()
                };

                if param_type.is_placeholder() {
                    if !storage.template_readonly.contains(template_name) {
                        if let Some((template_name, map)) =
                            template_result.template_types.get_index(i)
                        {
                            let placeholder_name =
                                format!("`_{}", analysis_data.type_variable_bounds.len());

                            let upper_bound = (*map.iter().next().unwrap().1).clone();

                            let mut placeholder_lower_bounds = vec![];

                            if let Some(bounds) = template_result.lower_bounds.get(template_name) {
                                if let Some(bounds) =
                                    bounds.get(&GenericParent::ClassLike(classlike_name))
                                {
                                    for bound in bounds {
                                        placeholder_lower_bounds.push(bound.clone());
                                    }
                                }
                            }

                            analysis_data.type_variable_bounds.insert(
                                placeholder_name.clone(),
                                (
                                    placeholder_lower_bounds,
                                    vec![TemplateBound {
                                        bound_type: upper_bound,
                                        appearance_depth: 0,
                                        arg_offset: None,
                                        equality_bound_classlike: None,
                                        pos: Some(statements_analyzer.get_hpos(pos)),
                                    }],
                                ),
                            );

                            template_result.lower_bounds.insert(
                                *template_name,
                                map.iter()
                                    .map(|(entity, _)| {
                                        (
                                            *entity,
                                            vec![TemplateBound::new(
                                                wrap_atomic(TAtomic::TTypeVariable {
                                                    name: placeholder_name.clone(),
                                                }),
                                                0,
                                                None,
                                                None,
                                            )],
                                        )
                                    })
                                    .collect::<FxHashMap<_, _>>(),
                            );
                        }
                    }
                } else {
                    populate_union_type(
                        &mut param_type,
                        &statements_analyzer.codebase.symbols,
                        &context
                            .function_context
                            .get_reference_source(&statements_analyzer.get_file_path().0),
                        &mut analysis_data.symbol_references,
                        false,
                    );

                    type_expander::expand_union(
                        statements_analyzer.codebase,
                        &Some(statements_analyzer.interner),
                        &mut param_type,
                        &TypeExpansionOptions {
                            parent_class: None,
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

                    if let Some((template_name, map)) = template_result.template_types.get_index(i)
                    {
                        template_result.lower_bounds.insert(
                            *template_name,
                            map.iter()
                                .map(|(entity, _)| {
                                    (
                                        *entity,
                                        vec![TemplateBound::new(param_type.clone(), 0, None, None)],
                                    )
                                })
                                .collect::<FxHashMap<_, _>>(),
                        );
                    }
                }

                let mut generic_param_type = if let Some(template_bounds) =
                    if let Some(result_map) = template_result.lower_bounds.get(template_name) {
                        result_map.get(&GenericParent::ClassLike(classlike_name))
                    } else {
                        None
                    } {
                    template::standin_type_replacer::get_most_specific_type_from_bounds(
                        template_bounds,
                        codebase,
                    )
                } else if !storage.template_extended_params.is_empty()
                    && !template_result.lower_bounds.is_empty()
                {
                    let found_generic_params = template_result
                        .lower_bounds
                        .iter()
                        .map(|(key, type_map)| {
                            (
                                *key,
                                type_map
                                    .iter()
                                    .map(|(map_key, bounds)| {
                                        (
                                            *map_key,
                                            Arc::new(get_most_specific_type_from_bounds(
                                                bounds, codebase,
                                            )),
                                        )
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<FxHashMap<_, _>>();

                    (*get_generic_param_for_offset(
                        &classlike_name,
                        template_name,
                        &storage.template_extended_params,
                        &found_generic_params,
                    ))
                    .clone()
                } else if let Some(Variance::Contravariant) = storage.generic_variance.get(&i) {
                    get_nothing()
                } else {
                    (*base_type_map.iter().next().unwrap().1).clone()
                };

                generic_param_type.had_template = true;

                v.push(generic_param_type);
            }

            generic_type_params = Some(v);
        }
    } else {
        if !expr.2.is_empty() {
            // todo complain about too many arguments
        }

        generic_type_params = if !storage.template_types.is_empty() {
            Some(if expr.1.len() == storage.template_types.len() {
                let mut generic_params = Vec::new();

                if !expr.1.is_empty() {
                    for type_arg in expr.1.iter() {
                        let mut param_type = get_type_from_hint(
                            &type_arg.1 .1,
                            context.function_context.calling_class.as_ref(),
                            statements_analyzer.get_type_resolution_context(),
                            statements_analyzer.get_file_analyzer().resolved_names,
                            *statements_analyzer.get_file_path(),
                            type_arg.1 .0.start_offset() as u32,
                        )
                        .unwrap();

                        populate_union_type(
                            &mut param_type,
                            &statements_analyzer.codebase.symbols,
                            &context
                                .function_context
                                .get_reference_source(&statements_analyzer.get_file_path().0),
                            &mut analysis_data.symbol_references,
                            false,
                        );

                        type_expander::expand_union(
                            statements_analyzer.codebase,
                            &Some(statements_analyzer.interner),
                            &mut param_type,
                            &TypeExpansionOptions {
                                parent_class: None,
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

                        generic_params.push(param_type);
                    }
                }

                generic_params
            } else {
                storage
                    .template_types
                    .iter()
                    .map(|(_, map)| {
                        let upper_bound = map.iter().next().unwrap().1.clone();
                        (*upper_bound).clone()
                    })
                    .collect::<Vec<_>>()
            })
        } else {
            None
        };
    }

    let mut result_type = wrap_atomic(TAtomic::TNamedObject {
        name: classlike_name,
        type_params: generic_type_params,
        is_this: from_static,
        extra_types: None,
        remapped_params: false,
    });

    if from_classname {
        let descendants = codebase.get_all_descendants(&classlike_name);

        for descendant_class in descendants {
            analysis_data
                .symbol_references
                .add_reference_to_overridden_class_member(
                    &context.function_context,
                    (descendant_class, StrId::CONSTRUCT),
                );
        }
    }

    result_type = add_dataflow(
        statements_analyzer,
        result_type,
        context,
        &declaring_method_id,
        codebase.get_method(&declaring_method_id),
        storage.specialize_instance,
        from_classname,
        analysis_data,
        pos,
    );

    result.return_type = Some(add_optional_union_type(
        result_type,
        result.return_type.as_ref(),
        codebase,
    ));

    Ok(())
}

fn add_dataflow<'a>(
    statements_analyzer: &'a StatementsAnalyzer,
    mut return_type_candidate: TUnion,
    context: &BlockContext,
    method_id: &MethodIdentifier,
    functionlike_storage: Option<&'a FunctionLikeInfo>,
    specialize_instance: bool,
    from_classname: bool,
    analysis_data: &mut FunctionAnalysisData,
    call_pos: &Pos,
) -> TUnion {
    // todo dispatch AddRemoveTaintsEvent

    let data_flow_graph = &mut analysis_data.data_flow_graph;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if !context.allow_taints {
            return return_type_candidate;
        }

        let codebase = statements_analyzer.codebase;

        let new_call_node = DataFlowNode::get_for_this_after_method(
            method_id,
            if let Some(functionlike_storage) = functionlike_storage {
                functionlike_storage.return_type_location
            } else {
                None
            },
            if specialize_instance {
                Some(statements_analyzer.get_hpos(call_pos))
            } else {
                None
            },
        );

        data_flow_graph.add_node(new_call_node.clone());

        return_type_candidate.parent_nodes = vec![new_call_node.clone()];

        if from_classname {
            let descendants = codebase.get_all_descendants(&method_id.0);

            for descendant_class in descendants {
                let new_call_node = DataFlowNode::get_for_this_after_method(
                    &MethodIdentifier(descendant_class, method_id.1),
                    if let Some(functionlike_storage) = functionlike_storage {
                        functionlike_storage.return_type_location
                    } else {
                        None
                    },
                    if specialize_instance {
                        Some(statements_analyzer.get_hpos(call_pos))
                    } else {
                        None
                    },
                );

                data_flow_graph.add_node(new_call_node.clone());

                return_type_candidate.parent_nodes.push(new_call_node);
            }
        }
    } else {
        let new_call_node = DataFlowNode::get_for_method_return(
            &FunctionLikeIdentifier::Method(method_id.0, method_id.1),
            Some(statements_analyzer.get_hpos(call_pos)),
            Some(statements_analyzer.get_hpos(call_pos)),
        );

        data_flow_graph.add_node(new_call_node.clone());

        return_type_candidate.parent_nodes = vec![new_call_node.clone()];
    }

    return_type_candidate
}
