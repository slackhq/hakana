use rustc_hash::FxHashMap;

use hakana_file_info::FileSource;
use oxidized::{aast, ast_defs};

use crate::expression_analyzer::get_class_id_classname;

// gets a var id from a simple variable
pub fn get_var_id(
    conditional: &aast::Expr<(), ()>,
    this_class_name: Option<&String>,
    source: &FileSource,
    resolved_names: &FxHashMap<usize, String>,
) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1 .1.clone()),
        aast::Expr_::ObjGet(boxed) => {
            if let ast_defs::PropOrMethod::IsProp = boxed.3 {
                if let Some(base_id) = get_var_id(&boxed.0, this_class_name, source, resolved_names)
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
                let class_name = get_class_id_classname(
                    &boxed.0,
                    &this_class_name.cloned(),
                    None,
                    resolved_names,
                );

                if let Some(class_name) = class_name {
                    if let aast::ClassGetExpr::CGstring(str) = &boxed.1 {
                        return Some(format!("{}::{}", class_name, str.1));
                    }
                }
            }

            None
        }
        aast::Expr_::ArrayGet(boxed) => {
            if let Some(base_id) = get_var_id(&boxed.0, this_class_name, source, resolved_names) {
                if let Some(dim) = &boxed.1 {
                    if let Some(dim_id) = get_dim_id(dim) {
                        return Some(format!("{}[{}]", base_id, dim_id));
                    } else if let Some(dim_id) =
                        get_var_id(dim, this_class_name, source, resolved_names)
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
    this_class_name: Option<&String>,
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
pub(crate) fn get_extended_var_id(
    conditional: &aast::Expr<(), ()>,
    this_class_name: Option<&String>,
    source: &FileSource,
    resolved_names: &FxHashMap<usize, String>,
) -> Option<String> {
    return get_var_id(conditional, this_class_name, source, resolved_names);
}

// gets a var id from variables but also array fetches
// and property fetches, which themselves can be nested
pub(crate) fn get_dim_id(conditional: &aast::Expr<(), ()>) -> Option<String> {
    match &conditional.2 {
        aast::Expr_::Lvar(var_expr) => Some(var_expr.1 .1.clone()),
        aast::Expr_::String(value) => Some(format!("'{}'", value.to_string())),
        aast::Expr_::Int(value) => Some(format!("{}", value.clone())),
        _ => None,
    }
}
