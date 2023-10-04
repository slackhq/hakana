use crate::config::Config;
use crate::custom_hook::FunctionLikeParamData;
use crate::dataflow::unused_variable_analyzer::{
    add_unused_expression_replacements, check_variables_used,
};
use crate::expr::call_analyzer::reconcile_lower_bounds_with_upper_bounds;
use crate::expr::fetch::atomic_property_fetch_analyzer;
use crate::expression_analyzer;
use crate::file_analyzer::InternalError;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::return_analyzer::handle_inout_at_return;
use crate::stmt_analyzer::AnalysisError;
use crate::{file_analyzer::FileAnalyzer, function_analysis_data::FunctionAnalysisData};
use hakana_reflection_info::analysis_result::{AnalysisResult, Replacement};
use hakana_reflection_info::classlike_info::ClassLikeInfo;
use hakana_reflection_info::code_location::{FilePath, HPos, StmtStart};
use hakana_reflection_info::codebase_info::CodebaseInfo;
use hakana_reflection_info::data_flow::graph::{DataFlowGraph, GraphKind};
use hakana_reflection_info::data_flow::node::{DataFlowNode, DataFlowNodeKind, VariableSourceKind};
use hakana_reflection_info::data_flow::path::PathKind;
use hakana_reflection_info::function_context::{FunctionContext, FunctionLikeIdentifier};
use hakana_reflection_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_reflection_info::issue::{Issue, IssueKind};
use hakana_reflection_info::member_visibility::MemberVisibility;
use hakana_reflection_info::method_identifier::MethodIdentifier;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::t_union::TUnion;
use hakana_reflection_info::{Interner, STR_AWAITABLE, STR_EMPTY};
use hakana_type::type_comparator::type_comparison_result::TypeComparisonResult;
use hakana_type::type_expander::{self, StaticClassType, TypeExpansionOptions};
use hakana_type::{add_optional_union_type, get_mixed_any, get_void, type_comparator, wrap_atomic};
use itertools::Itertools;
use oxidized::aast;
use oxidized::ast_defs::Pos;
use rustc_hash::FxHashSet;

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
        analysis_result: &mut AnalysisResult,
    ) -> Result<(), AnalysisError> {
        let resolved_names = self.file_analyzer.resolved_names.clone();

        let name = if let Some(name) = resolved_names.get(&stmt.name.0.start_offset()) {
            *name
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot resolve function name".to_string(),
                HPos::new(
                    stmt.name.pos(),
                    self.file_analyzer.get_file_source().file_path,
                    None,
                ),
            ));
        };

        if self.file_analyzer.analysis_config.ast_diff {
            if self.file_analyzer.codebase.safe_symbols.contains(&name) {
                return Ok(());
            }
        }

        let function_storage = if let Some(f) = self
            .file_analyzer
            .codebase
            .functionlike_infos
            .get(&(name, STR_EMPTY))
        {
            f
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot load function storage".to_string(),
                HPos::new(
                    stmt.name.pos(),
                    self.file_analyzer.get_file_source().file_path,
                    None,
                ),
            ));
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

        let mut function_context = FunctionContext::new();

        function_context.calling_functionlike_id = Some(FunctionLikeIdentifier::Function(
            function_storage.name.clone(),
        ));

        self.analyze_functionlike(
            &mut statements_analyzer,
            &function_storage,
            ScopeContext::new(function_context),
            &stmt.fun.params,
            &stmt.fun.body.fb_ast.0,
            analysis_result,
            None,
        )?;
        Ok(())
    }

    pub fn analyze_lambda(
        &mut self,
        stmt: &aast::Fun_<(), ()>,
        mut context: ScopeContext,
        analysis_data: &mut FunctionAnalysisData,
        analysis_result: &mut AnalysisResult,
        expr_pos: &Pos,
    ) -> Result<FunctionLikeInfo, AnalysisError> {
        let lambda_storage = analysis_data.closures.get(expr_pos).cloned();

        let mut lambda_storage = if let Some(lambda_storage) = lambda_storage {
            lambda_storage
        } else {
            match get_closure_storage(&self.file_analyzer, stmt.span.start_offset()) {
                None => {
                    return Err(AnalysisError::InternalError(
                        "Cannot get closure storage".to_string(),
                        HPos::new(
                            &stmt.span,
                            self.file_analyzer.get_file_source().file_path,
                            None,
                        ),
                    ));
                }
                Some(value) => value,
            }
        };

        analysis_data
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

        context.calling_closure_id = Some(lambda_storage.name);

        statements_analyzer.set_function_info(&lambda_storage);

        let (inferred_return_type, effects) = self.analyze_functionlike(
            &mut statements_analyzer,
            &lambda_storage,
            context,
            &stmt.params,
            &stmt.body.fb_ast.0,
            analysis_result,
            Some(analysis_data),
        )?;

        lambda_storage.return_type = Some(inferred_return_type.unwrap_or(get_mixed_any()));
        lambda_storage.effects = FnEffect::from_u8(&Some(effects));

        Ok(lambda_storage)
    }

    pub fn analyze_method(
        &mut self,
        stmt: &aast::Method_<(), ()>,
        classlike_storage: &ClassLikeInfo,
        analysis_result: &mut AnalysisResult,
    ) -> Result<(), AnalysisError> {
        if stmt.abstract_ {
            return Ok(());
        }

        let method_name = if let Some(method_name) = self.get_interner().get(&stmt.name.1) {
            method_name
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot resolve method name".to_string(),
                HPos::new(
                    &stmt.name.0,
                    self.file_analyzer.get_file_source().file_path,
                    None,
                ),
            ));
        };

        let codebase = self.file_analyzer.codebase;

        if self.file_analyzer.analysis_config.ast_diff {
            if codebase
                .safe_symbol_members
                .contains(&(classlike_storage.name, method_name))
            {
                return Ok(());
            }
        }

        let functionlike_storage = if let Some(functionlike_storage) = codebase
            .functionlike_infos
            .get(&(classlike_storage.name, method_name))
        {
            functionlike_storage
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot resolve function storage".to_string(),
                HPos::new(
                    &stmt.name.0,
                    self.file_analyzer.get_file_source().file_path,
                    None,
                ),
            ));
        };

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

        let mut function_context = FunctionContext::new();
        function_context.calling_functionlike_id = Some(FunctionLikeIdentifier::Method(
            classlike_storage.name,
            method_name.clone(),
        ));
        function_context.calling_class = Some(classlike_storage.name);
        function_context.calling_class_final = classlike_storage.is_final;

        let mut context = ScopeContext::new(function_context);

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

                                wrap_atomic(TAtomic::TGenericParam {
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

            if let GraphKind::WholeProgram(_) = &analysis_result.program_dataflow_graph.kind {
                if classlike_storage.specialize_instance {
                    let new_call_node = DataFlowNode::get_for_this_before_method(
                        &MethodIdentifier(classlike_storage.name, method_name.clone()),
                        functionlike_storage.return_type_location.clone(),
                        None,
                        &statements_analyzer.get_interner(),
                    );

                    this_type.parent_nodes = FxHashSet::from_iter([new_call_node]);
                }
            }

            context
                .vars_in_scope
                .insert("$this".to_string(), Rc::new(this_type));
        }

        statements_analyzer.set_function_info(&functionlike_storage);

        self.analyze_functionlike(
            &mut statements_analyzer,
            functionlike_storage,
            context,
            &stmt.params,
            &stmt.body.fb_ast.0,
            analysis_result,
            None,
        )?;

        Ok(())
    }

    fn add_properties_to_context(
        &mut self,
        classlike_storage: &ClassLikeInfo,
        analysis_data: &mut FunctionAnalysisData,
        function_storage: &FunctionLikeInfo,
        context: &mut ScopeContext,
    ) -> Result<(), InternalError> {
        let interner = &self.get_interner();
        for (property_name, declaring_class) in &classlike_storage.declaring_property_ids {
            let property_class_storage = if let Some(s) = self
                .file_analyzer
                .codebase
                .classlike_infos
                .get(declaring_class)
            {
                s
            } else {
                return Err(InternalError(
                    format!(
                        "Could not load property class storage for {}",
                        interner.lookup(declaring_class)
                    ),
                    classlike_storage.name_location,
                ));
            };

            let property_storage =
                if let Some(s) = property_class_storage.properties.get(property_name) {
                    s
                } else {
                    return Err(InternalError(
                        format!(
                            "Could not load property class storage for property {}",
                            interner.lookup(property_name)
                        ),
                        classlike_storage.name_location,
                    ));
                };

            if property_storage.is_static {
                let mut property_type = property_storage.type_.clone();

                let expr_id = format!(
                    "{}::${}",
                    interner.lookup(&classlike_storage.name),
                    interner.lookup(property_name),
                );

                if let Some(property_pos) = &property_storage.pos {
                    property_type =
                        atomic_property_fetch_analyzer::add_unspecialized_property_fetch_dataflow(
                            &Some(expr_id.clone()),
                            &(classlike_storage.name.clone(), property_name.clone()),
                            property_pos.clone(),
                            analysis_data,
                            false,
                            property_type,
                            interner,
                        );
                }

                let calling_class = context.function_context.calling_class.as_ref().unwrap();

                type_expander::expand_union(
                    self.file_analyzer.get_codebase(),
                    &Some(self.get_interner()),
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
                    &mut analysis_data.data_flow_graph,
                );

                context
                    .vars_in_scope
                    .insert(expr_id, Rc::new(property_type));
            }
        }

        Ok(())
    }

    fn analyze_functionlike(
        &mut self,
        statements_analyzer: &mut StatementsAnalyzer,
        functionlike_storage: &FunctionLikeInfo,
        mut context: ScopeContext,
        params: &Vec<aast::FunParam<(), ()>>,
        fb_ast: &Vec<aast::Stmt<(), ()>>,
        analysis_result: &mut AnalysisResult,
        parent_analysis_data: Option<&mut FunctionAnalysisData>,
    ) -> Result<(Option<TUnion>, u8), AnalysisError> {
        context.inside_async = functionlike_storage.is_async;

        let mut analysis_data = FunctionAnalysisData::new(
            DataFlowGraph::new(statements_analyzer.get_config().graph_kind),
            statements_analyzer.get_file_analyzer().get_file_source(),
            &statements_analyzer.comments,
            &self.get_config().all_custom_issues,
            if let Some(parent_analysis_data) = &parent_analysis_data {
                parent_analysis_data.current_stmt_offset.clone()
            } else {
                None
            },
            if let Some(parent_analysis_data) = &parent_analysis_data {
                Some(parent_analysis_data.hakana_fixme_or_ignores.clone())
            } else {
                None
            },
        );

        if let Some(parent_analysis_data) = &parent_analysis_data {
            analysis_data.type_variable_bounds = parent_analysis_data.type_variable_bounds.clone();

            if !statements_analyzer
                .get_config()
                .migration_symbols
                .is_empty()
            {
                analysis_data.data_flow_graph = parent_analysis_data.data_flow_graph.clone();
            }
        }

        if let Some(issue_filter) = &statements_analyzer.get_config().allowed_issues {
            analysis_data.issue_filter = Some(issue_filter.clone());
        }

        let mut completed_analysis = false;

        match self.add_param_types_to_context(
            params,
            functionlike_storage,
            &mut analysis_data,
            &mut context,
            statements_analyzer,
        ) {
            Err(AnalysisError::InternalError(error, pos)) => {
                return Err(AnalysisError::InternalError(error, pos));
            }
            _ => {
                if let Some(calling_class) = &context.function_context.calling_class {
                    if let Some(classlike_storage) = self
                        .file_analyzer
                        .get_codebase()
                        .classlike_infos
                        .get(calling_class)
                    {
                        if let Err(error) = self.add_properties_to_context(
                            classlike_storage,
                            &mut analysis_data,
                            functionlike_storage,
                            &mut context,
                        ) {
                            return Err(AnalysisError::InternalError(error.0, error.1));
                        }
                    }
                }

                //let start_t = std::time::Instant::now();

                match statements_analyzer.analyze(
                    &fb_ast,
                    &mut analysis_data,
                    &mut context,
                    &mut None,
                ) {
                    Ok(_) => {
                        completed_analysis = true;
                    }
                    Err(AnalysisError::InternalError(error, pos)) => {
                        return Err(AnalysisError::InternalError(error, pos))
                    }
                    _ => {}
                };

                // let end_t = start_t.elapsed();

                // if let Some(functionlike_id) = &context.function_context.calling_functionlike_id {
                //     if fb_ast.len() > 1 {
                //         let first_line = fb_ast[0].0.line() as u64;

                //         let last_line = fb_ast.last().unwrap().0.to_raw_span().end.line();

                //         if last_line - first_line > 10 && last_line - first_line < 100000 {
                //             println!(
                //                 "{}\t{}\t{}",
                //                 functionlike_id.to_string(&statements_analyzer.get_interner()),
                //                 last_line - first_line,
                //                 end_t.as_micros() as u64 / (last_line - first_line)
                //             );
                //         }
                //     }
                // }

                if !context.has_returned {
                    handle_inout_at_return(
                        functionlike_storage,
                        statements_analyzer,
                        &mut context,
                        &mut analysis_data,
                        None,
                    );
                }
            }
        }

        if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
            if let Some(method_storage) = &functionlike_storage.method_info {
                if !method_storage.is_static {
                    if let Some(this_type) = context.vars_in_scope.get("$this") {
                        let new_call_node = DataFlowNode::get_for_this_after_method(
                            &MethodIdentifier(
                                context.function_context.calling_class.unwrap().clone(),
                                functionlike_storage.name,
                            ),
                            functionlike_storage.name_location.clone(),
                            None,
                            &statements_analyzer.get_interner(),
                        );

                        for parent_node in &this_type.parent_nodes {
                            analysis_data.data_flow_graph.add_path(
                                parent_node,
                                &new_call_node,
                                PathKind::Default,
                                None,
                                None,
                            );
                        }

                        analysis_data.data_flow_graph.add_node(new_call_node);
                    }
                }
            }
        }

        let config = statements_analyzer.get_config();

        if completed_analysis && config.find_unused_expressions && parent_analysis_data.is_none() {
            report_unused_expressions(
                &mut analysis_data,
                config,
                fb_ast,
                statements_analyzer,
                &context.function_context.calling_functionlike_id,
                functionlike_storage,
            );
        }

        if config.remove_fixmes && parent_analysis_data.is_none() {
            for unused_fixme_position in analysis_data.get_unused_hakana_fixme_positions() {
                analysis_data.add_replacement(
                    (unused_fixme_position.0, unused_fixme_position.1),
                    if unused_fixme_position.3 {
                        Replacement::TrimTrailingWhitespace(unused_fixme_position.2)
                    } else {
                        Replacement::TrimPrecedingWhitespace(unused_fixme_position.2)
                    },
                );
            }
        }

        let codebase = statements_analyzer.get_codebase();

        let mut inferred_return_type = None;

        if let Some(expected_return_type) = &functionlike_storage.return_type {
            let mut expected_return_type = expected_return_type.clone();
            type_expander::expand_union(
                statements_analyzer.get_codebase(),
                &Some(statements_analyzer.get_interner()),
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
                &mut analysis_data.data_flow_graph,
            );

            let config = statements_analyzer.get_config();

            let return_result_handled = config.hooks.iter().any(|hook| {
                hook.after_functionlike_analysis(
                    &mut context,
                    functionlike_storage,
                    completed_analysis,
                    &mut analysis_data,
                    &mut inferred_return_type,
                    codebase,
                    statements_analyzer,
                    fb_ast,
                )
            });

            if !return_result_handled {
                if !analysis_data.inferred_return_types.is_empty() {
                    for callsite_return_type in &analysis_data.inferred_return_types {
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
                            name: STR_AWAITABLE,
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
            let return_result_handled = config.hooks.iter().any(|hook| {
                hook.after_functionlike_analysis(
                    &mut context,
                    functionlike_storage,
                    completed_analysis,
                    &mut analysis_data,
                    &mut inferred_return_type,
                    codebase,
                    statements_analyzer,
                    fb_ast,
                )
            });

            if !return_result_handled {
                if !analysis_data.inferred_return_types.is_empty() {
                    for callsite_return_type in &analysis_data.inferred_return_types {
                        inferred_return_type = Some(add_optional_union_type(
                            callsite_return_type.clone(),
                            inferred_return_type.as_ref(),
                            codebase,
                        ));
                    }
                } else {
                    inferred_return_type = Some(if functionlike_storage.is_async {
                        wrap_atomic(TAtomic::TNamedObject {
                            name: STR_AWAITABLE,
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
        }

        let mut effects = 0;

        if let FnEffect::Unknown = functionlike_storage.effects {
            for (_, effect) in &analysis_data.expr_effects {
                effects |= effect;
            }
        }

        if let Some(parent_analysis_data) = parent_analysis_data {
            if !analysis_data.replacements.is_empty() {
                parent_analysis_data
                    .replacements
                    .extend(analysis_data.replacements);
            }

            for issue in analysis_data.issues_to_emit {
                parent_analysis_data.maybe_add_issue(
                    issue,
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }

            parent_analysis_data
                .symbol_references
                .extend(analysis_data.symbol_references);

            parent_analysis_data
                .data_flow_graph
                .add_graph(analysis_data.data_flow_graph);

            parent_analysis_data
                .closure_spans
                .extend(analysis_data.closure_spans);

            parent_analysis_data
                .matched_ignore_positions
                .extend(analysis_data.matched_ignore_positions);

            for (kind, count) in analysis_data.issue_counts {
                *parent_analysis_data.issue_counts.entry(kind).or_insert(0) += count;
            }

            for (name, bounds) in analysis_data.type_variable_bounds {
                if let Some(existing_bounds) =
                    parent_analysis_data.type_variable_bounds.get_mut(&name)
                {
                    let existing_bounds_copy = existing_bounds.clone();
                    let filtered_lower_bounds = bounds
                        .0
                        .into_iter()
                        .filter(|bound| !(&existing_bounds_copy.0).contains(bound));
                    let filtered_upper_bounds = bounds
                        .1
                        .into_iter()
                        .filter(|bound| !(&existing_bounds_copy.1).contains(bound));

                    existing_bounds.0.extend(filtered_lower_bounds);
                    existing_bounds.1.extend(filtered_upper_bounds);
                    existing_bounds.1.dedup();
                } else {
                    parent_analysis_data
                        .type_variable_bounds
                        .insert(name, bounds);
                }
            }
        } else {
            if !analysis_data.type_variable_bounds.is_empty() {
                for (_, bounds) in analysis_data.type_variable_bounds.clone() {
                    reconcile_lower_bounds_with_upper_bounds(
                        &bounds.0,
                        &bounds.1,
                        statements_analyzer,
                        &mut analysis_data,
                        functionlike_storage
                            .name_location
                            .unwrap_or(functionlike_storage.def_location),
                    );
                }
            }

            update_analysis_result_with_tast(
                analysis_data,
                analysis_result,
                &statements_analyzer
                    .get_file_analyzer()
                    .get_file_source()
                    .file_path,
                functionlike_storage.ignore_taint_path,
            );
        }

        Ok((inferred_return_type, effects))
    }

    fn add_param_types_to_context(
        &mut self,
        params: &Vec<aast::FunParam<(), ()>>,
        functionlike_storage: &FunctionLikeInfo,
        analysis_data: &mut FunctionAnalysisData,
        context: &mut ScopeContext,
        statements_analyzer: &mut StatementsAnalyzer,
    ) -> Result<(), AnalysisError> {
        let interner = &statements_analyzer.get_interner();

        for (i, param) in functionlike_storage.params.iter().enumerate() {
            let mut param_type = if let Some(param_type) = &param.signature_type {
                for type_node in param_type.get_all_child_nodes() {
                    match type_node {
                        hakana_reflection_info::t_union::TypeNode::Atomic(atomic) => match atomic {
                            TAtomic::TReference { name, .. }
                            | TAtomic::TClosureAlias {
                                id: FunctionLikeIdentifier::Function(name),
                            } => match context.function_context.calling_functionlike_id {
                                Some(FunctionLikeIdentifier::Function(calling_function)) => {
                                    analysis_data
                                        .symbol_references
                                        .add_symbol_reference_to_symbol(
                                            calling_function,
                                            *name,
                                            true,
                                        );
                                }
                                Some(FunctionLikeIdentifier::Method(
                                    calling_classlike,
                                    calling_function,
                                )) => {
                                    analysis_data
                                        .symbol_references
                                        .add_class_member_reference_to_symbol(
                                            (calling_classlike, calling_function),
                                            *name,
                                            true,
                                        );
                                }
                                _ => {}
                            },

                            TAtomic::TEnumLiteralCase {
                                enum_name: name,
                                member_name,
                                ..
                            }
                            | TAtomic::TClosureAlias {
                                id: FunctionLikeIdentifier::Method(name, member_name),
                            } => match context.function_context.calling_functionlike_id {
                                Some(FunctionLikeIdentifier::Function(calling_function)) => {
                                    analysis_data
                                        .symbol_references
                                        .add_symbol_reference_to_class_member(
                                            calling_function,
                                            (*name, *member_name),
                                            true,
                                        );
                                }
                                Some(FunctionLikeIdentifier::Method(
                                    calling_classlike,
                                    calling_function,
                                )) => {
                                    analysis_data
                                        .symbol_references
                                        .add_class_member_reference_to_class_member(
                                            (calling_classlike, calling_function),
                                            (*name, *member_name),
                                            true,
                                        );
                                }
                                _ => {}
                            },
                            TAtomic::TClassTypeConstant {
                                class_type,
                                member_name,
                            } => match class_type.as_ref() {
                                TAtomic::TNamedObject { name, .. }
                                | TAtomic::TReference { name, .. } => {
                                    match context.function_context.calling_functionlike_id {
                                        Some(FunctionLikeIdentifier::Function(
                                            calling_function,
                                        )) => {
                                            analysis_data
                                                .symbol_references
                                                .add_symbol_reference_to_class_member(
                                                    calling_function,
                                                    (*name, *member_name),
                                                    true,
                                                );
                                        }
                                        Some(FunctionLikeIdentifier::Method(
                                            calling_classlike,
                                            calling_function,
                                        )) => {
                                            analysis_data
                                                .symbol_references
                                                .add_class_member_reference_to_class_member(
                                                    (calling_classlike, calling_function),
                                                    (*name, *member_name),
                                                    true,
                                                );
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        },
                        _ => {}
                    }
                }

                if param_type.is_mixed() {
                    param_type.clone()
                } else {
                    let mut param_type = param_type.clone();
                    let calling_class = context.function_context.calling_class.as_ref();

                    type_expander::expand_union(
                        self.file_analyzer.get_codebase(),
                        &Some(statements_analyzer.get_interner()),
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
                        &mut analysis_data.data_flow_graph,
                    );

                    for type_node in param_type.get_all_child_nodes() {
                        match type_node {
                            hakana_reflection_info::t_union::TypeNode::Atomic(
                                TAtomic::TReference { name, .. },
                            ) => {
                                analysis_data.add_issue(Issue::new(
                                    IssueKind::NonExistentClasslike,
                                    format!(
                                        "Class, enum or interface {} cannot be found",
                                        statements_analyzer.get_interner().lookup(name)
                                    ),
                                    if let Some(type_location) = &param.signature_type_location {
                                        type_location.clone()
                                    } else {
                                        param.name_location.clone()
                                    },
                                    &context.function_context.calling_functionlike_id,
                                ));

                                return Err(AnalysisError::UserError);
                            }
                            _ => {}
                        }
                    }

                    param_type
                }
            } else {
                get_mixed_any()
            };

            let param_node = if let Some(param_node) = params.get(i) {
                param_node
            } else {
                return Err(AnalysisError::InternalError(
                    "Param cannot be found".to_string(),
                    param.location,
                ));
            };

            if let Some(default) = &param_node.expr {
                expression_analyzer::analyze(
                    statements_analyzer,
                    default,
                    analysis_data,
                    context,
                    &mut None,
                )?;
            }

            if param.is_variadic {
                param_type = wrap_atomic(TAtomic::TVec {
                    known_items: None,
                    type_param: param_type,
                    known_count: None,
                    non_empty: false,
                });
            }

            let new_parent_node = if let GraphKind::WholeProgram(_) =
                &analysis_data.data_flow_graph.kind
            {
                DataFlowNode::get_for_assignment(param.name.clone(), param.name_location.clone())
            } else {
                let id = format!(
                    "{}-{}:{}-{}",
                    param.name,
                    interner.lookup(&param.name_location.file_path.0),
                    param.name_location.start_offset,
                    param.name_location.end_offset
                );

                DataFlowNode {
                    id,
                    kind: DataFlowNodeKind::VariableUseSource {
                        pos: param.name_location.clone(),
                        kind: if param.is_inout {
                            VariableSourceKind::InoutParam
                        } else if context.calling_closure_id.is_some() {
                            VariableSourceKind::ClosureParam
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
                        label: param.name.clone(),
                        pure: false,
                        has_awaitable: param_type.has_awaitable_types(),
                    },
                }
            };

            if !param.promoted_property {
                if analysis_data.data_flow_graph.kind == GraphKind::FunctionBody {
                    analysis_data
                        .data_flow_graph
                        .add_node(new_parent_node.clone());
                }
            }

            analysis_data
                .data_flow_graph
                .add_node(new_parent_node.clone());

            if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                let calling_id = if let Some(calling_closure_id) = context.calling_closure_id {
                    FunctionLikeIdentifier::Function(calling_closure_id)
                } else {
                    context
                        .function_context
                        .calling_functionlike_id
                        .clone()
                        .unwrap()
                };

                let argument_node = DataFlowNode::get_for_method_argument(
                    calling_id.to_string(&self.get_interner()),
                    i,
                    Some(param.name_location.clone()),
                    None,
                );

                analysis_data.data_flow_graph.add_path(
                    &argument_node,
                    &new_parent_node,
                    PathKind::Default,
                    None,
                    None,
                );

                analysis_data.data_flow_graph.add_node(argument_node);
            }

            param_type.parent_nodes.insert(new_parent_node);

            let config = statements_analyzer.get_config();

            for hook in &config.hooks {
                hook.handle_functionlike_param(
                    analysis_data,
                    FunctionLikeParamData {
                        context,
                        config,
                        param_type: &param_type,
                        param_node,
                        codebase: statements_analyzer.get_codebase(),
                        interner: statements_analyzer.get_interner(),
                    },
                );
            }

            context
                .vars_in_scope
                .insert(param.name.clone(), Rc::new(param_type.clone()));
        }

        Ok(())
    }
}

fn report_unused_expressions(
    analysis_data: &mut FunctionAnalysisData,
    config: &Config,
    fb_ast: &Vec<aast::Stmt<(), ()>>,
    statements_analyzer: &StatementsAnalyzer,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    functionlike_storage: &FunctionLikeInfo,
) {
    let unused_source_nodes = check_variables_used(&analysis_data.data_flow_graph);
    analysis_data.current_stmt_offset = None;

    let mut unused_variable_nodes = vec![];

    for node in &unused_source_nodes.0 {
        match &node.kind {
            DataFlowNodeKind::VariableUseSource {
                kind,
                label,
                pos,
                pure,
                has_awaitable,
            } => {
                if label.starts_with("$_") {
                    continue;
                }

                match &kind {
                    VariableSourceKind::Default => {
                        handle_unused_assignment(
                            config,
                            statements_analyzer,
                            pos,
                            &mut unused_variable_nodes,
                            node,
                            analysis_data,
                            label,
                            calling_functionlike_id,
                            pure,
                            has_awaitable,
                        );
                    }
                    _ => {}
                }
            }
            _ => {
                panic!()
            }
        };
    }

    for node in &unused_source_nodes.1 {
        match &node.kind {
            DataFlowNodeKind::VariableUseSource {
                kind,
                label,
                pos,
                has_awaitable,
                ..
            } => {
                if label.starts_with("$_") {
                    continue;
                }

                match &kind {
                    VariableSourceKind::PrivateParam => {
                        let pos = get_param_pos(functionlike_storage, label);

                        analysis_data.expr_fixme_positions.insert(
                            (pos.start_offset, pos.end_offset),
                            StmtStart {
                                offset: pos.start_offset,
                                line: pos.start_line,
                                column: pos.start_column,
                                add_newline: functionlike_storage.has_multi_line_params(),
                            },
                        );

                        analysis_data.maybe_add_issue(
                            Issue::new(
                                IssueKind::UnusedParameter,
                                "Unused param ".to_string() + label,
                                pos.clone(),
                                calling_functionlike_id,
                            ),
                            statements_analyzer.get_config(),
                            statements_analyzer.get_file_path_actual(),
                        );
                    }
                    VariableSourceKind::ClosureParam => {
                        if config
                            .issues_to_fix
                            .contains(&IssueKind::UnusedClosureParameter)
                            && !config.add_fixmes
                        {
                            if !analysis_data.add_replacement(
                                (pos.start_offset + 1, pos.start_offset + 1),
                                Replacement::Substitute("_".to_string()),
                            ) {
                                return;
                            }
                        } else {
                            analysis_data.maybe_add_issue(
                                Issue::new(
                                    IssueKind::UnusedClosureParameter,
                                    "Unused closure param ".to_string() + label,
                                    pos.clone(),
                                    calling_functionlike_id,
                                ),
                                statements_analyzer.get_config(),
                                statements_analyzer.get_file_path_actual(),
                            );
                        }
                    }
                    VariableSourceKind::NonPrivateParam => {
                        // todo register public/private param
                    }
                    VariableSourceKind::Default => {
                        handle_unused_assignment(
                            config,
                            statements_analyzer,
                            pos,
                            &mut unused_variable_nodes,
                            node,
                            analysis_data,
                            label,
                            calling_functionlike_id,
                            &false,
                            &has_awaitable,
                        );
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
            analysis_data,
            &unused_variable_nodes,
            statements_analyzer,
        )
    }
}

fn get_param_pos(functionlike_storage: &FunctionLikeInfo, label: &String) -> HPos {
    functionlike_storage
        .params
        .iter()
        .filter(|p| &p.name == label)
        .next()
        .unwrap()
        .location
}

fn handle_unused_assignment(
    config: &Config,
    statements_analyzer: &StatementsAnalyzer,
    pos: &HPos,
    unused_variable_nodes: &mut Vec<DataFlowNode>,
    node: &DataFlowNode,
    analysis_data: &mut FunctionAnalysisData,
    label: &String,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    pure: &bool,
    has_awaitable: &bool,
) {
    if config.allow_issue_kind_in_file(
        &IssueKind::UnusedAssignment,
        statements_analyzer.get_interner().lookup(&pos.file_path.0),
    ) {
        let unused_closure_variable =
            analysis_data
                .closure_spans
                .iter()
                .any(|(closure_start, closure_end)| {
                    &pos.start_offset > closure_start && &pos.start_offset < closure_end
                });

        if (config.issues_to_fix.contains(&IssueKind::UnusedAssignment)
            || (*pure
                && config
                    .issues_to_fix
                    .contains(&IssueKind::UnusedAssignmentStatement)))
            && !config.add_fixmes
            && !unused_closure_variable
        {
            unused_variable_nodes.push(node.clone());
        } else {
            analysis_data.maybe_add_issue(
                if label == "$$" {
                    Issue::new(
                        IssueKind::UnusedPipeVariable,
                        "The pipe data in this expression is not used anywhere".to_string(),
                        pos.clone(),
                        calling_functionlike_id,
                    )
                } else if unused_closure_variable {
                    Issue::new(
                        IssueKind::UnusedAssignmentInClosure,
                        format!("Assignment to {} is unused in this closure ", label),
                        pos.clone(),
                        calling_functionlike_id,
                    )
                } else {
                    if *pure {
                        Issue::new(
                            IssueKind::UnusedAssignmentStatement,
                            format!(
                                "Assignment to {} is unused, and this expression has no effect",
                                label
                            ),
                            pos.clone(),
                            calling_functionlike_id,
                        )
                    } else if *has_awaitable {
                        Issue::new(
                            IssueKind::UnusedAwaitable,
                            format!("Assignment to awaitable {} is unused", label),
                            pos.clone(),
                            calling_functionlike_id,
                        )
                    } else {
                        Issue::new(
                            IssueKind::UnusedAssignment,
                            format!("Assignment to {} is unused", label),
                            pos.clone(),
                            calling_functionlike_id,
                        )
                    }
                },
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }
}

pub(crate) fn update_analysis_result_with_tast(
    analysis_data: FunctionAnalysisData,
    analysis_result: &mut AnalysisResult,
    file_path: &FilePath,
    ignore_taint_path: bool,
) {
    if !analysis_data.replacements.is_empty() {
        analysis_result
            .replacements
            .entry(*file_path)
            .or_insert_with(BTreeMap::new)
            .extend(analysis_data.replacements);
    }

    let mut issues_to_emit = analysis_data.issues_to_emit;

    issues_to_emit.sort_by(|a, b| a.pos.start_offset.partial_cmp(&b.pos.start_offset).unwrap());

    analysis_result
        .emitted_issues
        .entry(*file_path)
        .or_insert_with(Vec::new)
        .extend(issues_to_emit.into_iter().unique().collect::<Vec<_>>());

    if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
        if !ignore_taint_path {
            analysis_result
                .program_dataflow_graph
                .add_graph(analysis_data.data_flow_graph);
        }
    } else {
        analysis_result
            .symbol_references
            .extend(analysis_data.symbol_references);

        for (source_id, c) in analysis_data.data_flow_graph.mixed_source_counts {
            if let Some(existing_count) = analysis_result.mixed_source_counts.get_mut(&source_id) {
                existing_count.extend(c);
            } else {
                analysis_result.mixed_source_counts.insert(source_id, c);
            }
        }

        for (kind, count) in analysis_data.issue_counts {
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

    fn get_interner(&self) -> &Interner {
        &self.file_analyzer.get_interner()
    }

    fn get_config(&self) -> &Config {
        self.file_analyzer.get_config()
    }
}

pub(crate) fn get_closure_storage(
    file_analyzer: &FileAnalyzer,
    offset: usize,
) -> Option<FunctionLikeInfo> {
    let file_storage = file_analyzer
        .codebase
        .files
        .get(&file_analyzer.get_file_source().file_path);

    if let Some(file_storage) = file_storage {
        file_storage.closure_infos.get(&offset).cloned()
    } else {
        return None;
    }
}
