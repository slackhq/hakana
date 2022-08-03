use crate::config::Config;
use crate::expr::fetch::atomic_property_fetch_analyzer;
use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::return_analyzer::handle_inout_at_return;
use crate::unused_variable_analyzer::{add_unused_expression_replacements, check_variables_used};
use crate::{file_analyzer::FileAnalyzer, typed_ast::TastInfo};
use function_context::FunctionLikeIdentifier;
use hakana_reflection_info::analysis_result::AnalysisResult;
use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::{DataFlowGraph, GraphKind};
use hakana_reflection_info::data_flow::node::{DataFlowNode, NodeKind};
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::member_visibility::MemberVisibility;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_type::type_comparator::type_comparison_result::TypeComparisonResult;
use hakana_type::type_expander::{self, StaticClassType};
use hakana_type::{add_optional_union_type, get_mixed_any, get_void, type_comparator, wrap_atomic};
use oxidized::aast;
use oxidized::ast_defs::Pos;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

pub(crate) struct FunctionLikeAnalyzer<'a> {
    file_analyzer: &'a FileAnalyzer<'a>,
}

impl<'a> FunctionLikeAnalyzer<'a> {
    pub fn new(file_analyzer: &'a FileAnalyzer) -> Self {
        Self { file_analyzer }
    }

    pub fn analyze_fun(
        &mut self,
        stmt: &aast::FunDef<(), ()>,
        context: &mut ScopeContext,
        analysis_result: &mut AnalysisResult,
    ) {
        let resolved_names = self.file_analyzer.resolved_names.clone();
        let resolved_name = resolved_names.get(&stmt.fun.name.0.start_offset());

        let mut name = match resolved_name {
            Some(resolved_name) => resolved_name.clone(),
            None => stmt.fun.name.1.clone(),
        };

        if name.starts_with("\\") {
            name = name[1..].to_string();
        }

        let function_storage =
            if let Some(f) = self.file_analyzer.codebase.functionlike_infos.get(&name) {
                f
            } else {
                panic!("Function {} could not be loaded", name);
            };

        let mut statements_analyzer = StatementsAnalyzer::new(
            self.file_analyzer,
            function_storage.type_resolution_context.as_ref().unwrap(),
            self.file_analyzer
                .get_file_source()
                .comments
                .iter()
                .filter(|c| {
                    c.0.start_offset() > stmt.fun.span.start_offset()
                        && c.0.end_offset() < stmt.fun.span.end_offset()
                })
                .collect(),
        );

        statements_analyzer.set_function_info(&function_storage);

        context.function_context.calling_functionlike_id =
            Some(FunctionLikeIdentifier::Function(name));

        self.analyze_functionlike(
            &mut statements_analyzer,
            function_storage,
            context,
            &stmt.fun.params,
            &stmt.fun.body.fb_ast,
            analysis_result,
            None,
        );
    }

    pub fn analyze_lambda(
        &mut self,
        stmt: &aast::Fun_<(), ()>,
        context: &mut ScopeContext,
        tast_info: &mut TastInfo,
        analysis_result: &mut AnalysisResult,
        expr_pos: &Pos,
    ) -> Option<FunctionLikeInfo> {
        let lambda_storage = tast_info.closures.get(expr_pos).cloned();
        let name = format!("{}:{}", stmt.name.0.filename(), stmt.name.0.start_offset());

        context.function_context.calling_functionlike_id =
            Some(FunctionLikeIdentifier::Function(name.clone()));

        let mut lambda_storage = if let Some(lambda_storage) = lambda_storage {
            lambda_storage
        } else {
            if let Some(lambda_storage) = self.file_analyzer.codebase.functionlike_infos.get(&name)
            {
                lambda_storage.clone()
            } else {
                return None;
            }
        };

        let mut statements_analyzer = StatementsAnalyzer::new(
            self.file_analyzer,
            lambda_storage.type_resolution_context.as_ref().unwrap(),
            self.file_analyzer
                .get_file_source()
                .comments
                .iter()
                .filter(|c| {
                    c.0.start_offset() > stmt.span.start_offset()
                        && c.0.end_offset() < stmt.span.end_offset()
                })
                .collect(),
        );

        statements_analyzer.set_function_info(&lambda_storage);

        let mut inferred_return_type = self
            .analyze_functionlike(
                &mut statements_analyzer,
                &lambda_storage,
                context,
                &stmt.params,
                &stmt.body.fb_ast,
                analysis_result,
                Some(tast_info),
            )
            .unwrap_or(get_mixed_any());

        if stmt.fun_kind.is_async() {
            if inferred_return_type.is_null() {
                inferred_return_type = get_void();
            }
            inferred_return_type = wrap_atomic(TAtomic::TNamedObject {
                name: "HH\\Awaitable".to_string(),
                type_params: Some(vec![inferred_return_type]),
                is_this: false,
                extra_types: None,
                remapped_params: false,
            })
        }

        lambda_storage.return_type = Some(inferred_return_type);

        Some(lambda_storage)
    }

    pub fn analyze_method(
        &mut self,
        stmt: &aast::Method_<(), ()>,
        classlike_storage: &ClassLikeInfo,
        context: &mut ScopeContext,
        analysis_result: &mut AnalysisResult,
    ) {
        if stmt.abstract_ {
            return;
        }
        let method_name = stmt.name.1.clone();

        let functionlike_storage = classlike_storage.methods.get(&method_name).unwrap();

        let mut statements_analyzer = StatementsAnalyzer::new(
            self.file_analyzer,
            functionlike_storage
                .type_resolution_context
                .as_ref()
                .unwrap(),
            self.file_analyzer
                .get_file_source()
                .comments
                .iter()
                .filter(|c| {
                    c.0.start_offset() > stmt.span.start_offset()
                        && c.0.end_offset() < stmt.span.end_offset()
                })
                .collect(),
        );

        context.vars_in_scope.insert(
            "$this".to_string(),
            Rc::new(wrap_atomic(TAtomic::TNamedObject {
                name: classlike_storage.name.clone(),
                type_params: None,
                is_this: true,
                extra_types: None,
                remapped_params: false,
            })),
        );

        statements_analyzer.set_function_info(&functionlike_storage);

        context.function_context.calling_functionlike_id = Some(FunctionLikeIdentifier::Method(
            classlike_storage.name.clone(),
            method_name.clone(),
        ));
        context.function_context.calling_class = Some(classlike_storage.name.clone());

        self.analyze_functionlike(
            &mut statements_analyzer,
            functionlike_storage,
            context,
            &stmt.params,
            &stmt.body.fb_ast,
            analysis_result,
            None,
        );
    }

    fn add_properties_to_context(
        &mut self,
        classlike_storage: &ClassLikeInfo,
        tast_info: &mut TastInfo,
        function_storage: &FunctionLikeInfo,
        context: &mut ScopeContext,
    ) {
        for (property_name, declaring_class) in &classlike_storage.declaring_property_ids {
            let property_class_storage = self
                .file_analyzer
                .codebase
                .classlike_infos
                .get(declaring_class)
                .unwrap();

            let property_storage = property_class_storage
                .properties
                .get(property_name)
                .unwrap();

            if property_storage.is_static {
                let mut property_type = property_storage.type_.clone();

                let expr_id = format!("{}::${}", classlike_storage.name, property_name);

                if let Some(property_pos) = &property_storage.pos {
                    property_type =
                        atomic_property_fetch_analyzer::add_unspecialized_property_fetch_dataflow(
                            &Some(expr_id.clone()),
                            &(classlike_storage.name.clone(), property_name.clone()),
                            property_pos.clone(),
                            tast_info,
                            false,
                            property_type,
                        );
                }

                let calling_class = context.function_context.calling_class.as_ref().unwrap();

                type_expander::expand_union(
                    self.file_analyzer.get_codebase(),
                    &mut property_type,
                    Some(calling_class),
                    &StaticClassType::Name(calling_class),
                    None,
                    &mut tast_info.data_flow_graph,
                    true,
                    true,
                    if let Some(method_info) = &function_storage.method_info {
                        method_info.is_final
                    } else {
                        false
                    },
                    true,
                    true,
                );

                context
                    .vars_in_scope
                    .insert(expr_id, Rc::new(property_type));
            }
        }
    }

    fn analyze_functionlike(
        &mut self,
        statements_analyzer: &mut StatementsAnalyzer,
        functionlike_storage: &FunctionLikeInfo,
        context: &mut ScopeContext,
        params: &Vec<aast::FunParam<(), ()>>,
        fb_ast: &Vec<aast::Stmt<(), ()>>,
        analysis_result: &mut AnalysisResult,
        parent_tast_info: Option<&mut TastInfo>,
    ) -> Option<TUnion> {
        let mut tast_info = TastInfo::new(
            DataFlowGraph::new(statements_analyzer.get_config().graph_kind),
            statements_analyzer.get_file_analyzer().get_file_source(),
        );

        if let Some(issue_filter) = &statements_analyzer.get_config().issue_filter {
            tast_info.issue_filter = Some(issue_filter.clone());
        }

        self.add_param_types_to_context(
            params,
            functionlike_storage,
            &mut tast_info,
            context,
            statements_analyzer,
        );

        if let Some(calling_class) = &context.function_context.calling_class {
            if let Some(classlike_storage) = self
                .file_analyzer
                .get_codebase()
                .classlike_infos
                .get(calling_class)
            {
                self.add_properties_to_context(
                    classlike_storage,
                    &mut tast_info,
                    functionlike_storage,
                    context,
                );
            }
        }

        let completed_analysis =
            statements_analyzer.analyze(&fb_ast, &mut tast_info, context, &mut None);

        if !context.has_returned {
            handle_inout_at_return(functionlike_storage, context, &mut tast_info, None);
        }

        let config = statements_analyzer.get_config();

        if config.find_unused_expressions && parent_tast_info.is_none() {
            report_unused_expressions(&mut tast_info, config, fb_ast, statements_analyzer);
        }

        let codebase = statements_analyzer.get_codebase();

        let mut inferred_return_type = None;

        if let Some(expected_return_type) = &functionlike_storage.return_type {
            let expected_type_id = expected_return_type.get_id();
            let mut expected_return_type = expected_return_type.clone();
            type_expander::expand_union(
                statements_analyzer.get_codebase(),
                &mut expected_return_type,
                context.function_context.calling_class.as_ref(),
                &if let Some(calling_class) = &context.function_context.calling_class {
                    StaticClassType::Name(calling_class)
                } else {
                    StaticClassType::None
                },
                None,
                &mut tast_info.data_flow_graph,
                true,
                false,
                if let Some(method_info) = &functionlike_storage.method_info {
                    method_info.is_final
                } else {
                    false
                },
                false,
                true,
            );

            let config = statements_analyzer.get_config();

            let return_result_handled = config.hooks.iter().any(|hook| {
                hook.post_functionlike_analysis(
                    context,
                    functionlike_storage,
                    completed_analysis,
                    &mut tast_info,
                    &mut inferred_return_type,
                    codebase,
                    statements_analyzer,
                    expected_type_id.clone(),
                )
            });

            if !return_result_handled {
                if !tast_info.inferred_return_types.is_empty() {
                    for callsite_return_type in &tast_info.inferred_return_types {
                        if type_comparator::union_type_comparator::is_contained_by(
                            codebase,
                            &callsite_return_type,
                            &expected_return_type,
                            false,
                            false,
                            false,
                            &mut TypeComparisonResult::new(),
                        ) {
                            inferred_return_type = Some(add_optional_union_type(
                                callsite_return_type.clone(),
                                inferred_return_type.as_ref(),
                                Some(codebase),
                            ));
                        } else {
                            inferred_return_type = Some(add_optional_union_type(
                                expected_return_type.clone(),
                                inferred_return_type.as_ref(),
                                Some(codebase),
                            ));
                        }
                    }
                } else {
                    inferred_return_type = Some(get_void());
                }
            }
        } else {
            if !tast_info.inferred_return_types.is_empty() {
                for callsite_return_type in &tast_info.inferred_return_types {
                    inferred_return_type = Some(add_optional_union_type(
                        callsite_return_type.clone(),
                        inferred_return_type.as_ref(),
                        Some(codebase),
                    ));
                }
            } else {
                inferred_return_type = Some(get_void());
            }
        }

        if let Some(parent_tast_info) = parent_tast_info {
            if !tast_info.replacements.is_empty() {
                parent_tast_info.replacements.extend(tast_info.replacements);
            }

            parent_tast_info
                .issues_to_emit
                .extend(tast_info.issues_to_emit);

            parent_tast_info
                .symbol_references
                .extend(tast_info.symbol_references);

            parent_tast_info
                .data_flow_graph
                .add_graph(tast_info.data_flow_graph);

            parent_tast_info.pure_exprs.extend(tast_info.pure_exprs);
        } else {
            update_analysis_result_with_tast(
                tast_info,
                analysis_result,
                &statements_analyzer
                    .get_file_analyzer()
                    .get_file_source()
                    .file_path,
                functionlike_storage.ignore_taint_path,
            );
        }

        inferred_return_type
    }

    fn add_param_types_to_context(
        &mut self,
        params: &Vec<aast::FunParam<(), ()>>,
        functionlike_storage: &FunctionLikeInfo,
        tast_info: &mut TastInfo,
        context: &mut ScopeContext,
        statements_analyzer: &mut StatementsAnalyzer,
    ) {
        for (i, param) in functionlike_storage.params.iter().enumerate() {
            let mut param_type = if let Some(param_type) = &param.signature_type {
                let mut param_type = param_type.clone();
                let calling_class = context.function_context.calling_class.as_ref();

                type_expander::expand_union(
                    self.file_analyzer.get_codebase(),
                    &mut param_type,
                    calling_class.clone(),
                    &if let Some(calling_class) = calling_class {
                        StaticClassType::Name(calling_class)
                    } else {
                        StaticClassType::None
                    },
                    None,
                    &mut tast_info.data_flow_graph,
                    true,
                    true,
                    if let Some(method_info) = &functionlike_storage.method_info {
                        method_info.is_final
                    } else {
                        false
                    },
                    true,
                    true,
                );
                param_type
            } else {
                get_mixed_any()
            };

            let param_node = &params[i];

            if let Some(default) = &param_node.expr {
                expression_analyzer::analyze(
                    statements_analyzer,
                    default,
                    tast_info,
                    context,
                    &mut None,
                );
            }

            if param.is_variadic {
                param_type = wrap_atomic(TAtomic::TVec {
                    known_items: None,
                    type_param: param_type,
                    known_count: None,
                    non_empty: false,
                });
            }

            if let Some(param_pos) = &param.location {
                let new_parent_node = DataFlowNode::get_for_param(
                    param.name.clone(),
                    if let Some(method_storage) = &functionlike_storage.method_info {
                        match &method_storage.visibility {
                            MemberVisibility::Public | MemberVisibility::Protected => {
                                NodeKind::NonPrivateParam
                            }
                            MemberVisibility::Private => NodeKind::PrivateParam,
                        }
                    } else {
                        NodeKind::PrivateParam
                    },
                    param_pos.clone(),
                );

                // todo for actual taint analysis this should flow into the object property
                if !param.promoted_property {
                    if tast_info.data_flow_graph.kind == GraphKind::Variable {
                        tast_info
                            .data_flow_graph
                            .add_source(new_parent_node.clone());
                    }
                }

                if param.is_inout {
                    if tast_info.data_flow_graph.kind == GraphKind::Variable {
                        tast_info.data_flow_graph.add_sink(new_parent_node.clone());
                    } else {
                        tast_info.data_flow_graph.add_node(new_parent_node.clone());
                    }

                    tast_info.data_flow_graph.add_path(
                        &new_parent_node,
                        &new_parent_node,
                        PathKind::Inout,
                        HashSet::new(),
                        HashSet::new(),
                    );
                } else {
                    tast_info.data_flow_graph.add_node(new_parent_node.clone());
                }

                if tast_info.data_flow_graph.kind == GraphKind::Taint {
                    let calling_id =
                        if let Some(id) = &context.function_context.calling_functionlike_id {
                            id.clone()
                        } else {
                            context
                                .function_context
                                .calling_functionlike_id
                                .clone()
                                .unwrap()
                        };

                    let argument_node = DataFlowNode::get_for_method_argument(
                        NodeKind::Default,
                        calling_id.to_string(),
                        i,
                        param.location.clone(),
                        None,
                    );

                    tast_info.data_flow_graph.add_path(
                        &argument_node,
                        &new_parent_node,
                        PathKind::Default,
                        HashSet::new(),
                        HashSet::new(),
                    );

                    tast_info.data_flow_graph.add_node(argument_node);
                }

                param_type
                    .parent_nodes
                    .insert(new_parent_node.id.clone(), new_parent_node);

                let config = statements_analyzer.get_config();

                for hook in &config.hooks {
                    hook.handle_expanded_param(context, config, &param_type, param_node, tast_info);
                }
            }

            context
                .vars_in_scope
                .insert(param.name.clone(), Rc::new(param_type.clone()));
        }
    }
}

fn report_unused_expressions(
    tast_info: &mut TastInfo,
    config: &Config,
    fb_ast: &Vec<aast::Stmt<(), ()>>,
    statements_analyzer: &StatementsAnalyzer,
) {
    let unused_source_nodes = check_variables_used(&tast_info.data_flow_graph);

    let mut unused_variable_nodes = vec![];

    for node in &unused_source_nodes {
        if node.label.starts_with("$_") {
            continue;
        }

        if let Some(pos) = &node.pos {
            match &node.kind {
                NodeKind::PrivateParam => {
                    if config.allow_issue_kind_in_file(&IssueKind::UnusedParameter, &pos.file_path)
                    {
                        tast_info.maybe_add_issue(Issue::new(
                            IssueKind::UnusedParameter,
                            "Unused param ".to_string() + node.label.as_str(),
                            pos.clone(),
                        ));
                    }
                }
                NodeKind::NonPrivateParam => {
                    // todo register public/private param
                }
                NodeKind::Default => {
                    if config.allow_issue_kind_in_file(&IssueKind::UnusedVariable, &pos.file_path) {
                        if config.issues_to_fix.contains(&IssueKind::UnusedVariable) {
                            unused_variable_nodes.push(node.clone());
                        } else {
                            tast_info.maybe_add_issue(Issue::new(
                                IssueKind::UnusedVariable,
                                "Unused variable ".to_string() + node.label.as_str(),
                                pos.clone(),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if !unused_variable_nodes.is_empty() {
        add_unused_expression_replacements(
            fb_ast,
            tast_info,
            &unused_variable_nodes,
            statements_analyzer,
        )
    }
}

pub(crate) fn update_analysis_result_with_tast(
    tast_info: TastInfo,
    analysis_result: &mut AnalysisResult,
    file_path: &String,
    ignore_taint_path: bool,
) {
    if !tast_info.replacements.is_empty() {
        analysis_result
            .replacements
            .entry(file_path.clone())
            .or_insert_with(BTreeMap::new)
            .extend(tast_info.replacements);
    }

    analysis_result
        .emitted_issues
        .entry(file_path.clone())
        .or_insert_with(Vec::new)
        .extend(tast_info.issues_to_emit);

    if tast_info.data_flow_graph.kind == GraphKind::Taint {
        if !ignore_taint_path {
            analysis_result
                .taint_flow_graph
                .add_graph(tast_info.data_flow_graph);
        }
    } else {
        analysis_result
            .symbol_references
            .extend(tast_info.symbol_references);

        for (source_id, c) in tast_info.data_flow_graph.mixed_source_counts {
            if let Some(existing_count) = analysis_result.mixed_source_counts.get_mut(&source_id) {
                existing_count.extend(c);
            } else {
                analysis_result.mixed_source_counts.insert(source_id, c);
            }
        }
    }
}

impl ScopeAnalyzer for FunctionLikeAnalyzer<'_> {
    fn get_namespace(&self) -> &Option<String> {
        return self.file_analyzer.get_namespace();
    }

    fn get_file_analyzer(&self) -> &FileAnalyzer {
        self.file_analyzer
    }

    fn get_codebase(&self) -> &CodebaseInfo {
        self.file_analyzer.get_codebase()
    }

    fn get_config(&self) -> &Config {
        self.file_analyzer.get_config()
    }
}
