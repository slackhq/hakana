use crate::custom_hook::AfterArgAnalysisData;
use crate::expr::fetch::array_fetch_analyzer::{
    add_array_fetch_dataflow, handle_array_access_on_dict, handle_array_access_on_vec,
};
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use hakana_code_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_code_info::data_flow::node::{DataFlowNode, DataFlowNodeId, DataFlowNodeKind};
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_parameter::FunctionLikeParameter;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::{TAtomic, TDict, TVec};
use hakana_code_info::t_union::TUnion;
use hakana_code_info::taint::{string_to_sink_types, SinkType};
use hakana_code_info::ttype::comparison::type_comparison_result::TypeComparisonResult;
use hakana_code_info::ttype::comparison::union_type_comparator;
use hakana_code_info::ttype::{
    add_union_type, get_arraykey, get_int, get_mixed, get_mixed_any, get_nothing,
};
use hakana_str::Interner;
use oxidized::aast;
use oxidized::pos::Pos;

use super::method_call_info::MethodCallInfo;

pub(crate) fn check_argument_matches(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_id: &FunctionLikeIdentifier,
    method_call_info: &Option<MethodCallInfo>,
    function_param: &FunctionLikeParameter,
    param_type: TUnion,
    argument_offset: usize,
    arg: &aast::Argument<(), ()>,
    arg_unpacked: bool,
    arg_value_type: TUnion,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    ignore_taints: bool,
    specialize_taint: bool,
    function_call_pos: &Pos,
    function_name_pos: Option<&Pos>,
) {
    let mut arg_value_type = arg_value_type;

    if arg_unpacked {
        arg_value_type = get_unpacked_type(
            statements_analyzer,
            arg_value_type,
            analysis_data,
            arg.to_expr_ref().pos(),
            context,
        );
    }

    let config = statements_analyzer.get_config();

    let newly_called = analysis_data.after_arg_hook_called.insert((
        arg.to_expr_ref().pos().start_offset() as u32,
        arg.to_expr_ref().pos().end_offset() as u32,
    ));

    for hook in &config.hooks {
        hook.after_argument_analysis(
            analysis_data,
            AfterArgAnalysisData {
                functionlike_id,
                statements_analyzer,
                context,
                arg_value_type: &arg_value_type,
                arg,
                param_type: &param_type,
                argument_offset,
                function_call_pos,
                function_name_pos,
                already_called: !newly_called,
            },
        );
    }

    self::verify_type(
        statements_analyzer,
        &arg_value_type,
        &param_type,
        functionlike_id,
        argument_offset,
        arg.to_expr_ref(),
        context,
        analysis_data,
        function_param,
        method_call_info,
        ignore_taints,
        specialize_taint,
        function_call_pos,
    );
}

fn get_unpacked_type(
    statements_analyzer: &StatementsAnalyzer,
    mut arg_value_type: TUnion,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    context: &mut BlockContext,
) -> TUnion {
    let mut has_valid_expected_offset = false;
    let inner_types = arg_value_type.types.drain(..).collect::<Vec<_>>();

    let mut inner_types = inner_types
        .into_iter()
        .map(|atomic_type| match atomic_type {
            TAtomic::TDict(TDict { .. }) => handle_array_access_on_dict(
                statements_analyzer,
                pos,
                analysis_data,
                context,
                &atomic_type,
                &get_arraykey(false),
                false,
                &mut has_valid_expected_offset,
                context.inside_isset,
                &mut false,
                &mut false,
            ),
            TAtomic::TVec(TVec { .. }) => handle_array_access_on_vec(
                statements_analyzer,
                pos,
                analysis_data,
                context,
                atomic_type,
                get_int(),
                false,
                &mut has_valid_expected_offset,
            ),
            TAtomic::TKeyset { type_param } => {
                has_valid_expected_offset = true;
                (*type_param).clone()
            }
            TAtomic::TMixedWithFlags(true, ..) => {
                for origin in &arg_value_type.parent_nodes {
                    analysis_data.data_flow_graph.add_mixed_data(origin, pos);
                }

                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedAnyArgument,
                        format!(
                            "Unpacking requires a collection type, {} provided",
                            atomic_type.get_id(Some(statements_analyzer.interner))
                        ),
                        statements_analyzer.get_hpos(pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                get_mixed_any()
            }
            TAtomic::TMixedWithFlags(_, true, _, _)
            | TAtomic::TMixedWithFlags(_, _, _, true)
            | TAtomic::TMixed => {
                for origin in &arg_value_type.parent_nodes {
                    analysis_data.data_flow_graph.add_mixed_data(origin, pos);
                }

                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedArgument,
                        format!(
                            "Unpacking requires a collection type, {} provided",
                            atomic_type.get_id(Some(statements_analyzer.interner))
                        ),
                        statements_analyzer.get_hpos(pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                get_mixed()
            }
            TAtomic::TMixedWithFlags(_, _, true, _) | TAtomic::TNothing => get_nothing(),
            _ => {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidArgument,
                        format!(
                            "Unpacking requires a collection type, {} provided",
                            arg_value_type.get_id(Some(statements_analyzer.interner))
                        ),
                        statements_analyzer.get_hpos(pos),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                get_mixed()
            }
        })
        .collect::<Vec<_>>();

    let codebase = statements_analyzer.codebase;
    let mut result_type = inner_types.pop().unwrap();

    for inner_type in &inner_types {
        result_type = add_union_type(result_type, inner_type, codebase, false);
    }

    add_array_fetch_dataflow(
        statements_analyzer,
        pos,
        analysis_data,
        None,
        &mut result_type,
        &mut get_arraykey(false),
    );

    result_type
}

pub(crate) fn verify_type(
    statements_analyzer: &StatementsAnalyzer,
    input_type: &TUnion,
    param_type: &TUnion,
    functionlike_id: &FunctionLikeIdentifier,
    argument_offset: usize,
    input_expr: &aast::Expr<(), ()>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    function_param: &FunctionLikeParameter,
    method_call_info: &Option<MethodCallInfo>,
    ignore_taints: bool,
    specialize_taint: bool,
    function_call_pos: &Pos,
) {
    let codebase = statements_analyzer.codebase;

    if input_type.is_nothing() {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::NoValue,
                format!(
                    "Argument {} of {} expects {}, nothing type provided",
                    (argument_offset + 1),
                    functionlike_id.to_string(statements_analyzer.interner),
                    param_type.get_id(Some(statements_analyzer.interner)),
                ),
                statements_analyzer.get_hpos(input_expr.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return;
    }

    let mut union_comparison_result = TypeComparisonResult::new();

    let type_match_found = union_type_comparator::is_contained_by(
        codebase,
        statements_analyzer.get_file_path(),
        input_type,
        param_type,
        false,
        input_type.ignore_falsable_issues,
        false,
        &mut union_comparison_result,
    );

    /*let mut replace_input_type = false;
    let mut input_type = input_type.clone();

    if let Some(replacement_type) = union_comparison_result.replacement_union_type {
        replace_input_type = true;
        input_type = replacement_type;
    }*/

    add_dataflow(
        statements_analyzer,
        functionlike_id,
        argument_offset,
        input_expr,
        input_type,
        param_type,
        context,
        analysis_data,
        function_param,
        method_call_info,
        ignore_taints,
        specialize_taint,
        function_call_pos,
    );

    /*if function_param.assert_untainted {
        replace_input_type = true;
        input_type.parent_nodes = FxHashMap::default();
    }*/

    if union_comparison_result.upcasted_awaitable && context.inside_general_use {
        analysis_data.maybe_add_issue(
            Issue::new(
                IssueKind::UpcastAwaitable,
                format!(
                    "{} contains Awaitable but was passed into a more general type {}",
                    input_type.get_id(Some(statements_analyzer.interner)),
                    param_type.get_id(Some(statements_analyzer.interner)),
                ),
                statements_analyzer.get_hpos(input_expr.pos()),
                &context.function_context.calling_functionlike_id,
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }

    if !type_match_found {
        if !param_type.is_mixed() {
            let mut mixed_from_any = false;

            if input_type.is_mixed_with_any(&mut mixed_from_any) {
                for origin in &input_type.parent_nodes {
                    analysis_data
                        .data_flow_graph
                        .add_mixed_data(origin, input_expr.pos());
                }

                analysis_data.maybe_add_issue(
                    Issue::new(
                        if mixed_from_any {
                            IssueKind::MixedAnyArgument
                        } else {
                            IssueKind::MixedArgument
                        },
                        format!(
                            "Argument {} of {} expects {}, {} provided",
                            (argument_offset + 1),
                            functionlike_id.to_string(statements_analyzer.interner),
                            param_type.get_id(Some(statements_analyzer.interner)),
                            input_type.get_id(Some(statements_analyzer.interner)),
                        ),
                        statements_analyzer.get_hpos(input_expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                return;
            }
        }

        if union_comparison_result.type_coerced.unwrap_or(false) && !input_type.is_mixed() {
            if union_comparison_result
                .type_coerced_from_nested_any
                .unwrap_or(false)
            {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::LessSpecificNestedAnyArgumentType,
                        format!(
                            "Argument {} of {} expects {}, parent type {} provided",
                            (argument_offset + 1),
                            functionlike_id.to_string(statements_analyzer.interner),
                            param_type.get_id(Some(statements_analyzer.interner)),
                            input_type.get_id(Some(statements_analyzer.interner)),
                        ),
                        statements_analyzer.get_hpos(input_expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            } else if union_comparison_result
                .type_coerced_from_nested_mixed
                .unwrap_or(false)
            {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::LessSpecificNestedArgumentType,
                        format!(
                            "Argument {} of {} expects {}, parent type {} provided",
                            (argument_offset + 1),
                            functionlike_id.to_string(statements_analyzer.interner),
                            param_type.get_id(Some(statements_analyzer.interner)),
                            input_type.get_id(Some(statements_analyzer.interner)),
                        ),
                        statements_analyzer.get_hpos(input_expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            } else {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::LessSpecificArgument,
                        format!(
                            "Argument {} of {} expects {}, parent type {} provided",
                            (argument_offset + 1),
                            functionlike_id.to_string(statements_analyzer.interner),
                            param_type.get_id(Some(statements_analyzer.interner)),
                            input_type.get_id(Some(statements_analyzer.interner)),
                        ),
                        statements_analyzer.get_hpos(input_expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }

        if !union_comparison_result.type_coerced.unwrap_or(false) {
            let types_can_be_identical = union_type_comparator::can_expression_types_be_identical(
                codebase,
                statements_analyzer.get_file_path(),
                input_type,
                param_type,
                false,
            );

            if types_can_be_identical {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::PossiblyInvalidArgument,
                        format!(
                            "Argument {} of {} expects {}, possibly different type {} provided{}",
                            (argument_offset + 1),
                            functionlike_id.to_string(statements_analyzer.interner),
                            param_type.get_id(Some(statements_analyzer.interner)),
                            input_type.get_id(Some(statements_analyzer.interner)),
                            if let Some(type_mismatch_parent_nodes) =
                                union_comparison_result.type_mismatch_parents
                            {
                                if !type_mismatch_parent_nodes.0.is_empty() {
                                    if let Some(pos) = type_mismatch_parent_nodes.0[0].get_pos() {
                                        format!(
                                            " in :{}:{} is a mismatch with {}",
                                            pos.start_line,
                                            pos.start_column,
                                            type_mismatch_parent_nodes
                                                .1
                                                .get_id(Some(statements_analyzer.interner))
                                        )
                                    } else {
                                        "".to_string()
                                    }
                                } else {
                                    "".to_string()
                                }
                            } else {
                                "".to_string()
                            }
                        ),
                        statements_analyzer.get_hpos(input_expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            } else {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidArgument,
                        format!(
                            "Argument {} of {} expects {}, different type {} provided",
                            (argument_offset + 1),
                            functionlike_id.to_string(statements_analyzer.interner),
                            param_type.get_id(Some(statements_analyzer.interner)),
                            input_type.get_id(Some(statements_analyzer.interner)),
                        ),
                        statements_analyzer.get_hpos(input_expr.pos()),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }

            return;
        }
    }

    for (name, mut bound) in union_comparison_result.type_variable_lower_bounds {
        if let Some((lower_bounds, _)) = analysis_data.type_variable_bounds.get_mut(&name) {
            bound.pos = Some(statements_analyzer.get_hpos(input_expr.pos()));
            lower_bounds.push(bound);
        }
    }

    for (name, mut bound) in union_comparison_result.type_variable_upper_bounds {
        if let Some((_, upper_bounds)) = analysis_data.type_variable_bounds.get_mut(&name) {
            bound.pos = Some(statements_analyzer.get_hpos(input_expr.pos()));
            upper_bounds.push(bound);
        }
    }
}

fn add_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_id: &FunctionLikeIdentifier,
    argument_offset: usize,
    input_expr: &aast::Expr<(), ()>,
    input_type: &TUnion,
    param_type: &TUnion,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    function_param: &FunctionLikeParameter,
    method_call_info: &Option<MethodCallInfo>,
    ignore_taints: bool,
    specialize_taint: bool,
    function_call_pos: &Pos,
) {
    let codebase = statements_analyzer.codebase;

    let data_flow_graph = &mut analysis_data.data_flow_graph;

    if let GraphKind::WholeProgram(WholeProgramKind::Taint) = &data_flow_graph.kind {
        if !input_type.has_taintable_value() || !param_type.has_taintable_value() {
            return;
        }

        if !context.allow_taints || ignore_taints {
            return;
        }

        for at in &param_type.types {
            if let Some(shape_name) = at.get_shape_name() {
                if let Some(t) = codebase.type_definitions.get(shape_name) {
                    if t.shape_field_taints.is_some() {
                        return;
                    }
                }
            }
        }
    }

    let taints = if matches!(data_flow_graph.kind, GraphKind::WholeProgram(_)) {
        let mut taints = get_argument_taints(
            functionlike_id,
            argument_offset,
            statements_analyzer.interner,
        );

        if let Some(sinks) = &function_param.taint_sinks {
            taints.extend(sinks.clone());
        }

        taints
    } else {
        vec![]
    };

    let function_call_hpos = statements_analyzer.get_hpos(function_call_pos);

    let method_node = {
        let arg_location = function_param.name_location;
        let mut is_specialized = false;

        let arg_id = DataFlowNodeId::FunctionLikeArg(*functionlike_id, argument_offset as u8);

        let mut id = arg_id.clone();

        if specialize_taint {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedFunctionLikeArg(
                *functionlike_id,
                argument_offset as u8,
                function_call_hpos.file_path,
                function_call_hpos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: if data_flow_graph.kind == GraphKind::FunctionBody && context.inside_general_use {
                DataFlowNodeKind::VariableUseSink {
                    pos: function_param.name_location,
                }
            } else if taints.is_empty() {
                DataFlowNodeKind::Vertex {
                    pos: Some(arg_location),
                    is_specialized,
                }
            } else {
                DataFlowNodeKind::TaintSink {
                    pos: if is_specialized {
                        statements_analyzer.get_hpos(input_expr.pos())
                    } else {
                        arg_location
                    },
                    types: taints,
                }
            },
        }
    };

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if let FunctionLikeIdentifier::Method(_, method_name) = functionlike_id {
            if let Some(method_call_info) = method_call_info {
                if let Some(dependent_classlikes) = codebase
                    .all_classlike_descendants
                    .get(&method_call_info.classlike_storage.name)
                {
                    if method_name != &statements_analyzer.interner.get("__construct").unwrap() {
                        for dependent_classlike in dependent_classlikes {
                            if codebase.declaring_method_exists(dependent_classlike, method_name) {
                                let new_sink = DataFlowNode::get_for_method_argument(
                                    &FunctionLikeIdentifier::Method(
                                        *dependent_classlike,
                                        *method_name,
                                    ),
                                    argument_offset,
                                    None,
                                    if specialize_taint {
                                        Some(function_call_hpos)
                                    } else {
                                        None
                                    },
                                );

                                data_flow_graph.add_node(new_sink.clone());

                                data_flow_graph.add_path(
                                    &method_node,
                                    &new_sink,
                                    PathKind::Default,
                                    vec![],
                                    vec![],
                                );
                            }
                        }
                    }
                }
            }
        }

        if let Some(MethodCallInfo {
            declaring_method_id: Some(declaring_method_id),
            ..
        }) = method_call_info
        {
            if let Some(method_id) = functionlike_id.as_method_identifier() {
                if declaring_method_id != &method_id {
                    let new_sink = DataFlowNode::get_for_method_argument(
                        &FunctionLikeIdentifier::Method(
                            declaring_method_id.0,
                            declaring_method_id.1,
                        ),
                        argument_offset,
                        Some(statements_analyzer.get_hpos(input_expr.pos())),
                        None,
                    );

                    data_flow_graph.add_node(new_sink.clone());

                    data_flow_graph.add_path(
                        &method_node,
                        &new_sink,
                        PathKind::Default,
                        vec![],
                        vec![],
                    );
                }
            }
        }
    }

    // maybe todo prevent numeric types from being tainted
    // ALTHOUGH numbers may still contain PII

    let removed_taints = if data_flow_graph.kind == GraphKind::FunctionBody {
        vec![]
    } else {
        get_removed_taints_in_comments(statements_analyzer, input_expr.pos())
    };
    // TODO add plugin hooks for adding/removing taints

    for parent_node in &input_type.parent_nodes {
        data_flow_graph.add_path(
            parent_node,
            &method_node,
            PathKind::Default,
            vec![],
            removed_taints.clone(),
        );
    }

    data_flow_graph.add_node(method_node);
}

pub(crate) fn get_removed_taints_in_comments(
    statements_analyzer: &StatementsAnalyzer,
    input_expr_pos: &Pos,
) -> Vec<SinkType> {
    let mut removed_taints = vec![];

    let tags = statements_analyzer
        .comments
        .iter()
        .filter(|c| {
            let diff = (input_expr_pos.line() as i64) - (c.0.line() as i64);
            diff == 0 || diff == 1
        })
        .collect::<Vec<_>>();

    for tag in tags {
        match &tag.1 {
            oxidized::prim_defs::Comment::CmtLine(_) => {}
            oxidized::prim_defs::Comment::CmtBlock(text) => {
                let trimmed_text = text.trim();

                if let Some(without_prefix) = trimmed_text.strip_prefix("HAKANA_SECURITY_IGNORE[") {
                    let trimmed_text = without_prefix.to_string();

                    if let Some(bracket_pos) = trimmed_text.find(']') {
                        let string_types = trimmed_text[..bracket_pos].split(',');

                        for string_type in string_types {
                            removed_taints
                                .extend(string_to_sink_types(string_type.trim().to_string()));
                        }
                    }
                }
            }
        }
    }

    removed_taints
}

fn get_argument_taints(
    function_id: &FunctionLikeIdentifier,
    arg_offset: usize,
    interner: &Interner,
) -> Vec<SinkType> {
    match function_id {
        FunctionLikeIdentifier::Function(id) => match interner.lookup(id) {
            "echo" | "print" | "var_dump" => {
                return vec![SinkType::HtmlTag, SinkType::Output];
            }
            "exec" | "passthru" | "pcntl_exec" | "shell_exec" | "system" | "popen"
            | "proc_open" => {
                if arg_offset == 0 {
                    return vec![SinkType::Shell];
                }
            }
            "file_get_contents" | "file_put_contents" | "fopen" | "unlink" | "file" | "mkdir"
            | "parse_ini_file" | "chown" | "lchown" | "readfile" | "rmdir" | "symlink"
            | "tempnam" => {
                if arg_offset == 0 {
                    return vec![SinkType::FileSystem];
                }
            }
            "copy" | "link" | "move_uploaded_file" | "rename" => {
                if arg_offset == 0 || arg_offset == 1 {
                    return vec![SinkType::FileSystem];
                }
            }
            "header" => {
                if arg_offset == 0 {
                    // return vec![TaintType::ResponseHeader];
                }
            }
            "igbinary_unserialize"
            | "unserialize"
            | "unserialize_pure"
            | "fb_unserialize"
            | "fb_compact_unserialize" => {
                if arg_offset == 0 {
                    return vec![SinkType::Unserialize];
                }
            }
            // "ldap" => {
            //     if arg_offset == 1 || arg_offset == 2 {
            //         return vec![TaintType::Ldap];
            //     }
            // }
            "setcookie" => {
                if arg_offset == 0 || arg_offset == 1 {
                    return vec![SinkType::Cookie];
                }
            }
            "curl_init" | "getimagesize" => {
                if arg_offset == 0 {
                    return vec![SinkType::CurlUri];
                }
            }
            "curl_setopt" => {
                if arg_offset == 2 {
                    return vec![SinkType::CurlHeader];
                }
            }
            _ => {}
        },
        FunctionLikeIdentifier::Method(fq_class, method_name) => {
            if let ("AsyncMysqlConnection", "query") =
                (interner.lookup(fq_class), interner.lookup(method_name))
            {
                if arg_offset == 0 {
                    return vec![SinkType::Sql];
                }
            }
        }
        _ => {}
    }

    vec![]
}
