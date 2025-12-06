use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope::control_action::ControlAction;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt_analyzer::AnalysisError;
use hakana_code_info::EFFECT_WRITE_PROPS;
use hakana_code_info::t_atomic::{TAtomic, TNamedObject};
use hakana_code_info::ttype::{get_mixed_any, get_named_object, wrap_atomic};
use hakana_str::StrId;
use oxidized::aast;
use oxidized::pos::Pos;

use super::atomic_method_call_analyzer::AtomicMethodCallAnalysisResult;
use super::atomic_static_call_analyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    expr: (
        &aast::ClassId<(), ()>,
        &(Pos, String),
        &Vec<aast::Targ<()>>,
        &Vec<aast::Argument<(), ()>>,
        &Option<aast::Expr<(), ()>>,
    ),
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let codebase = statements_analyzer.codebase;

    let mut classlike_name = None;

    let resolved_names = statements_analyzer.file_analyzer.resolved_names;

    let lhs_type = match &expr.0.2 {
        aast::ClassId_::CIexpr(lhs_expr) => {
            if let aast::Expr_::Id(id) = &lhs_expr.2 {
                let name = if let Some(name) = resolved_names.get(&(id.0.start_offset() as u32)) {
                    name
                } else {
                    return Err(AnalysisError::InternalError(
                        "Cannot resolve class name in static call".to_string(),
                        statements_analyzer.get_hpos(pos),
                    ));
                };
                match *name {
                    StrId::SELF => {
                        let self_name =
                            if let Some(calling_class) = &context.function_context.calling_class {
                                calling_class
                            } else {
                                return Err(AnalysisError::UserError);
                            };

                        classlike_name = Some(*self_name);

                        get_named_object(*self_name, None)
                    }
                    StrId::PARENT => {
                        let self_name =
                            if let Some(calling_class) = &context.function_context.calling_class {
                                calling_class
                            } else {
                                return Err(AnalysisError::UserError);
                            };

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        let parent_name =
                            if let Some(parent_class) = classlike_storage.direct_parent_class {
                                parent_class
                            } else {
                                // todo handle for traits
                                return Err(AnalysisError::UserError);
                            };

                        classlike_name = Some(parent_name);

                        let type_params = if let Some(type_params) = classlike_storage
                            .template_extended_offsets
                            .get(&parent_name)
                        {
                            Some(
                                type_params
                                    .iter()
                                    .map(|t| {
                                        let t = (**t).clone();

                                        t
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        } else {
                            None
                        };

                        wrap_atomic(TAtomic::TNamedObject(TNamedObject {
                            name: *self_name,
                            type_params: type_params,
                            is_this: !classlike_storage.is_final,
                            extra_types: None,
                            remapped_params: false,
                        }))
                    }
                    StrId::STATIC => {
                        let self_name =
                            if let Some(calling_class) = &context.function_context.calling_class {
                                calling_class
                            } else {
                                return Err(AnalysisError::UserError);
                            };

                        classlike_name = Some(*self_name);

                        let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();

                        wrap_atomic(TAtomic::TNamedObject(TNamedObject {
                            name: *self_name,
                            type_params: None,
                            is_this: !classlike_storage.is_final,
                            extra_types: None,
                            remapped_params: false,
                        }))
                    }
                    _ => {
                        let type_resolution_context =
                            statements_analyzer.get_type_resolution_context();

                        let lhs = get_named_object(*name, Some(type_resolution_context));

                        match lhs.get_single() {
                            TAtomic::TNamedObject(TNamedObject { name, .. }) => {
                                classlike_name = Some(*name);
                            }
                            TAtomic::TGenericClassname { as_type, .. }
                            | TAtomic::TGenericClassPtr { as_type, .. } => {
                                if let TAtomic::TNamedObject(TNamedObject { name, .. }) = &**as_type {
                                    classlike_name = Some(*name);
                                }
                            }
                            _ => (),
                        }

                        lhs
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
                    true,
                )?;
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
            lhs_type_part,
            classlike_name,
            &mut result,
        )?;
    }

    if analysis_data
        .expr_effects
        .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
        .unwrap_or(&0)
        >= &EFFECT_WRITE_PROPS
    {
        context.remove_mutable_object_vars();
    }

    if let Some(stmt_type) = result.return_type {
        if stmt_type.is_nothing() && !context.inside_loop {
            context.has_returned = true;
            context.control_actions.insert(ControlAction::End);
        }

        analysis_data.set_expr_type(pos, stmt_type);
    } else {
        analysis_data.set_expr_type(pos, get_mixed_any());
    }

    Ok(())
}
