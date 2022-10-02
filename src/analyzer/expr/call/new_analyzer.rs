use std::sync::Arc;

use hakana_reflection_info::classlike_info::Variance;
use hakana_reflection_info::codebase_info::symbols::Symbol;
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_type::template::standin_type_replacer::get_most_specific_type_from_bounds;
use rustc_hash::FxHashMap;

use crate::expr::call_analyzer::{check_method_args, get_generic_param_for_offset};
use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::{populate_union_type, TUnion};
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_type::template::{self, TemplateBound, TemplateResult};
use hakana_type::{
    add_optional_union_type, get_mixed_any, get_named_object, get_nothing, wrap_atomic,
};
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    //let method_id = None;

    let codebase = statements_analyzer.get_codebase();

    let mut can_extend = false;

    let lhs_type = match &expr.0 .2 {
        aast::ClassId_::CIexpr(lhs_expr) => {
            if let aast::Expr_::Id(id) = &lhs_expr.2 {
                let name_string = id.1.clone();
                match name_string.as_str() {
                    "self" => {
                        let self_name = &context.function_context.calling_class.clone().unwrap();

                        get_named_object(self_name.clone())
                    }
                    "parent" => {
                        let self_name = &context.function_context.calling_class.clone().unwrap();

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        get_named_object(classlike_storage.direct_parent_class.clone().unwrap())
                    }
                    "static" => {
                        let self_name = &context.function_context.calling_class.clone().unwrap();

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        if !classlike_storage.is_final {
                            can_extend = true;
                        }

                        wrap_atomic(TAtomic::TNamedObject {
                            name: self_name.clone(),
                            type_params: None,
                            is_this: !classlike_storage.is_final,
                            extra_types: None,
                            remapped_params: false,
                        })
                    }
                    _ => {
                        let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

                        let name_string = resolved_names.get(&id.0.start_offset()).unwrap().clone();

                        get_named_object(name_string)
                    }
                }
            } else {
                let was_inside_general_use = context.inside_general_use;
                context.inside_general_use = true;
                expression_analyzer::analyze(
                    statements_analyzer,
                    lhs_expr,
                    tast_info,
                    context,
                    if_body_context,
                );
                context.inside_general_use = was_inside_general_use;
                tast_info
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

    for (_, lhs_type_part) in &lhs_type.types {
        analyze_atomic(
            statements_analyzer,
            expr,
            pos,
            tast_info,
            context,
            if_body_context,
            lhs_type_part,
            can_extend,
            &mut result,
        );
    }

    tast_info.set_expr_type(&pos, result.return_type.clone().unwrap_or(get_mixed_any()));

    true
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    lhs_type_part: &TAtomic,
    can_extend: bool,
    result: &mut AtomicMethodCallAnalysisResult,
) {
    let mut from_static = false;
    let classlike_name = match &lhs_type_part {
        TAtomic::TNamedObject { name, is_this, .. } => {
            from_static = *is_this;
            // todo check class name and register usage
            name.clone()
        }
        TAtomic::TClassname { as_type, .. } | TAtomic::TTemplateParamClass { as_type, .. } => {
            let as_type = *as_type.clone();
            if let TAtomic::TNamedObject { name, .. } = as_type {
                // todo check class name and register usage
                name
            } else {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedMethodCall,
                        "Method called on unknown object".to_string(),
                        statements_analyzer.get_hpos(&pos),
                    ),
                    statements_analyzer.get_config(),
                );

                return;
            }
        }
        TAtomic::TLiteralClassname { name } => name.clone(),
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut classlike_name = None;
            for (_, generic_param_type) in &as_type.types {
                if let TAtomic::TNamedObject { name, .. } = generic_param_type {
                    classlike_name = Some(name.clone());
                    break;
                } else {
                    return;
                }
            }

            if let Some(classlike_name) = classlike_name {
                classlike_name
            } else {
                // todo emit issue
                return;
            }
        }
        _ => {
            if lhs_type_part.is_mixed() {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedMethodCall,
                        "Method called on unknown object".to_string(),
                        statements_analyzer.get_hpos(&pos),
                    ),
                    statements_analyzer.get_config(),
                );
            }

            // todo handle nonobject call
            return;
        }
    };

    analyze_named_constructor(
        statements_analyzer,
        expr,
        pos,
        tast_info,
        context,
        if_body_context,
        classlike_name,
        from_static,
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
    tast_info: &mut TastInfo,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    classlike_name: Symbol,
    from_static: bool,
    can_extend: bool,
    result: &mut AtomicMethodCallAnalysisResult,
) {
    let codebase = statements_analyzer.get_codebase();
    let storage = if let Some(storage) = codebase.classlike_infos.get(&classlike_name) {
        storage
    } else {
        return;
    };

    if from_static {
        // todo check for unsafe instantiation
    }

    if storage.is_abstract && !can_extend {
        // todo complain about abstract instantiation
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

    let method_name = "__construct".to_string();
    let method_id = MethodIdentifier(classlike_name.clone(), method_name);
    let declaring_method_id = codebase.get_declaring_method_id(&method_id);

    if codebase.method_exists(&method_id.0, &method_id.1) {
        tast_info.symbol_references.add_reference_to_class_member(
            &context.function_context,
            (classlike_name.clone(), format!("{}()", method_id.1)),
        );

        let mut template_result = TemplateResult::new(
            if expr.1.is_empty() {
                IndexMap::new()
            } else {
                storage.template_types.clone()
            },
            IndexMap::new(),
        );

        let method_storage = codebase.get_method(&declaring_method_id).unwrap();

        if !check_method_args(
            statements_analyzer,
            tast_info,
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
            &mut template_result,
            context,
            if_body_context,
            pos,
        ) {
            return;
        }

        // todo check method visibility

        // todo check purity

        if !storage.template_types.is_empty() {
            for (i, type_arg) in expr.1.iter().enumerate() {
                let mut param_type = get_type_from_hint(
                    &type_arg.1 .1,
                    context.function_context.calling_class.as_ref(),
                    &statements_analyzer.get_type_resolution_context(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                );

                if param_type.is_placeholder() {
                    continue;
                }

                populate_union_type(&mut param_type, &statements_analyzer.get_codebase().symbols);

                if let Some((template_name, map)) = template_result.template_types.get_index(i) {
                    template_result.lower_bounds.insert(
                        template_name.clone(),
                        map.iter()
                            .map(|(entity, _)| {
                                (
                                    entity.clone(),
                                    vec![TemplateBound::new(param_type.clone(), 0, None, None)],
                                )
                            })
                            .collect::<FxHashMap<_, _>>(),
                    );
                }
            }

            let mut v = vec![];
            for (i, (template_name, base_type_map)) in storage.template_types.iter().enumerate() {
                let mut generic_param_type = if let Some(template_bounds) =
                    if let Some(result_map) = template_result.lower_bounds.get(template_name) {
                        result_map.get(&classlike_name)
                    } else {
                        None
                    } {
                    template::standin_type_replacer::get_most_specific_type_from_bounds(
                        template_bounds,
                        Some(codebase),
                    )
                } else if !storage.template_extended_params.is_empty()
                    && !template_result.lower_bounds.is_empty()
                {
                    let found_generic_params = template_result
                        .lower_bounds
                        .iter()
                        .map(|(key, type_map)| {
                            (
                                key.clone(),
                                type_map
                                    .iter()
                                    .map(|(map_key, bounds)| {
                                        (
                                            map_key.clone(),
                                            Arc::new(get_most_specific_type_from_bounds(
                                                bounds,
                                                Some(codebase),
                                            )),
                                        )
                                    })
                                    .collect::<FxHashMap<_, _>>(),
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
                } else {
                    if let Some(Variance::Contravariant) = storage.generic_variance.get(&i) {
                        get_nothing()
                    } else {
                        (**base_type_map.iter().next().unwrap().1).clone()
                    }
                };

                generic_param_type.had_template = true;

                v.push(generic_param_type);
            }

            generic_type_params = Some(v);
        }
    } else {
        tast_info
            .symbol_references
            .add_reference_to_symbol(&context.function_context, classlike_name.clone());

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
                            &statements_analyzer.get_type_resolution_context(),
                            statements_analyzer.get_file_analyzer().resolved_names,
                        );

                        populate_union_type(
                            &mut param_type,
                            &statements_analyzer.get_codebase().symbols,
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

    if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
        result_type = add_dataflow(
            statements_analyzer,
            result_type,
            context,
            &method_id,
            codebase.get_method(&declaring_method_id),
            storage.specialize_instance,
            tast_info,
            pos,
        );
    }

    result.return_type = Some(add_optional_union_type(
        result_type,
        result.return_type.as_ref(),
        Some(codebase),
    ));
}

fn add_dataflow<'a>(
    statements_analyzer: &'a StatementsAnalyzer,
    mut return_type_candidate: TUnion,
    context: &ScopeContext,
    method_id: &MethodIdentifier,
    functionlike_storage: Option<&'a FunctionLikeInfo>,
    specialize_instance: bool,
    tast_info: &mut TastInfo,
    call_pos: &Pos,
) -> TUnion {
    // todo dispatch AddRemoveTaintsEvent

    let ref mut data_flow_graph = tast_info.data_flow_graph;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if !context.allow_taints {
            return return_type_candidate;
        }
    }

    let new_call_node = DataFlowNode::get_for_this_after_method(
        method_id,
        if let Some(functionlike_storage) = functionlike_storage {
            functionlike_storage.return_type_location.clone()
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

    return_type_candidate.parent_nodes =
        FxHashMap::from_iter([(new_call_node.get_id().clone(), new_call_node.clone())]);

    return_type_candidate
}
