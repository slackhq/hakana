use std::collections::BTreeMap;
use std::sync::Arc;

use hakana_code_info::assertion::Assertion;
use hakana_code_info::code_location::{FilePath, StmtStart};
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::ttype::template::standin_type_replacer::StandinOpts;
use hakana_code_info::{EFFECT_WRITE_LOCAL, GenericParent, VarId};

use hakana_code_info::data_flow::node::DataFlowNode;
use hakana_code_info::taint::SinkType;
use hakana_str::{Interner, StrId};
use rustc_hash::FxHashMap;

use crate::expr::binop::assignment_analyzer;
use crate::expr::call_analyzer::get_generic_param_for_offset;
use crate::expr::expression_identifier::{self, get_var_id};
use crate::expr::fetch::array_fetch_analyzer::add_array_fetch_dataflow;
use crate::expr::fetch::class_constant_fetch_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{expression_analyzer, functionlike_analyzer};
use hakana_code_info::classlike_info::ClassLikeInfo;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::data_flow::graph::GraphKind;
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_code_info::functionlike_parameter::{DefaultType, FunctionLikeParameter};
use hakana_code_info::t_atomic::{TAtomic, TGenericParam};
use hakana_code_info::t_union::{TUnion, populate_union_type};
use hakana_code_info::ttype::template::{
    self, TemplateBound, TemplateResult, inferred_type_replacer, standin_type_replacer,
};
use hakana_code_info::ttype::type_expander::{self, StaticClassType, TypeExpansionOptions};
use hakana_code_info::ttype::{
    add_optional_union_type, combine_optional_union_types, get_arraykey, get_mixed_any, wrap_atomic,
};
use hakana_reflector::typehint_resolver::get_type_from_hint;
use indexmap::IndexMap;
use oxidized::aast;
use oxidized::pos::Pos;

use super::argument_analyzer::{self, get_removed_taints_in_comments};
use super::method_call_info::MethodCallInfo;

pub(crate) fn check_arguments_match(
    statements_analyzer: &StatementsAnalyzer,
    type_args: &[aast::Targ<()>],
    args: &[aast::Argument<(), ()>],
    unpacked_arg: &Option<aast::Expr<(), ()>>,
    functionlike_id: &FunctionLikeIdentifier,
    functionlike_info: &FunctionLikeInfo,
    calling_classlike: Option<(StrId, Option<&TAtomic>)>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    template_result: &mut TemplateResult,
    function_call_pos: &Pos,
    function_name_pos: Option<&Pos>,
) -> Result<(), AnalysisError> {
    let functionlike_params = &functionlike_info.params;
    // todo handle map and filter

    let calling_classlike_storage = calling_classlike.and_then(|calling_classlike| {
        statements_analyzer
            .codebase
            .classlike_infos
            .get(&calling_classlike.0)
    });

    if !type_args.is_empty() {
        for (i, type_arg) in type_args.iter().enumerate() {
            let mut param_type = get_type_from_hint(
                &type_arg.1.1,
                context.function_context.calling_class,
                statements_analyzer.get_type_resolution_context(),
                statements_analyzer.file_analyzer.resolved_names,
                *statements_analyzer.get_file_path(),
                type_arg.1.0.start_offset() as u32,
            )
            .unwrap();

            if param_type.is_placeholder() {
                continue;
            }

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
                &statements_analyzer.file_analyzer.file_source.file_path,
                &mut param_type,
                &TypeExpansionOptions {
                    parent_class: None,
                    function_is_final: if let Some(calling_classlike_storage) =
                        calling_classlike_storage
                    {
                        calling_classlike_storage.is_final
                    } else {
                        false
                    },
                    expand_type_aliases: false,
                    ..Default::default()
                },
                &mut analysis_data.data_flow_graph,
                &mut 0,
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

                if let Some(method_id) = functionlike_id.as_method_identifier() {
                    let declaring_method_id = statements_analyzer
                        .codebase
                        .get_declaring_method_id(&method_id);

                    if method_id != declaring_method_id {
                        let classlike_storage = statements_analyzer
                            .codebase
                            .classlike_infos
                            .get(&method_id.0)
                            .unwrap();

                        if let Some(extended_params) = classlike_storage
                            .template_extended_params
                            .get(&declaring_method_id.0)
                        {
                            for (foreign_template_id, type_) in extended_params {
                                template_result
                                    .lower_bounds
                                    .entry(*foreign_template_id)
                                    .or_insert_with(FxHashMap::default)
                                    .insert(
                                        GenericParent::ClassLike(declaring_method_id.0),
                                        vec![TemplateBound::new((**type_).clone(), 0, None, None)],
                                    );
                            }
                        }
                    }
                }
            }
        }
    }

    let last_param = functionlike_params.last();

    let mut param_types = BTreeMap::new();

    let codebase = statements_analyzer.codebase;

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
                    .or_insert_with(Vec::new)
                    .push((
                        *class,
                        Arc::new(lower_bounds.first().unwrap().bound_type.clone()),
                    ));
            }
        }
    }

    refine_template_result_for_functionlike(
        template_result,
        codebase,
        analysis_data,
        statements_analyzer.get_file_path(),
        &method_call_info,
        class_storage,
        calling_classlike_storage,
        functionlike_info,
        &class_generic_params,
    );

    for arg in args {
        let was_inside_call = context.inside_general_use;

        if functionlike_id != &FunctionLikeIdentifier::Method(StrId::SHAPES, StrId::PUT) {
            if matches!(functionlike_info.effects, FnEffect::Some(_))
                || matches!(functionlike_info.effects, FnEffect::Arg(_))
                || functionlike_info.has_throw
                || functionlike_info.user_defined
                || functionlike_info.method_info.is_some()
                || matches!(
                    functionlike_id,
                    FunctionLikeIdentifier::Function(StrId::ASIO_JOIN)
                )
            {
                context.inside_general_use = true;
            }
        }

        let arg_expr = arg.to_expr_ref();

        // don't analyse closures here
        if !matches!(arg_expr.2, aast::Expr_::Lfun(_) | aast::Expr_::Efun(_)) {
            expression_analyzer::analyze(
                statements_analyzer,
                arg_expr,
                analysis_data,
                context,
                false,
            )?;
        }

        if !was_inside_call {
            context.inside_general_use = false;
        }
    }

    let mut reordered_args = args.iter().enumerate().collect::<Vec<_>>();

    reordered_args.sort_by(|a, b| {
        matches!(a.1.to_expr_ref().2, aast::Expr_::Lfun(..))
            .cmp(&matches!(b.1.to_expr_ref().2, aast::Expr_::Lfun(..)))
    });

    for (argument_offset, arg) in reordered_args.clone() {
        let arg_expr = arg.to_expr_ref();
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
            if let Some(calling_classlike) = calling_classlike {
                calling_classlike.1
            } else {
                None
            },
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
                false,
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
            statements_analyzer.get_file_path(),
            arg_value_type.clone(),
            argument_offset,
            arg_expr.pos(),
            context,
            template_result,
            statements_analyzer,
            functionlike_id,
        );

        check_classname_passed_as_string(
            statements_analyzer,
            context,
            analysis_data,
            &param_type,
            &arg_value_type,
            arg,
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
            if let Some(calling_classlike) = calling_classlike {
                calling_classlike.1
            } else {
                None
            },
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
            statements_analyzer.get_file_path(),
            arg_value_type,
            reordered_args.len(),
            unpacked_arg.pos(),
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
            false,
        )?;
    }

    let function_params = &functionlike_info.params;

    if function_params.len() > args.len() {
        // Check for TooFewArguments - find the minimum required arguments
        // This is the position of the last required (non-default, non-variadic) parameter + 1
        // Skip this check if there's an unpacked argument as it could provide multiple values
        let mut min_required_args = 0;
        for (i, param) in function_params.iter().enumerate() {
            if !param.is_optional && !param.is_variadic {
                min_required_args = i + 1;
            }
        }

        if args.len() < min_required_args && unpacked_arg.is_none() {
            let interner = statements_analyzer.interner;
            let function_name = match functionlike_id {
                FunctionLikeIdentifier::Function(fn_name) => interner.lookup(fn_name).to_string(),
                FunctionLikeIdentifier::Method(fq_classlike_name, method_name) => {
                    format!(
                        "{}::{}",
                        interner.lookup(fq_classlike_name),
                        interner.lookup(method_name)
                    )
                }
                FunctionLikeIdentifier::Closure(..) => "closure".to_string(),
            };

            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::TooFewArguments,
                    format!(
                        "Too few arguments for {} - expecting {} but got {}",
                        function_name,
                        min_required_args,
                        args.len()
                    ),
                    statements_analyzer.get_hpos(function_call_pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }

        let mut i = args.len();
        let i_max = function_params.len();

        while i < i_max {
            let function_param = &function_params[i];
            if let Some(default_type) = &function_param.default_type {
                if let Some(param_type) = &function_param.signature_type {
                    if param_type.has_template() {
                        let default_type =
                            if let DefaultType::NormalData(default_union) = &default_type {
                                wrap_atomic(default_union.clone())
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

    for (argument_offset, arg) in reordered_args {
        let arg_expr = arg.to_expr_ref();
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
                let Some((inout_token_pos, ..)) = arg.as_ainout() else {
                    analysis_data.add_issue(Issue::new(
                        IssueKind::MissingInoutToken,
                        "Expecting inout token for inout argument".to_string(),
                        statements_analyzer.get_hpos(&arg_expr.1),
                        &context.function_context.calling_functionlike_id,
                    ));
                    continue;
                };

                if statements_analyzer.get_config().add_fixmes {
                    if let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset {
                        if current_stmt_offset.line != arg.as_ainout().unwrap().0.line() as u32 {
                            if !matches!(arg_expr.2, aast::Expr_::Xml(..)) {
                                *current_stmt_offset = StmtStart {
                                    offset: inout_token_pos.start_offset() as u32,
                                    line: inout_token_pos.line() as u32,
                                    column: inout_token_pos.to_raw_span().start.column() as u16,
                                    add_newline: true,
                                };
                            } else {
                                current_stmt_offset.line = inout_token_pos.line() as u32;
                            }
                        }

                        analysis_data.expr_fixme_positions.insert(
                            (
                                inout_token_pos.start_offset() as u32,
                                arg_expr.1.end_offset() as u32,
                            ),
                            *current_stmt_offset,
                        );
                    }
                }

                handle_possibly_matching_inout_param(
                    statements_analyzer,
                    analysis_data,
                    function_param,
                    functionlike_id,
                    args,
                    argument_offset,
                    inout_token_pos,
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
            arg,
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
                    statements_analyzer.file_analyzer.resolved_names,
                    Some((statements_analyzer.codebase, statements_analyzer.interner)),
                ) {
                    analysis_data.if_true_assertions.insert(
                        (
                            function_call_pos.start_offset() as u32,
                            function_call_pos.end_offset() as u32,
                        ),
                        FxHashMap::from_iter([(
                            "hakana taints".to_string(),
                            vec![Assertion::RemoveTaints(
                                VarId(statements_analyzer.interner.get(&expr_var_id).unwrap()),
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
                    &aast::Argument::Anormal(unpacked_arg.clone()),
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

    // for (_, map) in &template_result.lower_bounds {
    //     for (_, bounds) in map {
    //         if bounds.len() > 1 {
    //             //println!("{:#?}", bounds);
    //         }
    //     }
    // }

    Ok(())
}

fn adjust_param_type(
    class_generic_params: &IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
    param_type: &mut TUnion,
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    mut arg_value_type: TUnion,
    argument_offset: usize,
    arg_pos: &Pos,
    context: &mut BlockContext,
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
            statements_analyzer.interner,
            file_path,
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
                statements_analyzer.codebase,
                statements_analyzer.interner,
                statements_analyzer.get_file_path(),
                &Some(&arg_value_type),
                Some(argument_offset),
                Some(statements_analyzer.get_hpos(arg_pos)),
                StandinOpts {
                    calling_class: if let Some(calling_class) =
                        &context.function_context.calling_class
                    {
                        if !context.function_context.is_static {
                            if let FunctionLikeIdentifier::Method(_, method_name) = functionlike_id
                            {
                                if *method_name == StrId::CONSTRUCT {
                                    None
                                } else {
                                    Some(*calling_class)
                                }
                            } else {
                                Some(*calling_class)
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    },
                    calling_function: context.function_context.calling_functionlike_id,
                    ..Default::default()
                },
            );
        }

        for template_type in bindable_template_params {
            if let TAtomic::TGenericParam(TGenericParam {
                param_name,
                defining_entity,
                as_type,
                ..
            }) = template_type
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
    calling_static_type: Option<&TAtomic>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    if let Some(param) = param {
        if let Some(param_type) = &param.signature_type {
            let mut param_type = param_type.clone();

            type_expander::expand_union(
                codebase,
                &Some(statements_analyzer.interner),
                &statements_analyzer.file_analyzer.file_source.file_path,
                &mut param_type,
                &TypeExpansionOptions {
                    self_class: if let Some(classlike_storage) = class_storage {
                        Some(classlike_storage.name)
                    } else {
                        None
                    },
                    static_class_type: if let Some(calling_class_storage) =
                        calling_classlike_storage
                    {
                        if let Some(calling_static_type) = calling_static_type {
                            StaticClassType::Object(calling_static_type)
                        } else {
                            StaticClassType::Name(calling_class_storage.name)
                        }
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
                &mut 0,
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
    context: &mut BlockContext,
    functionlike_id: &FunctionLikeIdentifier,
    template_result: &mut TemplateResult,
    args: &[aast::Argument<(), ()>],
    closure_expr: &aast::Expr<(), ()>,
    param_type: &TUnion,
) {
    let codebase = statements_analyzer.codebase;

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
                        .collect::<Vec<_>>(),
                )
            })
            .collect(),
        IndexMap::new(),
    );

    let mut replaced_type = standin_type_replacer::replace(
        param_type,
        &mut replace_template_result,
        codebase,
        statements_analyzer.interner,
        statements_analyzer.get_file_path(),
        &None,
        None,
        None,
        StandinOpts {
            calling_class: None,
            calling_function: context.function_context.calling_functionlike_id,
            ..Default::default()
        },
    );

    replaced_type =
        inferred_type_replacer::replace(&replaced_type, &replace_template_result, codebase);

    let mut closure_storage = {
        match functionlike_analyzer::get_closure_storage(
            statements_analyzer.file_analyzer,
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
                if let TAtomic::TClosure(replaced_closure) = replaced_type_part {
                    let replaced_param_type =
                        if let Some(signature_type) = replaced_closure.params.get(param_offset) {
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
        ) || !statements_analyzer.get_config().in_migration
        {
            if let FunctionLikeIdentifier::Function(
                StrId::LIB_VEC_MAP
                | StrId::LIB_DICT_MAP
                | StrId::LIB_KEYSET_MAP
                | StrId::LIB_VEC_MAP_ASYNC
                | StrId::LIB_DICT_MAP_ASYNC
                | StrId::LIB_KEYSET_MAP_ASYNC
                | StrId::LIB_VEC_FILTER
                | StrId::LIB_DICT_FILTER
                | StrId::LIB_KEYSET_FILTER
                | StrId::LIB_VEC_TAKE
                | StrId::LIB_DICT_TAKE
                | StrId::LIB_KEYSET_TAKE
                | StrId::LIB_C_FIND
                | StrId::LIB_C_FINDX
                | StrId::LIB_VEC_MAP_WITH_KEY
                | StrId::LIB_DICT_MAP_WITH_KEY
                | StrId::LIB_KEYSET_MAP_WITH_KEY
                | StrId::LIB_DICT_MAP_WITH_KEY_ASYNC
                | StrId::LIB_DICT_FROM_KEYS
                | StrId::LIB_DICT_FROM_KEYS_ASYNC,
            ) = functionlike_id
            {
                if param_offset == 0 {
                    if let Some(ref mut signature_type) = param_storage.signature_type {
                        add_array_fetch_dataflow(
                            statements_analyzer,
                            &args[0].to_expr_ref().1,
                            analysis_data,
                            None,
                            signature_type,
                            &mut get_arraykey(false),
                        );
                    }
                }
            }
        }
    }

    analysis_data
        .closures
        .insert(closure_expr.pos().clone(), closure_storage);
}

fn map_class_generic_params(
    class_generic_params: &IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
    param_type: &mut TUnion,
    codebase: &CodebaseInfo,
    interner: &Interner,
    file_path: &FilePath,
    arg_value_type: &mut TUnion,
    argument_offset: usize,
    context: &mut BlockContext,
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
        interner,
        file_path,
        &Some(arg_value_type),
        Some(argument_offset),
        None,
        StandinOpts {
            calling_class: context.function_context.calling_class,
            calling_function: context.function_context.calling_functionlike_id,
            ..Default::default()
        },
    );

    if arg_has_template_types {
        *arg_value_type = template::standin_type_replacer::replace(
            &*arg_value_type,
            template_result,
            codebase,
            interner,
            file_path,
            &Some(arg_value_type),
            Some(argument_offset),
            None,
            StandinOpts {
                calling_class: context.function_context.calling_class,
                calling_function: context.function_context.calling_functionlike_id,
                ..Default::default()
            },
        );
    }
}

pub(crate) fn evaluate_arbitrary_param(
    statements_analyzer: &StatementsAnalyzer,
    arg: &aast::Argument<(), ()>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let was_inside_call = context.inside_general_use;
    context.inside_general_use = true;

    let is_inout = matches!(arg, aast::Argument::Ainout(..));

    expression_analyzer::analyze(
        statements_analyzer,
        arg.to_expr_ref(),
        analysis_data,
        context,
        false,
    )?;

    if !was_inside_call {
        context.inside_general_use = false;
    }

    if is_inout {
        let var_id = get_var_id(
            arg.to_expr_ref(),
            context.function_context.calling_class,
            statements_analyzer.file_analyzer.resolved_names,
            Some((statements_analyzer.codebase, statements_analyzer.interner)),
        );

        if let Some(var_id) = var_id {
            if let Some(t) = context.locals.get(var_id.as_str()) {
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
                arg.to_expr_ref().pos().start_offset() as u32,
                arg.to_expr_ref().pos().end_offset() as u32,
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
    all_args: &[aast::Argument<(), ()>],
    argument_offset: usize,
    inout_token_pos: &Pos,
    expr: &aast::Expr<(), ()>,
    classlike_storage: Option<&ClassLikeInfo>,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    context: &mut BlockContext,
    template_result: &mut TemplateResult,
    function_call_pos: &Pos,
) -> Result<(), AnalysisError> {
    let mut inout_type = functionlike_param
        .signature_type
        .clone()
        .unwrap_or(get_mixed_any());

    let codebase = statements_analyzer.codebase;

    let arg_type = analysis_data.get_expr_type(expr.pos()).cloned();

    if !template_result.template_types.is_empty() {
        let original_inout_type = inout_type.clone();

        inout_type = standin_type_replacer::replace(
            &inout_type,
            template_result,
            codebase,
            statements_analyzer.interner,
            statements_analyzer.get_file_path(),
            &if let Some(arg_type) = &arg_type {
                Some(arg_type)
            } else {
                None
            },
            Some(argument_offset),
            None,
            StandinOpts {
                calling_class: context.function_context.calling_class,
                calling_function: if let Some(m) = &context.function_context.calling_functionlike_id
                {
                    Some(*m)
                } else {
                    None
                },
                ..Default::default()
            },
        );

        if !template_result.lower_bounds.is_empty() {
            inout_type =
                inferred_type_replacer::replace(&original_inout_type, template_result, codebase);
        }
    }

    type_expander::expand_union(
        codebase,
        &Some(statements_analyzer.interner),
        &statements_analyzer.file_analyzer.file_source.file_path,
        &mut inout_type,
        &TypeExpansionOptions {
            self_class: if let Some(classlike_storage) = classlike_storage {
                Some(classlike_storage.name)
            } else {
                None
            },
            static_class_type: if let Some(calling_class_storage) = calling_classlike_storage {
                StaticClassType::Name(calling_class_storage.name)
            } else {
                StaticClassType::None
            },
            parent_class: None,
            function_is_final: if let Some(calling_class_storage) = calling_classlike_storage {
                calling_class_storage.is_final
            } else {
                false
            },
            ..Default::default()
        },
        &mut analysis_data.data_flow_graph,
        &mut 0,
    );

    let arg_type = arg_type.unwrap_or(get_mixed_any());

    let assignment_node = DataFlowNode::get_for_method_argument_out(
        functionlike_id,
        argument_offset,
        Some(functionlike_param.name_location),
        Some(statements_analyzer.get_hpos(function_call_pos)),
    );

    if let GraphKind::FunctionBody = &analysis_data.data_flow_graph.kind {
        for arg_node in &arg_type.parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &arg_node.id,
                &assignment_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
    }

    if matches!(
        functionlike_id,
        FunctionLikeIdentifier::Function(
            StrId::PREG_MATCH_WITH_MATCHES | StrId::PREG_MATCH_ALL_WITH_MATCHES
        )
    ) && argument_offset == 2
    {
        let removed_taints =
            get_removed_taints_in_comments(statements_analyzer, all_args[0].to_expr_ref().pos());

        let argument_node = DataFlowNode::get_for_method_argument(
            functionlike_id,
            0,
            Some(statements_analyzer.get_hpos(all_args[1].to_expr_ref().pos())),
            Some(statements_analyzer.get_hpos(function_call_pos)),
        );

        analysis_data
            .data_flow_graph
            .add_node(argument_node.clone());

        analysis_data.data_flow_graph.add_path(
            &argument_node.id,
            &assignment_node.id,
            PathKind::Aggregate,
            vec![],
            vec![],
        );

        let argument_node = DataFlowNode::get_for_method_argument(
            functionlike_id,
            1,
            Some(statements_analyzer.get_hpos(all_args[1].to_expr_ref().pos())),
            Some(statements_analyzer.get_hpos(function_call_pos)),
        );

        analysis_data
            .data_flow_graph
            .add_node(argument_node.clone());

        analysis_data.data_flow_graph.add_path(
            &argument_node.id,
            &assignment_node.id,
            PathKind::Default,
            vec![],
            removed_taints,
        );
    } else if matches!(
        functionlike_id,
        FunctionLikeIdentifier::Function(StrId::JSON_DECODE_WITH_ERROR)
    ) && argument_offset == 1
    {
        let argument_node = DataFlowNode::get_for_method_argument(
            functionlike_id,
            0,
            Some(statements_analyzer.get_hpos(all_args[1].to_expr_ref().pos())),
            Some(statements_analyzer.get_hpos(function_call_pos)),
        );

        analysis_data
            .data_flow_graph
            .add_node(argument_node.clone());

        analysis_data.data_flow_graph.add_path(
            &argument_node.id,
            &assignment_node.id,
            PathKind::Aggregate,
            vec![],
            vec![],
        );
    }

    analysis_data
        .data_flow_graph
        .add_node(assignment_node.clone());

    assignment_analyzer::analyze_inout_param(
        statements_analyzer,
        expr,
        arg_type,
        &inout_type,
        inout_token_pos,
        assignment_node,
        analysis_data,
        context,
    )?;

    Ok(())
}

fn refine_template_result_for_functionlike(
    template_result: &mut TemplateResult,
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    file_path: &FilePath,
    method_call_info: &Option<MethodCallInfo>,
    classlike_storage: Option<&ClassLikeInfo>,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    functionlike_storage: &FunctionLikeInfo,
    class_template_params: &IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
) {
    let template_types = get_template_types_for_class_member(
        codebase,
        analysis_data,
        file_path,
        classlike_storage,
        if let Some(method_call_info) = method_call_info {
            Some(method_call_info.self_fq_classlike_name)
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

pub(crate) fn get_template_types_for_class_member(
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    file_path: &FilePath,
    declaring_classlike_storage: Option<&ClassLikeInfo>,
    appearing_class_name: Option<StrId>,
    calling_classlike_storage: Option<&ClassLikeInfo>,
    existing_template_types: &[(StrId, Vec<(GenericParent, Arc<TUnion>)>)],
    class_template_params: &IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>>,
) -> IndexMap<StrId, FxHashMap<GenericParent, TUnion>> {
    let mut template_types: IndexMap<StrId, Vec<(GenericParent, Arc<TUnion>)>> =
        IndexMap::from_iter(existing_template_types.to_owned());

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
                                let output_type_candidate =
                                    if let TAtomic::TGenericParam(TGenericParam {
                                        defining_entity: GenericParent::ClassLike(defining_entity),
                                        param_name,
                                        ..
                                    }) = &atomic_type
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
                            .or_insert_with(Vec::new)
                            .push((
                                GenericParent::ClassLike(declaring_classlike_storage.name),
                                output_type,
                            ));
                    }
                }
            }
        } else if !declaring_classlike_storage.template_types.is_empty() {
            for (template_name, type_map) in &declaring_classlike_storage.template_types {
                for (key, type_) in type_map {
                    template_types
                        .entry(*template_name)
                        .or_insert_with(Vec::new)
                        .push((
                            *key,
                            class_template_params
                                .get(template_name)
                                .unwrap_or(&vec![])
                                .iter()
                                .filter(|(k, _)| k == key)
                                .map(|(_, v)| v)
                                .next()
                                .cloned()
                                .unwrap_or(type_.clone()),
                        ));
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
                            file_path,
                            &mut v,
                            &TypeExpansionOptions {
                                self_class: appearing_class_name,
                                static_class_type: if let Some(calling_class_storage) =
                                    calling_classlike_storage
                                {
                                    StaticClassType::Name(calling_class_storage.name)
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
                            &mut 0,
                        );
                        v
                    })
                })
                .collect(),
        );
    }

    expanded_template_types
}

/// Check for expressions of type classname<T> used in a string context.
///
/// Presently, this only checks for and autofixes the by far the most common case
/// of using a classname literal (i.e. C::class) as a parameter to a function that takes string.
fn check_classname_passed_as_string(
    statements_analyzer: &StatementsAnalyzer,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    param_type: &TUnion,
    arg_value_type: &TUnion,
    arg: &aast::Argument<(), ()>,
) {
    let is_string_param = param_type.types.iter().any(|t| *t == TAtomic::TString);

    if is_string_param {
        class_constant_fetch_analyzer::check_class_ptr_used_as_string(
            statements_analyzer,
            context,
            analysis_data,
            arg_value_type,
            arg.to_expr_ref(),
        );
    }
}
