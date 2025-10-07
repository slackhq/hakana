use std::rc::Rc;

use hakana_code_info::{
    ExprId, VarId, ast::get_id_name, codebase_info::CodebaseInfo,
    functionlike_identifier::FunctionLikeIdentifier, t_atomic::TAtomic, t_union::TUnion,
};
use hakana_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

use oxidized::{aast, ast::PropOrMethod, ast_defs};

use crate::statements_analyzer::StatementsAnalyzer;

/** Gets the identifier for a simple variable */
pub fn get_var_id(
    conditional: &aast::Expr<(), ()>,
    this_class_name: Option<StrId>,
    resolved_names: &FxHashMap<u32, StrId>,
    codebase: Option<(&CodebaseInfo, &Interner)>,
) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1.1.clone()),
        aast::Expr_::ObjGet(boxed) => {
            if let ast_defs::PropOrMethod::IsProp = boxed.3 {
                if let Some(base_id) =
                    get_var_id(&boxed.0, this_class_name, resolved_names, codebase)
                {
                    if let aast::Expr_::Id(boxed) = &boxed.1.2 {
                        return Some(format!("{}->{}", base_id, boxed.1));
                    }
                }
            }

            None
        }
        aast::Expr_::ClassGet(boxed) => {
            if let ast_defs::PropOrMethod::IsProp = boxed.2 {
                let class_name = match &boxed.0.2 {
                    aast::ClassId_::CIexpr(inner_expr) => {
                        if let aast::Expr_::Id(id) = &inner_expr.2 {
                            if let Some((codebase, _)) = codebase {
                                get_id_name(
                                    id,
                                    &this_class_name,
                                    false,
                                    codebase,
                                    &mut false,
                                    resolved_names,
                                )
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(class_name) = class_name {
                    return match &boxed.1 {
                        aast::ClassGetExpr::CGstring(str) => Some(format!(
                            "{}::{}",
                            codebase.unwrap().1.lookup(&class_name),
                            str.1
                        )),
                        aast::ClassGetExpr::CGexpr(rhs_expr) => match &rhs_expr.2 {
                            aast::Expr_::Lvar(rhs_var_expr) => Some(format!(
                                "{}::${}",
                                codebase.unwrap().1.lookup(&class_name),
                                rhs_var_expr.1.1
                            )),
                            _ => None,
                        },
                    };
                }
            }

            None
        }
        aast::Expr_::ArrayGet(boxed) => {
            if let Some(base_id) = get_var_id(&boxed.0, this_class_name, resolved_names, codebase) {
                if let Some(dim) = &boxed.1 {
                    if let Some(dim_id) = get_dim_id(dim, codebase, resolved_names) {
                        return Some(format!("{}[{}]", base_id, dim_id));
                    } else if let Some(dim_id) =
                        get_var_id(dim, this_class_name, resolved_names, codebase)
                    {
                        if dim_id.contains('\'') {
                            return None;
                        }
                        return Some(format!("{}[{}]", base_id, dim_id));
                    }
                }
            }

            None
        }
        _ => None,
    }
}

/** Gets the beginning var identifier from a chain */
pub(crate) fn get_root_var_id(conditional: &aast::Expr<(), ()>) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1.1.clone()),
        aast::Expr_::ArrayGet(boxed) => get_root_var_id(&boxed.0),
        aast::Expr_::ObjGet(boxed) => get_root_var_id(&boxed.0),
        _ => None,
    }
}

/**
 * Gets a var identifier from variables but also array fetches
 * and property fetches, which themselves can be nested
 **/
pub(crate) fn get_dim_id(
    conditional: &aast::Expr<(), ()>,
    codebase: Option<(&CodebaseInfo, &Interner)>,
    resolved_names: &FxHashMap<u32, StrId>,
) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1.1.clone()),
        aast::Expr_::String(value) => Some(format!("'{}'", value)),
        aast::Expr_::Int(value) => Some(value.clone().to_string()),
        aast::Expr_::ClassConst(boxed) => {
            if let Some((codebase, interner)) = codebase {
                if let aast::ClassId_::CIexpr(lhs_expr) = &boxed.0.2 {
                    if let aast::Expr_::Id(id) = &lhs_expr.2 {
                        let mut is_static = false;
                        let classlike_name = match get_id_name(
                            id,
                            &None,
                            false,
                            codebase,
                            &mut is_static,
                            resolved_names,
                        ) {
                            Some(value) => value,
                            None => return None,
                        };

                        let constant_type = codebase.get_class_constant_type(
                            &classlike_name,
                            is_static,
                            &interner.get(&boxed.1.1)?,
                            FxHashSet::default(),
                        );

                        if let Some(constant_type) = constant_type {
                            if let Some(constant_type_string) =
                                constant_type.get_single_literal_string_value()
                            {
                                return Some(format!("'{}'", constant_type_string));
                            }
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub fn get_functionlike_id_from_call(
    call_expr: &oxidized::ast::CallExpr,
    interner: &Interner,
    resolved_names: &FxHashMap<u32, StrId>,
    expr_types: &FxHashMap<(u32, u32), Rc<TUnion>>,
) -> Option<FunctionLikeIdentifier> {
    get_static_functionlike_id_from_call(call_expr, interner, resolved_names)
        .or_else(|| get_method_id_from_call(call_expr, interner, expr_types))
}

pub fn get_static_functionlike_id_from_call(
    call: &oxidized::ast::CallExpr,
    interner: &Interner,
    resolved_names: &FxHashMap<u32, StrId>,
) -> Option<FunctionLikeIdentifier> {
    match &call.func.2 {
        aast::Expr_::Id(boxed_id) => {
            let name = if boxed_id.1 == "isset" {
                StrId::ISSET
            } else if boxed_id.1 == "\\in_array" {
                StrId::IN_ARRAY
            } else if let Some(resolved_name) =
                resolved_names.get(&(boxed_id.0.start_offset() as u32))
            {
                *resolved_name
            } else {
                return None;
            };

            Some(FunctionLikeIdentifier::Function(name))
        }
        aast::Expr_::ClassConst(boxed) => {
            let (class_id, rhs_expr) = (&boxed.0, &boxed.1);

            if let aast::ClassId_::CIexpr(lhs_expr) = &class_id.2 {
                if let aast::Expr_::Id(id) = &lhs_expr.2 {
                    if let (Some(class_name), Some(method_name)) = (
                        resolved_names.get(&(id.0.start_offset() as u32)),
                        interner.get(&rhs_expr.1),
                    ) {
                        Some(FunctionLikeIdentifier::Method(*class_name, method_name))
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
        _ => None,
    }
}

pub fn get_method_id_from_call(
    call_expr: &oxidized::ast::CallExpr,
    interner: &Interner,
    expr_types: &FxHashMap<(u32, u32), Rc<TUnion>>,
) -> Option<FunctionLikeIdentifier> {
    // Instance method call
    match &call_expr.func.2 {
        aast::Expr_::ObjGet(boxed) => {
            let (lhs_expr, rhs_expr, _nullfetch, prop_or_method) =
                (&boxed.0, &boxed.1, &boxed.2, &boxed.3);

            if *prop_or_method == PropOrMethod::IsProp {
                return None;
            }

            let class_id = if let Some(lhs_expr_type) = expr_types.get(&(
                lhs_expr.1.start_offset() as u32,
                lhs_expr.1.end_offset() as u32,
            )) {
                let t = lhs_expr_type.types.first().unwrap();
                if let TAtomic::TNamedObject { name, .. } = t {
                    name
                } else {
                    return None;
                }
            } else {
                return None;
            };

            if let aast::Expr_::Id(method_name_node) = &rhs_expr.2 {
                interner
                    .get(&method_name_node.1)
                    .map(|method_id| FunctionLikeIdentifier::Method(*class_id, method_id))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn get_expr_id(
    conditional: &aast::Expr<(), ()>,
    statements_analyzer: &StatementsAnalyzer,
) -> Option<ExprId> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => statements_analyzer
            .interner
            .get(&var_expr.1.1)
            .map(ExprId::Var),
        aast::Expr_::ObjGet(boxed) => {
            if let ast_defs::PropOrMethod::IsProp = boxed.3 {
                if let Some(ExprId::Var(base_id)) = get_expr_id(&boxed.0, statements_analyzer) {
                    if let aast::Expr_::Id(boxed) = &boxed.1.2 {
                        if let Some(prop_name) = statements_analyzer.interner.get(boxed.name()) {
                            return Some(ExprId::InstanceProperty(
                                VarId(base_id),
                                statements_analyzer.get_hpos(boxed.pos()),
                                prop_name,
                            ));
                        }
                    }
                }
            }

            None
        }

        _ => None,
    }
}
