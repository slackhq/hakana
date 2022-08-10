use crate::expr::fetch::array_fetch_analyzer::{
    handle_array_access_on_dict, handle_array_access_on_vec,
};
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::typed_ast::TastInfo;
use function_context::FunctionLikeIdentifier;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::{DataFlowNode, NodeKind};
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::functionlike_parameter::FunctionLikeParameter;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::taint::{string_to_taints, TaintType};
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
        hook.handle_argument(
            functionlike_id,
            config,
            context,
            &arg_value_type,
            tast_info,
            arg,
            &param_type,
            argument_offset,
            function_call_pos,
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
        .map(|(_, atomic_type)| match atomic_type {
            TAtomic::TDict { .. } => handle_array_access_on_dict(
                statements_analyzer,
                pos,
                tast_info,
                context,
                &atomic_type,
                &get_arraykey(),
                false,
                &mut has_valid_expected_offset,
                context.inside_isset,
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
            TAtomic::TMixedAny => {
                for (_, origin) in &arg_value_type.parent_nodes {
                    tast_info.data_flow_graph.add_mixed_data(origin, pos);
                }

                tast_info.maybe_add_issue(Issue::new(
                    IssueKind::MixedAnyArgument,
                    format!(
                        "Unpacking requires a collection type, {} provided",
                        atomic_type.get_id()
                    ),
                    statements_analyzer.get_hpos(&pos),
                ));

                get_mixed_any()
            }
            TAtomic::TNonnullMixed | TAtomic::TTruthyMixed | TAtomic::TMixed => {
                for (_, origin) in &arg_value_type.parent_nodes {
                    tast_info.data_flow_graph.add_mixed_data(origin, pos);
                }

                tast_info.maybe_add_issue(Issue::new(
                    IssueKind::MixedArgument,
                    format!(
                        "Unpacking requires a collection type, {} provided",
                        atomic_type.get_id()
                    ),
                    statements_analyzer.get_hpos(&pos),
                ));

                get_mixed()
            }
            TAtomic::TFalsyMixed | TAtomic::TNothing => get_nothing(),
            _ => {
                tast_info.maybe_add_issue(Issue::new(
                    IssueKind::InvalidArgument,
                    format!(
                        "Unpacking requires a collection type, {} provided",
                        arg_value_type.get_id()
                    ),
                    statements_analyzer.get_hpos(&pos),
                ));

                get_mixed()
            }
        })
        .collect::<Vec<_>>();

    let codebase = statements_analyzer.get_codebase();
    let mut result_type = inner_types.pop().unwrap();

    for inner_type in &inner_types {
        result_type = add_union_type(result_type, inner_type, Some(codebase), false);
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
        for (_, origin) in &input_type.parent_nodes {
            tast_info
                .data_flow_graph
                .add_mixed_data(origin, input_expr.pos());
        }

        tast_info.maybe_add_issue(Issue::new(
            if mixed_from_any {
                IssueKind::MixedAnyArgument
            } else {
                IssueKind::MixedArgument
            },
            format!(
                "Argument {} of {} expects {}, {} provided",
                (argument_offset + 1).to_string(),
                functionlike_id.to_string(),
                param_type.get_id(),
                input_type.get_id(),
            ),
            statements_analyzer.get_hpos(&input_expr.pos()),
        ));

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
        tast_info.maybe_add_issue(Issue::new(
            IssueKind::NoValue,
            format!(
                "Argument {} of {} expects {}, nothing type provided",
                (argument_offset + 1).to_string(),
                functionlike_id.to_string(),
                param_type.get_id(),
            ),
            statements_analyzer.get_hpos(&input_expr.pos()),
        ));

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
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::LessSpecificNestedAnyArgumentType,
                format!(
                    "Argument {} of {} expects {}, parent type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
        } else if union_comparison_result
            .type_coerced_from_nested_mixed
            .unwrap_or(false)
        {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::LessSpecificNestedArgumentType,
                format!(
                    "Argument {} of {} expects {}, parent type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
        } else {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::LessSpecificArgument,
                format!(
                    "Argument {} of {} expects {}, parent type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
        }
    }

    if !type_match_found && !union_comparison_result.type_coerced.unwrap_or(false) {
        let types_can_be_identical = union_type_comparator::can_expression_types_be_identical(
            codebase,
            &input_type,
            &param_type,
            true,
        );

        if types_can_be_identical {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::PossiblyInvalidArgument,
                format!(
                    "Argument {} of {} expects {}, possibly different type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
        } else {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::InvalidArgument,
                format!(
                    "Argument {} of {} expects {}, different type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
        }

        return true;
    }

    if !param_type.is_nullable()
        && functionlike_id != &FunctionLikeIdentifier::Function("echo".to_string())
        && functionlike_id != &FunctionLikeIdentifier::Function("print".to_string())
    {
        if input_type.is_null() && !param_type.is_null() {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::NullArgument,
                format!(
                    "Argument {} of {} expects {}, different type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));

            return true;
        }

        if input_type.is_nullable() {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::PossiblyNullArgument,
                format!(
                    "Argument {} of {} expects {}, different type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
        }
    }

    if !param_type.is_falsable()
        && !param_type.has_bool()
        && !param_type.has_scalar()
        && functionlike_id != &FunctionLikeIdentifier::Function("echo".to_string())
        && functionlike_id != &FunctionLikeIdentifier::Function("print".to_string())
    {
        if input_type.is_false() {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::PossiblyFalseArgument,
                format!(
                    "Argument {} of {} expects {}, different type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
            return true;
        }

        if input_type.is_falsable() && !input_type.ignore_falsable_issues {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::FalseArgument,
                format!(
                    "Argument {} of {} expects {}, different type {} provided",
                    (argument_offset + 1).to_string(),
                    functionlike_id.to_string(),
                    param_type.get_id(),
                    input_type.get_id(),
                ),
                statements_analyzer.get_hpos(&input_expr.pos()),
            ));
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

    if data_flow_graph.kind == GraphKind::Taint {
        if !input_type.has_taintable_value() || !param_type.has_taintable_value() {
            return;
        }

        if !context.allow_taints || ignore_taints {
            return;
        }

        for (_, at) in &param_type.types {
            if let Some(shape_name) = at.get_shape_name() {
                if let Some(t) = codebase.type_definitions.get(shape_name) {
                    if t.shape_field_taints.is_some() {
                        return;
                    }
                }
            }
        }
    }

    let mut method_node = DataFlowNode::get_for_method_argument(
        NodeKind::Default,
        functionlike_id.to_string(),
        argument_offset,
        if data_flow_graph.kind == GraphKind::Taint {
            function_param.location.clone()
        } else {
            None
        },
        if specialize_taint {
            Some(statements_analyzer.get_hpos(function_call_pos))
        } else {
            None
        },
    );

    if data_flow_graph.kind == GraphKind::Taint {
        if let FunctionLikeIdentifier::Method(_, method_name) = functionlike_id {
            if let Some(method_call_info) = method_call_info {
                if let Some(dependent_classlikes) = codebase
                    .classlike_descendents
                    .get(&method_call_info.classlike_storage.name)
                {
                    if method_name != "__construct" {
                        for dependent_classlike in dependent_classlikes {
                            let new_sink = DataFlowNode::get_for_method_argument(
                                NodeKind::Default,
                                dependent_classlike.clone() + "::" + method_name,
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
        }

        if let Some(MethodCallInfo {
            declaring_method_id: Some(declaring_method_id),
            ..
        }) = method_call_info
        {
            if let Some(method_id) = functionlike_id.as_method_identifier() {
                if declaring_method_id != &method_id {
                    let new_sink = DataFlowNode::get_for_method_argument(
                        NodeKind::Default,
                        declaring_method_id.to_string(),
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

    let argument_value_node = DataFlowNode::get_for_assignment(
        "call to ".to_string() + functionlike_id.to_string().as_str(),
        statements_analyzer.get_hpos(input_expr.pos()),
        None,
    );

    // maybe todo prevent numeric types from being tainted
    // ALTHOUGH numbers may still contain PII

    let removed_taints = if data_flow_graph.kind == GraphKind::Variable {
        FxHashSet::default()
    } else {
        let tags = statements_analyzer
            .get_comments()
            .iter()
            .filter(|c| {
                let diff = (input_expr.pos().line() as i64) - (c.0.line() as i64);
                diff == 0 || diff == 1
            })
            .collect::<Vec<_>>();

        let mut removed_taints = FxHashSet::default();

        for tag in tags {
            match &tag.1 {
                oxidized::prim_defs::Comment::CmtLine(_) => {}
                oxidized::prim_defs::Comment::CmtBlock(text) => {
                    let trimmed_text = text.trim();

                    if trimmed_text.starts_with("HAKANA_SECURITY_IGNORE[") {
                        let trimmed_text = trimmed_text[23..].to_string();

                        if let Some(bracket_pos) = trimmed_text.find("]") {
                            removed_taints
                                .extend(string_to_taints(trimmed_text[..bracket_pos].to_string()));
                        }
                    }
                }
            }
        }

        removed_taints
    };
    // TODO add plugin hooks for adding/removing taints

    for (_, parent_node) in &input_type.parent_nodes {
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

    if data_flow_graph.kind == GraphKind::Variable {
        data_flow_graph.add_sink(argument_value_node);
    } else {
        let mut taints = get_argument_taints(functionlike_id, argument_offset);

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
            method_node.taints = Some(taints.into_iter().collect());
            data_flow_graph.add_sink(method_node);
        } else {
            data_flow_graph.add_node(method_node);
        }
    }
}

fn get_argument_taints(function_id: &FunctionLikeIdentifier, arg_offset: usize) -> Vec<TaintType> {
    match function_id {
        FunctionLikeIdentifier::Function(id) => match id.as_str() {
            "echo" | "print" | "var_dump" => {
                return vec![
                    TaintType::HtmlTag,
                    TaintType::UserSecret,
                    TaintType::InternalSecret,
                ];
            }
            "exec" | "passthru" | "pcntl_exec" | "shell_exec" | "system" | "popen"
            | "proc_open" => {
                if arg_offset == 0 {
                    return vec![TaintType::Shell];
                }
            }
            "file_get_contents" | "file_put_contents" | "fopen" | "unlink" | "file" | "mkdir"
            | "parse_ini_file" | "chown" | "lchown" | "readfile" | "rmdir" | "symlink"
            | "tempnam" => {
                if arg_offset == 0 {
                    return vec![TaintType::FileSystem];
                }
            }
            "copy" | "link" | "move_uploaded_file" | "rename" => {
                if arg_offset == 0 || arg_offset == 1 {
                    return vec![TaintType::FileSystem];
                }
            }
            "header" => {
                if arg_offset == 0 {
                    // return vec![TaintType::ResponseHeader];
                }
            }
            "igbinary_unserialize" | "unserialize" => {
                if arg_offset == 0 {
                    return vec![TaintType::Unserialize];
                }
            }
            // "ldap" => {
            //     if arg_offset == 1 || arg_offset == 2 {
            //         return vec![TaintType::Ldap];
            //     }
            // }
            "setcookie" => {
                if arg_offset == 0 || arg_offset == 1 {
                    return vec![TaintType::Cookie];
                }
            }
            "curl_init" | "getimagesize" => {
                if arg_offset == 0 {
                    return vec![TaintType::CurlUri];
                }
            }
            "curl_setopt" => {
                if arg_offset == 2 {
                    return vec![TaintType::CurlHeader];
                }
            }
            _ => {}
        },
        FunctionLikeIdentifier::Method(fq_class, method_name) => {
            match (fq_class.as_str(), method_name.as_str()) {
                ("AsyncMysqlConnection", "query") => {
                    if arg_offset == 0 {
                        return vec![TaintType::Sql];
                    }
                }
                _ => {}
            }
        }
    }

    return vec![];
}
