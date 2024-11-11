use bstr::BString;
use hakana_code_info::classlike_info::ClassConstantType;
use hakana_code_info::code_location::HPos;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::data_flow::graph::{DataFlowGraph, GraphKind};
use hakana_code_info::data_flow::node::DataFlowNodeId;
use hakana_code_info::data_flow::node::{DataFlowNode, DataFlowNodeKind};
use hakana_code_info::data_flow::path::{ArrayDataKind, PathKind};
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::t_atomic::{DictKey, TAtomic, TDict};
use hakana_code_info::t_union::TUnion;
use hakana_code_info::taint::{SinkType, SourceType};
use hakana_code_info::ttype::comparison::type_comparison_result::TypeComparisonResult;
use hakana_code_info::ttype::comparison::union_type_comparator;
use hakana_code_info::ttype::type_expander::TypeExpansionOptions;
use hakana_code_info::ttype::{
    add_union_type, extend_dataflow_uniquely, get_arrayish_params, get_float, get_int,
    get_literal_string, get_mixed, get_mixed_any, get_mixed_vec, get_nothing, get_null, get_object,
    get_string, get_vec, template, type_expander, wrap_atomic,
};
use hakana_code_info::{GenericParent, VarId, EFFECT_IMPURE};
use hakana_str::{Interner, StrId};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::expr::binop::concat_analyzer::{analyze_concat_nodes, get_concat_nodes};
use crate::expr::fetch::array_fetch_analyzer::handle_array_access_on_dict;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;

use hakana_code_info::ttype::template::{TemplateBound, TemplateResult};
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
    context: &mut BlockContext,
) -> TUnion {
    let codebase = statements_analyzer.codebase;
    let mut stmt_type = None;

    if let FunctionLikeIdentifier::Function(name) = functionlike_id {
        if let Some(t) = handle_special_functions(
            statements_analyzer,
            name,
            expr.2,
            pos,
            codebase,
            analysis_data,
            context,
        ) {
            stmt_type = Some(t);
        }
    }

    // todo support custom return type providers for functions

    let stmt_type = if let Some(stmt_type) = stmt_type {
        stmt_type
    } else if let Some(function_return_type) = &function_storage.return_type {
        if !function_storage.template_types.is_empty()
            && !function_storage.template_types.is_empty()
        {
            let fn_id = match functionlike_id {
                FunctionLikeIdentifier::Function(function_id) => function_id,
                FunctionLikeIdentifier::Method(_, _) => panic!(),
                _ => {
                    panic!()
                }
            };

            for (template_name, _) in &function_storage.template_types {
                if template_result.lower_bounds.get(template_name).is_none() {
                    template_result.lower_bounds.insert(
                        *template_name,
                        FxHashMap::from_iter([(
                            GenericParent::FunctionLike(*fn_id),
                            vec![TemplateBound::new(get_nothing(), 1, None, None)],
                        )]),
                    );
                }
            }
        }

        let mut function_return_type = function_return_type.clone();

        if !template_result.lower_bounds.is_empty() && !function_storage.template_types.is_empty() {
            type_expander::expand_union(
                codebase,
                &Some(statements_analyzer.interner),
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
            &Some(statements_analyzer.interner),
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

        if function_return_type.is_nothing() && context.function_context.ignore_noreturn_calls {
            function_return_type = get_mixed();
        }

        // todo dispatch AfterFunctionCallAnalysisEvent

        function_return_type
    } else {
        get_mixed_any()
    };

    add_dataflow(
        statements_analyzer,
        expr,
        pos,
        functionlike_id,
        function_storage,
        stmt_type,
        &template_result,
        analysis_data,
        context,
    )
}

fn handle_special_functions(
    statements_analyzer: &StatementsAnalyzer,
    name: &StrId,
    args: &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
    pos: &Pos,
    codebase: &CodebaseInfo,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Option<TUnion> {
    match name {
        &StrId::INVARIANT => {
            if let Some((_, aast::Expr(_, _, aast::Expr_::False))) = args.first() {
                Some(get_nothing())
            } else {
                None
            }
        }
        &StrId::TYPE_STRUCTURE_FN => {
            if let (Some((_, first_arg_expr)), Some((_, second_arg_expr))) =
                (args.first(), args.get(1))
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
        &StrId::GLOBAL_GET => {
            if let Some((_, arg_expr)) = args.first() {
                if let Some(expr_type) = analysis_data.get_expr_type(arg_expr.pos()) {
                    expr_type.get_single_literal_string_value().map(|value| {
                        get_type_for_superglobal(statements_analyzer, value, pos, analysis_data)
                    })
                } else {
                    None
                }
            } else {
                None
            }
        }
        &StrId::PREG_SPLIT => {
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
        &StrId::DEBUG_BACKTRACE => Some(wrap_atomic(TAtomic::TVec {
            known_items: None,
            type_param: Box::new(wrap_atomic(TAtomic::TDict(TDict {
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
            }))),
            known_count: None,
            non_empty: true,
        })),
        &StrId::STR_REPLACE => {
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
        &StrId::PREG_REPLACE => {
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
        &StrId::MICROTIME => {
            if let Some((_, arg_expr)) = args.first() {
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
        &StrId::LIB_STR_JOIN => {
            if let (Some((_, first_arg_expr)), Some((_, second_arg_expr))) =
                (args.first(), args.get(1))
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
        &StrId::LIB_STR_FORMAT | &StrId::SPRINTF => {
            if let Some(first_arg) = args.first() {
                match &first_arg.1 .2 {
                    aast::Expr_::String(simple_string) => {
                        return Some(handle_str_format(
                            simple_string,
                            first_arg,
                            args,
                            statements_analyzer,
                            analysis_data,
                            pos,
                        ));
                    }
                    aast::Expr_::Binop(boxed) => {
                        let mut concat_nodes = get_concat_nodes(&boxed.lhs);
                        concat_nodes.push(&boxed.rhs);

                        let mut more_complex_string = BString::new(vec![]);

                        for concat_node in concat_nodes {
                            if let aast::Expr_::String(simple_string) = &concat_node.2 {
                                more_complex_string.append(&mut simple_string.clone());
                            }
                        }

                        return Some(handle_str_format(
                            &more_complex_string,
                            first_arg,
                            args,
                            statements_analyzer,
                            analysis_data,
                            pos,
                        ));
                    }
                    _ => (),
                }
            }

            None
        }
        &StrId::LIB_STR_TRIM
        | &StrId::LIB_STR_STRIP_SUFFIX
        | &StrId::LIB_STR_SLICE
        | &StrId::LIB_STR_REPLACE => {
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
        &StrId::LIB_STR_SPLIT => {
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
        &StrId::RANGE => {
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
        &StrId::IDX_FN => {
            if args.len() >= 2 {
                let dict_type = analysis_data.get_rc_expr_type(args[0].1.pos()).cloned();
                let dim_type = analysis_data.get_rc_expr_type(args[1].1.pos()).cloned();

                let mut expr_type = None;

                if let (Some(dict_type), Some(dim_type)) = (dict_type, dim_type) {
                    for atomic_type in &dict_type.types {
                        if let TAtomic::TDict(TDict { .. }) = atomic_type {
                            let mut expr_type_inner = handle_array_access_on_dict(
                                statements_analyzer,
                                pos,
                                analysis_data,
                                context,
                                atomic_type,
                                &dim_type,
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
                        expr_type = expr_type.map(|expr_type| {
                            if let Some(default_type) = default_type {
                                add_union_type(expr_type, default_type, codebase, false)
                            } else {
                                add_union_type(expr_type, &get_mixed_any(), codebase, false)
                            }
                        });
                    }
                }

                Some(expr_type.unwrap_or(get_mixed_any()))
            } else {
                None
            }
        }
        &StrId::DIRNAME => {
            if args.len() == 1 {
                let file_type = analysis_data.get_rc_expr_type(args[0].1.pos()).cloned();

                if let Some(file_type) = file_type {
                    if let Some(literal_value) = file_type.get_single_literal_string_value() {
                        let path = Path::new(&literal_value);
                        if let Some(dir) = path.parent() {
                            return Some(get_literal_string(dir.to_str().unwrap().to_owned()));
                        }
                    }
                }
            }

            None
        }
        &StrId::ASIO_JOIN => {
            if args.len() == 1 {
                let mut awaited_type = analysis_data
                    .get_expr_type(args[0].1.pos())
                    .cloned()
                    .unwrap_or(get_mixed_any());

                let awaited_types = awaited_type.types.drain(..).collect::<Vec<_>>();

                let mut new_types = vec![];

                for atomic_type in awaited_types {
                    if let TAtomic::TAwaitable { value } = atomic_type {
                        let inside_type = (*value).clone();
                        extend_dataflow_uniquely(
                            &mut awaited_type.parent_nodes,
                            inside_type.parent_nodes,
                        );
                        new_types.extend(inside_type.types);

                        analysis_data.expr_effects.insert(
                            (pos.start_offset() as u32, pos.end_offset() as u32),
                            EFFECT_IMPURE,
                        );
                    } else {
                        new_types.push(atomic_type);
                    }
                }

                awaited_type.types = new_types;

                Some(awaited_type)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_type_for_superglobal(
    statements_analyzer: &StatementsAnalyzer,
    name: String,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
) -> TUnion {
    match name.as_str() {
        "_FILES" | "_SERVER" | "_ENV" => get_mixed(),
        "_GET" | "_REQUEST" | "_POST" | "_COOKIE" => {
            let mut var_type = get_mixed();

            let taint_pos = statements_analyzer.get_hpos(pos);
            let taint_source = DataFlowNode {
                id: DataFlowNodeId::Var(
                    VarId(
                        statements_analyzer
                            .interner
                            .get(&format!("${}", name))
                            .unwrap(),
                    ),
                    taint_pos.file_path,
                    taint_pos.start_offset,
                    taint_pos.end_offset,
                ),
                kind: DataFlowNodeKind::TaintSource {
                    pos: None,
                    types: if name == "_GET" || name == "_REQUEST" {
                        vec![SourceType::UriRequestHeader]
                    } else {
                        vec![SourceType::NonUriRequestHeader]
                    },
                },
            };

            analysis_data.data_flow_graph.add_node(taint_source.clone());

            var_type.parent_nodes.push(taint_source);

            var_type
        }
        "argv" => get_mixed_any(),
        "argc" => get_int(),
        _ => get_mixed_any(),
    }
}

fn handle_str_format(
    simple_string: &BString,
    first_arg: &(ast_defs::ParamKind, aast::Expr<(), ()>),
    args: &[(ast_defs::ParamKind, aast::Expr<(), ()>)],
    statements_analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
) -> TUnion {
    let mut escaped = false;
    let mut in_format_string = false;
    let mut literals = vec![];
    let mut cur_literal = "".to_string();

    for c in simple_string.iter().copied() {
        if in_format_string {
            in_format_string = false;
            continue;
        }

        if !escaped {
            if c as char == '%' {
                in_format_string = true;
                literals.push(aast::Expr(
                    (),
                    first_arg.1.pos().clone(),
                    aast::Expr_::String(BString::from(cur_literal)),
                ));
                cur_literal = "".to_string();
                continue;
            }

            if c as char == '\\' {
                escaped = true;
            }

            in_format_string = false;
        } else {
            if c as char == '\\' {
                cur_literal += "\\";
                escaped = false;
                continue;
            }

            escaped = false;
        }

        cur_literal += (c as char).to_string().as_str();
    }

    literals.push(aast::Expr(
        (),
        first_arg.1.pos().clone(),
        aast::Expr_::String(BString::from(cur_literal)),
    ));

    let mut concat_args = vec![];

    for (i, literal) in literals.iter().enumerate() {
        concat_args.push(literal);
        if let Some(arg) = args.get(i + 1) {
            concat_args.push(&arg.1);
        } else {
            break;
        }
    }

    analyze_concat_nodes(concat_args, statements_analyzer, analysis_data, pos)
}

fn get_type_structure_type(
    statements_analyzer: &StatementsAnalyzer,
    first_expr_type: &TUnion,
    second_expr_type: &TUnion,
    this_class: Option<StrId>,
) -> Option<TUnion> {
    if let Some(second_arg_string) = second_expr_type.get_single_literal_string_value() {
        let const_name = statements_analyzer.interner.get(&second_arg_string)?;

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
                .codebase
                .classlike_infos
                .get(&classname)
            {
                if let Some(type_constant_info) = classlike_info.type_constants.get(&const_name) {
                    return Some(wrap_atomic(TAtomic::TTypeAlias {
                        name: StrId::TYPE_STRUCTURE,
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
    context: &mut BlockContext,
) -> TUnion {
    // todo dispatch AddRemoveTaintsEvent

    //let added_taints = Vec::new();
    //let removed_taints = Vec::new();

    if let FunctionLikeIdentifier::Function(StrId::ASIO_JOIN) = functionlike_id {
        return stmt_type;
    }

    let data_flow_graph = &mut analysis_data.data_flow_graph;

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if !context.allow_taints {
            return stmt_type;
        }
    }

    let mut stmt_type = stmt_type;

    // todo conditionally remove taints

    let function_call_node = DataFlowNode::get_for_method_return(
        functionlike_id,
        if data_flow_graph.kind == GraphKind::FunctionBody {
            Some(statements_analyzer.get_hpos(pos))
        } else if let Some(return_pos) = &functionlike_storage.return_type_location {
            Some(*return_pos)
        } else {
            functionlike_storage.name_location
        },
        if functionlike_storage.specialize_call {
            Some(statements_analyzer.get_hpos(pos))
        } else {
            None
        },
    );

    data_flow_graph.add_node(function_call_node.clone());

    let (param_offsets, variadic_path) =
        if !functionlike_storage.user_defined && (!expr.2.is_empty() || expr.3.is_some()) {
            get_special_argument_nodes(functionlike_id, expr)
        } else {
            (vec![], None)
        };

    let added_removed_taints = if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        get_special_added_removed_taints(functionlike_id, statements_analyzer.interner)
    } else {
        FxHashMap::default()
    };

    let mut last_arg = usize::MAX;

    for (param_offset, path_kind) in param_offsets {
        if let Some(arg) = expr.2.get(param_offset) {
            let arg_pos = statements_analyzer.get_hpos(arg.1.pos());

            add_special_param_dataflow(
                statements_analyzer,
                functionlike_id,
                true,
                param_offset,
                arg_pos,
                pos,
                &added_removed_taints,
                data_flow_graph,
                &function_call_node,
                path_kind,
            );
        }

        last_arg = param_offset;
    }

    if let Some(path_kind) = &variadic_path {
        for (param_offset, (_, arg)) in expr.2.iter().enumerate() {
            if last_arg == usize::MAX || param_offset > last_arg {
                let arg_pos = statements_analyzer.get_hpos(arg.pos());

                add_special_param_dataflow(
                    statements_analyzer,
                    functionlike_id,
                    true,
                    param_offset,
                    arg_pos,
                    pos,
                    &added_removed_taints,
                    data_flow_graph,
                    &function_call_node,
                    path_kind.clone(),
                );
            }
        }

        if let Some(expanded_arg) = expr.3 {
            add_special_param_dataflow(
                statements_analyzer,
                functionlike_id,
                true,
                expr.2.len(),
                statements_analyzer.get_hpos(expanded_arg.pos()),
                pos,
                &added_removed_taints,
                data_flow_graph,
                &function_call_node,
                path_kind.clone(),
            );
        }
    }

    if let GraphKind::WholeProgram(_) = &data_flow_graph.kind {
        if !functionlike_storage.taint_source_types.is_empty() {
            let function_call_node_source = DataFlowNode {
                id: function_call_node.id.clone(),
                kind: DataFlowNodeKind::TaintSource {
                    pos: function_call_node.get_pos(),
                    types: functionlike_storage.taint_source_types.clone(),
                },
            };
            data_flow_graph.add_node(function_call_node_source);
        }
    }

    stmt_type.parent_nodes.push(function_call_node);

    stmt_type
}

pub(crate) fn add_special_param_dataflow(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_id: &FunctionLikeIdentifier,
    specialize_call: bool,
    param_offset: usize,
    arg_pos: HPos,
    pos: &Pos,
    added_removed_taints: &FxHashMap<usize, (Vec<SinkType>, Vec<SinkType>)>,
    data_flow_graph: &mut DataFlowGraph,
    function_call_node: &DataFlowNode,
    path_kind: PathKind,
) {
    let argument_node = DataFlowNode::get_for_method_argument(
        functionlike_id,
        param_offset,
        Some(arg_pos),
        if specialize_call {
            Some(statements_analyzer.get_hpos(pos))
        } else {
            None
        },
    );

    let (added_taints, removed_taints) =
        if let Some(added_removed_taints) = added_removed_taints.get(&param_offset) {
            added_removed_taints.clone()
        } else {
            (vec![], vec![])
        };

    data_flow_graph.add_path(
        &argument_node,
        function_call_node,
        path_kind,
        added_taints,
        removed_taints,
    );
    data_flow_graph.add_node(argument_node);
}

/*
Returns a list of paths with (input_argument_position, path to return output).
The optional path is for functions with ... params.
*/
fn get_special_argument_nodes(
    functionlike_id: &FunctionLikeIdentifier,
    expr: (
        (&Pos, &ast_defs::Id_),
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
) -> (Vec<(usize, PathKind)>, Option<PathKind>) {
    match functionlike_id {
        FunctionLikeIdentifier::Function(function_name) => match *function_name {
            StrId::ASIO_JOIN => (vec![], None),
            StrId::VAR_EXPORT
            | StrId::PRINT_R
            | StrId::HIGHLIGHT_STRING
            | StrId::STRTOLOWER
            | StrId::STRTOUPPER
            | StrId::TRIM
            | StrId::LTRIM
            | StrId::RTRIM
            | StrId::LIB_STR_LOWERCASE
            | StrId::LIB_STR_UPPERCASE
            | StrId::LIB_STR_CAPITALIZE
            | StrId::LIB_STR_CAPITALIZE_WORDS
            | StrId::STRIP_TAGS
            | StrId::STRIPSLASHES
            | StrId::STRIPCSLASHES
            | StrId::HTMLENTITIES
            | StrId::HTMLENTITYDECODE
            | StrId::HTMLSPECIALCHARS
            | StrId::HTMLSPECIALCHARS_DECODE
            | StrId::STR_ROT13
            | StrId::STR_SHUFFLE
            | StrId::STRSTR
            | StrId::STRISTR
            | StrId::STRCHR
            | StrId::STRPBRK
            | StrId::STRRCHR
            | StrId::STRREV
            | StrId::PREG_QUOTE
            | StrId::WORDWRAP
            | StrId::REALPATH
            | StrId::STRVAL
            | StrId::STRGETCSV
            | StrId::ADDCSLASHES
            | StrId::ADDSLASHES
            | StrId::UCFIRST
            | StrId::UCWORDS
            | StrId::LCFIRST
            | StrId::NL2BR
            | StrId::QUOTED_PRINTABLE_DECODE
            | StrId::QUOTED_PRINTABLE_ENCODE
            | StrId::QUOTE_META
            | StrId::CHOP
            | StrId::CONVERT_UUDECODE
            | StrId::CONVERT_UUENCODE
            | StrId::BASE64_ENCODE
            | StrId::BASE64_DECODE
            | StrId::URLENCODE
            | StrId::URLDECODE
            | StrId::GZINFLATE
            | StrId::LIB_DICT_FILTER
            | StrId::LIB_DICT_FILTER_ASYNC
            | StrId::LIB_DICT_FILTER_KEYS
            | StrId::LIB_DICT_FILTER_NULLS
            | StrId::LIB_DICT_FILTER_WITH_KEY
            | StrId::LIB_DICT_FLATTEN
            | StrId::LIB_VEC_FILTER
            | StrId::LIB_VEC_FILTER_ASYNC
            | StrId::LIB_VEC_FILTER_NULLS
            | StrId::LIB_VEC_FILTER_WITH_KEY
            | StrId::LIB_VEC_REVERSE
            | StrId::LIB_DICT_REVERSE
            | StrId::LIB_VEC_UNIQUE
            | StrId::LIB_KEYSET_FILTER
            | StrId::LIB_KEYSET_FILTER_NULLS
            | StrId::LIB_KEYSET_FILTER_ASYNC
            | StrId::LIB_KEYSET_FLATTEN
            | StrId::LIB_KEYSET_KEYS
            | StrId::KEYSET
            | StrId::VEC
            | StrId::DICT
            | StrId::GET_OBJECT_VARS
            | StrId::RAWURLENCODE
            | StrId::LIB_DICT_FROM_ASYNC
            | StrId::LIB_VEC_FROM_ASYNC
            | StrId::ORD
            | StrId::LOG
            | StrId::IP2LONG
            | StrId::BIN2HEX
            | StrId::HEX2BIN
            | StrId::ESCAPESHELLARG
            | StrId::FIXME_UNSAFE_CAST
            | StrId::LIB_DICT_COUNT_VALUES
            | StrId::LIB_DICT_UNIQUE
            | StrId::LIB_STR_REVERSE
            | StrId::LIB_VEC_CAST_CLEAR_LEGACY_ARRAY_MARK
            | StrId::CLASS_METH_GET_CLASS
            | StrId::CLASS_METH_GET_METHOD
            | StrId::CHR
            | StrId::DECBIN
            | StrId::DECHEX
            | StrId::FB_SERIALIZE
            | StrId::HEXDEC
            | StrId::LZ4_COMPRESS
            | StrId::LZ4_UNCOMPRESS
            | StrId::RAWURLDECODE
            | StrId::UTF8_DECODE
            | StrId::UTF8_ENCODE
            | StrId::STREAM_GET_META_DATA
            | StrId::DIRNAME => (vec![(0, PathKind::Default)], None),
            StrId::LIB_REGEX_FIRST_MATCH
            | StrId::LIB_DICT_MERGE
            | StrId::ARRAY_MERGE
            | StrId::LIB_VEC_CONCAT
            | StrId::LIB_KEYSET_UNION
            | StrId::PACK
            | StrId::UNPACK
            | StrId::JSON_DECODE => (vec![(0, PathKind::Default)], Some(PathKind::Default)),
            StrId::LIB_DICT_SELECT_KEYS
            | StrId::LIB_VEC_TAKE
            | StrId::LIB_DICT_TAKE
            | StrId::LIB_KEYSET_TAKE
            | StrId::LIB_STR_SLICE
            | StrId::LIB_STR_FORMAT_NUMBER
            | StrId::LIB_DICT_DIFF_BY_KEY
            | StrId::NUMBER_FORMAT
            | StrId::LIB_DICT_CHUNK
            | StrId::LIB_VEC_DIFF
            | StrId::LIB_KEYSET_DIFF
            | StrId::LIB_KEYSET_INTERSECT
            | StrId::LIB_DICT_DROP
            | StrId::LIB_KEYSET_DROP
            | StrId::LIB_VEC_INTERSECT
            | StrId::LIB_VEC_SLICE
            | StrId::LIB_VEC_RANGE
            | StrId::LIB_VEC_CHUNK
            | StrId::LIB_KEYSET_CHUNK
            | StrId::LIB_STR_STRIP_PREFIX
            | StrId::LIB_STR_STRIP_SUFFIX
            | StrId::LIB_STR_REPEAT
            | StrId::SUBSTR
            | StrId::LIB_DICT_ASSOCIATE
            | StrId::GZCOMPRESS
            | StrId::GZDECODE
            | StrId::GZDEFLATE
            | StrId::GZUNCOMPRESS
            | StrId::JSON_DECODE_WITH_ERROR
            | StrId::LIB__PRIVATE_REGEX_MATCH
            | StrId::LIB_STR_TRIM
            | StrId::LIB_STR_TRIM_LEFT
            | StrId::LIB_STR_TRIM_RIGHT
            | StrId::STR_REPEAT
            | StrId::LIB_VEC_DROP
            | StrId::BASENAME => (vec![(0, PathKind::Default)], Some(PathKind::Aggregate)),
            StrId::LIB_STR_SLICE_L => (
                vec![
                    (0, PathKind::Aggregate),
                    (1, PathKind::Default),
                    (1, PathKind::Aggregate),
                    (2, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::LIB_C_IS_EMPTY
            | StrId::LIB_C_COUNT
            | StrId::COUNT
            | StrId::LIB_C_ANY
            | StrId::LIB_C_EVERY
            | StrId::LIB_C_SEARCH
            | StrId::LIB_STR_IS_EMPTY
            | StrId::LIB_STR_LENGTH
            | StrId::LIB_VEC_KEYS
            | StrId::LIB_STR_TO_INT
            | StrId::LIB_MATH_SUM
            | StrId::LIB_MATH_SUM_FLOAT
            | StrId::LIB_MATH_MIN
            | StrId::LIB_MATH_MIN_BY
            | StrId::LIB_MATH_MAX
            | StrId::LIB_MATH_MEAN
            | StrId::LIB_MATH_MEDIAN
            | StrId::LIB_MATH_CEIL
            | StrId::LIB_MATH_COS
            | StrId::LIB_MATH_FLOOR
            | StrId::LIB_MATH_IS_NAN
            | StrId::LIB_MATH_LOG
            | StrId::LIB_MATH_SIN
            | StrId::LIB_MATH_SQRT
            | StrId::LIB_MATH_TAN
            | StrId::LIB_MATH_ABS
            | StrId::INTVAL
            | StrId::GET_CLASS
            | StrId::CTYPE_LOWER
            | StrId::SHA1
            | StrId::MD5
            | StrId::NON_CRYPTO_MD5_LOWER
            | StrId::NON_CRYPTO_MD5_UPPER
            | StrId::CRC32
            | StrId::FILTER_VAR
            | StrId::LIB_LOCALE_CREATE
            | StrId::IS_A
            | StrId::IS_BOOL
            | StrId::IS_CALLABLE
            | StrId::IS_CALLABLE_WITH_NAME
            | StrId::IS_FINITE
            | StrId::IS_FLOAT
            | StrId::IS_INFINITE
            | StrId::IS_INT
            | StrId::IS_NAN
            | StrId::IS_NULL
            | StrId::IS_NUMERIC
            | StrId::IS_OBJECT
            | StrId::IS_RESOURCE
            | StrId::IS_SCALAR
            | StrId::IS_STRING
            | StrId::CTYPE_ALNUM
            | StrId::CTYPE_ALPHA
            | StrId::CTYPE_DIGIT
            | StrId::CTYPE_PUNCT
            | StrId::CTYPE_SPACE
            | StrId::CTYPE_UPPER
            | StrId::CTYPE_XDIGIT
            | StrId::IS_DICT
            | StrId::IS_VEC
            | StrId::IS_ANY_ARRAY
            | StrId::IS_DICT_OR_DARRAY
            | StrId::IS_VEC_OR_VARRAY
            | StrId::ASIN
            | StrId::CEIL
            | StrId::ABS
            | StrId::DEG2RAD
            | StrId::FLOOR
            | StrId::CLASS_EXISTS
            | StrId::LONG2IP
            | StrId::RAD2DEG
            | StrId::ROUND
            | StrId::GETTYPE
            | StrId::IS_FUN
            | StrId::IS_PHP_ARRAY
            | StrId::FUNCTION_EXISTS
            | StrId::GET_PARENT_CLASS
            | StrId::GET_RESOURCE_TYPE
            | StrId::FLOATVAL
            | StrId::TYPE_STRUCTURE_FN => (vec![(0, PathKind::Aggregate)], None),
            StrId::LIB_MATH_ALMOST_EQUALS
            | StrId::LIB_MATH_BASE_CONVERT
            | StrId::LIB_MATH_EXP
            | StrId::LIB_MATH_FROM_BASE
            | StrId::LIB_MATH_INT_DIV
            | StrId::LIB_MATH_TO_BASE
            | StrId::LIB_MATH_MAX_BY
            | StrId::LIB_MATH_MAXVA
            | StrId::LIB_MATH_MINVA
            | StrId::LIB_STR_STARTS_WITH
            | StrId::LIB_STR_STARTS_WITH_CI
            | StrId::LIB_STR_ENDS_WITH
            | StrId::LIB_STR_ENDS_WITH_CI
            | StrId::LIB_STR_SEARCH
            | StrId::LIB_STR_SEARCH_L
            | StrId::LIB_STR_SEARCH_LAST
            | StrId::LIB_STR_SEARCH_LAST_L
            | StrId::LIB_STR_SEARCH_CI
            | StrId::LIB_STR_CONTAINS
            | StrId::LIB_STR_CONTAINS_CI
            | StrId::LIB_STR_COMPARE
            | StrId::LIB_STR_COMPARE_CI
            | StrId::HASH_EQUALS
            | StrId::RANGE
            | StrId::STRPOS
            | StrId::SUBSTR_COUNT
            | StrId::STRCMP
            | StrId::STRNATCASECMP
            | StrId::LIB_KEYSET_EQUAL
            | StrId::LIB_DICT_EQUAL
            | StrId::LIB_LEGACY_FIXME_EQ
            | StrId::LIB_LEGACY_FIXME_LT
            | StrId::LIB_LEGACY_FIXME_NEQ
            | StrId::LIB_STR_LENGTH_L
            | StrId::IS_SUBCLASS_OF
            | StrId::STRIPOS
            | StrId::STRLEN
            | StrId::STRNATCMP
            | StrId::STRNCMP
            | StrId::STRRPOS
            | StrId::STRSPN
            | StrId::LEVENSHTEIN
            | StrId::INTDIV
            | StrId::STRCASECMP
            | StrId::STRCSPN
            | StrId::SUBSTR_COMPARE
            | StrId::VERSION_COMPARE
            | StrId::FMOD
            | StrId::POW
            | StrId::LIB_MATH_ROUND
            | StrId::ATAN2
            | StrId::MB_DETECT_ENCODING => (vec![], Some(PathKind::Aggregate)),
            StrId::LIB_C_CONTAINS
            | StrId::LIB_C_CONTAINS_KEY
            | StrId::IN_ARRAY
            | StrId::PREG_MATCH
            | StrId::LIB_REGEX_MATCHES
            | StrId::PREG_MATCH_WITH_MATCHES
            | StrId::PREG_MATCH_ALL_WITH_MATCHES
            | StrId::HASH => (
                vec![
                    (0, PathKind::Aggregate),
                    (1, PathKind::Aggregate),
                    (3, PathKind::Aggregate),
                    (4, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::PREG_MATCH_WITH_MATCHES_AND_ERROR => (
                vec![
                    (0, PathKind::Aggregate),
                    (1, PathKind::Aggregate),
                    (4, PathKind::Aggregate),
                    (5, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::JSON_ENCODE | StrId::SERIALIZE => (vec![(0, PathKind::Serialize)], None),
            StrId::VAR_DUMP | StrId::PRINTF => {
                (vec![(0, PathKind::Serialize)], Some(PathKind::Serialize))
            }
            StrId::SSCANF | StrId::SUBSTR_REPLACE => {
                (vec![(0, PathKind::Default), (1, PathKind::Default)], None)
            }
            StrId::STR_REPLACE | StrId::STR_IREPLACE | StrId::PREG_FILTER | StrId::PREG_REPLACE => {
                (
                    vec![
                        (0, PathKind::Aggregate),
                        (1, PathKind::Default),
                        (2, PathKind::Default),
                    ],
                    None,
                )
            }
            StrId::PREG_REPLACE_WITH_COUNT => (
                vec![
                    (0, PathKind::Aggregate),
                    (1, PathKind::Default),
                    (2, PathKind::Default),
                    (0, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::PREG_GREP => (vec![(0, PathKind::Aggregate), (1, PathKind::Default)], None),
            StrId::LIB_STR_REPLACE_EVERY | StrId::VSPRINTF | StrId::IMPLODE | StrId::JOIN => (
                vec![
                    (0, PathKind::Default),
                    (1, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)),
                ],
                None,
            ),
            StrId::STR_PAD
            | StrId::LIB_STR_PAD_LEFT
            | StrId::LIB_STR_PAD_RIGHT
            | StrId::CHUNK_SPLIT
            | StrId::LIB_REGEX_REPLACE
            | StrId::LIB_STR_REPLACE
            | StrId::LIB_STR_REPLACE_CI
            | StrId::STRTR => (
                vec![
                    (0, PathKind::Default),
                    (1, PathKind::Aggregate),
                    (2, PathKind::Default),
                ],
                None,
            ),
            StrId::LIB_STR_SPLICE => (
                vec![
                    (0, PathKind::Default),
                    (1, PathKind::Default),
                    (2, PathKind::Aggregate),
                    (3, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::LIB_DICT_FILL_KEYS => (
                vec![
                    (0, PathKind::Default),
                    (
                        1,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                ],
                None,
            ),
            StrId::LIB_VEC_FILL | StrId::EXPLODE | StrId::PREG_SPLIT => (
                vec![
                    (0, PathKind::Aggregate),
                    (
                        1,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                ],
                None,
            ),
            StrId::HTTP_BUILD_QUERY => (
                vec![(0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue))],
                None,
            ),
            StrId::LIB_REGEX_SPLIT => (
                vec![
                    (
                        0,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                    (1, PathKind::Aggregate),
                    (2, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::LIB_VEC_ZIP => (
                vec![
                    (
                        0,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                    (
                        1,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                ],
                None,
            ),
            StrId::PATHINFO => (
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
            StrId::STR_SPLIT
            | StrId::LIB_STR_SPLIT
            | StrId::LIB_STR_CHUNK
            | StrId::LIB_REGEX_EVERY_MATCH => (
                vec![
                    (
                        0,
                        PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                    ),
                    (1, PathKind::Aggregate),
                    (2, PathKind::Aggregate),
                ],
                None,
            ),
            StrId::LIB_VEC_SORT => (vec![(0, PathKind::Default)], None),
            StrId::LIB_STR_JOIN => (
                vec![
                    (0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)),
                    (1, PathKind::Default),
                ],
                None,
            ),
            StrId::LIB_VEC_MAP
            | StrId::LIB_DICT_MAP
            | StrId::LIB_KEYSET_MAP
            | StrId::LIB_VEC_MAP_ASYNC
            | StrId::LIB_DICT_MAP_ASYNC
            | StrId::LIB_KEYSET_MAP_ASYNC
            | StrId::LIB_VEC_MAP_WITH_KEY
            | StrId::LIB_DICT_MAP_WITH_KEY
            | StrId::LIB_KEYSET_MAP_WITH_KEY
            | StrId::LIB_DICT_MAP_WITH_KEY_ASYNC => (
                vec![(
                    1,
                    PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                )],
                None,
            ),
            StrId::LIB_DICT_FROM_ENTRIES => (
                // todo improve this
                vec![(0, PathKind::Default)],
                None,
            ),
            StrId::LIB_DICT_FLIP => (
                // todo improve this
                vec![(0, PathKind::Default)],
                None,
            ),
            StrId::LIB_DICT_FROM_KEYS | StrId::LIB_DICT_FROM_KEYS_ASYNC => (
                vec![(
                    1,
                    PathKind::UnknownArrayAssignment(ArrayDataKind::ArrayValue),
                )],
                None,
            ),
            StrId::LIB_C_FIRST
            | StrId::LIB_C_FIRSTX
            | StrId::LIB_C_NFIRST
            | StrId::LIB_C_LAST
            | StrId::LIB_C_LASTX
            | StrId::LIB_C_ONLYX
            | StrId::LIB_C_FIND
            | StrId::LIB_C_FINDX => (
                vec![(0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue))],
                None,
            ),
            StrId::LIB_VEC_FLATTEN => (
                vec![(0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue))],
                None,
            ),
            StrId::IDX_FN => {
                if let Some(second_arg) = expr.2.get(1) {
                    if let aast::Expr_::String(str) = &second_arg.1 .2 {
                        return (
                            vec![
                                (
                                    0,
                                    PathKind::ArrayFetch(
                                        ArrayDataKind::ArrayValue,
                                        str.to_string(),
                                    ),
                                ),
                                (1, PathKind::Aggregate),
                            ],
                            None,
                        );
                    }
                }
                (
                    vec![
                        (0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayValue)),
                        (1, PathKind::Aggregate),
                    ],
                    None,
                )
            }
            StrId::LIB_C_FIRST_KEY
            | StrId::LIB_C_FIRST_KEYX
            | StrId::LIB_C_LAST_KEY
            | StrId::LIB_C_LAST_KEYX
            | StrId::LIB_C_FIND_KEY => (
                vec![(0, PathKind::UnknownArrayFetch(ArrayDataKind::ArrayKey))],
                None,
            ),
            // handled separately
            StrId::LIB_STR_FORMAT | StrId::SPRINTF => (vec![], None),
            _ => {
                // if !matches!(functionlike_info.effects, FnEffect::Some(_))
                //     && !matches!(functionlike_info.effects, FnEffect::Arg(_))
                //     && !functionlike_info.pure_can_throw
                //     && !functionlike_info.user_defined
                // {
                //     println!("{}", functionlike_id.to_string(interner));
                // }

                // this is a cop-out, but will guarantee false-positives vs false-negatives
                // in taint analysis
                (vec![], Some(PathKind::Default))
            }
        },
        _ => panic!(),
    }
}

fn get_special_added_removed_taints(
    functionlike_id: &FunctionLikeIdentifier,
    interner: &Interner,
) -> FxHashMap<usize, (Vec<SinkType>, Vec<SinkType>)> {
    match functionlike_id {
        FunctionLikeIdentifier::Function(function_name) => match interner.lookup(function_name) {
            "html_entity_decode" | "htmlspecialchars_decode" => {
                FxHashMap::from_iter([(0, (vec![SinkType::HtmlTag], vec![]))])
            }
            "htmlentities" | "htmlspecialchars" | "strip_tags" | "urlencode" => {
                FxHashMap::from_iter([(
                    0,
                    (vec![], vec![SinkType::HtmlTag, SinkType::HtmlAttributeUri]),
                )])
            }
            _ => FxHashMap::default(),
        },
        _ => panic!(),
    }
}
