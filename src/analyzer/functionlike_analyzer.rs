use crate::config::Config;
use crate::custom_hook::FunctionLikeParamData;
use crate::dataflow::unused_variable_analyzer::{
    add_unused_expression_replacements, check_variables_used,
};
use crate::expr::fetch::atomic_property_fetch_analyzer;
use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::return_analyzer::handle_inout_at_return;
use crate::{file_analyzer::FileAnalyzer, typed_ast::TastInfo};
use hakana_reflection_info::analysis_result::{AnalysisResult, Replacement};
use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::{DataFlowGraph, GraphKind};
use hakana_reflection_info::data_flow::node::{DataFlowNode, VariableSourceKind};
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::function_context::FunctionLikeIdentifier;
use hakana_reflection_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::member_visibility::MemberVisibility;
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_type::type_comparator::type_comparison_result::TypeComparisonResult;
use hakana_type::type_expander::{self, StaticClassType, TypeExpansionOptions};
use hakana_type::{add_optional_union_type, get_mixed_any, get_void, type_comparator, wrap_atomic};
use itertools::Itertools;
use oxidized::aast;
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
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
        let name = resolved_names
            .get(&stmt.fun.name.0.start_offset())
            .unwrap()
            .clone();

        let function_storage =
            if let Some(f) = self.file_analyzer.codebase.functionlike_infos.get(&name) {
                f
            } else {
                panic!(
                    "Function {} could not be loaded",
                    self.get_codebase().interner.lookup(name)
                );
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

        context.function_context.calling_functionlike_id = Some(FunctionLikeIdentifier::Function(
            function_storage.name.clone(),
        ));

        self.analyze_functionlike(
            &mut statements_analyzer,
            &function_storage,
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

        let mut lambda_storage = if let Some(lambda_storage) = lambda_storage {
            lambda_storage
        } else {
            let name = self
                .get_codebase()
                .interner
                .get(format!("{}:{}", stmt.name.0.filename(), stmt.name.0.start_offset()).as_str())
                .unwrap();
            if let Some(lambda_storage) = self.file_analyzer.codebase.functionlike_infos.get(&name)
            {
                lambda_storage.clone()
            } else {
                return None;
            }
        };

        context.function_context.calling_functionlike_id = Some(FunctionLikeIdentifier::Function(
            lambda_storage.name.clone(),
        ));

        tast_info
            .closure_spans
            .push((stmt.span.start_offset(), stmt.span.end_offset()));

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

        let (inferred_return_type, effects) = self.analyze_functionlike(
            &mut statements_analyzer,
            &lambda_storage,
            context,
            &stmt.params,
            &stmt.body.fb_ast,
            analysis_result,
            Some(tast_info),
        );

        lambda_storage.return_type = Some(inferred_return_type.unwrap_or(get_mixed_any()));
        lambda_storage.effects = FnEffect::from_u8(&Some(effects));

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
        let method_name = self.get_codebase().interner.get(&stmt.name.1).unwrap();

        let functionlike_storage = &classlike_storage.methods.get(&method_name).unwrap();

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

        if !stmt.static_ {
            let mut this_type = wrap_atomic(TAtomic::TNamedObject {
                name: classlike_storage.name.clone(),
                type_params: if !classlike_storage.template_types.is_empty() {
                    Some(
                        classlike_storage
                            .template_types
                            .iter()
                            .map(|(param_name, template_map)| {
                                let first_map_entry = template_map.iter().next().unwrap();

                                wrap_atomic(TAtomic::TTemplateParam {
                                    param_name: param_name.clone(),
                                    as_type: (**first_map_entry.1).clone(),
                                    defining_entity: first_map_entry.0.clone(),
                                    from_class: false,
                                    extra_types: None,
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                },
                is_this: true,
                extra_types: None,
                remapped_params: false,
            });

            if classlike_storage.specialize_instance {
                let new_call_node = DataFlowNode::get_for_this_before_method(
                    &MethodIdentifier(classlike_storage.name.clone(), method_name.clone()),
                    functionlike_storage.return_type_location.clone(),
                    None,
                    &statements_analyzer.get_codebase().interner,
                );

                this_type.parent_nodes =
                    FxHashMap::from_iter([(new_call_node.get_id().clone(), new_call_node)]);
            }

            context
                .vars_in_scope
                .insert("$this".to_string(), Rc::new(this_type));
        }

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
        let interner = &self.get_codebase().interner;
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

                let expr_id = format!(
                    "{}::${}",
                    interner.lookup(classlike_storage.name),
                    interner.lookup(*property_name),
                );

                if let Some(property_pos) = &property_storage.pos {
                    property_type =
                        atomic_property_fetch_analyzer::add_unspecialized_property_fetch_dataflow(
                            &Some(expr_id.clone()),
                            &(classlike_storage.name.clone(), property_name.clone()),
                            property_pos.clone(),
                            tast_info,
                            false,
                            property_type,
                            interner,
                        );
                }

                let calling_class = context.function_context.calling_class.as_ref().unwrap();

                type_expander::expand_union(
                    self.file_analyzer.get_codebase(),
                    &mut property_type,
                    &TypeExpansionOptions {
                        self_class: Some(calling_class),
                        static_class_type: StaticClassType::Name(calling_class),
                        function_is_final: if let Some(method_info) = &function_storage.method_info
                        {
                            method_info.is_final
                        } else {
                            false
                        },
                        expand_generic: true,
                        file_path: Some(&self.file_analyzer.get_file_source().file_path),

                        ..Default::default()
                    },
                    &mut tast_info.data_flow_graph,
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
    ) -> (Option<TUnion>, u8) {
        let mut tast_info = TastInfo::new(
            DataFlowGraph::new(statements_analyzer.get_config().graph_kind),
            statements_analyzer.get_file_analyzer().get_file_source(),
            &statements_analyzer.comments,
            &self.get_config().all_custom_issues,
        );

        if let Some(issue_filter) = &statements_analyzer.get_config().allowed_issues {
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
            handle_inout_at_return(
                functionlike_storage,
                statements_analyzer,
                context,
                &mut tast_info,
                None,
            );
        }

        if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
            if let Some(method_storage) = &functionlike_storage.method_info {
                if !method_storage.is_static {
                    if let Some(this_type) = context.vars_in_scope.get("$this") {
                        if this_type.parent_nodes.len() == 1
                            && this_type
                                .parent_nodes
                                .contains_key(&"$this-11057:82-88".to_string())
                        {
                            //panic!();
                        }
                        let new_call_node = DataFlowNode::get_for_this_after_method(
                            &MethodIdentifier(
                                context.function_context.calling_class.unwrap().clone(),
                                functionlike_storage.name,
                            ),
                            functionlike_storage.name_location.clone(),
                            None,
                            &statements_analyzer.get_codebase().interner,
                        );

                        for (_, parent_node) in &this_type.parent_nodes {
                            tast_info.data_flow_graph.add_path(
                                parent_node,
                                &new_call_node,
                                PathKind::Default,
                                None,
                                None,
                            );
                        }

                        tast_info.data_flow_graph.add_node(new_call_node);
                    }
                }
            }
        }

        let config = statements_analyzer.get_config();

        if config.find_unused_expressions && parent_tast_info.is_none() {
            report_unused_expressions(&mut tast_info, config, fb_ast, statements_analyzer);
        }

        if config.remove_fixmes && parent_tast_info.is_none() {
            for unused_fixme_position in tast_info.get_unused_hakana_fixme_positions() {
                tast_info.replacements.insert(
                    (unused_fixme_position.0, unused_fixme_position.1),
                    Replacement::TrimPrecedingWhitespace(unused_fixme_position.2),
                );
            }
        }

        let codebase = statements_analyzer.get_codebase();

        let mut inferred_return_type = None;

        if let Some(expected_return_type) = &functionlike_storage.return_type {
            let expected_type_id = expected_return_type.get_id(Some(&codebase.interner));
            let mut expected_return_type = expected_return_type.clone();
            type_expander::expand_union(
                statements_analyzer.get_codebase(),
                &mut expected_return_type,
                &TypeExpansionOptions {
                    self_class: context.function_context.calling_class.as_ref(),
                    static_class_type: if let Some(calling_class) =
                        &context.function_context.calling_class
                    {
                        StaticClassType::Name(calling_class)
                    } else {
                        StaticClassType::None
                    },
                    function_is_final: if let Some(method_info) = &functionlike_storage.method_info
                    {
                        method_info.is_final
                    } else {
                        false
                    },
                    file_path: Some(statements_analyzer.get_file_path()),

                    ..Default::default()
                },
                &mut tast_info.data_flow_graph,
            );

            let config = statements_analyzer.get_config();

            let return_result_handled = config.hooks.iter().any(|hook| {
                hook.after_functionlike_analysis(
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
                            callsite_return_type.ignore_falsable_issues,
                            false,
                            &mut TypeComparisonResult::new(),
                        ) {
                            inferred_return_type = Some(add_optional_union_type(
                                callsite_return_type.clone(),
                                inferred_return_type.as_ref(),
                                codebase,
                            ));
                        } else {
                            inferred_return_type = Some(add_optional_union_type(
                                expected_return_type.clone(),
                                inferred_return_type.as_ref(),
                                codebase,
                            ));
                        }
                    }
                } else {
                    inferred_return_type = Some(if functionlike_storage.is_async {
                        wrap_atomic(TAtomic::TNamedObject {
                            name: codebase.interner.get("HH\\Awaitable").unwrap(),
                            type_params: Some(vec![get_void()]),
                            is_this: false,
                            extra_types: None,
                            remapped_params: false,
                        })
                    } else {
                        get_void()
                    });
                }
            }
        } else {
            if !tast_info.inferred_return_types.is_empty() {
                for callsite_return_type in &tast_info.inferred_return_types {
                    inferred_return_type = Some(add_optional_union_type(
                        callsite_return_type.clone(),
                        inferred_return_type.as_ref(),
                        codebase,
                    ));
                }
            } else {
                inferred_return_type = Some(if functionlike_storage.is_async {
                    wrap_atomic(TAtomic::TNamedObject {
                        name: codebase.interner.get("HH\\Awaitable").unwrap(),
                        type_params: Some(vec![get_void()]),
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    })
                } else {
                    get_void()
                });
            }
        }

        let mut effects = 0;

        if let FnEffect::Unknown = functionlike_storage.effects {
            for (_, effect) in &tast_info.expr_effects {
                effects |= effect;
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

            parent_tast_info
                .closure_spans
                .extend(tast_info.closure_spans);

            parent_tast_info
                .matched_ignore_positions
                .extend(tast_info.matched_ignore_positions);

            for (kind, count) in tast_info.issue_counts {
                *parent_tast_info.issue_counts.entry(kind).or_insert(0) += count;
            }
        } else {
            update_analysis_result_with_tast(
                tast_info,
                analysis_result,
                self.get_codebase().interner.lookup(
                    statements_analyzer
                        .get_file_analyzer()
                        .get_file_source()
                        .file_path,
                ),
                functionlike_storage.ignore_taint_path,
            );
        }

        (inferred_return_type, effects)
    }

    fn add_param_types_to_context(
        &mut self,
        params: &Vec<aast::FunParam<(), ()>>,
        functionlike_storage: &FunctionLikeInfo,
        tast_info: &mut TastInfo,
        context: &mut ScopeContext,
        statements_analyzer: &mut StatementsAnalyzer,
    ) {
        let interner = &statements_analyzer.get_codebase().interner;

        for (i, param) in functionlike_storage.params.iter().enumerate() {
            let mut param_type = if let Some(param_type) = &param.signature_type {
                let mut param_type = param_type.clone();
                let calling_class = context.function_context.calling_class.as_ref();

                type_expander::expand_union(
                    self.file_analyzer.get_codebase(),
                    &mut param_type,
                    &TypeExpansionOptions {
                        self_class: calling_class.clone(),
                        static_class_type: if let Some(calling_class) = calling_class {
                            StaticClassType::Name(calling_class)
                        } else {
                            StaticClassType::None
                        },
                        evaluate_class_constants: true,
                        evaluate_conditional_types: true,
                        function_is_final: if let Some(method_info) =
                            &functionlike_storage.method_info
                        {
                            method_info.is_final
                        } else {
                            false
                        },
                        expand_generic: true,
                        expand_templates: true,
                        file_path: Some(statements_analyzer.get_file_path()),

                        ..Default::default()
                    },
                    &mut tast_info.data_flow_graph,
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
                let new_parent_node = if let GraphKind::WholeProgram(_) =
                    &tast_info.data_flow_graph.kind
                {
                    DataFlowNode::get_for_assignment(param.name.clone(), param_pos.clone())
                } else {
                    let id = format!(
                        "{}-{}:{}-{}",
                        param.name,
                        interner.lookup(param_pos.file_path),
                        param_pos.start_offset,
                        param_pos.end_offset
                    );

                    DataFlowNode::VariableUseSource {
                        kind: if param.is_inout {
                            VariableSourceKind::InoutParam
                        } else {
                            if let Some(method_storage) = &functionlike_storage.method_info {
                                match &method_storage.visibility {
                                    MemberVisibility::Public | MemberVisibility::Protected => {
                                        VariableSourceKind::NonPrivateParam
                                    }
                                    MemberVisibility::Private => VariableSourceKind::PrivateParam,
                                }
                            } else {
                                VariableSourceKind::PrivateParam
                            }
                        },
                        id,
                        pos: param_pos.clone(),
                        name: param.name.clone(),
                    }
                };

                if !param.promoted_property {
                    if tast_info.data_flow_graph.kind == GraphKind::FunctionBody {
                        tast_info.data_flow_graph.add_node(new_parent_node.clone());
                    }
                }

                tast_info.data_flow_graph.add_node(new_parent_node.clone());

                if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
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
                        calling_id.to_string(&self.get_codebase().interner),
                        i,
                        param.location.clone(),
                        None,
                    );

                    tast_info.data_flow_graph.add_path(
                        &argument_node,
                        &new_parent_node,
                        PathKind::Default,
                        None,
                        None,
                    );

                    tast_info.data_flow_graph.add_node(argument_node);
                }

                param_type
                    .parent_nodes
                    .insert(new_parent_node.get_id().clone(), new_parent_node);

                let config = statements_analyzer.get_config();

                for hook in &config.hooks {
                    hook.handle_functionlike_param(
                        tast_info,
                        FunctionLikeParamData {
                            context,
                            config,
                            param_type: &param_type,
                            param_node,
                            codebase: statements_analyzer.get_codebase(),
                        },
                    );
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
        match node {
            DataFlowNode::VariableUseSource {
                kind,
                id,
                pos,
                name,
            } => {
                if name.starts_with("$_") {
                    continue;
                }

                match &kind {
                    VariableSourceKind::PrivateParam => {
                        tast_info.maybe_add_issue(
                            Issue::new(
                                IssueKind::UnusedParameter,
                                "Unused param ".to_string() + id.as_str(),
                                pos.clone(),
                            ),
                            statements_analyzer.get_config(),
                            statements_analyzer.get_file_path_actual(),
                        );
                    }
                    VariableSourceKind::NonPrivateParam => {
                        // todo register public/private param
                    }
                    VariableSourceKind::Default => {
                        if config.allow_issue_kind_in_file(
                            &IssueKind::UnusedAssignment,
                            statements_analyzer
                                .get_codebase()
                                .interner
                                .lookup(pos.file_path),
                        ) {
                            if config.issues_to_fix.contains(&IssueKind::UnusedAssignment) {
                                unused_variable_nodes.push(node.clone());
                            } else {
                                let unused_closure_variable = tast_info.closure_spans.iter().any(
                                    |(closure_start, closure_end)| {
                                        &pos.start_offset > closure_start
                                            && &pos.start_offset < closure_end
                                    },
                                );

                                tast_info.maybe_add_issue(
                                    if name == "$$" {
                                        Issue::new(
                                            IssueKind::UnusedPipeVariable,
                                            "The pipe data in this expression is not used anywhere"
                                                .to_string(),
                                            pos.clone(),
                                        )
                                    } else if unused_closure_variable {
                                        Issue::new(
                                            IssueKind::UnusedAssignmentInClosure,
                                            format!(
                                                "Assignment to {} is unused in this closure ",
                                                name
                                            ),
                                            pos.clone(),
                                        )
                                    } else {
                                        Issue::new(
                                            IssueKind::UnusedAssignment,
                                            format!("Assignment to {} is unused", name),
                                            pos.clone(),
                                        )
                                    },
                                    statements_analyzer.get_config(),
                                    statements_analyzer.get_file_path_actual(),
                                );
                            }
                        }
                    }
                    VariableSourceKind::InoutParam => {
                        // do nothing
                    }
                }
            }
            _ => {
                panic!()
            }
        };
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
    file_path: &str,
    ignore_taint_path: bool,
) {
    if !tast_info.replacements.is_empty() {
        analysis_result
            .replacements
            .entry(file_path.to_string())
            .or_insert_with(BTreeMap::new)
            .extend(tast_info.replacements);
    }

    analysis_result
        .emitted_issues
        .entry(file_path.to_string())
        .or_insert_with(Vec::new)
        .extend(
            tast_info
                .issues_to_emit
                .into_iter()
                .unique()
                .collect::<Vec<_>>(),
        );

    if let GraphKind::WholeProgram(_) = &tast_info.data_flow_graph.kind {
        if !ignore_taint_path {
            analysis_result
                .program_dataflow_graph
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

        for (kind, count) in tast_info.issue_counts {
            *analysis_result.issue_counts.entry(kind).or_insert(0) += count;
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
