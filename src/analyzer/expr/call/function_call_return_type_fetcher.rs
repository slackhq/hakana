use hakana_reflection_info::classlike_info::ClassConstantType;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::GraphKind;
use hakana_reflection_info::data_flow::node::{DataFlowNode, DataFlowNodeKind};
use hakana_reflection_info::data_flow::path::{ArrayDataKind, PathKind};
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::t_atomic::{DictKey, TAtomic};
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::taint::SinkType;
use hakana_reflection_info::{Interner, StrId, STR_TYPE_STRUCTURE};
use hakana_type::type_comparator::type_comparison_result::TypeComparisonResult;
use hakana_type::type_comparator::union_type_comparator;
use hakana_type::type_expander::TypeExpansionOptions;
use hakana_type::{
    add_union_type, get_arrayish_params, get_float, get_int, get_mixed, get_mixed_any,
    get_mixed_vec, get_nothing, get_null, get_object, get_string, get_vec, template, type_expander,
    wrap_atomic,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::expr::fetch::array_fetch_analyzer::handle_array_access_on_dict;
use crate::expr::variable_fetch_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;

use hakana_type::template::{TemplateBound, TemplateResult};
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

pub(crate) fn fetch(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        (&Pos, &ast_defs::Id_),
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    functionlike_id: &FunctionLikeIdentifier,
    function_storage: &FunctionLikeInfo,
    mut template_result: TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> TUnion {
    let codebase = statements_analyzer.get_codebase();
    let mut stmt_type = None;

    match functionlike_id {
        FunctionLikeIdentifier::Function(name) => {
            if let Some(t) = handle_special_functions(
                statements_analyzer,
                statements_analyzer.get_interner().lookup(name),
                expr.2,
                pos,
                codebase,
                analysis_data,
                context,
            ) {
                stmt_type = Some(t);
            }
        }
        _ => {}
    }

    // todo support custom return type providers for functions

    let stmt_type = if let Some(stmt_type) = stmt_type {
        stmt_type
    } else {
        if let Some(function_return_type) = &function_storage.return_type {
            if !function_storage.template_types.is_empty() {
                if !function_storage.template_types.is_empty() {
                    let fn_id = statements_analyzer
                        .get_interner()
                        .get(
                            format!(
                                "fn-{}",
                                match functionlike_id {
                                    FunctionLikeIdentifier::Function(function_id) =>
                                        function_id.0.to_string(),
                                    FunctionLikeIdentifier::Method(_, _) => panic!(),
                                }
                            )
                            .as_str(),
                        )
                        .unwrap();
                    for (template_name, _) in &function_storage.template_types {
                        if let None = template_result.lower_bounds.get(template_name) {
                            template_result.lower_bounds.insert(
                                template_name.clone(),
                                FxHashMap::from_iter([(
                                    fn_id.clone(),
                                    vec![TemplateBound::new(get_nothing(), 1, None, None)],
                                )]),
                            );
                        }
                    }
                }
            }

            let mut function_return_type = function_return_type.clone();

            if !template_result.lower_bounds.is_empty()
                && !function_storage.template_types.is_empty()
            {
                type_expander::expand_union(
                    codebase,
                    &Some(statements_analyzer.get_interner()),
                    &mut function_return_type,
                    &TypeExpansionOptions {
                        expand_templates: false,
                        ..Default::default()
                    },
                    &mut analysis_data.data_flow_graph,
                );

                function_return_type = template::inferred_type_replacer::replace(
                    &function_return_type,
                    &template_result,
                    codebase,
                );
            }

            type_expander::expand_union(
                codebase,
                &Some(statements_analyzer.get_interner()),
                &mut function_return_type,
                &TypeExpansionOptions {
                    expand_templates: false,
                    expand_generic: true,
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

            // todo dispatch AfterFunctionCallAnalysisEvent

            function_return_type
        } else {
            get_mixed_any()
        }
    };

    return add_dataflow(
        statements_analyzer,
        expr,
        pos,
        functionlike_id,
        function_storage,
        stmt_type,
        &template_result,
        analysis_data,
        context,
    );
}

fn handle_special_functions(
    statements_analyzer: &StatementsAnalyzer,
    name: &str,
    args: &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
    pos: &Pos,
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> Option<TUnion> {
    match name {
        "HH\\type_structure" => {
            if let (Some((_, first_arg_expr)), Some((_, second_arg_expr))) =
                (args.get(0), args.get(1))
            {
                if let (Some(first_expr_type), Some(second_expr_type)) = (
                    analysis_data.get_expr_type(first_arg_expr.pos()),
                    analysis_data.get_expr_type(second_arg_expr.pos()),
                ) {
                    get_type_structure_type(
                        statements_analyzer,
                        first_expr_type,
                        second_expr_type,
                        context.function_context.calling_class,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        }
        "HH\\global_get" => {
            if let Some((_, arg_expr)) = args.get(0) {
                if let Some(expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    if let Some(value) = expr_type.get_single_literal_string_value() {
                        Some(variable_fetch_analyzer::get_type_for_superglobal(
                            statements_analyzer,
                            value,
                            pos,
                            analysis_data,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        "preg_split" => {
            if let Some((_, arg_expr)) = args.get(3) {
                if let Some(expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    return if let Some(value) = expr_type.get_single_literal_int_value() {
                        match value {
                            0 | 2 => {
                                let mut false_or_string_vec = TUnion::new(vec![
                                    TAtomic::TVec {
                                        known_items: None,
                                        type_param: Box::new(get_string()),
                                        known_count: None,
                                        non_empty: true,
                                    },
                                    TAtomic::TFalse,
                                ]);
                                false_or_string_vec.ignore_falsable_issues = true;
                                Some(false_or_string_vec)
                            }
                            1 | 3 => {
                                let mut false_or_string_vec = TUnion::new(vec![
                                    TAtomic::TVec {
                                        known_items: None,
                                        type_param: Box::new(get_string()),
                                        known_count: None,
                                        non_empty: false,
                                    },
                                    TAtomic::TFalse,
                                ]);
                                false_or_string_vec.ignore_falsable_issues = true;
                                Some(false_or_string_vec)
                            }
                            _ => {
                                let mut false_or_string_vec = TUnion::new(vec![
                                    TAtomic::TVec {
                                        known_items: None,
                                        type_param: Box::new(wrap_atomic(TAtomic::TVec {
                                            known_items: Some(BTreeMap::from([
                                                (0, (false, get_string())),
                                                (1, (false, get_int())),
                                            ])),
                                            type_param: Box::new(get_nothing()),
                                            known_count: None,
                                            non_empty: true,
                                        })),
                                        known_count: None,
                                        non_empty: false,
                                    },
                                    TAtomic::TFalse,
                                ]);
                                false_or_string_vec.ignore_falsable_issues = true;
                                Some(false_or_string_vec)
                            }
                        }
                    } else {
                        let mut false_or_string_vec = TUnion::new(vec![
                            TAtomic::TVec {
                                known_items: None,
                                type_param: Box::new(get_mixed()),
                                known_count: None,
                                non_empty: true,
                            },
                            TAtomic::TFalse,
                        ]);
                        false_or_string_vec.ignore_falsable_issues = true;
                        Some(false_or_string_vec)
                    };
                }
            } else {
                let mut false_or_string_vec = TUnion::new(vec![
                    TAtomic::TVec {
                        known_items: None,
                        type_param: Box::new(get_string()),
                        known_count: None,
                        non_empty: true,
                    },
                    TAtomic::TFalse,
                ]);
                false_or_string_vec.ignore_falsable_issues = true;
                return Some(false_or_string_vec);
            }

            None
        }
        "debug_backtrace" => Some(wrap_atomic(TAtomic::TVec {
            known_items: None,
            type_param: Box::new(wrap_atomic(TAtomic::TDict {
                known_items: Some(BTreeMap::from([
                    (
                        DictKey::String("file".to_string()),
                        (false, Arc::new(get_string())),
                    ),
                    (
                        DictKey::String("line".to_string()),
                        (false, Arc::new(get_int())),
                    ),
                    (
                        DictKey::String("function".to_string()),
                        (false, Arc::new(get_string())),
                    ),
                    (
                        DictKey::String("class".to_string()),
                        (true, Arc::new(get_string())),
                    ),
                    (
                        DictKey::String("object".to_string()),
                        (true, Arc::new(get_object())),
                    ),
                    (
                        DictKey::String("type".to_string()),
                        (true, Arc::new(get_string())),
                    ),
                    (
                        DictKey::String("args".to_string()),
                        (true, Arc::new(get_mixed_vec())),
                    ),
                ])),
                params: None,
                non_empty: true,
                shape_name: None,
            })),
            known_count: None,
            non_empty: true,
        })),
        "str_replace" => {
            // returns string if the second arg is a string
            if let Some((_, arg_expr)) = args.get(1) {
                if let Some(expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    if union_type_comparator::is_contained_by(
                        codebase,
                        expr_type,
                        &get_string(),
                        false,
                        expr_type.ignore_falsable_issues,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) {
                        Some(get_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        "preg_replace" => {
            // returns string if the third arg is a string
            if let Some((_, arg_expr)) = args.get(2) {
                if let Some(expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    if union_type_comparator::is_contained_by(
                        codebase,
                        expr_type,
                        &get_string(),
                        false,
                        expr_type.ignore_falsable_issues,
                        false,
                        &mut TypeComparisonResult::new(),
                    ) {
                        let null_or_string = TUnion::new(vec![TAtomic::TString, TAtomic::TNull]);
                        Some(null_or_string)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        "microtime" => {
            if let Some((_, arg_expr)) = args.get(0) {
                if let Some(expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    if expr_type.is_always_truthy() {
                        Some(get_float())
                    } else if expr_type.is_always_falsy() {
                        Some(get_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        "HH\\Lib\\Str\\join" => {
            if let (Some((_, first_arg_expr)), Some((_, second_arg_expr))) =
                (args.get(0), args.get(1))
            {
                if let (Some(first_expr_type), Some(second_expr_type)) = (
                    analysis_data.get_expr_type(first_arg_expr.pos()),
                    analysis_data.get_expr_type(second_arg_expr.pos()),
                ) {
                    if second_expr_type.all_literals() && first_expr_type.is_single() {
                        let first_expr_type = first_expr_type.get_single();
                        let first_arg_params = get_arrayish_params(first_expr_type, codebase);

                        if let Some(first_arg_params) = first_arg_params {
                            if first_arg_params.1.all_literals() {
                                Some(wrap_atomic(TAtomic::TStringWithFlags(true, false, true)))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        "HH\\Lib\\Str\\format"
        | "HH\\Lib\\Str\\trim"
        | "HH\\Lib\\Str\\strip_suffix"
        | "HH\\Lib\\Str\\slice"
        | "HH\\Lib\\Str\\replace" => {
            let mut all_literals = true;
            for (_, arg_expr) in args {
                if let Some(arg_expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    if !arg_expr_type.all_literals() {
                        all_literals = false;
                        break;
                    }
                } else {
                    all_literals = false;
                    break;
                }
            }

            Some(wrap_atomic(if all_literals {
                TAtomic::TStringWithFlags(false, false, true)
            } else {
                TAtomic::TString
            }))
        }
        "HH\\Lib\\Str\\split" => {
            let mut all_literals = true;
            for (_, arg_expr) in args {
                if let Some(arg_expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    if !arg_expr_type.all_literals() {
                        all_literals = false;
                        break;
                    }
                } else {
                    all_literals = false;
                    break;
                }
            }

            Some(get_vec(wrap_atomic(if all_literals {
                TAtomic::TStringWithFlags(false, false, true)
            } else {
                TAtomic::TString
            })))
        }
        "range" => {
            let mut all_ints = true;
            for (_, arg_expr) in args {
                if let Some(arg_expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    if !arg_expr_type.is_int() {
                        all_ints = false;
                        break;
                    }
                } else {
                    all_ints = false;
                    break;
                }
            }

            if all_ints {
                Some(get_vec(get_int()))
            } else {
                None
            }
        }
        "HH\\idx" => {
            if args.len() >= 2 {
                let dict_type = analysis_data.get_rc_expr_type(args[0].1.pos()).cloned();
                let dim_type = analysis_data.get_rc_expr_type(args[1].1.pos()).cloned();

                let mut expr_type = None;

                if let (Some(dict_type), Some(dim_type)) = (dict_type, dim_type) {
                    for atomic_type in &dict_type.types {
                        if let TAtomic::TDict { .. } = atomic_type {
                            let mut expr_type_inner = handle_array_access_on_dict(
                                statements_analyzer,
                                pos,
                                analysis_data,
                                context,
                                atomic_type,
                                &*dim_type,
                                false,
                                &mut false,
                                true,
                                &mut false,
                                &mut false,
                            );

                            if args.len() == 2 && !expr_type_inner.is_mixed() {
                                expr_type_inner =
                                    add_union_type(expr_type_inner, &get_null(), codebase, false);
                            }

                            expr_type = Some(expr_type_inner);
                        }
                    }

                    if args.len() > 2 {
                        let default_type = analysis_data.get_expr_type(args[2].1.pos());
                        expr_type = if let Some(expr_type) = expr_type {
                            Some(if let Some(default_type) = default_type {
                                add_union_type(expr_type, default_type, codebase, false)
                            } else {
                                add_union_type(expr_type, &get_mixed_any(), codebase, false)
                            })
                        } else {
                            None
                        };
                    }
                }

                Some(expr_type.unwrap_or(get_mixed_any()))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_type_structure_type(
    statements_analyzer: &StatementsAnalyzer,
    first_expr_type: &TUnion,
    second_expr_type: &TUnion,
    this_class: Option<StrId>,
) -> Option<TUnion> {
    if let Some(second_arg_string) = second_expr_type.get_single_literal_string_value() {
        let const_name =
            if let Some(const_name) = statements_analyzer.get_interner().get(&second_arg_string) {
                const_name
            } else {
                return None;
            };

        if first_expr_type.is_single() {
            let classname = match first_expr_type.get_single() {
                TAtomic::TLiteralClassname { name } => *name,
                TAtomic::TClassname { as_type } => match &**as_type {
                    TAtomic::TNamedObject { name, is_this, .. } => {
                        if *is_this {
                            if let Some(this_class) = this_class {
                                this_class
                            } else {
                                *name
                            }
                        } else {
                            *name
                        }
                    }
                    _ => {
                        return None;
                    }
                },
                TAtomic::TNamedObject { name, is_this, .. } => {
                    if *is_this {
                        if let Some(this_class) = this_class {
                            this_class
                        } else {
                            *name
                        }
                    } else {
                        *name
                    }
                }
                _ => {
                    return None;
                }
            };

            if let Some(classlike_info) = statements_analyzer
                .get_codebase()
                .classlike_infos
                .get(&classname)
            {
                if let Some(type_constant_info) = classlike_info.type_constants.get(&const_name) {
                    return Some(wrap_atomic(TAtomic::TTypeAlias {
                        name: STR_TYPE_STRUCTURE,
                        type_params: Some(vec![match type_constant_info {
                            ClassConstantType::Concrete(actual_type) => actual_type.clone(),
                            ClassConstantType::Abstract(Some(as_type)) => as_type.clone(),
                            _ => get_mixed_any(),
                        }]),
                        as_type: None,
                    }));
                }
            }
        }
    }

    None
}

fn add_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        (&Pos, &ast_defs::Id_),
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    functionlike_id: &FunctionLikeIdentifier,
    functionlike_storage: &FunctionLikeInfo,
    stmt_type: TUnion,
    _template_result: &TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
) -> TUnion {
    // todo dispatch AddRemoveTaintsEvent

    //let added_taints = Vec::new();
    //let removed_taints = Vec::new();

    let ref mut data_flow_graph = analysis_data.data_flow_graph;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if !context.allow_taints {
            return stmt_type;
        }
    }

    let mut stmt_type = stmt_type;

    // todo conditionally remove taints

    let function_call_node;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        function_call_node = DataFlowNode::get_for_method_return(
            functionlike_id.to_string(&statements_analyzer.get_interner()),
            if let Some(return_pos) = &functionlike_storage.return_type_location {
                Some(return_pos.clone())
            } else {
                functionlike_storage.name_location.clone()
            },
            if functionlike_storage.specialize_call {
                Some(statements_analyzer.get_hpos(pos))
            } else {
                None
            },
        );

        if !functionlike_storage.return_source_params.is_empty() {
            // todo dispatch AddRemoveTaintEvent
            // and also handle simple preg_replace calls
        }

        let (param_offsets, _variadic_path) =
            get_special_argument_nodes(functionlike_id, &statements_analyzer.get_interner());
        let added_removed_taints =
            get_special_added_removed_taints(functionlike_id, &statements_analyzer.get_interner());

        for (param_offset, path_kind) in param_offsets {
            let argument_node = DataFlowNode::get_for_method_argument(
                statements_analyzer
                    .get_interner()
                    .lookup(&functionlike_storage.name)
                    .to_string(),
                param_offset,
                if let Some(arg) = expr.2.get(param_offset) {
                    Some(statements_analyzer.get_hpos(arg.1.pos()))
                } else {
                    None
                },
                if functionlike_storage.specialize_call {
                    Some(statements_analyzer.get_hpos(pos))
                } else {
                    None
                },
            );

            let (added_taints, removed_taints) =
                if let Some(added_removed_taints) = added_removed_taints.get(&param_offset) {
                    added_removed_taints.clone()
                } else {
                    (FxHashSet::default(), FxHashSet::default())
                };

            data_flow_graph.add_path(
                &argument_node,
                &function_call_node,
                path_kind,
                if added_taints.is_empty() {
                    None
                } else {
                    Some(added_taints)
                },
                if removed_taints.is_empty() {
                    None
                } else {
                    Some(removed_taints)
                },
            );
            data_flow_graph.add_node(argument_node);
        }

        if !functionlike_storage.taint_source_types.is_empty() {
            let function_call_node_source = DataFlowNode {
                id: function_call_node.get_id().clone(),
                kind: DataFlowNodeKind::TaintSource {
                    pos: function_call_node.get_pos().clone(),
                    label: function_call_node.get_label().clone(),
                    types: functionlike_storage.taint_source_types.clone(),
                },
            };
            data_flow_graph.add_node(function_call_node_source);
        }

        data_flow_graph.add_node(function_call_node.clone());
    } else {
        function_call_node = DataFlowNode::get_for_method_return(
            functionlike_id.to_string(&statements_analyzer.get_interner()),
            Some(statements_analyzer.get_hpos(pos)),
            Some(statements_analyzer.get_hpos(pos)),
        );

        data_flow_graph.add_node(function_call_node.clone());
    }

    stmt_type.parent_nodes.insert(function_call_node);

    stmt_type
}

/*
Returns a list of paths with (input_argument_position, path to return output).
The optional path is for functions with ... params.
*/
fn get_special_argument_nodes(
    functionlike_id: &FunctionLikeIdentifier,
    interner: &Interner,
) -> (Vec<(usize, PathKind)>, Option<PathKind>) {
    match functionlike_id {
        FunctionLikeIdentifier::Function(function_name) => match interner.lookup(function_name) {
            "var_export"
            | "print_r"
            | "highlight_string"
            | "strtolower"
            | "HH\\Lib\\Str\\lowercase"
            | "strtoupper"
            | "HH\\Lib\\Str\\uppercase"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "HH\\Lib\\Str\\trim"
            | "HH\\Lib\\Str\\trim_left"
            | "HH\\Lib\\Str\\trim_right"
            | "HH\\Asio\\join"
            | "strip_tags"
            | "stripslashes"
            | "stripcslashes"
            | "htmlentities"
            | "htmlentitydecode"
            | "htmlspecialchars"
            | "htmlspecialchars_decode"
            | "str_repeat"
            | "str_rot13"
            | "str_shuffle"
            | "strstr"
            | "stristr"
            | "strchr"
            | "strpbrk"
            | "strrchr"
            | "strrev"
            | "substr"
            | "preg_quote"
            | "wordwrap"
            | "realpath"
            | "strval"
            | "strgetcsv"
            | "addcslashes"
            | "addslashes"
            | "ucfirst"
            | "ucwords"
            | "lcfirst"
            | "nl2br"
            | "quoted_printable_decode"
            | "quoted_printable_encode"
            | "quote_meta"
            | "chop"
            | "convert_uudecode"
            | "convert_uuencode"
            | "json_decode"
            | "base64_encode"
            | "base64_decode"
            | "HH\\Lib\\Dict\\filter"
            | "HH\\Lib\\Dict\\filter_async"
            | "HH\\Lib\\Dict\\filter_keys"
            | "HH\\Lib\\Dict\\filter_nulls"
            | "HH\\Lib\\Dict\\filter_with_key"
            | "HH\\Lib\\Vec\\filter"
            | "HH\\Lib\\Vec\\filter_async"
            | "HH\\Lib\\Vec\\filter_nulls"
            | "HH\\Lib\\Vec\\filter_with_key"
            | "HH\\Lib\\Keyset\\filter"
            | "HH\\Lib\\Keyset\\filter_async"
            | "HH\\Lib\\Vec\\slice"
            | "HH\\Lib\\Str\\slice" => (vec![(0, PathKind::Default)], None),
            "json_encode" | "serialize" => (vec![(0, PathKind::Serialize)], None),
            "var_dump" | "printf" => (vec![(0, PathKind::Serialize)], Some(PathKind::Serialize)),
            "sscanf" | "substr_replace" => {
                (vec![(0, PathKind::Default), (1, PathKind::Default)], None)
            }
            "str_replace" | "str_ireplace" | "preg_filter" | "preg_replace" => {
                (vec![(1, PathKind::Default), (2, PathKind::Default)], None)
            }
            "HH\\Lib\\Str\\replace" | "HH\\Lib\\Str\\replace_ci" => {
                (vec![(0, PathKind::Default), (2, PathKind::Default)], None)
            }
            "str_pad" | "chunk_split" => {
                (vec![(0, PathKind::Default), (2, PathKind::Default)], None)
            }
            "implode" | "join" => (
                vec![
                    (0, PathKind::Default),
                    (1, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)),
                ],
                None,
            ),
            "explode" | "preg_split" => (
                vec![(
                    1,
                    PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                )],
                None,
            ),
            "pathinfo" => (
                vec![
                    (
                        0,
                        PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, "dirname".to_string()),
                    ),
                    (
                        0,
                        PathKind::ArrayAssignment(
                            ArrayDataKind::ArrayValue,
                            "basename".to_string(),
                        ),
                    ),
                    (
                        0,
                        PathKind::ArrayAssignment(
                            ArrayDataKind::ArrayValue,
                            "extension".to_string(),
                        ),
                    ),
                    (
                        0,
                        PathKind::ArrayAssignment(
                            ArrayDataKind::ArrayValue,
                            "filename".to_string(),
                        ),
                    ),
                ],
                None,
            ),
            "str_split"
            | "HH\\Lib\\Str\\split"
            | "HH\\Lib\\Str\\chunk"
            | "HH\\Lib\\Regex\\every_match" => (
                vec![(
                    0,
                    PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                )],
                None,
            ),
            "HH\\Lib\\Vec\\sort" => (vec![(0, PathKind::Default)], None),
            "HH\\Lib\\Str\\join" => (
                vec![
                    (0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)),
                    (1, PathKind::Default),
                ],
                None,
            ),
            "HH\\Lib\\Vec\\map"
            | "HH\\Lib\\Dict\\map"
            | "HH\\Lib\\Keyset\\map"
            | "HH\\Lib\\Vec\\map_async"
            | "HH\\Lib\\Dict\\map_async"
            | "HH\\Lib\\Keyset\\map_async"
            | "HH\\Lib\\Vec\\map_with_key"
            | "HH\\Lib\\Dict\\map_with_key"
            | "HH\\Lib\\Keyset\\map_with_key"
            | "HH\\Lib\\Dict\\map_with_key_async" => (
                vec![(
                    1,
                    PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                )],
                None,
            ),
            "HH\\Lib\\C\\first" | "HH\\Lib\\C\\firstx" | "HH\\Lib\\C\\last"
            | "HH\\Lib\\C\\lastx" | "HH\\Lib\\C\\onlyx" | "HH\\Lib\\C\\find"
            | "HH\\Lib\\C\\findx" => (
                vec![(0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue))],
                None,
            ),
            "HH\\Lib\\C\\first_key"
            | "HH\\Lib\\C\\first_keyx"
            | "HH\\Lib\\C\\last_key"
            | "HH\\Lib\\C\\last_keyx"
            | "HH\\Lib\\C\\find_key" => (
                vec![(0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayKey))],
                None,
            ),
            "HH\\Lib\\Dict\\merge" | "HH\\Lib\\Vec\\concat" | "HH\\Lib\\Keyset\\union" => {
                (vec![(0, PathKind::Default)], Some(PathKind::Default))
            }
            "HH\\Lib\\C\\contains"
            | "HH\\Lib\\C\\contains_key"
            | "HH\\Lib\\Str\\is_empty"
            | "HH\\Lib\\Str\\contains"
            | "HH\\Lib\\Str\\contains_ci"
            | "HH\\Lib\\Str\\compare"
            | "HH\\Lib\\Str\\compare_ci"
            | "HH\\Lib\\Str\\starts_with"
            | "HH\\Lib\\Str\\starts_with_ci"
            | "HH\\Lib\\Str\\ends_with"
            | "HH\\Lib\\Str\\ends_with_ci"
            | "HH\\Lib\\C\\is_empty"
            | "HH\\Lib\\Str\\length"
            | "HH\\Lib\\C\\count"
            | "HH\\Lib\\C\\any"
            | "HH\\Lib\\C\\every"
            | "HH\\Lib\\C\\search" => (vec![], None),
            _ => {
                // if function_name.starts_with("HH\\Lib\\")
                //     && !function_name.starts_with("HH\\Lib\\Math\\")
                // {
                //     println!("no taints through {}", function_name);
                // }
                (vec![], None)
            }
        },
        FunctionLikeIdentifier::Method(_, _) => panic!(),
    }
}

fn get_special_added_removed_taints(
    functionlike_id: &FunctionLikeIdentifier,
    interner: &Interner,
) -> FxHashMap<usize, (FxHashSet<SinkType>, FxHashSet<SinkType>)> {
    match functionlike_id {
        FunctionLikeIdentifier::Function(function_name) => match interner.lookup(function_name) {
            "html_entity_decode" | "htmlspecialchars_decode" => FxHashMap::from_iter([(
                0,
                (
                    FxHashSet::from_iter([SinkType::HtmlTag]),
                    FxHashSet::default(),
                ),
            )]),
            "htmlentities" | "htmlspecialchars" | "strip_tags" => FxHashMap::from_iter([(
                0,
                (
                    FxHashSet::default(),
                    FxHashSet::from_iter([SinkType::HtmlTag]),
                ),
            )]),
            _ => FxHashMap::default(),
        },
        FunctionLikeIdentifier::Method(_, _) => panic!(),
    }
}
