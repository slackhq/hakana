use rustc_hash::FxHashMap;

use crate::{codebase_info::CodebaseInfo, Interner, StrId};

pub fn get_id_name(
    id: &Box<oxidized::ast_defs::Id>,
    calling_class: &Option<StrId>,
    codebase: &CodebaseInfo,
    is_static: &mut bool,
    resolved_names: &FxHashMap<usize, StrId>,
) -> Option<StrId> {
    Some(match id.1.as_str() {
        "self" => {
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            self_name.clone()
        }
        "parent" => {
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();
            classlike_storage.direct_parent_class.clone().unwrap()
        }
        "static" => {
            *is_static = true;
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            self_name.clone()
        }
        _ => resolved_names.get(&id.0.start_offset()).unwrap().clone(),
    })
}

pub fn get_id_str_name<'a>(
    id: &'a str,
    calling_class: &Option<StrId>,
    codebase: &'a CodebaseInfo,
    interner: &'a Interner,
) -> Option<&'a str> {
    Some(match id {
        "self" => {
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            interner.lookup(self_name)
        }
        "parent" => {
            let self_name = if let Some(calling_class) = calling_class {
                calling_class
            } else {
                return None;
            };

            let classlike_storage = codebase.classlike_infos.get(self_name).unwrap();
            interner.lookup(&classlike_storage.direct_parent_class.clone().unwrap())
        }
        "static" => {
            return None;
        }
        _ => id,
    })
}
