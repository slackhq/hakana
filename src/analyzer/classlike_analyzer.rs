use crate::expression_analyzer;
use crate::file_analyzer::FileAnalyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::functionlike_analyzer::{update_analysis_result_with_tast, FunctionLikeAnalyzer};
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::codebase_info::symbols::SymbolKind;
use hakana_code_info::data_flow::graph::DataFlowGraph;
use hakana_code_info::function_context::{FunctionContext, FunctionLikeIdentifier};
use hakana_code_info::issue::IssueKind;
use hakana_code_info::{analysis_result::AnalysisResult, issue::Issue};
use hakana_str::StrId;
use oxidized::aast;
use rustc_hash::FxHashMap;

pub(crate) struct ClassLikeAnalyzer<'a> {
    file_analyzer: &'a FileAnalyzer<'a>,
}

impl<'a> ClassLikeAnalyzer<'a> {
    pub fn new(file_analyzer: &'a FileAnalyzer) -> Self {
        Self { file_analyzer }
    }

    pub fn analyze(
        &mut self,
        stmt: &aast::Class_<(), ()>,
        statements_analyzer: &StatementsAnalyzer,
        analysis_result: &mut AnalysisResult,
    ) -> Result<(), AnalysisError> {
        let resolved_names = self.file_analyzer.resolved_names.clone();
        let name =
            if let Some(resolved_name) = resolved_names.get(&(stmt.name.0.start_offset() as u32)) {
                *resolved_name
            } else {
                return Err(AnalysisError::InternalError(
                    format!("Cannot resolve class name {}", &stmt.name.1),
                    statements_analyzer.get_hpos(stmt.name.pos()),
                ));
            };

        let codebase = self.file_analyzer.codebase;

        if self.file_analyzer.analysis_config.ast_diff
            && self.file_analyzer.codebase.safe_symbols.contains(&name)
        {
            return Ok(());
        }

        let classlike_storage = if let Some(storage) = codebase.classlike_infos.get(&name) {
            storage
        } else {
            return Err(AnalysisError::InternalError(
                format!("Cannot get class storage for {}", &stmt.name.1),
                statements_analyzer.get_hpos(&stmt.name.0),
            ));
        };

        for parent_class in &classlike_storage.all_parent_classes {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name, *parent_class, true);
        }

        for parent_interface in &classlike_storage.all_parent_interfaces {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name, *parent_interface, true);
        }

        for trait_name in &classlike_storage.used_traits {
            analysis_result
                .symbol_references
                .add_symbol_reference_to_symbol(name, *trait_name, true);
        }

        let mut function_context = FunctionContext::new();
        function_context.calling_class = Some(name);
        function_context.calling_class_final = stmt.final_;

        let mut class_context = BlockContext::new(function_context);

        let mut analysis_data = FunctionAnalysisData::new(
            DataFlowGraph::new(statements_analyzer.get_config().graph_kind),
            statements_analyzer.file_analyzer.get_file_source(),
            &statements_analyzer.comments,
            &statements_analyzer.get_config().all_custom_issues,
            None,
            classlike_storage.meta_start.start_offset,
            None,
        );

        if let Some(issue_filter) = &statements_analyzer.get_config().allowed_issues {
            analysis_data.issue_filter = Some(issue_filter.clone());
        }

        if stmt.kind.is_cclass()
            && classlike_storage
                .direct_parent_class
                .map(|parent_class_name| {
                    codebase
                        .classlike_infos
                        .get(&parent_class_name)
                        .map(|parent_classlike_storage| parent_classlike_storage.is_final)
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::ExtendFinalClass,
                    "Cannot extend final class".to_string(),
                    classlike_storage.name_location,
                    &Some(FunctionLikeIdentifier::Function(name)),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }

        if stmt.kind.is_cclass()
            && !stmt.is_xhp
            && !classlike_storage.is_abstract
            && !classlike_storage.is_final
            && classlike_storage.child_classlikes.is_none()
            && function_context.is_production(codebase)
            && !classlike_storage
                .all_parent_classes
                .iter()
                .any(|c| c == &StrId::EXCEPTION)
        {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::MissingFinalOrAbstract,
                    "Class should always be declared abstract, final, or <<__Sealed>>".to_string(),
                    classlike_storage.name_location,
                    &Some(FunctionLikeIdentifier::Function(name)),
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }

        let mut existing_enum_str_values = FxHashMap::default();
        let mut existing_enum_int_values = FxHashMap::default();

        for constant in &stmt.consts {
            match &constant.kind {
                aast::ClassConstKind::CCAbstract(Some(expr))
                | aast::ClassConstKind::CCConcrete(expr) => {
                    expression_analyzer::analyze(
                        statements_analyzer,
                        expr,
                        &mut analysis_data,
                        &mut class_context,
                    )?;

                    if codebase.enum_exists(&name) {
                        if let (Some(expr_value), Some(constant_name)) = (
                            analysis_data.get_expr_type(expr.pos()),
                            resolved_names.get(&(constant.id.0.start_offset() as u32)),
                        ) {
                            if let Some(string_value) = expr_value.get_single_literal_string_value()
                            {
                                if let Some(existing_name) =
                                    existing_enum_str_values.get(&string_value)
                                {
                                    emit_dupe_enum_case_issue(
                                        &mut analysis_data,
                                        statements_analyzer,
                                        name,
                                        existing_name,
                                        expr,
                                    );
                                } else {
                                    existing_enum_str_values.insert(string_value, *constant_name);
                                }
                            } else if let Some(int_value) =
                                expr_value.get_single_literal_int_value()
                            {
                                if let Some(existing_name) =
                                    existing_enum_int_values.get(&int_value)
                                {
                                    emit_dupe_enum_case_issue(
                                        &mut analysis_data,
                                        statements_analyzer,
                                        name,
                                        existing_name,
                                        expr,
                                    );
                                } else {
                                    existing_enum_int_values.insert(int_value, *constant_name);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        for var in &stmt.vars {
            if let Some(default) = &var.expr {
                expression_analyzer::analyze(
                    statements_analyzer,
                    default,
                    &mut analysis_data,
                    &mut class_context,
                )?;
            }
        }

        update_analysis_result_with_tast(
            analysis_data,
            analysis_result,
            statements_analyzer.get_file_path(),
            false,
        );

        for method in &stmt.methods {
            if method.abstract_ || matches!(classlike_storage.kind, SymbolKind::Interface) {
                continue;
            }

            let mut method_analyzer = FunctionLikeAnalyzer::new(self.file_analyzer);
            method_analyzer.analyze_method(method, classlike_storage, analysis_result)?;
        }

        Ok(())
    }
}

fn emit_dupe_enum_case_issue(
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer<'_>,
    enum_name: StrId,
    existing_name: &hakana_str::StrId,
    expr: &aast::Expr<(), ()>,
) {
    analysis_data.maybe_add_issue(
        Issue::new(
            IssueKind::DuplicateEnumValue,
            format!(
                "Duplicate enum value for {}, previously defined by case {}",
                statements_analyzer.interner.lookup(&enum_name),
                statements_analyzer.interner.lookup(existing_name)
            ),
            statements_analyzer.get_hpos(expr.pos()),
            &Some(FunctionLikeIdentifier::Function(enum_name)),
        ),
        statements_analyzer.get_config(),
        statements_analyzer.get_file_path_actual(),
    );
}
