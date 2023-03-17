use crate::expression_analyzer;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::scope_context::ScopeContext;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use hakana_reflection_info::t_atomic::TAtomic;
use hakana_reflection_info::EFFECT_WRITE_PROPS;
use hakana_type::{get_mixed_any, get_named_object, wrap_atomic};
use oxidized::pos::Pos;
use oxidized::{aast, ast_defs};

use super::atomic_method_call_analyzer::AtomicMethodCallAnalysisResult;
use super::atomic_static_call_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::ClassId<(), ()>,
        &(Pos, String),
        &Vec<aast::Targ<()>>,
        &Vec<(ast_defs::ParamKind, aast::Expr<(), ()>)>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut ScopeContext,
    if_body_context: &mut Option<ScopeContext>,
) -> bool {
    //let method_id = None;

    let codebase = statements_analyzer.get_codebase();

    let lhs_type = match &expr.0 .2 {
        aast::ClassId_::CIexpr(lhs_expr) => {
            if let aast::Expr_::Id(id) = &lhs_expr.2 {
                let name_string = id.1.clone();
                match name_string.as_str() {
                    "self" => {
                        let self_name =
                            if let Some(calling_class) = &context.function_context.calling_class {
                                calling_class
                            } else {
                                return false;
                            };

                        get_named_object(self_name.clone())
                    }
                    "parent" => {
                        let self_name =
                            if let Some(calling_class) = &context.function_context.calling_class {
                                calling_class
                            } else {
                                return false;
                            };

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        get_named_object(
                            if let Some(parent_class) =
                                classlike_storage.direct_parent_class.clone()
                            {
                                parent_class
                            } else {
                                // todo handle for traits
                                return false;
                            },
                        )
                    }
                    "static" => {
                        let self_name =
                            if let Some(calling_class) = &context.function_context.calling_class {
                                calling_class
                            } else {
                                return false;
                            };

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        wrap_atomic(TAtomic::TNamedObject {
                            name: self_name.clone(),
                            type_params: None,
                            is_this: !classlike_storage.is_final,
                            extra_types: None,
                            remapped_params: false,
                        })
                    }
                    _ => {
                        let resolved_names = statements_analyzer.get_file_analyzer().resolved_names;

                        let name_string = resolved_names.get(&id.0.start_offset()).unwrap().clone();

                        get_named_object(name_string)
                    }
                }
            } else {
                let was_inside_general_use = context.inside_general_use;
                context.inside_general_use = true;
                expression_analyzer::analyze(
                    statements_analyzer,
                    lhs_expr,
                    analysis_data,
                    context,
                    if_body_context,
                );
                context.inside_general_use = was_inside_general_use;
                analysis_data
                    .get_expr_type(&lhs_expr.1)
                    .cloned()
                    .unwrap_or(get_mixed_any())
            }
        }
        _ => {
            panic!("cannot get here")
        }
    };

    let mut result = AtomicMethodCallAnalysisResult::new();

    for lhs_type_part in &lhs_type.types {
        atomic_static_call_analyzer::analyze(
            statements_analyzer,
            expr,
            pos,
            analysis_data,
            context,
            if_body_context,
            lhs_type_part,
            &mut result,
        );
    }

    if analysis_data
        .expr_effects
        .get(&(pos.start_offset(), pos.end_offset()))
        .unwrap_or(&0)
        >= &EFFECT_WRITE_PROPS
    {
        context.remove_mutable_object_vars();
    }

    analysis_data.set_expr_type(&pos, result.return_type.clone().unwrap_or(get_mixed_any()));

    true
}
