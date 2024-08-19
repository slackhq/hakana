use crate::classlike_analyzer::ClassLikeAnalyzer;
use crate::custom_hook::AfterDefAnalysisData;
use crate::file_analyzer::InternalError;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::functionlike_analyzer::FunctionLikeAnalyzer;
use crate::scope::loop_scope::LoopScope;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use crate::{expression_analyzer, stmt_analyzer};
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::function_context::FunctionContext;
use hakana_code_info::issue::{Issue, IssueKind};
use oxidized::aast;

pub(crate) fn analyze(
    scope_analyzer: &mut dyn ScopeAnalyzer,
    statements_analyzer: &StatementsAnalyzer,
    def: &aast::Def<(), ()>,
    context: &mut BlockContext,
    loop_scope: &mut Option<LoopScope>,
    analysis_data: &mut FunctionAnalysisData,
    analysis_result: &mut AnalysisResult,
) -> Result<(), InternalError> {
    if statements_analyzer.get_config().in_codegen
        && !statements_analyzer.get_config().hooks.iter().any(|hook| {
            hook.can_codegen_def(
                statements_analyzer.get_interner(),
                statements_analyzer.get_codebase(),
                statements_analyzer.get_file_analyzer().resolved_names,
                def,
            )
        })
    {
        return Ok(());
    }

    match def {
        aast::Def::Fun(_) => {
            let file_analyzer = scope_analyzer.get_file_analyzer();
            let mut function_analyzer = FunctionLikeAnalyzer::new(file_analyzer);
            if let Err(AnalysisError::InternalError(error, pos)) =
                function_analyzer.analyze_fun(def.as_fun().unwrap(), analysis_result)
            {
                return Err(InternalError(error, pos));
            }
        }
        aast::Def::Class(boxed) => {
            let file_analyzer = scope_analyzer.get_file_analyzer();
            let mut class_analyzer = ClassLikeAnalyzer::new(file_analyzer);
            if let Err(AnalysisError::InternalError(error, pos)) =
                class_analyzer.analyze(boxed, statements_analyzer, analysis_result)
            {
                return Err(InternalError(error, pos));
            }
        }
        aast::Def::Typedef(_) | aast::Def::NamespaceUse(_) => {
            // already handled
        }
        aast::Def::Stmt(boxed) => {
            if let Err(AnalysisError::InternalError(error, pos)) = stmt_analyzer::analyze(
                statements_analyzer,
                boxed,
                analysis_data,
                context,
                loop_scope,
            ) {
                return Err(InternalError(error, pos));
            }
        }
        aast::Def::Constant(boxed) => {
            let mut function_context = FunctionContext::new();
            function_context.calling_class = Some(
                if let Some(resolved_name) = statements_analyzer
                    .get_file_analyzer()
                    .resolved_names
                    .get(&(boxed.name.pos().start_offset() as u32))
                {
                    *resolved_name
                } else {
                    return Err(InternalError(
                        "Could not resolve constant name".to_string(),
                        statements_analyzer.get_hpos(boxed.name.pos()),
                    ));
                },
            );

            let mut context = BlockContext::new(function_context);

            if let Err(AnalysisError::InternalError(error, pos)) = expression_analyzer::analyze(
                statements_analyzer,
                &boxed.value,
                analysis_data,
                &mut context,
            ) {
                return Err(InternalError(error, pos));
            }
        }
        aast::Def::Namespace(_) => {
            // already handled?
        }
        aast::Def::SetNamespaceEnv(_) => {
            // maybe unnecessary
        }
        aast::Def::FileAttributes(_) => {
            // not sure
        }
        aast::Def::Module(boxed) => {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::UnrecognizedStatement,
                    "Unrecognized statement".to_string(),
                    statements_analyzer.get_hpos(&boxed.span),
                    &None,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
        aast::Def::SetModule(boxed) => {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::UnrecognizedStatement,
                    "Unrecognized statement".to_string(),
                    statements_analyzer.get_hpos(boxed.pos()),
                    &None,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }

    for hook in &statements_analyzer.get_config().hooks {
        hook.after_def_analysis(
            analysis_data,
            analysis_result,
            AfterDefAnalysisData {
                statements_analyzer,
                def,
                context,
            },
        );
    }

    Ok(())
}
