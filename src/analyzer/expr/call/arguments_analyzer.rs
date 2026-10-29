use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use hakana_code_info::assertion::Assertion;
use hakana_code_info::code_location::{FilePath, HPos, StmtStart};
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::ttype::comparison::{
    type_comparison_result::TypeComparisonResult, union_type_comparator,
};
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
use hakana_code_info::t_atomic::{TAtomic, TGenericParam, TVec};
use hakana_code_info::t_union::{TUnion, populate_union_type};
use hakana_code_info::ttype::template::{
    self, TemplateBound, TemplateResult, inferred_type_replacer, standin_type_replacer,
};
use hakana_code_info::ttype::type_expander::{self, StaticClassType, TypeExpansionOptions};
use hakana_code_info::ttype::{
    add_optional_union_type, combine_optional_union_types, combine_union_types,
    deep_generalize_template_bound, get_arrayish_params, get_arraykey, get_mixed_any, get_nothing,
    wrap_atomic,
};
use hakana_reflector::typehint_resolver::get_type_from_hint;
use indexmap::IndexMap;
use oxidized::aast;
use oxidized::pos::Pos;

use super::argument_analyzer::{self, get_removed_taints_in_comments};
use super::method_call_info::MethodCallInfo;
use super::template_inference::{self, CallTemplateInference};
use hakana_code_info::ttype::type_variable;

pub(crate) fn check_arguments_match(
    statements_analyzer: &StatementsAnalyzer,
    type_args: &[aast::Targ<()>],
    args: &[aast::Argument<(), ()>],
    unpacked_arg: &Option<aast::Expr<(), ()>>,
    functionlike_id: &FunctionLikeIdentifier,
    functionlike_info: &FunctionLikeInfo,
    contextual_return_type: Option<&TUnion>,
    calling_classlike: Option<(StrId, Option<&TAtomic>, bool)>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    template_result: &mut TemplateResult,
    function_call_pos: &Pos,
    function_name_pos: Option<&Pos>,
) -> Result<(), AnalysisError> {
    crate::expr::call_usage::record_call(
        statements_analyzer,
        analysis_data,
        context,
        functionlike_id,
        functionlike_info,
        args,
        unpacked_arg.is_some(),
        function_call_pos,
    );
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

        if class_storage.is_none() {
            return Err(AnalysisError::InternalError(
                "Class storage does not exist".to_string(),
                statements_analyzer.get_hpos(function_call_pos),
            ));
        }
        method_call_info = Some(MethodCallInfo {
            self_fq_classlike_name,
            declaring_method_id: Some(declaring_method_id),
            dispatch_class: calling_classlike
                .filter(|(_, _, is_virtual)| *is_virtual)
                .map(|(class_name, _, _)| class_name),
        });
    }

    let mut class_generic_params = IndexMap::new();

    for (template_name, type_map) in &template_result.lower_bounds {
        for (class, lower_bounds) in type_map {
            if lower_bounds.len() == 1 && matches!(class, GenericParent::ClassLike(_)) {
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

    if let (Some(declaring), Some(calling)) = (class_storage, calling_classlike_storage)
        && declaring.name != calling.name
        && let Some(inherited_params) = calling.template_extended_params.get(&declaring.name)
    {
        for (name, ty) in inherited_params {
            if !ty.has_template() {
                // A concrete parent specialization (Child extends Parent<int>)
                // is already fixed before any argument is analyzed.
                template_result
                    .lower_bounds
                    .entry(*name)
                    .or_default()
                    .entry(GenericParent::ClassLike(declaring.name))
                    .or_insert_with(|| vec![TemplateBound::new((**ty).clone(), 0, None, None)]);
            }
        }
    }

    let calling_static_type = calling_classlike.and_then(|calling_classlike| calling_classlike.1);

    let expanded_param_types = functionlike_params
        .iter()
        .map(|param| {
            get_param_type(
                Some(param),
                codebase,
                class_storage,
                calling_classlike_storage,
                calling_static_type,
                statements_analyzer,
                analysis_data,
            )
        })
        .collect::<Vec<_>>();
    let inference = CallTemplateInference::new(template_result, analysis_data, statements_analyzer);
    let instantiated_param_types = vec![OnceCell::<Rc<TUnion>>::new(); functionlike_params.len()];
    let instantiate_param = |offset: usize| {
        Rc::clone(instantiated_param_types[offset].get_or_init(|| {
            Rc::new(inference.instantiate(&expanded_param_types[offset], codebase))
        }))
    };
    // The actual result is checked by the caller after return-type
    // specialization. Only use the declaration early when it can solve the
    // callee's generics; otherwise it can pollute the caller's constraints.
    if !inference.is_empty()
        && let (Some(return_type), Some(expected_type)) = (
            contextual_return_type,
            analysis_data
                .expected_arg_types
                .get(&(
                    function_call_pos.start_offset() as u32,
                    function_call_pos.end_offset() as u32,
                ))
                .cloned(),
        )
    {
        let mut return_type = return_type.clone();
        type_expander::expand_union(
            codebase,
            &Some(statements_analyzer.interner),
            statements_analyzer.get_file_path(),
            &mut return_type,
            &TypeExpansionOptions {
                self_class: class_storage.map(|storage| storage.name),
                static_class_type: match (calling_classlike_storage, calling_static_type) {
                    (_, Some(ty)) => StaticClassType::Object(ty),
                    (Some(storage), None) => StaticClassType::Name(storage.name),
                    _ => StaticClassType::None,
                },
                function_is_final: calling_classlike_storage
                    .is_some_and(|storage| storage.is_final),
                expand_templates: false,
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
            &mut 0,
        );
        template_inference::constrain(
            statements_analyzer,
            analysis_data,
            &inference.instantiate(&return_type, codebase),
            &expected_type,
            statements_analyzer.get_hpos(function_call_pos),
            None,
        );
        template_inference::propagate(statements_analyzer, analysis_data);
    }

    for (argument_offset, arg) in args.iter().enumerate() {
        let arg_expr = arg.to_expr_ref();
        if !can_use_expected_type(arg_expr) {
            continue;
        }
        if let Some(param_offset) = get_param_offset_for_arg(
            statements_analyzer,
            functionlike_params,
            arg,
            argument_offset,
        ) && !functionlike_params[param_offset].is_splat
        {
            // A splat parameter describes the whole argument tuple, not the
            // expected type of the first value passed into that tuple.
            let expected_type = instantiate_param(param_offset);
            if !expected_type.is_mixed() {
                analysis_data.expected_arg_types.insert(
                    (
                        arg_expr.pos().start_offset() as u32,
                        arg_expr.pos().end_offset() as u32,
                    ),
                    expected_type,
                );
            }
        }
    }

    for arg in args {
        let was_inside_call = context.inside_general_use;

        if functionlike_id != &FunctionLikeIdentifier::Method(StrId::SHAPES, StrId::PUT)
            && (matches!(functionlike_info.effects, FnEffect::Some(_))
                || matches!(functionlike_info.effects, FnEffect::Arg(_))
                || functionlike_info.has_throw
                || functionlike_info.user_defined
                || functionlike_info.method_info.is_some()
                || matches!(
                    functionlike_id,
                    FunctionLikeIdentifier::Function(StrId::ASIO_JOIN)
                ))
        {
            context.inside_general_use = true;
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

    for (argument_offset, arg) in args.iter().enumerate() {
        if !inference.is_empty()
            && let Some(param_offset) = get_param_offset_for_arg(
                statements_analyzer,
                functionlike_params,
                arg,
                argument_offset,
            )
            && !matches!(
                arg.to_expr_ref().2,
                aast::Expr_::FunctionPointer(..) | aast::Expr_::Lfun(..) | aast::Expr_::Efun(..)
            )
            && let Some(arg_type) = if functionlike_params[param_offset].is_splat {
                Some(get_splat_argument_type(
                    args,
                    argument_offset,
                    analysis_data,
                ))
            } else {
                analysis_data
                    .get_expr_type(arg.to_expr_ref().pos())
                    .cloned()
            }
        {
            template_inference::constrain(
                statements_analyzer,
                analysis_data,
                &arg_type,
                &instantiate_param(param_offset),
                statements_analyzer.get_hpos(arg.to_expr_ref().pos()),
                Some(argument_offset),
            );
        }
    }
    template_inference::propagate(statements_analyzer, analysis_data);
    let known_types = inference.known_types(analysis_data, codebase);

    let mut reordered_args = args.iter().enumerate().collect::<Vec<_>>();

    reordered_args.sort_by(|a, b| {
        matches!(a.1.to_expr_ref().2, aast::Expr_::Lfun(..))
            .cmp(&matches!(b.1.to_expr_ref().2, aast::Expr_::Lfun(..)))
    });

    let mut param_types = BTreeMap::new();
    let mut callback_annotations_constrained = false;

    for (argument_offset, arg) in &reordered_args {
        let arg_expr = arg.to_expr_ref();

        let param_offset = get_param_offset_for_arg(
            statements_analyzer,
            functionlike_params,
            arg,
            *argument_offset,
        );
        let mut param_type = param_offset
            .map(|offset| expanded_param_types[offset].clone())
            .unwrap_or_else(get_mixed_any);
        if !known_types.lower_bounds.is_empty() {
            param_type = inferred_type_replacer::replace(&param_type, &known_types, codebase);
        }

        let was_inside_call = context.inside_general_use;

        context.inside_general_use = true;

        if !was_inside_call {
            context.inside_general_use = false;
        }

        let mut arg_value_type = analysis_data
            .get_expr_type(arg_expr.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        let closure_param_type = if let aast::Expr_::Lfun(_) | aast::Expr_::Efun(_) = arg_expr.2 {
            Some(param_type.clone())
        } else {
            None
        };

        if let Some(closure_param_type) = &closure_param_type {
            let _ = handle_closure_arg(
                statements_analyzer,
                analysis_data,
                context,
                functionlike_id,
                template_result,
                args,
                arg_expr,
                closure_param_type,
                *argument_offset,
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

        if matches!(arg_expr.2, aast::Expr_::FunctionPointer(..))
            && arg_value_type.has_template_types()
        {
            arg_value_type = infer_function_pointer_type(
                statements_analyzer,
                &arg_value_type,
                &param_type,
                template_result,
            );
            analysis_data.set_expr_type(arg_expr.pos(), arg_value_type.clone());
        }

        // The constraint pass retains variable relationships. The legacy
        // template matcher needs their current values; otherwise it replaces a
        // known collection variable with any and feeds that back into inference.
        if type_variable::contains(&arg_value_type) {
            arg_value_type = type_variable::resolve(
                &arg_value_type,
                &analysis_data.type_variable_bounds,
                codebase,
            );
        }

        adjust_param_type(
            &class_generic_params,
            &mut param_type,
            codebase,
            statements_analyzer.get_file_path(),
            if param_offset.is_some_and(|offset| functionlike_params[offset].is_splat) {
                get_splat_argument_type(args, *argument_offset, analysis_data)
            } else {
                arg_value_type.clone()
            },
            *argument_offset,
            arg_expr.pos(),
            context,
            template_result,
            statements_analyzer,
            functionlike_id,
            &analysis_data.closure_return_types,
        );

        if let Some(closure_param_type) = &closure_param_type
            && let Some(resolved_closure_type) = handle_closure_arg(
                statements_analyzer,
                analysis_data,
                context,
                functionlike_id,
                template_result,
                args,
                arg_expr,
                closure_param_type,
                *argument_offset,
            )
        {
            param_type = resolved_closure_type;

            expression_analyzer::analyze(
                statements_analyzer,
                arg_expr,
                analysis_data,
                context,
                false,
            )?;
        }

        if !inference.is_empty()
            && let Some(param_offset) = param_offset
            && matches!(arg_expr.2, aast::Expr_::Lfun(..) | aast::Expr_::Efun(..))
            && let Some(arg_type) = analysis_data.get_expr_type(arg_expr.pos()).cloned()
            && let Some(closure_storage) = functionlike_analyzer::get_closure_storage(
                statements_analyzer.file_analyzer,
                arg_expr.pos().start_offset(),
            )
            && closure_storage
                .params
                .iter()
                .any(|param| param.signature_type.is_some())
        {
            let mut expected = (*instantiate_param(param_offset)).clone();
            for atomic in &mut expected.types {
                if let TAtomic::TClosure(closure) = atomic {
                    // Inferred parameters repeat the context supplied by this
                    // call; only annotations contribute new upper bounds.
                    for (offset, param) in closure.params.iter_mut().enumerate() {
                        if closure_storage
                            .params
                            .get(offset)
                            .is_none_or(|param| param.signature_type.is_none())
                        {
                            param.signature_type = None;
                        }
                    }
                    // Return constraints are collected individually above,
                    // preserving their positions for diagnostics and FIXMEs.
                    closure.return_type = None;
                }
            }
            template_inference::constrain(
                statements_analyzer,
                analysis_data,
                &arg_type,
                &expected,
                statements_analyzer.get_hpos(arg_expr.pos()),
                Some(*argument_offset),
            );
            callback_annotations_constrained = true;
        }

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

        expression_analyzer::analyze(
            statements_analyzer,
            unpacked_arg,
            analysis_data,
            context,
            false,
        )?;

        let arg_value_type = analysis_data
            .get_expr_type(unpacked_arg.pos())
            .cloned()
            .unwrap_or(get_mixed_any());

        // the variadic param type describes a single element, so solve
        // templates against the unpacked container's value type
        let mut arg_element_type = None;
        for arg_atomic_type in &arg_value_type.types {
            if let Some((_, value_param)) = get_arrayish_params(arg_atomic_type, codebase) {
                arg_element_type = Some(combine_optional_union_types(
                    arg_element_type.as_ref(),
                    Some(&value_param),
                    codebase,
                ));
            }
        }

        adjust_param_type(
            &class_generic_params,
            &mut param_type,
            codebase,
            statements_analyzer.get_file_path(),
            arg_element_type.unwrap_or(arg_value_type),
            reordered_args.len(),
            unpacked_arg.pos(),
            context,
            template_result,
            statements_analyzer,
            functionlike_id,
            &analysis_data.closure_return_types,
        );

        last_param_type = Some(param_type.clone());
    }

    inference.finish(template_result, statements_analyzer, analysis_data);

    let function_params = &functionlike_info.params;

    if function_params.len() > args.len() {
        // Check for TooFewArguments - find the minimum required positional arguments
        // This is the position of the last required (non-default, non-variadic, non-named) parameter + 1
        // Skip this check if there's an unpacked argument as it could provide multiple values
        // Named parameters are checked separately in check_named_arguments()
        let mut min_required_args = 0;
        for (i, param) in function_params.iter().enumerate() {
            if param.is_required() {
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
            if let Some(default_type) = &function_param.default_type
                && let Some(param_type) = &function_param.signature_type
                && param_type.has_template()
            {
                let default_type = if let DefaultType::NormalData(default_union) = &default_type {
                    wrap_atomic(default_union.clone())
                } else {
                    // todo handle unresolved constants
                    get_mixed_any()
                };

                if default_type.has_literal_value() {
                    // todo check templated default arg matches
                }
            }
            i += 1;
        }
    }

    // Check for named argument issues
    check_named_arguments(
        statements_analyzer,
        args,
        function_params,
        functionlike_id,
        analysis_data,
        context,
        function_call_pos,
    );

    let mut templates_contain_variables = None;
    for (argument_offset, arg) in &reordered_args {
        let arg_expr = arg.to_expr_ref();

        let Some(param_offset) =
            get_param_offset_for_arg(statements_analyzer, function_params, arg, *argument_offset)
        else {
            continue;
        };
        let function_param = &function_params[param_offset];

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

                if statements_analyzer.get_config().add_fixmes
                    && let Some(ref mut current_stmt_offset) = analysis_data.current_stmt_offset
                {
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

                handle_possibly_matching_inout_param(
                    statements_analyzer,
                    analysis_data,
                    function_param,
                    functionlike_id,
                    args,
                    *argument_offset,
                    inout_token_pos,
                    arg_expr,
                    class_storage,
                    calling_classlike_storage,
                    context,
                    template_result,
                    function_call_pos,
                )?;
                templates_contain_variables = None;
            }
        }

        let Some(mut arg_value_type) = analysis_data.get_expr_type(arg_expr.pos()).cloned() else {
            // todo increment mixed count
            continue;
        };

        // Constructor variables stay live for later writes. Only call-result
        // variables need expansion here to check a nested call's result.
        if !inference.is_empty()
            && type_variable::contains(&arg_value_type)
            && !matches!(arg_expr.2, aast::Expr_::Lfun(..) | aast::Expr_::Efun(..))
        {
            arg_value_type = type_variable::resolve(
                &arg_value_type,
                &analysis_data.type_variable_bounds,
                codebase,
            );
        }
        // Callback annotations may have added equality constraints after the
        // first argument pass. Check against their current solution.
        let resolved_param_type = if callback_annotations_constrained
            || *templates_contain_variables.get_or_insert_with(|| {
                template_result.lower_bounds.values().any(|entities| {
                    entities.values().any(|bounds| {
                        bounds
                            .iter()
                            .any(|bound| type_variable::contains(&bound.bound_type))
                    })
                })
            })
            || type_variable::contains(&expanded_param_types[param_offset])
        {
            let substituted = inferred_type_replacer::replace(
                &expanded_param_types[param_offset],
                template_result,
                codebase,
            );
            Some(type_variable::resolve(
                &substituted,
                &analysis_data.type_variable_bounds,
                codebase,
            ))
        } else {
            None
        };
        let inferred_param_type = resolved_param_type
            .as_ref()
            .filter(|ty| callback_annotations_constrained || type_variable::contains(ty))
            .unwrap_or_else(|| param_types.get(&argument_offset).unwrap());

        // A splat parameter is typed as a generic tuple, so zip up all arguments starting from the index of the splat parameter
        // and compare the type of the resulting tuple with the parameter type tuple.
        // The typechecker makes sure every function has at most one splat parameter which also has to be the last parameter,
        // so no further special casing is needed here.
        if function_param.is_splat {
            arg_value_type = get_splat_argument_type(args, *argument_offset, analysis_data);
        }

        let was_inside_call = context.inside_general_use;

        if matches!(functionlike_info.effects, FnEffect::Some(_)) {
            context.inside_general_use = true;
        }

        argument_analyzer::check_argument_matches(
            statements_analyzer,
            functionlike_id,
            &method_call_info,
            function_param,
            inferred_param_type,
            *argument_offset,
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

        if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind
            && let Some(removed_taints) = &function_param.removed_taints_when_returning_true
            && let Some(expr_var_id) = expression_identifier::get_var_id(
                arg_expr,
                None,
                statements_analyzer.file_analyzer.resolved_names,
                Some((statements_analyzer.codebase, statements_analyzer.interner)),
            )
        {
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

    // analyze unpacked arg
    if let Some(unpacked_arg) = unpacked_arg
        && let Some(last_param) = function_params.last()
        && last_param.is_variadic
    {
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
            &last_param_type.unwrap(),
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

    super::type_assertion_analyzer::check(
        statements_analyzer,
        functionlike_info,
        args,
        template_result,
        analysis_data,
        context,
    );

    // for (_, map) in &template_result.lower_bounds {
    //     for (_, bounds) in map {
    //         if bounds.len() > 1 {
    //             //println!("{:#?}", bounds);
    //         }
    //     }
    // }

    Ok(())
}

/// Whether an expression can use the type its parent expects of it: a call, which
/// can solve its templates from it, or a wrapper that passes it to its contents.
pub(crate) fn can_use_expected_type(expr: &aast::Expr<(), ()>) -> bool {
    match &expr.2 {
        aast::Expr_::Call(_) | aast::Expr_::Await(_) => true,
        aast::Expr_::ValCollection(boxed) => matches!(boxed.0.1, oxidized::ast::VcKind::Vec),
        _ => false,
    }
}

fn get_splat_argument_type(
    args: &[aast::Argument<(), ()>],
    argument_offset: usize,
    analysis_data: &FunctionAnalysisData,
) -> TUnion {
    let arg_types = args
        .iter()
        .skip(argument_offset)
        .enumerate()
        .map(|(i, arg)| {
            let arg_type = analysis_data
                .get_expr_type(arg.to_expr_ref().pos())
                .cloned()
                .unwrap_or_else(get_mixed_any);
            (i, (false, arg_type))
        })
        .collect::<BTreeMap<_, _>>();
    TUnion::new(vec![TAtomic::TVec(TVec {
        type_param: Box::new(get_nothing()),
        known_count: Some(arg_types.len()),
        non_empty: !arg_types.is_empty(),
        known_items: Some(arg_types),
        variadic_type: None,
    })])
}

fn get_param_offset_for_arg(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_params: &[FunctionLikeParameter],
    arg: &aast::Argument<(), ()>,
    argument_offset: usize,
) -> Option<usize> {
    if let aast::Argument::Anamed(sid, _) = arg {
        let arg_name = &sid.1;
        return functionlike_params.iter().position(|p| {
            let param_name = statements_analyzer.interner.lookup(&p.name.0);
            let clean_name = param_name.strip_prefix('$').unwrap_or(param_name);
            clean_name == arg_name
        });
    }

    if argument_offset < functionlike_params.len() {
        Some(argument_offset)
    } else if functionlike_params.last().is_some_and(|p| p.is_variadic) {
        Some(functionlike_params.len() - 1)
    } else {
        None
    }
}

fn infer_function_pointer_type(
    statements_analyzer: &StatementsAnalyzer,
    arg_type: &TUnion,
    param_type: &TUnion,
    template_result: &TemplateResult,
) -> TUnion {
    let codebase = statements_analyzer.codebase;
    let expected_type = inferred_type_replacer::replace(param_type, template_result, codebase);
    let mut pointer_templates = IndexMap::new();
    for template in arg_type.get_template_types() {
        if let TAtomic::TGenericParam(TGenericParam {
            param_name,
            defining_entity: defining_entity @ GenericParent::FunctionLike(_),
            as_type,
        }) = template
        {
            pointer_templates
                .entry(*param_name)
                .or_insert_with(Vec::new)
                .push((*defining_entity, Arc::new((**as_type).clone())));
        }
    }
    let mut pointer_result = TemplateResult::new(pointer_templates, IndexMap::new());

    // A generic function reference is instantiated for the callback's inputs.
    // For example, map(..., Keyset\flatten<>) must infer flatten's Tv before
    // using its keyset<Tv> return type as a bound for map's output.
    for arg in &arg_type.types {
        for expected in &expected_type.types {
            if let (TAtomic::TClosure(arg), TAtomic::TClosure(expected)) = (arg, expected) {
                for (arg_param, expected_param) in arg.params.iter().zip(&expected.params) {
                    if let (Some(arg_param), Some(expected_param)) =
                        (&arg_param.signature_type, &expected_param.signature_type)
                    {
                        standin_type_replacer::replace(
                            arg_param,
                            &mut pointer_result,
                            codebase,
                            statements_analyzer.interner,
                            statements_analyzer.get_file_path(),
                            &Some(expected_param),
                            None,
                            None,
                            StandinOpts::default(),
                        );
                    }
                }
            }
        }
    }

    for template in arg_type.get_template_types() {
        if let TAtomic::TGenericParam(TGenericParam { as_type, .. }) = template {
            let inferred = inferred_type_replacer::replace(
                &wrap_atomic(template.clone()),
                &pointer_result,
                codebase,
            );
            let bound = inferred_type_replacer::replace(as_type, &pointer_result, codebase);
            if !union_type_comparator::is_contained_by(
                codebase,
                statements_analyzer.get_file_path(),
                &inferred,
                &bound,
                false,
                false,
                false,
                &mut TypeComparisonResult::new(),
            ) {
                // Retain the constraint so ordinary argument checking can
                // report the invalid callback instead of accepting it.
                return arg_type.clone();
            }
        }
    }

    inferred_type_replacer::replace(arg_type, &pointer_result, codebase)
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
    closure_return_types: &FxHashMap<(FilePath, u32), Vec<(TUnion, HPos)>>,
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
                    closure_return_types: Some(closure_return_types),
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
                && (if let Some(bounds_by_param) = template_result.lower_bounds.get(&param_name) {
                    bounds_by_param.get(&defining_entity)
                } else {
                    None
                })
                .is_none()
            {
                let bound_type =
                    if let Some(bounds_by_param) = template_result.upper_bounds.get(&param_name) {
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
                    self_class: class_storage.map(|classlike_storage| classlike_storage.name),
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
    argument_offset: usize,
) -> Option<TUnion> {
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
                            let mut resolved_type =
                                template::standin_type_replacer::get_most_specific_type_from_bounds(
                                    lower_bounds,
                                    codebase,
                                );

                            // Nested closure returns normally lose to shallower bounds, but they
                            // must still widen parameters inferred for this closure's body.
                            for closure_bound in lower_bounds
                                .iter()
                                .filter(|bound| bound.arg_offset == Some(argument_offset))
                            {
                                let generalized_type = deep_generalize_template_bound(
                                    &closure_bound.bound_type,
                                    codebase,
                                );
                                resolved_type = add_optional_union_type(
                                    combine_union_types(
                                        &closure_bound.bound_type,
                                        &generalized_type,
                                        codebase,
                                        true,
                                    ),
                                    Some(&resolved_type),
                                    codebase,
                                );
                            }

                            (*map_key, Arc::new(resolved_type))
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
                return None;
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

        if (matches!(
            analysis_data.data_flow_graph.kind,
            GraphKind::WholeProgram(_)
        ) || !statements_analyzer.get_config().in_migration)
            && let FunctionLikeIdentifier::Function(
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
            && param_offset == 0
            && let Some(ref mut signature_type) = param_storage.signature_type
        {
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

    let params_changed = match analysis_data.closures.get(closure_expr.pos()) {
        Some(existing_storage) => {
            existing_storage.params.len() != closure_storage.params.len()
                || existing_storage
                    .params
                    .iter()
                    .zip(&closure_storage.params)
                    .any(|(existing_param, new_param)| {
                        existing_param.signature_type != new_param.signature_type
                    })
        }
        None => true,
    };

    analysis_data
        .closures
        .insert(closure_expr.pos().clone(), closure_storage);

    params_changed.then_some(replaced_type)
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
                calling_function: context
                    .function_context
                    .calling_functionlike_id
                    .as_ref()
                    .map(|m| *m),
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
            self_class: classlike_storage.map(|classlike_storage| classlike_storage.name),
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
        method_call_info
            .as_ref()
            .map(|method_call_info| method_call_info.self_fq_classlike_name),
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
/// Check for named argument issues:
/// - UnexpectedNamedArgument: when a named argument doesn't match any parameter
/// - MissingRequiredNamedArgument: when a required named parameter wasn't provided
fn check_named_arguments(
    statements_analyzer: &StatementsAnalyzer,
    args: &[aast::Argument<(), ()>],
    function_params: &[FunctionLikeParameter],
    _functionlike_id: &FunctionLikeIdentifier,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    function_call_pos: &Pos,
) {
    let interner = statements_analyzer.interner;

    // Build a map of parameter names to their info
    // Parameter names are stored with $ prefix, so we strip it for comparison
    let param_names: FxHashMap<String, (usize, &FunctionLikeParameter)> = function_params
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let name = interner.lookup(&param.name.0);
            // Strip the $ prefix from parameter name for comparison
            let clean_name = name.strip_prefix('$').unwrap_or(name).to_string();
            (clean_name, (i, param))
        })
        .collect();

    // Track which named parameters have been provided
    let mut provided_named_params: FxHashMap<String, bool> = FxHashMap::default();

    // Check each argument
    for arg in args {
        if let aast::Argument::Anamed(sid, _expr) = arg {
            let arg_name = &sid.1;

            // Check if this named argument matches a parameter
            if !param_names.contains_key(arg_name.as_str()) {
                // Unexpected named argument
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::UnexpectedNamedArgument,
                        format!("Unexpected named argument: {}", arg_name),
                        statements_analyzer.get_hpos(function_call_pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            } else {
                provided_named_params.insert(arg_name.clone(), true);
            }
        }
    }

    // Check for missing required named parameters
    for (param_name, (_idx, param)) in &param_names {
        if param.is_named
            && !param.is_optional
            && !param.is_variadic
            && !provided_named_params.contains_key(param_name.as_str())
        {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::MissingRequiredNamedArgument,
                    format!("Missing required named argument: {}", param_name),
                    statements_analyzer.get_hpos(function_call_pos),
                    &context.function_context.calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }
}
