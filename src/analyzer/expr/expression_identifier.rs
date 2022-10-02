use hakana_reflection_info::codebase_info::{symbols::Symbol, CodebaseInfo};
use rustc_hash::{FxHashMap, FxHashSet};

use hakana_file_info::FileSource;
use oxidized::{aast, ast_defs};

use super::fetch::class_constant_fetch_analyzer::get_id_name;

// gets a var id from a simple variable
pub fn get_var_id(
    conditional: &aast::Expr<(), ()>,
    this_class_name: Option<&Symbol>,
    source: &FileSource,
    resolved_names: &FxHashMap<usize, Symbol>,
    codebase: Option<&CodebaseInfo>,
) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1 .1.clone()),
        aast::Expr_::ObjGet(boxed) => {
            if let ast_defs::PropOrMethod::IsProp = boxed.3 {
                if let Some(base_id) =
                    get_var_id(&boxed.0, this_class_name, source, resolved_names, codebase)
                {
                    if let aast::Expr_::Id(boxed) = &boxed.1 .2 {
                        return Some(format!("{}->{}", base_id, boxed.1));
                    }
                }
            }

            None
        }
        aast::Expr_::ClassGet(boxed) => {
            if let ast_defs::PropOrMethod::IsProp = boxed.2 {
                let class_name = match &boxed.0 .2 {
                    aast::ClassId_::CIexpr(inner_expr) => {
                        if let aast::Expr_::Id(id) = &inner_expr.2 {
                            if let Some(codebase) = codebase {
                                get_id_name(
                                    id,
                                    &this_class_name.cloned(),
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
                        aast::ClassGetExpr::CGstring(str) => {
                            Some(format!("{}::{}", class_name, str.1))
                        }
                        aast::ClassGetExpr::CGexpr(rhs_expr) => match &rhs_expr.2 {
                            aast::Expr_::Lvar(rhs_var_expr) => {
                                Some(format!("{}::${}", class_name, rhs_var_expr.1 .1))
                            }
                            _ => None,
                        },
                    };
                }
            }

            None
        }
        aast::Expr_::ArrayGet(boxed) => {
            if let Some(base_id) =
                get_var_id(&boxed.0, this_class_name, source, resolved_names, codebase)
            {
                if let Some(dim) = &boxed.1 {
                    if let Some(dim_id) = get_dim_id(dim, codebase, resolved_names) {
                        return Some(format!("{}[{}]", base_id, dim_id));
                    } else if let Some(dim_id) =
                        get_var_id(dim, this_class_name, source, resolved_names, codebase)
                    {
                        if dim_id.contains("'") {
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

// gets a the beginning var id from a chain
pub(crate) fn get_root_var_id(
    conditional: &aast::Expr<(), ()>,
    this_class_name: Option<&Symbol>,
    source: Option<&FileSource>,
) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1 .1.clone()),
        aast::Expr_::ArrayGet(boxed) => get_root_var_id(&boxed.0, this_class_name, source),
        aast::Expr_::ObjGet(boxed) => get_root_var_id(&boxed.0, this_class_name, source),
        _ => None,
    }
}

// gets a var id from variables but also array fetches
// and property fetches, which themselves can be nested
pub(crate) fn get_dim_id(
    conditional: &aast::Expr<(), ()>,
    codebase: Option<&CodebaseInfo>,
    resolved_names: &FxHashMap<usize, Symbol>,
) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1 .1.clone()),
        aast::Expr_::String(value) => Some(format!("'{}'", value.to_string())),
        aast::Expr_::Int(value) => Some(format!("{}", value.clone())),
        aast::Expr_::ClassConst(boxed) => {
            if let Some(codebase) = codebase {
                match &boxed.0 .2 {
                    aast::ClassId_::CIexpr(lhs_expr) => {
                        if let aast::Expr_::Id(id) = &lhs_expr.2 {
                            let mut is_static = false;
                            let classlike_name = match get_id_name(
                                id,
                                &None,
                                codebase,
                                &mut is_static,
                                resolved_names,
                            ) {
                                Some(value) => value,
                                None => return None,
                            };

                            let constant_type = codebase.get_class_constant_type(
                                &classlike_name,
                                &boxed.1 .1,
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
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}
