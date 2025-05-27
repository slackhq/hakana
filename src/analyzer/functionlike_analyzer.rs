use crate::config::Config;
use crate::custom_hook::FunctionLikeParamData;
use crate::dataflow::unused_variable_analyzer::{
    add_unused_expression_replacements, check_variables_used,
};
use crate::expr::call_analyzer::reconcile_lower_bounds_with_upper_bounds;
use crate::expr::fetch::atomic_property_fetch_analyzer;
use crate::expression_analyzer;
use crate::file_analyzer::InternalError;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::return_analyzer::handle_inout_at_return;
use crate::stmt_analyzer::AnalysisError;
use crate::{file_analyzer::FileAnalyzer, function_analysis_data::FunctionAnalysisData};
use hakana_code_info::analysis_result::{AnalysisResult, Replacement};
use hakana_code_info::classlike_info::ClassLikeInfo;
use hakana_code_info::code_location::{FilePath, HPos, StmtStart};
use hakana_code_info::data_flow::graph::{DataFlowGraph, GraphKind};
use hakana_code_info::data_flow::node::{
    DataFlowNode, DataFlowNodeId, DataFlowNodeKind, VariableSourceKind,
};
use hakana_code_info::data_flow::path::PathKind;
use hakana_code_info::function_context::{FunctionContext, FunctionLikeIdentifier};
use hakana_code_info::functionlike_info::{FnEffect, FunctionLikeInfo};
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::member_visibility::MemberVisibility;
use hakana_code_info::method_identifier::MethodIdentifier;
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::comparison::type_comparison_result::TypeComparisonResult;
use hakana_code_info::ttype::type_expander::{self, StaticClassType, TypeExpansionOptions};
use hakana_code_info::ttype::{
    add_optional_union_type, comparison, get_mixed_any, get_nothing, get_void, wrap_atomic,
};
use hakana_code_info::var_name::VarName;
use hakana_str::{Interner, StrId};
use itertools::Itertools;
use oxidized::ast_defs::Pos;
use oxidized::{aast, tast};
use rustc_hash::FxHashSet;

use std::rc::Rc;

pub(crate) struct FunctionLikeAnalyzer<'a> {
    file_analyzer: &'a FileAnalyzer<'a>,
    interner: &'a Interner,
}

impl<'a> FunctionLikeAnalyzer<'a> {
    pub fn new(file_analyzer: &'a FileAnalyzer) -> Self {
        Self {
            file_analyzer,
            interner: file_analyzer.interner,
        }
    }

    pub fn analyze_fun(
        &mut self,
        stmt: &aast::FunDef<(), ()>,
        analysis_result: &mut AnalysisResult,
    ) -> Result<(), AnalysisError> {
        let resolved_names = self.file_analyzer.resolved_names.clone();

        let name = if let Some(name) = resolved_names.get(&(stmt.name.0.start_offset() as u32)) {
            *name
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot resolve function name".to_string(),
                HPos::new(stmt.name.pos(), self.file_analyzer.file_source.file_path),
            ));
        };

        if self.file_analyzer.analysis_config.ast_diff
            && self.file_analyzer.codebase.safe_symbols.contains(&name)
        {
            return Ok(());
        }

        let function_storage = if let Some(f) = self
            .file_analyzer
            .codebase
            .functionlike_infos
            .get(&(name, StrId::EMPTY))
        {
            f
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot load function storage".to_string(),
                HPos::new(stmt.name.pos(), self.file_analyzer.file_source.file_path),
            ));
        };

        let mut statements_analyzer = StatementsAnalyzer::new(
            self.file_analyzer,
            function_storage.type_resolution_context.as_ref().unwrap(),
            self.file_analyzer
                .file_source
                .comments
                .iter()
                .filter(|c| {
                    c.0.start_offset() > stmt.fun.span.start_offset()
                        && c.0.end_offset() < stmt.fun.span.end_offset()
                })
                .collect(),
        );

        statements_analyzer.set_function_info(function_storage);

        let mut function_context = FunctionContext::new();

        let functionlike_id = FunctionLikeIdentifier::Function(name);

        function_context.calling_functionlike_id = Some(functionlike_id);
        function_context.ignore_noreturn_calls = function_storage.ignore_noreturn_calls;

        self.analyze_functionlike(
            &mut statements_analyzer,
            functionlike_id,
            function_storage,
            BlockContext::new(function_context),
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
        mut context: BlockContext,
        analysis_data: &mut FunctionAnalysisData,
        analysis_result: &mut AnalysisResult,
        expr_pos: &Pos,
    ) -> Result<FunctionLikeInfo, AnalysisError> {
        let lambda_storage = analysis_data.closures.get(expr_pos).cloned();

        let mut lambda_storage = if let Some(lambda_storage) = lambda_storage {
            lambda_storage
        } else {
            match get_closure_storage(self.file_analyzer, stmt.span.start_offset()) {
                None => {
                    return Err(AnalysisError::InternalError(
                        "Cannot get closure storage".to_string(),
                        HPos::new(&stmt.span, self.file_analyzer.file_source.file_path),
                    ));
                }
                Some(value) => value,
            }
        };

        analysis_data.closure_spans.push((
            stmt.span.start_offset() as u32,
            stmt.span.end_offset() as u32,
        ));

        let mut statements_analyzer = StatementsAnalyzer::new(
            self.file_analyzer,
            lambda_storage.type_resolution_context.as_ref().unwrap(),
            self.file_analyzer
                .file_source
                .comments
                .iter()
                .filter(|c| {
                    c.0.start_offset() > stmt.span.start_offset()
                        && c.0.end_offset() < stmt.span.end_offset()
                })
                .collect(),
        );

        context.calling_closure_id = Some(expr_pos.start_offset() as u32);

        statements_analyzer.set_function_info(&lambda_storage);

        let file_path = *statements_analyzer.get_file_path();

        let (inferred_return_type, effects) = self.analyze_functionlike(
            &mut statements_analyzer,
            FunctionLikeIdentifier::Closure(file_path, expr_pos.start_offset() as u32),
            &lambda_storage,
            context,
            &stmt.params,
            &stmt.body.fb_ast.0,
            analysis_result,
            Some(analysis_data),
        )?;

        if if let Some(existing_return_type) = &lambda_storage.return_type {
            !existing_return_type.is_nothing()
        } else {
            true
        } {
            lambda_storage.return_type = Some(inferred_return_type.unwrap_or(get_mixed_any()));
            lambda_storage.effects = FnEffect::from_u8(&Some(effects));
        }

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

        let method_name = if let Some(method_name) = self.interner.get(&stmt.name.1) {
            method_name
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot resolve method name".to_string(),
                HPos::new(&stmt.name.0, self.file_analyzer.file_source.file_path),
            ));
        };

        let codebase = self.file_analyzer.codebase;

        if self.file_analyzer.analysis_config.ast_diff
            && codebase
                .safe_symbol_members
                .contains(&(classlike_storage.name, method_name))
        {
            return Ok(());
        }

        let functionlike_storage = if let Some(functionlike_storage) = codebase
            .functionlike_infos
            .get(&(classlike_storage.name, method_name))
        {
            functionlike_storage
        } else {
            return Err(AnalysisError::InternalError(
                "Cannot resolve function storage".to_string(),
                HPos::new(&stmt.name.0, self.file_analyzer.file_source.file_path),
            ));
        };

        let mut statements_analyzer = StatementsAnalyzer::new(
            self.file_analyzer,
            functionlike_storage
                .type_resolution_context
                .as_ref()
                .unwrap(),
            self.file_analyzer
                .file_source
                .comments
                .iter()
                .filter(|c| {
                    c.0.start_offset() > stmt.span.start_offset()
                        && c.0.end_offset() < stmt.span.end_offset()
                })
                .collect(),
        );

        let mut function_context = FunctionContext::new();
        let functionlike_id = FunctionLikeIdentifier::Method(classlike_storage.name, method_name);
        function_context.calling_functionlike_id = Some(functionlike_id);
        function_context.ignore_noreturn_calls = functionlike_storage.ignore_noreturn_calls;
        function_context.calling_class = Some(classlike_storage.name);
        function_context.calling_class_final = classlike_storage.is_final;

        let mut context = BlockContext::new(function_context);

        if !stmt.static_ {
            let mut this_type = wrap_atomic(TAtomic::TNamedObject {
                name: classlike_storage.name,
                type_params: if !classlike_storage.template_types.is_empty() {
                    Some(
                        classlike_storage
                            .template_types
                            .iter()
                            .map(|(param_name, template_map)| {
                                let first_map_entry = template_map.iter().next().unwrap();

                                wrap_atomic(TAtomic::TGenericParam {
                                    param_name: *param_name,
                                    as_type: Box::new((*first_map_entry.1).clone()),
                                    defining_entity: first_map_entry.0,
                                    extra_types: None,
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                },
                is_this: !classlike_storage.is_final,
                extra_types: None,
                remapped_params: false,
            });

            if let GraphKind::WholeProgram(_) = &analysis_result.program_dataflow_graph.kind {
                if classlike_storage.specialize_instance {
                    let new_call_node = DataFlowNode::get_for_this_before_method(
                        &MethodIdentifier(classlike_storage.name, method_name),
                        functionlike_storage.return_type_location,
                        None,
                    );

                    this_type.parent_nodes = vec![new_call_node];
                }
            }

            context
                .locals
                .insert(VarName::new("$this".to_string()), Rc::new(this_type));
        }

        statements_analyzer.set_function_info(functionlike_storage);

        self.analyze_functionlike(
            &mut statements_analyzer,
            functionlike_id,
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
        context: &mut BlockContext,
        cost: &mut u32,
    ) -> Result<(), InternalError> {
        let interner = &self.interner;
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
                            DataFlowNode::get_for_localized_property(
                                (classlike_storage.name, *property_name),
                                *property_pos,
                            ),
                            &(classlike_storage.name, *property_name),
                            analysis_data,
                            false,
                            property_type,
                        );
                }

                let calling_class = context.function_context.calling_class.as_ref().unwrap();

                type_expander::expand_union(
                    self.file_analyzer.codebase,
                    &Some(self.interner),
                    &self.file_analyzer.file_source.file_path,
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

                        ..Default::default()
                    },
                    &mut analysis_data.data_flow_graph,
                    cost,
                );

                context
                    .locals
                    .insert(VarName::new(expr_id), Rc::new(property_type));
            }
        }

        Ok(())
    }

    fn analyze_functionlike(
        &mut self,
        statements_analyzer: &mut StatementsAnalyzer,
        functionlike_id: FunctionLikeIdentifier,
        functionlike_storage: &FunctionLikeInfo,
        mut context: BlockContext,
        params: &[aast::FunParam<(), ()>],
        fb_ast: &Vec<aast::Stmt<(), ()>>,
        analysis_result: &mut AnalysisResult,
        parent_analysis_data: Option<&mut FunctionAnalysisData>,
    ) -> Result<(Option<TUnion>, u8), AnalysisError> {
        context.inside_async = functionlike_storage.is_async;

        statements_analyzer.in_migratable_function =
            if !self.file_analyzer.get_config().migration_symbols.is_empty() {
                if let Some(calling_functionlike_id) =
                    context.function_context.calling_functionlike_id
                {
                    self.file_analyzer
                        .get_config()
                        .migration_symbols
                        .contains_key(
                            &calling_functionlike_id.to_string(self.file_analyzer.interner),
                        )
                } else {
                    false
                }
            } else {
                false
            };

        let mut analysis_data = FunctionAnalysisData::new(
            DataFlowGraph::new(statements_analyzer.get_config().graph_kind),
            &statements_analyzer.file_analyzer.file_source,
            &statements_analyzer.comments,
            &self.get_config().all_custom_issues,
            if let Some(parent_analysis_data) = &parent_analysis_data {
                parent_analysis_data.current_stmt_offset
            } else {
                None
            },
            functionlike_storage.meta_start.start_offset,
            parent_analysis_data
                .as_ref()
                .map(|parent_analysis_data| parent_analysis_data.hakana_fixme_or_ignores.clone()),
        );

        if let Some(parent_analysis_data) = &parent_analysis_data {
            analysis_data
                .type_variable_bounds
                .clone_from(&parent_analysis_data.type_variable_bounds);

            if statements_analyzer.get_config().in_migration {
                analysis_data.data_flow_graph = parent_analysis_data.data_flow_graph.clone();
            }
        }

        if let Some(issue_filter) = &statements_analyzer.get_config().allowed_issues {
            analysis_data.issue_filter = Some(issue_filter.clone());
        }

        let mut completed_analysis = false;

        let mut cost = 0;

        match self.add_param_types_to_context(
            params,
            functionlike_storage,
            &mut analysis_data,
            &mut context,
            statements_analyzer,
            &mut cost,
        ) {
            Err(AnalysisError::InternalError(error, pos)) => {
                return Err(AnalysisError::InternalError(error, pos));
            }
            _ => {
                if let Some(calling_class) = &context.function_context.calling_class {
                    if let Some(classlike_storage) = self
                        .file_analyzer
                        .codebase
                        .classlike_infos
                        .get(calling_class)
                    {
                        if let Err(error) = self.add_properties_to_context(
                            classlike_storage,
                            &mut analysis_data,
                            functionlike_storage,
                            &mut context,
                            &mut cost,
                        ) {
                            return Err(AnalysisError::InternalError(error.0, error.1));
                        }
                    }
                }

                //let start_t = std::time::Instant::now();

                match statements_analyzer.analyze(
                    fb_ast,
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
                //                 functionlike_id.to_string(statements_analyzer.interner),
                //                 last_line - first_line,
                //                 end_t.as_micros() as u64 / (last_line - first_line)
                //             );
                //         }
                //     }
                // }

                if !context.has_returned {
                    handle_inout_at_return(
                        functionlike_storage,
                        &mut context,
                        &mut analysis_data,
                        self.interner,
                    );
                }
            }
        }

        if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
            if let Some(method_storage) = &functionlike_storage.method_info {
                if !method_storage.is_static {
                    if let Some(this_type) = context.locals.get("$this") {
                        let new_call_node = DataFlowNode::get_for_this_after_method(
                            &MethodIdentifier(
                                context.function_context.calling_class.unwrap(),
                                match functionlike_id {
                                    FunctionLikeIdentifier::Method(_, method_name) => method_name,
                                    _ => {
                                        panic!()
                                    }
                                },
                            ),
                            functionlike_storage.name_location,
                            None,
                        );

                        for parent_node in &this_type.parent_nodes {
                            analysis_data.data_flow_graph.add_path(
                                parent_node,
                                &new_call_node,
                                PathKind::Default,
                                vec![],
                                vec![],
                            );
                        }

                        analysis_data.data_flow_graph.add_node(new_call_node);
                    }
                }
            }
        }

        let config = statements_analyzer.get_config();

        if completed_analysis
            && config.find_unused_expressions
            && parent_analysis_data.is_none()
            && analysis_data
                .issue_counts
                .get(&IssueKind::UndefinedVariable)
                .unwrap_or(&0)
                == &0
            && analysis_data
                .issue_counts
                .get(&IssueKind::NonExistentClass)
                .unwrap_or(&0)
                == &0
            && analysis_data
                .issue_counts
                .get(&IssueKind::NonExistentMethod)
                .unwrap_or(&0)
                == &0
            && analysis_data
                .issue_counts
                .get(&IssueKind::NonExistentFunction)
                .unwrap_or(&0)
                == &0
        {
            report_unused_expressions(
                &mut analysis_data,
                config,
                fb_ast,
                statements_analyzer,
                &context.function_context.calling_functionlike_id,
                functionlike_storage,
            );
        }

        // Check for unnecessary service call attributes
        if completed_analysis && parent_analysis_data.is_none() {
            let mut all_expected_service_calls = functionlike_storage
                .transitive_service_calls
                .iter()
                .collect::<FxHashSet<_>>();

            all_expected_service_calls.retain(|c| !analysis_data.actual_service_calls.contains(*c));

            if !all_expected_service_calls.is_empty() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::UnnecessaryServiceCallsAttribute,
                        format!("This function expects to call services ({}) but no associated calls can be found", all_expected_service_calls.iter().join(", ")),
                        functionlike_storage.name_location.unwrap_or(functionlike_storage.def_location),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }

        if config.remove_fixmes && parent_analysis_data.is_none() {
            for unused_fixme_position in analysis_data.get_unused_hakana_fixme_positions() {
                analysis_data.add_replacement(
                    (unused_fixme_position.0, unused_fixme_position.1),
                    if unused_fixme_position.4 {
                        Replacement::TrimTrailingWhitespace(unused_fixme_position.3)
                    } else {
                        Replacement::TrimPrecedingWhitespace(unused_fixme_position.2)
                    },
                );
            }
        }

        let codebase = statements_analyzer.codebase;

        let mut inferred_return_type = None;

        if let Some(expected_return_type) = &functionlike_storage.return_type {
            let mut expected_return_type = expected_return_type.clone();
            type_expander::expand_union(
                statements_analyzer.codebase,
                &Some(statements_analyzer.interner),
                statements_analyzer.get_file_path(),
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

                    ..Default::default()
                },
                &mut analysis_data.data_flow_graph,
                &mut cost,
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
                        if comparison::union_type_comparator::is_contained_by(
                            codebase,
                            statements_analyzer.get_file_path(),
                            callsite_return_type,
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
                    let fn_return_value = if context.has_returned {
                        get_nothing()
                    } else {
                        get_void()
                    };
                    inferred_return_type = Some(if functionlike_storage.is_async {
                        wrap_atomic(TAtomic::TAwaitable {
                            value: Box::new(fn_return_value),
                        })
                    } else {
                        fn_return_value
                    });
                }

                if let Some(inferred_yield_type) = &analysis_data.inferred_yield_type {
                    inferred_return_type = Some(wrap_atomic(TAtomic::TNamedObject {
                        name: if functionlike_storage.is_async {
                            StrId::ASYNC_GENERATOR
                        } else {
                            StrId::GENERATOR
                        },
                        type_params: Some(vec![
                            wrap_atomic(TAtomic::TArraykey { from_any: true }),
                            inferred_yield_type.clone(),
                            get_nothing(),
                        ]),
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    }))
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
                        wrap_atomic(TAtomic::TAwaitable {
                            value: Box::new(get_void()),
                        })
                    } else {
                        get_void()
                    });
                }
            }
        }

        let mut effects = 0;

        if let FnEffect::Unknown = functionlike_storage.effects {
            for effect in analysis_data.expr_effects.values() {
                effects |= effect;
            }
        }

        if let Some(name_location) = functionlike_storage.name_location {
            if cost > 50_000 {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::LargeTypeExpansion,
                        format!("Very large type used — {} elements loaded", cost),
                        name_location,
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }
        }

        if let Some(parent_analysis_data) = parent_analysis_data {
            if !analysis_data.replacements.is_empty() {
                parent_analysis_data
                    .replacements
                    .extend(analysis_data.replacements);
            }

            if !analysis_data.insertions.is_empty() {
                parent_analysis_data
                    .insertions
                    .extend(analysis_data.insertions);
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

            parent_analysis_data
                .expr_effects
                .extend(analysis_data.expr_effects);

            // Copy service calls from child analysis to parent
            parent_analysis_data
                .actual_service_calls
                .extend(analysis_data.actual_service_calls);

            for (kind, count) in analysis_data.issue_counts {
                *parent_analysis_data.issue_counts.entry(kind).or_insert(0) += count;
            }

            if !matches!(parent_analysis_data.migrate_function, Some(false))
                && analysis_data.migrate_function.is_some()
            {
                parent_analysis_data.migrate_function = analysis_data.migrate_function;
            }

            if statements_analyzer.get_config().add_fixmes {
                parent_analysis_data
                    .expr_fixme_positions
                    .extend(analysis_data.expr_fixme_positions);
            }

            for (name, bounds) in analysis_data.type_variable_bounds {
                if let Some(existing_bounds) =
                    parent_analysis_data.type_variable_bounds.get_mut(&name)
                {
                    let existing_bounds_copy = existing_bounds.clone();
                    let filtered_lower_bounds = bounds
                        .0
                        .into_iter()
                        .filter(|bound| !existing_bounds_copy.0.contains(bound));
                    let filtered_upper_bounds = bounds
                        .1
                        .into_iter()
                        .filter(|bound| !existing_bounds_copy.1.contains(bound));

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

            if let Some(b) = analysis_data.migrate_function {
                analysis_result
                    .functions_to_migrate
                    .insert(context.function_context.calling_functionlike_id.unwrap(), b);
            }

            update_analysis_result_with_tast(
                analysis_data,
                analysis_result,
                &statements_analyzer.file_analyzer.file_source.file_path,
                functionlike_storage.ignore_taint_path,
            );
        }

        Ok((inferred_return_type, effects))
    }

    fn add_param_types_to_context(
        &mut self,
        params: &[aast::FunParam<(), ()>],
        functionlike_storage: &FunctionLikeInfo,
        analysis_data: &mut FunctionAnalysisData,
        context: &mut BlockContext,
        statements_analyzer: &mut StatementsAnalyzer,
        cost: &mut u32,
    ) -> Result<(), AnalysisError> {
        for (i, param) in functionlike_storage.params.iter().enumerate() {
            let mut param_type = if let Some(param_type) = &param.signature_type {
                add_symbol_references(
                    param_type,
                    context.function_context.calling_functionlike_id,
                    analysis_data,
                );

                if param_type.is_mixed() {
                    param_type.clone()
                } else {
                    let mut param_type = param_type.clone();
                    let calling_class = context.function_context.calling_class.as_ref();

                    type_expander::expand_union(
                        self.file_analyzer.codebase,
                        &Some(statements_analyzer.interner),
                        statements_analyzer.get_file_path(),
                        &mut param_type,
                        &TypeExpansionOptions {
                            self_class: calling_class,
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

                            ..Default::default()
                        },
                        &mut analysis_data.data_flow_graph,
                        cost,
                    );

                    for type_node in param_type.get_all_child_nodes() {
                        if let hakana_code_info::t_union::TypeNode::Atomic(TAtomic::TReference {
                            name,
                            ..
                        }) = type_node
                        {
                            analysis_data.add_issue(Issue::new(
                                IssueKind::NonExistentClasslike,
                                format!(
                                    "Class, enum or interface {} cannot be found",
                                    statements_analyzer.interner.lookup(name)
                                ),
                                if let Some(type_location) = &param.signature_type_location {
                                    *type_location
                                } else {
                                    param.name_location
                                },
                                &context.function_context.calling_functionlike_id,
                            ));

                            return Err(AnalysisError::UserError);
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

            if let tast::FunParamInfo::ParamOptional(Some(default)) = &param_node.info {
                expression_analyzer::analyze(statements_analyzer, default, analysis_data, context)?;
            }

            if param.is_variadic {
                param_type = wrap_atomic(TAtomic::TVec {
                    known_items: None,
                    type_param: Box::new(param_type),
                    known_count: None,
                    non_empty: false,
                });
            }

            let new_parent_node =
                if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                    DataFlowNode::get_for_lvar(param.name, param.name_location)
                } else {
                    DataFlowNode {
                        id: DataFlowNodeId::Param(
                            param.name,
                            param.name_location.file_path,
                            param.name_location.start_offset,
                            param.name_location.end_offset,
                        ),
                        kind: DataFlowNodeKind::VariableUseSource {
                            pos: param.name_location,
                            kind: if param.is_inout {
                                VariableSourceKind::InoutParam
                            } else if context.calling_closure_id.is_some() {
                                VariableSourceKind::ClosureParam
                            } else if let Some(method_storage) = &functionlike_storage.method_info {
                                match &method_storage.visibility {
                                    MemberVisibility::Public | MemberVisibility::Protected => {
                                        VariableSourceKind::NonPrivateParam
                                    }
                                    MemberVisibility::Private => VariableSourceKind::PrivateParam,
                                }
                            } else {
                                VariableSourceKind::PrivateParam
                            },
                            pure: false,
                            has_awaitable: param_type.has_awaitable_types(),
                            has_parent_nodes: true,
                            from_loop_init: false,
                        },
                    }
                };

            analysis_data
                .data_flow_graph
                .add_node(new_parent_node.clone());

            if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                let calling_id = if let Some(calling_closure_id) = context.calling_closure_id {
                    FunctionLikeIdentifier::Closure(
                        self.file_analyzer.file_source.file_path,
                        calling_closure_id,
                    )
                } else {
                    context.function_context.calling_functionlike_id.unwrap()
                };

                let argument_node = DataFlowNode::get_for_method_argument(
                    &calling_id,
                    i,
                    Some(param.name_location),
                    None,
                );

                analysis_data.data_flow_graph.add_path(
                    &argument_node,
                    &new_parent_node,
                    PathKind::Default,
                    vec![],
                    vec![],
                );

                analysis_data.data_flow_graph.add_node(argument_node);
            }

            param_type.parent_nodes.push(new_parent_node);

            let config = statements_analyzer.get_config();

            for hook in &config.hooks {
                hook.handle_functionlike_param(
                    analysis_data,
                    FunctionLikeParamData {
                        context,
                        config,
                        param_type: &param_type,
                        param_node,
                        codebase: statements_analyzer.codebase,
                        interner: statements_analyzer.interner,
                        in_migratable_function: statements_analyzer.in_migratable_function,
                    },
                );
            }

            context.locals.insert(
                VarName::new(
                    statements_analyzer
                        .interner
                        .lookup(&param.name.0)
                        .to_string(),
                ),
                Rc::new(param_type.clone()),
            );
        }

        Ok(())
    }
}

fn add_symbol_references(
    param_type: &TUnion,
    calling_functionlike_id: Option<FunctionLikeIdentifier>,
    analysis_data: &mut FunctionAnalysisData,
) {
    for type_node in param_type.get_all_child_nodes() {
        if let hakana_code_info::t_union::TypeNode::Atomic(atomic) = type_node {
            match atomic {
                TAtomic::TReference { name, .. }
                | TAtomic::TClosureAlias {
                    id: FunctionLikeIdentifier::Function(name),
                } => match calling_functionlike_id {
                    Some(FunctionLikeIdentifier::Function(calling_function)) => {
                        analysis_data
                            .symbol_references
                            .add_symbol_reference_to_symbol(calling_function, *name, true);
                    }
                    Some(FunctionLikeIdentifier::Method(calling_classlike, calling_function)) => {
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
                } => match calling_functionlike_id {
                    Some(FunctionLikeIdentifier::Function(calling_function)) => {
                        analysis_data
                            .symbol_references
                            .add_symbol_reference_to_class_member(
                                calling_function,
                                (*name, *member_name),
                                true,
                            );
                    }
                    Some(FunctionLikeIdentifier::Method(calling_classlike, calling_function)) => {
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
                    ..
                } => match class_type.as_ref() {
                    TAtomic::TNamedObject { name, .. } | TAtomic::TReference { name, .. } => {
                        match calling_functionlike_id {
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
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
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
    let unused_source_nodes =
        check_variables_used(&analysis_data.data_flow_graph, statements_analyzer.interner);
    analysis_data.current_stmt_offset = None;

    let mut unused_variable_nodes = vec![];

    let interner = statements_analyzer.interner;

    for node in &unused_source_nodes.0 {
        match &node.kind {
            DataFlowNodeKind::VariableUseSource {
                kind,
                pos,
                pure,
                has_awaitable,
                ..
            } => {
                if let DataFlowNodeId::Var(var_id, ..) | DataFlowNodeId::Param(var_id, ..) =
                    &node.id
                {
                    if interner.lookup(&var_id.0).starts_with("$_") {
                        continue;
                    }
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
                            calling_functionlike_id,
                            pure,
                            has_awaitable,
                            false,
                        );
                    }
                    _ => (),
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
                pos,
                has_awaitable,
                ..
            } => {
                if let DataFlowNodeId::Var(var_id, ..) | DataFlowNodeId::Param(var_id, ..) =
                    &node.id
                {
                    if interner.lookup(&var_id.0).starts_with("$_") {
                        continue;
                    }
                }

                match &kind {
                    VariableSourceKind::PrivateParam => {
                        let pos = get_param_pos(functionlike_storage, &node.id);

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
                                "Unused param ".to_string() + &node.id.to_label(interner),
                                pos,
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
                                    "Unused closure param ".to_string()
                                        + &node.id.to_label(interner),
                                    *pos,
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
                            calling_functionlike_id,
                            &false,
                            has_awaitable,
                            false,
                        );
                    }
                    VariableSourceKind::InoutArg => {
                        handle_unused_assignment(
                            config,
                            statements_analyzer,
                            pos,
                            &mut unused_variable_nodes,
                            node,
                            analysis_data,
                            calling_functionlike_id,
                            &false,
                            has_awaitable,
                            true,
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

fn get_param_pos(functionlike_storage: &FunctionLikeInfo, id: &DataFlowNodeId) -> HPos {
    if let DataFlowNodeId::Param(var_id, ..) = id {
        functionlike_storage
            .params
            .iter()
            .find(|p| &p.name == var_id)
            .unwrap()
            .location
    } else {
        panic!()
    }
}

fn handle_unused_assignment(
    config: &Config,
    statements_analyzer: &StatementsAnalyzer,
    pos: &HPos,
    unused_variable_nodes: &mut Vec<DataFlowNode>,
    node: &DataFlowNode,
    analysis_data: &mut FunctionAnalysisData,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    pure: &bool,
    has_awaitable: &bool,
    from_inout: bool,
) {
    if config.allow_issue_kind_in_file(
        &IssueKind::UnusedAssignment,
        statements_analyzer.interner.lookup(&pos.file_path.0),
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
        {
            unused_variable_nodes.push(node.clone());
        } else {
            let interner = statements_analyzer.interner;
            analysis_data.maybe_add_issue(
                if node.id.to_label(interner) == "$$" {
                    Issue::new(
                        IssueKind::UnusedPipeVariable,
                        "The pipe data in this expression is not used anywhere".to_string(),
                        *pos,
                        calling_functionlike_id,
                    )
                } else if from_inout {
                    Issue::new(
                        IssueKind::UnusedInoutAssignment,
                        format!(
                            "Assignment to {} from inout argument is unused",
                            node.id.to_label(interner),
                        ),
                        *pos,
                        calling_functionlike_id,
                    )
                } else if unused_closure_variable {
                    Issue::new(
                        IssueKind::UnusedAssignmentInClosure,
                        format!(
                            "Assignment to {} is unused in this closure ",
                            node.id.to_label(interner),
                        ),
                        *pos,
                        calling_functionlike_id,
                    )
                } else if *pure {
                    Issue::new(
                        IssueKind::UnusedAssignmentStatement,
                        format!(
                            "Assignment to {} is unused, and this expression has no effect",
                            node.id.to_label(interner),
                        ),
                        *pos,
                        calling_functionlike_id,
                    )
                } else if *has_awaitable {
                    Issue::new(
                        IssueKind::UnusedAwaitable,
                        format!(
                            "Assignment to awaitable {} is unused",
                            node.id.to_label(interner)
                        ),
                        *pos,
                        calling_functionlike_id,
                    )
                } else {
                    Issue::new(
                        IssueKind::UnusedAssignment,
                        format!("Assignment to {} is unused", node.id.to_label(interner),),
                        *pos,
                        calling_functionlike_id,
                    )
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
            .or_default()
            .extend(analysis_data.replacements);
    }

    if !analysis_data.insertions.is_empty() {
        let file_insertions = analysis_result.insertions.entry(*file_path).or_default();

        for (offset, insertions) in analysis_data.insertions {
            file_insertions
                .entry(offset)
                .or_default()
                .extend(insertions);
        }
    }

    let mut issues_to_emit = analysis_data.issues_to_emit;

    issues_to_emit.sort_by(|a, b| a.pos.start_offset.partial_cmp(&b.pos.start_offset).unwrap());

    analysis_result
        .emitted_issues
        .entry(*file_path)
        .or_default()
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

    fn get_config(&self) -> &Config {
        self.file_analyzer.get_config()
    }
}

pub(crate) fn get_closure_storage(
    file_analyzer: &FileAnalyzer,
    offset: usize,
) -> Option<FunctionLikeInfo> {
    file_analyzer
        .codebase
        .functionlike_infos
        .get(&(file_analyzer.file_source.file_path.0, StrId(offset as u32)))
        .cloned()
}
