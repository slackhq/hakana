use crate::custom_hook::AfterArgAnalysisData;
use crate::expr::fetch::array_fetch_analyzer::{
    handle_array_access_on_dict, handle_array_access_on_vec,
};
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use hakana_reflection_info::data_flow::graph::{GraphKind, WholeProgramKind};
use hakana_reflection_info::data_flow::node::DataFlowNode;
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::taint::{string_to_sink_types, SinkType};
use hakana_reflection_info::Interner;
use hakana_type::type_comparator::type_comparison_result::TypeComparisonResult;
use hakana_type::type_comparator::union_type_comparator;
use hakana_type::{add_union_type, get_arraykey, get_int, get_mixed, get_mixed_any, get_nothing};
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};
use rustc_hash::FxHashSet;

use super::method_call_info::MethodCallInfo;

pub(crate) fn check_argument_matches(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_id: &FunctionLikeIdentifier,
    method_call_info: &Option<MethodCallInfo>,
    function_param: &FunctionLikeParameter,
    param_type: TUnion,
    argument_offset: usize,
    arg: (&ast_defs::ParamKind, &aast::Expr<(), ()>),
    arg_unpacked: bool,
    arg_value_type: TUnion,
    context: &mut ScopeContext,
    tast_info: &mut TastInfo,
    ignore_taints: bool,
    specialize_taint: bool,
    function_call_pos: &Pos,
) -> bool {
    let mut arg_value_type = arg_value_type;

    if arg_unpacked {
        arg_value_type = get_unpacked_type(
            statements_analyzer,
            arg_value_type,
            tast_info,
            arg.1.pos(),
            context,
        );
    }

    let config = statements_analyzer.get_config();

    for hook in &config.hooks {
        hook.after_argument_analysis(
            tast_info,
            AfterArgAnalysisData {
                functionlike_id,
                statements_analyzer,
                context,
                arg_value_type: &arg_value_type,
                arg,
                param_type: &param_type,
                argument_offset,
                function_call_pos,
            },
        );
    }

    self::verify_type(
        statements_analyzer,
        &arg_value_type,
        &param_type,
        functionlike_id,
        argument_offset,
        arg.1,
        context,
        tast_info,
        function_param,
        method_call_info,
        ignore_taints,
        specialize_taint,
        function_call_pos,
    )
}

fn get_unpacked_type(
    statements_analyzer: &StatementsAnalyzer,
    arg_value_type: TUnion,
    tast_info: &mut TastInfo,
    pos: &Pos,
    context: &mut ScopeContext,
) -> TUnion {
    let mut has_valid_expected_offset = false;
    let mut inner_types = arg_value_type
        .clone()
        .types
        .into_iter()
        .map(|atomic_type| match atomic_type {
            TAtomic::TDict { .. } => handle_array_access_on_dict(
                statements_analyzer,
                pos,
                tast_info,
                context,
                &atomic_type,
                &get_arraykey(false),
                false,
                &mut has_valid_expected_offset,
                context.inside_isset,
                &mut false,
                &mut false,
            ),
            TAtomic::TVec { .. } => handle_array_access_on_vec(
                statements_analyzer,
                pos,
                tast_info,
                context,
                atomic_type,
                get_int(),
                false,
                &mut has_valid_expected_offset,
            ),
            TAtomic::TKeyset { type_param } => {
                has_valid_expected_offset = true;
                type_param
            }
            TAtomic::TMixedWithFlags(true, ..) => {
                for origin in &arg_value_type.parent_nodes {
                    tast_info.data_flow_graph.add_mixed_data(origin, pos);
                }

                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedAnyArgument,
                        format!(
                            "Unpacking requires a collection type, {} provided",
                            atomic_type.get_id(Some(&statements_analyzer.get_codebase().interner))
                        ),
                        statements_analyzer.get_hpos(&pos),
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
                    tast_info.data_flow_graph.add_mixed_data(origin, pos);
                }

                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::MixedArgument,
                        format!(
                            "Unpacking requires a collection type, {} provided",
                            atomic_type.get_id(Some(&statements_analyzer.get_codebase().interner))
                        ),
                        statements_analyzer.get_hpos(&pos),
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                get_mixed()
            }
            TAtomic::TMixedWithFlags(_, _, true, _) | TAtomic::TNothing => get_nothing(),
            _ => {
                tast_info.maybe_add_issue(
                    Issue::new(
                        IssueKind::InvalidArgument,
                        format!(
                            "Unpacking requires a collection type, {} provided",
                            arg_value_type
                                .get_id(Some(&statements_analyzer.get_codebase().interner))
                        ),
                        statements_analyzer.get_hpos(&pos),
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );

                get_mixed()
            }
        })
        .collect::<Vec<_>>();

    let codebase = statements_analyzer.get_codebase();
    let mut result_type = inner_types.pop().unwrap();

    for inner_type in &inner_types {
        result_type = add_union_type(result_type, inner_type, codebase, false);
    }

    result_type
}

pub(crate) fn verify_type(
    statements_analyzer: &StatementsAnalyzer,
    input_type: &TUnion,
    param_type: &TUnion,
    functionlike_id: &FunctionLikeIdentifier,
    argument_offset: usize,
    input_expr: &aast::Expr<(), ()>,
    context: &mut ScopeContext,
    tast_info: &mut TastInfo,
    function_param: &FunctionLikeParameter,
    method_call_info: &Option<MethodCallInfo>,
    ignore_taints: bool,
    specialize_taint: bool,
    function_call_pos: &Pos,
) -> bool {
    let codebase = statements_analyzer.get_codebase();

    if param_type.is_mixed() {
        if codebase.infer_types_from_usage && !input_type.is_mixed() && !param_type.had_template {
            if let Some(method_call_info) = method_call_info {
                if let Some(_declaring_method_id) = &method_call_info.declaring_method_id {
                    // todo log potential method param type
                }
            } else {
                // todo log potential function param type
            }
        }

        add_dataflow(
            statements_analyzer,
            functionlike_id,
            argument_offset,
            input_expr,
            input_type,
            param_type,
            context,
            tast_info,
            function_param,
            method_call_info,
            ignore_taints,
            specialize_taint,
            function_call_pos,
        );

        return true;
    }

    let mut mixed_from_any = false;
    if input_type.is_mixed_with_any(&mut mixed_from_any) {
        for origin in &input_type.parent_nodes {
            tast_info
                .data_flow_graph
                .add_mixed_data(origin, input_expr.pos());
        }

        tast_info.maybe_add_issue(
            Issue::new(
                if mixed_from_any {
                    IssueKind::MixedAnyArgument
                } else {
                    IssueKind::MixedArgument
                },
                format!(
                    "Argument {} of {} expects {}, {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(&codebase.interner),
                    param_type.get_id(Some(&codebase.interner)),
                    input_type.get_id(Some(&codebase.interner)),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        // todo handle mixed values, including coercing when passed into functions
        // that have hard type expectations

        add_dataflow(
            statements_analyzer,
            functionlike_id,
            argument_offset,
            input_expr,
            input_type,
            param_type,
            context,
            tast_info,
            function_param,
            method_call_info,
            ignore_taints,
            specialize_taint,
            function_call_pos,
        );

        return true;
    }

    if input_type.is_nothing() {
        tast_info.maybe_add_issue(
            Issue::new(
                IssueKind::NoValue,
                format!(
                    "Argument {} of {} expects {}, nothing type provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(&codebase.interner),
                    param_type.get_id(Some(&codebase.interner)),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ),
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );

        return true;
    }

    let mut union_comparison_result = TypeComparisonResult::new();

    let type_match_found = union_type_comparator::is_contained_by(
        codebase,
        input_type,
        param_type,
        true,
        true,
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
        &input_type,
        &param_type,
        context,
        tast_info,
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

    if union_comparison_result.type_coerced.unwrap_or(false) && !input_type.is_mixed() {
        if union_comparison_result
            .type_coerced_from_nested_any
            .unwrap_or(false)
        {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::LessSpecificNestedAnyArgumentType,
                    format!(
                        "Argument {} of {} expects {}, parent type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        } else if union_comparison_result
            .type_coerced_from_nested_mixed
            .unwrap_or(false)
        {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::LessSpecificNestedArgumentType,
                    format!(
                        "Argument {} of {} expects {}, parent type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        } else {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::LessSpecificArgument,
                    format!(
                        "Argument {} of {} expects {}, parent type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    if !type_match_found && !union_comparison_result.type_coerced.unwrap_or(false) {
        let types_can_be_identical = union_type_comparator::can_expression_types_be_identical(
            codebase,
            &input_type,
            &param_type,
            false,
        );

        if types_can_be_identical {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::PossiblyInvalidArgument,
                    format!(
                        "Argument {} of {} expects {}, possibly different type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        } else {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::InvalidArgument,
                    format!(
                        "Argument {} of {} expects {}, different type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }

        return true;
    }

    if !param_type.is_nullable()
        && (match functionlike_id {
            FunctionLikeIdentifier::Function(function_id) => {
                match statements_analyzer
                    .get_codebase()
                    .interner
                    .lookup(function_id)
                {
                    "echo" | "print" => false,
                    _ => true,
                }
            }
            FunctionLikeIdentifier::Method(_, _) => true,
        })
    {
        if input_type.is_null() && !param_type.is_null() {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::NullArgument,
                    format!(
                        "Argument {} of {} expects {}, different type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            return true;
        }

        if input_type.is_nullable() {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::PossiblyNullArgument,
                    format!(
                        "Argument {} of {} expects {}, different type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    if !param_type.is_falsable()
        && !param_type.has_bool()
        && !param_type.has_scalar()
        && (match functionlike_id {
            FunctionLikeIdentifier::Function(function_id) => {
                match statements_analyzer
                    .get_codebase()
                    .interner
                    .lookup(function_id)
                {
                    "echo" | "print" => false,
                    _ => true,
                }
            }
            FunctionLikeIdentifier::Method(_, _) => true,
        })
    {
        if input_type.is_false() {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::PossiblyFalseArgument,
                    format!(
                        "Argument {} of {} expects {}, different type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
            return true;
        }

        if input_type.is_falsable() && !input_type.ignore_falsable_issues {
            tast_info.maybe_add_issue(
                Issue::new(
                    IssueKind::FalseArgument,
                    format!(
                        "Argument {} of {} expects {}, different type {} provided",
                        (argument_offset + 1).to_string(),
                        functionlike_id.to_string(&codebase.interner),
                        param_type.get_id(Some(&codebase.interner)),
                        input_type.get_id(Some(&codebase.interner)),
                    ),
                    statements_analyzer.get_hpos(&input_expr.pos()),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    true
}

fn add_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_id: &FunctionLikeIdentifier,
    argument_offset: usize,
    input_expr: &aast::Expr<(), ()>,
    input_type: &TUnion,
    param_type: &TUnion,
    context: &ScopeContext,
    tast_info: &mut TastInfo,
    function_param: &FunctionLikeParameter,
    method_call_info: &Option<MethodCallInfo>,
    ignore_taints: bool,
    specialize_taint: bool,
    function_call_pos: &Pos,
) {
    let codebase = statements_analyzer.get_codebase();

    let ref mut data_flow_graph = tast_info.data_flow_graph;

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

    let method_node = DataFlowNode::get_for_method_argument(
        functionlike_id.to_string(&codebase.interner),
        argument_offset,
        if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
            Some(function_param.location.clone())
        } else {
            None
        },
        if specialize_taint {
            Some(statements_analyzer.get_hpos(function_call_pos))
        } else {
            None
        },
    );

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if let FunctionLikeIdentifier::Method(_, method_name) = functionlike_id {
            if let Some(method_call_info) = method_call_info {
                if let Some(dependent_classlikes) = codebase
                    .classlike_descendents
                    .get(&method_call_info.classlike_storage.name)
                {
                    if method_name != &codebase.interner.get("__construct").unwrap() {
                        for dependent_classlike in dependent_classlikes {
                            if codebase.declaring_method_exists(&dependent_classlike, &method_name)
                            {
                                let new_sink = DataFlowNode::get_for_method_argument(
                                    codebase.interner.lookup(dependent_classlike).to_string()
                                        + "::"
                                        + codebase.interner.lookup(method_name),
                                    argument_offset,
                                    None,
                                    if specialize_taint {
                                        Some(statements_analyzer.get_hpos(function_call_pos))
                                    } else {
                                        None
                                    },
                                );

                                data_flow_graph.add_node(new_sink.clone());

                                data_flow_graph.add_path(
                                    &method_node,
                                    &new_sink,
                                    PathKind::Default,
                                    None,
                                    None,
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
                        declaring_method_id.to_string(&codebase.interner),
                        argument_offset,
                        Some(statements_analyzer.get_hpos(input_expr.pos())),
                        None,
                    );

                    data_flow_graph.add_node(new_sink.clone());

                    data_flow_graph.add_path(
                        &method_node,
                        &new_sink,
                        PathKind::Default,
                        None,
                        None,
                    );
                }
            }
        }
    }

    // maybe todo prevent numeric types from being tainted
    // ALTHOUGH numbers may still contain PII

    let removed_taints = if data_flow_graph.kind == GraphKind::FunctionBody {
        FxHashSet::default()
    } else {
        get_removed_taints_in_comments(statements_analyzer, input_expr.pos())
    };
    // TODO add plugin hooks for adding/removing taints

    let argument_value_node = if data_flow_graph.kind == GraphKind::FunctionBody {
        DataFlowNode::VariableUseSink {
            id: "call to ".to_string() + functionlike_id.to_string(&codebase.interner).as_str(),
            pos: statements_analyzer.get_hpos(input_expr.pos()),
        }
    } else {
        DataFlowNode::get_for_assignment(
            "call to ".to_string() + functionlike_id.to_string(&codebase.interner).as_str(),
            statements_analyzer.get_hpos(input_expr.pos()),
        )
    };

    for parent_node in &input_type.parent_nodes {
        data_flow_graph.add_path(
            parent_node,
            &argument_value_node,
            PathKind::Default,
            None,
            if removed_taints.is_empty() {
                None
            } else {
                Some(removed_taints.clone())
            },
        );
    }

    if data_flow_graph.kind == GraphKind::FunctionBody {
        data_flow_graph.add_node(argument_value_node);
    } else {
        let mut taints = get_argument_taints(functionlike_id, argument_offset, &codebase.interner);

        if let Some(sinks) = &function_param.taint_sinks {
            taints.extend(sinks.clone());
        }

        data_flow_graph.add_node(argument_value_node.clone());

        data_flow_graph.add_path(
            &argument_value_node,
            &method_node,
            PathKind::Default,
            None,
            None,
        );

        if !taints.is_empty() {
            let method_node_sink = DataFlowNode::TaintSink {
                id: method_node.get_id().clone(),
                label: method_node.get_label().clone(),
                pos: method_node.get_pos().clone(),
                types: taints.into_iter().collect(),
            };
            data_flow_graph.add_node(method_node_sink);
        }

        data_flow_graph.add_node(method_node);
    }
}

pub(crate) fn get_removed_taints_in_comments(
    statements_analyzer: &StatementsAnalyzer,
    input_expr_pos: &Pos,
) -> FxHashSet<SinkType> {
    let mut removed_taints = FxHashSet::default();

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

                if trimmed_text.starts_with("HAKANA_SECURITY_IGNORE[") {
                    let trimmed_text = trimmed_text[23..].to_string();

                    if let Some(bracket_pos) = trimmed_text.find("]") {
                        let string_types = trimmed_text[..bracket_pos].split(",");

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
            match (interner.lookup(fq_class), interner.lookup(method_name)) {
                ("AsyncMysqlConnection", "query") => {
                    if arg_offset == 0 {
                        return vec![SinkType::Sql];
                    }
                }
                _ => {}
            }
        }
    }

    return vec![];
}
