use std::collections::BTreeMap;
use std::sync::Arc;

use hakana_reflection_info::assertion::Assertion;
use hakana_reflection_info::{Interner, StrId, EFFECT_WRITE_LOCAL};

use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::taint::SinkType;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::expr::binop::assignment_analyzer;
use crate::expr::call_analyzer::get_generic_param_for_offset;
use crate::expr::expression_identifier::{self, get_var_id};
use crate::expr::fetch::array_fetch_analyzer::add_array_fetch_dataflow;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{expression_analyzer, functionlike_analyzer};
use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_reflection_info::functionlike_parameter::{DefaultType, FunctionLikeParameter};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::{populate_union_type, TUnion};
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_type::template::{
    self, inferred_type_replacer, standin_type_replacer, TemplateBound, TemplateResult,
};
use hakana_type::type_expander::{self, StaticClassType, TypeExpansionOptions};
use hakana_type::{
    add_optional_union_type, combine_optional_union_types, get_arraykey, get_mixed_any, wrap_atomic,
};
use indexmap::IndexMap;
use oxidized::ast_defs::ParamKind;
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::argument_analyzer;
use super::method_call_info::MethodCallInfo;

pub(crate) fn check_arguments_match(
    statements_analyzer: &StatementsAnalyzer,
    type_args: &Vec<aast::Targ<()>>,
    args: &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
    unpacked_arg: &Option<aast::Expr<(), ()>>,
    functionlike_id: &FunctionLikeIdentifier,
    functionlike_info: &FunctionLikeInfo,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
    template_result: &mut TemplateResult,
    function_call_pos: &Pos,
    function_name_pos: Option<&Pos>,
) -> Result<(), AnalysisError> {
    let functionlike_params = &functionlike_info.params;
    // todo handle map and filter

    if !type_args.is_empty() {
        for (i, type_arg) in type_args.iter().enumerate() {
            let mut param_type = get_type_from_hint(
                &type_arg.1 .1,
                context.function_context.calling_class.as_ref(),
                statements_analyzer.get_type_resolution_context(),
                statements_analyzer.get_file_analyzer().resolved_names,
            )
            .unwrap();

            if param_type.is_placeholder() {
                continue;
            }

            populate_union_type(
                &mut param_type,
                &statements_analyzer.get_codebase().symbols,
                &context
                    .function_context
                    .get_reference_source(&statements_analyzer.get_file_path().0),
                &mut analysis_data.symbol_references,
                false,
            );

            if let Some((template_name, map)) = template_result.template_types.get_index(i) {
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
    }

    let last_param = functionlike_params.last();

    let mut param_types = BTreeMap::new();

    let codebase = statements_analyzer.get_codebase();

    let mut method_call_info = None;
    let mut class_storage = calling_classlike_storage;

    let fq_classlike_name = match functionlike_id {
        FunctionLikeIdentifier::Method(fq_classlike_name, _) => Some(fq_classlike_name),
        _ => None,
    };

    if let Some(method_id) = functionlike_id.as_method_identifier() {
        let static_fq_class_name = fq_classlike_name.unwrap();
        let mut self_fq_classlike_name = *static_fq_class_name;

        let declaring_method_id = codebase.get_declaring_method_id(&method_id);

        if declaring_method_id != method_id {
            self_fq_classlike_name = declaring_method_id.0;
            class_storage = codebase.classlike_infos.get(&declaring_method_id.0);
        }

        let appearing_method_id = codebase.get_declaring_method_id(&method_id);

        if appearing_method_id != method_id {
            self_fq_classlike_name = appearing_method_id.0;
        }

        method_call_info = Some(MethodCallInfo {
            self_fq_classlike_name,
            declaring_method_id: Some(declaring_method_id),
            classlike_storage: if let Some(class_storage) = class_storage {
                class_storage
            } else {
                return Err(AnalysisError::InternalError(
                    "Class storage does not exist".to_string(),
                    statements_analyzer.get_hpos(function_call_pos),
                ));
            },
        });
    }

    let mut class_generic_params = IndexMap::new();

    for (template_name, type_map) in &template_result.lower_bounds {
        for (class, lower_bounds) in type_map {
            if lower_bounds.len() == 1 {
                class_generic_params
                    .entry(*template_name)
                    .or_insert_with(FxHashMap::default)
                    .insert(
                        *class,
                        Arc::new(lower_bounds.first().unwrap().bound_type.clone()),
                    );
            }
        }
    }

    refine_template_result_for_functionlike(
        template_result,
        codebase,
        analysis_data,
        &method_call_info,
        class_storage,
        calling_classlike_storage,
        functionlike_info,
        &class_generic_params,
    );

    for (_, arg_expr) in args.iter() {
        let was_inside_call = context.inside_general_use;

        // if matches!(functionlike_info.effects, FnEffect::Some(_))
        //     || matches!(functionlike_info.effects, FnEffect::Arg(_))
        //     || functionlike_info.pure_can_throw
        //     || functionlike_info.user_defined
        // {
        context.inside_general_use = true;
        // }

        // don't analyse closures here
        if !matches!(arg_expr.2, aast::Expr_::Lfun(_) | aast::Expr_::Efun(_)) {
            expression_analyzer::analyze(
                statements_analyzer,
                arg_expr,
                analysis_data,
                context,
                if_body_context,
            )?;
        }

        if !was_inside_call {
            context.inside_general_use = false;
        }
    }

    let mut reordered_args = args.iter().enumerate().collect::<Vec<_>>();

    reordered_args.sort_by(|a, b| {
        matches!(a.1 .1 .2, aast::Expr_::Lfun(..)).cmp(&matches!(b.1 .1 .2, aast::Expr_::Lfun(..)))
    });

    for (argument_offset, (_, arg_expr)) in reordered_args.clone() {
        let mut param = functionlike_params.get(argument_offset);

        if param.is_none() {
            if let Some(last_param) = last_param {
                if last_param.is_variadic {
                    param = Some(last_param);
                }
            }
        }

        let mut param_type = get_param_type(
            param,
            codebase,
            class_storage,
            calling_classlike_storage,
            statements_analyzer,
            analysis_data,
        );

        let was_inside_call = context.inside_general_use;

        context.inside_general_use = true;

        if !was_inside_call {
            context.inside_general_use = false;
        }

        let mut arg_value_type = analysis_data
            .get_expr_type(arg_expr.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        if let aast::Expr_::Lfun(_) | aast::Expr_::Efun(_) = arg_expr.2 {
            handle_closure_arg(
                statements_analyzer,
                analysis_data,
                context,
                functionlike_id,
                template_result,
                args,
                arg_expr,
                &param_type,
            );

            expression_analyzer::analyze(
                statements_analyzer,
                arg_expr,
                analysis_data,
                context,
                if_body_context,
            )?;

            arg_value_type = analysis_data
                .get_expr_type(arg_expr.pos())
                .cloned()
                .unwrap_or(get_mixed_any());
        }

        adjust_param_type(
            &class_generic_params,
            &mut param_type,
            codebase,
            arg_value_type,
            argument_offset,
            context,
            template_result,
            statements_analyzer,
            functionlike_id,
        );

        param_types.insert(argument_offset, param_type);
    }

    let mut last_param_type = None;

    if let Some(unpacked_arg) = unpacked_arg {
        let param = functionlike_params.last();

        let mut param_type = get_param_type(
            param,
            codebase,
            class_storage,
            calling_classlike_storage,
            statements_analyzer,
            analysis_data,
        );

        let was_inside_call = context.inside_general_use;

        context.inside_general_use = true;

        if !was_inside_call {
            context.inside_general_use = false;
        }

        let arg_value_type = analysis_data
            .get_expr_type(unpacked_arg.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        adjust_param_type(
            &class_generic_params,
            &mut param_type,
            codebase,
            arg_value_type,
            reordered_args.len(),
            context,
            template_result,
            statements_analyzer,
            functionlike_id,
        );

        last_param_type = Some(param_type.clone());

        expression_analyzer::analyze(
            statements_analyzer,
            unpacked_arg,
            analysis_data,
            context,
            if_body_context,
        )?;
    }

    let function_params = &functionlike_info.params;

    if function_params.len() > args.len() {
        let mut i = args.len();
        let i_max = function_params.len();

        while i < i_max {
            let function_param = &function_params[i];
            if let Some(default_type) = &function_param.default_type {
                if let Some(param_type) = &function_param.signature_type {
                    if param_type.has_template() {
                        let default_type =
                            if let DefaultType::NormalData(default_union) = &default_type {
                                default_union.clone()
                            } else {
                                // todo handle unresolved constants
                                get_mixed_any()
                            };

                        if default_type.has_literal_value() {
                            // todo check templated default arg matches
                        }
                    }
                }
            }
            i += 1;
        }
    }

    for (argument_offset, (param_kind, arg_expr)) in reordered_args {
        let function_param = if let Some(function_param) = function_params.get(argument_offset) {
            function_param
        } else {
            let last_param = function_params.last();

            if let Some(last_param) = last_param {
                if last_param.is_variadic {
                    last_param
                } else {
                    break;
                }
            } else {
                break;
            }
        };

        if function_param.is_inout {
            // First inout param for HH\Shapes::removeKey is already handled
            if if let FunctionLikeIdentifier::Method(classname, method_name) = functionlike_id {
                *classname != StrId::SHAPES || *method_name != StrId::REMOVE_KEY
            } else {
                true
            } {
                handle_possibly_matching_inout_param(
                    statements_analyzer,
                    analysis_data,
                    function_param,
                    functionlike_id,
                    argument_offset,
                    arg_expr,
                    class_storage,
                    calling_classlike_storage,
                    context,
                    template_result,
                    function_call_pos,
                )?;
            }
        }

        let arg_value_type = analysis_data.get_expr_type(arg_expr.pos());

        let arg_value_type = if let Some(arg_value_type) = arg_value_type {
            arg_value_type.clone()
        } else {
            // todo increment mixed count

            continue;
        };

        let was_inside_call = context.inside_general_use;

        if matches!(functionlike_info.effects, FnEffect::Some(_)) {
            context.inside_general_use = true;
        }

        argument_analyzer::check_argument_matches(
            statements_analyzer,
            functionlike_id,
            &method_call_info,
            function_param,
            param_types.remove(&argument_offset).unwrap(),
            argument_offset,
            (param_kind, arg_expr),
            false,
            arg_value_type,
            context,
            analysis_data,
            functionlike_info.ignore_taint_path,
            functionlike_info.specialize_call,
            function_call_pos,
            function_name_pos,
        );

        if !was_inside_call {
            context.inside_general_use = false;
        }

        if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
            if let Some(removed_taints) = &function_param.removed_taints_when_returning_true {
                if let Some(expr_var_id) = expression_identifier::get_var_id(
                    arg_expr,
                    None,
                    statements_analyzer.get_file_analyzer().get_file_source(),
                    statements_analyzer.get_file_analyzer().resolved_names,
                    Some((
                        statements_analyzer.get_codebase(),
                        statements_analyzer.get_interner(),
                    )),
                ) {
                    analysis_data.if_true_assertions.insert(
                        (
                            function_call_pos.start_offset() as u32,
                            function_call_pos.end_offset() as u32,
                        ),
                        FxHashMap::from_iter([(
                            "hakana taints".to_string(),
                            vec![Assertion::RemoveTaints(
                                expr_var_id,
                                if removed_taints.is_empty() {
                                    SinkType::user_controllable_taints()
                                } else {
                                    removed_taints.clone()
                                },
                            )],
                        )]),
                    );
                }
            }
        }
    }

    // analyze unpacked arg
    if let Some(unpacked_arg) = unpacked_arg {
        if let Some(last_param) = function_params.last() {
            if last_param.is_variadic {
                let arg_value_type = analysis_data.get_expr_type(unpacked_arg.pos());

                let arg_value_type = if let Some(arg_value_type) = arg_value_type {
                    arg_value_type.clone()
                } else {
                    // todo increment mixed count

                    get_mixed_any()
                };

                let was_inside_call = context.inside_general_use;

                if matches!(functionlike_info.effects, FnEffect::Some(_)) {
                    context.inside_general_use = true;
                }

                argument_analyzer::check_argument_matches(
                    statements_analyzer,
                    functionlike_id,
                    &method_call_info,
                    last_param,
                    last_param_type.unwrap().clone(),
                    args.len(),
                    (&ParamKind::Pnormal, unpacked_arg),
                    true,
                    arg_value_type,
                    context,
                    analysis_data,
                    functionlike_info.ignore_taint_path,
                    functionlike_info.specialize_call,
                    function_call_pos,
                    function_name_pos,
                );

                context.inside_general_use = was_inside_call;
            }
        }
    }

    Ok(())
}

fn adjust_param_type(
    class_generic_params: &IndexMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,
    param_type: &mut TUnion,
    codebase: &CodebaseInfo,
    mut arg_value_type: TUnion,
    argument_offset: usize,
    context: &mut ScopeContext,
    template_result: &mut TemplateResult,
    statements_analyzer: &StatementsAnalyzer,
    functionlike_id: &FunctionLikeIdentifier,
) {
    let bindable_template_params = param_type
        .get_template_types()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();

    if !class_generic_params.is_empty() {
        map_class_generic_params(
            class_generic_params,
            param_type,
            codebase,
            statements_analyzer.get_interner(),
            &mut arg_value_type,
            argument_offset,
            context,
            template_result,
        );
    }
    if !template_result.template_types.is_empty() {
        let param_has_templates = param_type.has_template_types();

        if param_has_templates {
            *param_type = standin_type_replacer::replace(
                &*param_type,
                template_result,
                statements_analyzer.get_codebase(),
                &Some(statements_analyzer.get_interner()),
                &Some(&arg_value_type),
                Some(argument_offset),
                if let Some(calling_class) = &context.function_context.calling_class {
                    if !context.function_context.is_static {
                        if let FunctionLikeIdentifier::Method(_, method_name) = functionlike_id {
                            if statements_analyzer.get_interner().lookup(method_name)
                                == "__construct"
                            {
                                None
                            } else {
                                Some(calling_class)
                            }
                        } else {
                            Some(calling_class)
                        }
                    } else {
                        None
                    }
                } else {
                    None
                },
                context.function_context.calling_functionlike_id.as_ref(),
                true,
                false,
                None,
                1,
            );
        }

        for template_type in bindable_template_params {
            if let TAtomic::TGenericParam {
                param_name,
                defining_entity,
                as_type,
                ..
            } = template_type
            {
                if (if let Some(bounds_by_param) = template_result.lower_bounds.get(&param_name) {
                    bounds_by_param.get(&defining_entity)
                } else {
                    None
                })
                .is_none()
                {
                    let bound_type = if let Some(bounds_by_param) =
                        template_result.upper_bounds.get(&param_name)
                    {
                        if let Some(upper_bound) = bounds_by_param.get(&defining_entity) {
                            upper_bound.bound_type.clone()
                        } else {
                            (*as_type).clone()
                        }
                    } else {
                        (*as_type).clone()
                    };

                    template_result
                        .upper_bounds
                        .entry(param_name)
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            defining_entity,
                            TemplateBound::new(bound_type, 0, None, None),
                        );
                }
            }
        }
    }
}

fn get_param_type(
    param: Option<&FunctionLikeParameter>,
    codebase: &CodebaseInfo,
    class_storage: Option<&ClassLikeInfo>,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    if let Some(param) = param {
        if let Some(param_type) = &param.signature_type {
            let mut param_type = param_type.clone();

            type_expander::expand_union(
                codebase,
                &Some(statements_analyzer.get_interner()),
                &mut param_type,
                &TypeExpansionOptions {
                    self_class: if let Some(classlike_storage) = class_storage {
                        Some(&classlike_storage.name)
                    } else {
                        None
                    },
                    static_class_type: if let Some(calling_class_storage) =
                        calling_classlike_storage
                    {
                        StaticClassType::Name(&calling_class_storage.name)
                    } else {
                        StaticClassType::None
                    },
                    parent_class: None,
                    function_is_final: if let Some(calling_class_storage) =
                        calling_classlike_storage
                    {
                        calling_class_storage.is_final
                    } else {
                        false
                    },
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

            param_type
        } else {
            get_mixed_any()
        }
    } else {
        get_mixed_any()
    }
}

fn handle_closure_arg(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    functionlike_id: &FunctionLikeIdentifier,
    template_result: &mut TemplateResult,
    args: &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
    closure_expr: &aast::Expr<(), ()>,
    param_type: &TUnion,
) {
    let codebase = statements_analyzer.get_codebase();

    let mut replace_template_result = TemplateResult::new(
        template_result
            .lower_bounds
            .iter()
            .map(|(key, template_map)| {
                (
                    *key,
                    template_map
                        .iter()
                        .map(|(map_key, lower_bounds)| {
                            (
                                *map_key,
                                Arc::new(template::standin_type_replacer::get_most_specific_type_from_bounds(
                                    lower_bounds,
                                    codebase,
                                )),
                            )
                        })
                        .collect::<FxHashMap<_, _>>(),
                )
            })
            .collect::<IndexMap<_, _>>(),
        IndexMap::new(),
    );

    let mut replaced_type = standin_type_replacer::replace(
        param_type,
        &mut replace_template_result,
        codebase,
        &Some(statements_analyzer.get_interner()),
        &None,
        None,
        None,
        context.function_context.calling_functionlike_id.as_ref(),
        true,
        false,
        None,
        1,
    );

    replaced_type =
        inferred_type_replacer::replace(&replaced_type, &replace_template_result, codebase);

    let mut closure_storage = {
        match functionlike_analyzer::get_closure_storage(
            statements_analyzer.get_file_analyzer(),
            closure_expr.1.start_offset(),
        ) {
            None => {
                return;
            }
            Some(value) => value,
        }
    };

    for (param_offset, param_storage) in closure_storage.params.iter_mut().enumerate() {
        if param_storage.signature_type.is_none() {
            let mut newly_inferred_type = None;
            for replaced_type_part in &replaced_type.types {
                if let TAtomic::TClosure {
                    params: replaced_params,
                    ..
                } = replaced_type_part
                {
                    let replaced_param_type =
                        if let Some(signature_type) = replaced_params.get(param_offset) {
                            &signature_type.signature_type
                        } else {
                            &None
                        };

                    if let Some(replaced_param_type) = &replaced_param_type {
                        newly_inferred_type = Some(combine_optional_union_types(
                            newly_inferred_type.as_ref(),
                            Some(replaced_param_type),
                            codebase,
                        ));
                    }
                }
            }

            if let Some(newly_inferred_type) = newly_inferred_type {
                param_storage.signature_type = Some(newly_inferred_type);
            }
        }

        if matches!(
            analysis_data.data_flow_graph.kind,
            GraphKind::WholeProgram(_)
        ) || !statements_analyzer
            .get_config()
            .migration_symbols
            .is_empty()
        {
            if let FunctionLikeIdentifier::Function(function_name) = functionlike_id {
                match statements_analyzer.get_interner().lookup(function_name) {
                    "HH\\Lib\\Vec\\map"
                    | "HH\\Lib\\Dict\\map"
                    | "HH\\Lib\\Keyset\\map"
                    | "HH\\Lib\\Vec\\map_async"
                    | "HH\\Lib\\Dict\\map_async"
                    | "HH\\Lib\\Keyset\\map_async"
                    | "HH\\Lib\\Vec\\filter"
                    | "HH\\Lib\\Dict\\filter"
                    | "HH\\Lib\\Keyset\\filter"
                    | "HH\\Lib\\Vec\\take"
                    | "HH\\Lib\\Dict\\take"
                    | "HH\\Lib\\Keyset\\take"
                    | "HH\\Lib\\C\\find"
                    | "HH\\Lib\\C\\findx"
                    | "HH\\Lib\\Vec\\map_with_key"
                    | "HH\\Lib\\Dict\\map_with_key"
                    | "HH\\Lib\\Keyset\\map_with_key"
                    | "HH\\Lib\\Dict\\map_with_key_async"
                    | "HH\\Lib\\Dict\\from_keys"
                    | "HH\\Lib\\Dict\\from_keys_async" => {
                        if param_offset == 0 {
                            if let Some(ref mut signature_type) = param_storage.signature_type {
                                add_array_fetch_dataflow(
                                    statements_analyzer,
                                    args[0].1.pos(),
                                    analysis_data,
                                    None,
                                    signature_type,
                                    &mut get_arraykey(false),
                                );
                            }
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    analysis_data
        .closures
        .insert(closure_expr.pos().clone(), closure_storage);
}

fn map_class_generic_params(
    class_generic_params: &IndexMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,
    param_type: &mut TUnion,
    codebase: &CodebaseInfo,
    interner: &Interner,
    arg_value_type: &mut TUnion,
    argument_offset: usize,
    context: &mut ScopeContext,
    template_result: &mut TemplateResult,
) {
    let arg_has_template_types = arg_value_type.has_template_types();

    // here we're replacing the param types and arg types with the bound
    // class template params.
    //
    // For example, if we're operating on a class Foo with params TKey and TValue,
    // and we're calling a method "add(TKey $key, TValue $value)" on an instance
    // of that class where we know that TKey is int and TValue is string, then we
    // want to substitute the expected parameters so it's as if we were actually
    // calling "add(int $key, string $value)"

    let mapped_params = class_generic_params.clone();
    let mut readonly_template_result = TemplateResult::new(mapped_params, IndexMap::new());

    // This flag ensures that the template results will never be written to
    // It also supercedes the `$add_lower_bounds` flag so that closure readonly_template_result
    // don’t get overwritten
    readonly_template_result.readonly = true;

    *param_type = template::standin_type_replacer::replace(
        &*param_type,
        &mut readonly_template_result,
        codebase,
        &Some(interner),
        &Some(arg_value_type),
        Some(argument_offset),
        context.function_context.calling_class.as_ref(),
        context.function_context.calling_functionlike_id.as_ref(),
        true,
        false,
        None,
        1,
    );

    if arg_has_template_types {
        *arg_value_type = template::standin_type_replacer::replace(
            &*arg_value_type,
            template_result,
            codebase,
            &Some(interner),
            &Some(arg_value_type),
            Some(argument_offset),
            context.function_context.calling_class.as_ref(),
            context.function_context.calling_functionlike_id.as_ref(),
            true,
            false,
            None,
            1,
        );
    }
}

pub(crate) fn evaluate_arbitrary_param(
    statements_analyzer: &StatementsAnalyzer,
    expr: &aast::Expr<(), ()>,
    is_inout: bool,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> Result<(), AnalysisError> {
    let was_inside_call = context.inside_general_use;
    context.inside_general_use = true;

    expression_analyzer::analyze(
        statements_analyzer,
        expr,
        analysis_data,
        context,
        if_body_context,
    )?;

    if !was_inside_call {
        context.inside_general_use = false;
    }

    if is_inout {
        let var_id = get_var_id(
            expr,
            context.function_context.calling_class.as_ref(),
            statements_analyzer.get_file_analyzer().get_file_source(),
            statements_analyzer.get_file_analyzer().resolved_names,
            Some((
                statements_analyzer.get_codebase(),
                statements_analyzer.get_interner(),
            )),
        );

        if let Some(var_id) = var_id {
            if let Some(t) = context.vars_in_scope.get(&var_id) {
                let t = (**t).clone();

                context.remove_var_from_conflicting_clauses(
                    &var_id,
                    Some(&t),
                    Some(statements_analyzer),
                    analysis_data,
                );
            } else {
                context.remove_var_from_conflicting_clauses(
                    &var_id,
                    None,
                    Some(statements_analyzer),
                    analysis_data,
                );
            }
        }

        analysis_data.expr_effects.insert(
            (
                expr.pos().start_offset() as u32,
                expr.pos().end_offset() as u32,
            ),
            EFFECT_WRITE_LOCAL,
        );
    }

    Ok(())
}

fn handle_possibly_matching_inout_param(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    functionlike_param: &FunctionLikeParameter,
    functionlike_id: &FunctionLikeIdentifier,
    argument_offset: usize,
    expr: &aast::Expr<(), ()>,
    classlike_storage: Option<&ClassLikeInfo>,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    context: &mut ScopeContext,
    template_result: &mut TemplateResult,
    function_call_pos: &Pos,
) -> Result<(), AnalysisError> {
    let mut inout_type = functionlike_param
        .signature_type
        .clone()
        .unwrap_or(get_mixed_any());

    let codebase = statements_analyzer.get_codebase();

    let arg_type = analysis_data.get_expr_type(expr.pos()).cloned();

    if !template_result.template_types.is_empty() {
        let original_inout_type = inout_type.clone();

        inout_type = standin_type_replacer::replace(
            &inout_type,
            template_result,
            codebase,
            &Some(statements_analyzer.get_interner()),
            &if let Some(arg_type) = &arg_type {
                Some(arg_type)
            } else {
                None
            },
            Some(argument_offset),
            context.function_context.calling_class.as_ref(),
            if let Some(m) = &context.function_context.calling_functionlike_id {
                Some(m)
            } else {
                None
            },
            true,
            false,
            None,
            1,
        );

        if !template_result.lower_bounds.is_empty() {
            inout_type =
                inferred_type_replacer::replace(&original_inout_type, template_result, codebase);
        }
    }

    type_expander::expand_union(
        codebase,
        &Some(statements_analyzer.get_interner()),
        &mut inout_type,
        &TypeExpansionOptions {
            self_class: if let Some(classlike_storage) = classlike_storage {
                Some(&classlike_storage.name)
            } else {
                None
            },
            static_class_type: if let Some(calling_class_storage) = calling_classlike_storage {
                StaticClassType::Name(&calling_class_storage.name)
            } else {
                StaticClassType::None
            },
            parent_class: None,
            function_is_final: if let Some(calling_class_storage) = calling_classlike_storage {
                calling_class_storage.is_final
            } else {
                false
            },
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

    let arg_type = arg_type.unwrap_or(get_mixed_any());

    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        let out_node = DataFlowNode::get_for_method_argument_out(
            functionlike_id.to_string(statements_analyzer.get_interner()),
            argument_offset,
            Some(functionlike_param.name_location),
            Some(statements_analyzer.get_hpos(function_call_pos)),
        );

        inout_type.parent_nodes = FxHashSet::from_iter([out_node.clone()]);

        analysis_data.data_flow_graph.add_node(out_node);
    } else {
        inout_type
            .parent_nodes
            .extend(arg_type.parent_nodes.clone());
    }

    assignment_analyzer::analyze_inout_param(
        statements_analyzer,
        expr,
        arg_type,
        &inout_type,
        analysis_data,
        context,
    )?;

    Ok(())
}

fn refine_template_result_for_functionlike(
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    method_call_info: &Option<MethodCallInfo>,
    classlike_storage: Option<&ClassLikeInfo>,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    functionlike_storage: &FunctionLikeInfo,
    class_template_params: &IndexMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,
) {
    let template_types = get_template_types_for_call(
        codebase,
        analysis_data,
        classlike_storage,
        if let Some(method_call_info) = method_call_info {
            Some(&method_call_info.self_fq_classlike_name)
        } else {
            None
        },
        calling_classlike_storage,
        &functionlike_storage.template_types,
        class_template_params,
    );

    if template_types.is_empty() {
        return;
    }

    if template_result.template_types.is_empty() {
        template_result.template_types = template_types
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|(k, v)| (k, Arc::new(v))).collect()))
            .collect::<IndexMap<_, _>>();
    }
}

pub(crate) fn get_template_types_for_call(
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    declaring_classlike_storage: Option<&ClassLikeInfo>,
    appearing_class_name: Option<&StrId>,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    existing_template_types: &IndexMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,
    class_template_params: &IndexMap<StrId, FxHashMap<StrId, Arc<TUnion>>>,
) -> IndexMap<StrId, FxHashMap<StrId, TUnion>> {
    let mut template_types = existing_template_types.clone();

    if let Some(declaring_classlike_storage) = declaring_classlike_storage {
        let calling_has_extends = if let Some(calling_classlike_storage) = calling_classlike_storage
        {
            calling_classlike_storage.name != declaring_classlike_storage.name
                && !calling_classlike_storage
                    .template_extended_params
                    .is_empty()
        } else {
            false
        };
        if calling_has_extends {
            let calling_template_extended =
                &calling_classlike_storage.unwrap().template_extended_params;

            for (class_name, type_map) in calling_template_extended {
                for (template_name, type_) in type_map {
                    if class_name == &declaring_classlike_storage.name {
                        let output_type = if type_.has_template() {
                            let mut output_type = None;
                            for atomic_type in &type_.types {
                                let output_type_candidate = if let TAtomic::TGenericParam {
                                    defining_entity,
                                    param_name,
                                    ..
                                } = &atomic_type
                                {
                                    (*get_generic_param_for_offset(
                                        defining_entity,
                                        param_name,
                                        calling_template_extended,
                                        &{
                                            let mut p = class_template_params.clone();
                                            p.extend(template_types.clone());
                                            p.into_iter().collect::<FxHashMap<_, _>>()
                                        },
                                    ))
                                    .clone()
                                } else {
                                    wrap_atomic(atomic_type.clone())
                                };

                                output_type = Some(add_optional_union_type(
                                    output_type_candidate,
                                    output_type.as_ref(),
                                    codebase,
                                ));
                            }

                            Arc::new(output_type.unwrap())
                        } else {
                            type_.clone()
                        };

                        template_types
                            .entry(*template_name)
                            .or_insert_with(FxHashMap::default)
                            .insert(declaring_classlike_storage.name, output_type);
                    }
                }
            }
        } else if !declaring_classlike_storage.template_types.is_empty() {
            for (template_name, type_map) in &declaring_classlike_storage.template_types {
                for (key, type_) in type_map {
                    template_types
                        .entry(*template_name)
                        .or_insert_with(FxHashMap::default)
                        .insert(
                            *key,
                            class_template_params
                                .get(template_name)
                                .unwrap_or(&FxHashMap::default())
                                .get(key)
                                .cloned()
                                .unwrap_or(type_.clone()),
                        );
                }
            }
        }
    }

    let mut expanded_template_types = IndexMap::new();

    for (key, type_map) in template_types {
        expanded_template_types.insert(
            key,
            type_map
                .into_iter()
                .map(|(k, v)| {
                    (k, {
                        let mut v = (*v).clone();
                        type_expander::expand_union(
                            codebase,
                            &None,
                            &mut v,
                            &TypeExpansionOptions {
                                self_class: appearing_class_name,
                                static_class_type: if let Some(calling_class_storage) =
                                    calling_classlike_storage
                                {
                                    StaticClassType::Name(&calling_class_storage.name)
                                } else {
                                    StaticClassType::None
                                },
                                parent_class: None,
                                function_is_final: if let Some(calling_class_storage) =
                                    calling_classlike_storage
                                {
                                    calling_class_storage.is_final
                                } else {
                                    false
                                },
                                ..Default::default()
                            },
                            &mut analysis_data.data_flow_graph,
                        );
                        v
                    })
                })
                .collect(),
        );
    }

    expanded_template_types
}
