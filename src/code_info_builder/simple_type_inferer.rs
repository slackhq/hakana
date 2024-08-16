use hakana_reflection_info::t_atomic::{DictKey, TAtomic};
use hakana_str::StrId;
use hakana_type::{get_nothing, wrap_atomic};
use oxidized::{aast, ast_defs};
use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, num::ParseIntError, sync::Arc};

pub fn infer(expr: &aast::Expr<(), ()>, resolved_names: &FxHashMap<u32, StrId>) -> Option<TAtomic> {
    return match &expr.2 {
        aast::Expr_::ArrayGet(_) => None,
        aast::Expr_::ClassConst(boxed) => match &boxed.0 .2 {
            aast::ClassId_::CIexpr(lhs_expr) => {
                if let aast::Expr_::Id(id) = &lhs_expr.2 {
                    match id.1.as_str() {
                        "self" | "parent" | "static" => None,
                        _ => {
                            let name_string =
                                *resolved_names.get(&(id.0.start_offset() as u32)).unwrap();

                            if boxed.1 .1 == "class" {
                                Some(TAtomic::TLiteralClassname { name: name_string })
                            } else {
                                Some(TAtomic::TMemberReference {
                                    classlike_name: name_string,
                                    member_name: *resolved_names
                                        .get(&(boxed.1 .0.start_offset() as u32))
                                        .unwrap(),
                                })
                            }
                        }
                    }
                } else {
                    None
                }
            }
            _ => {
                panic!()
            }
        },
        aast::Expr_::FunctionPointer(_) => None,
        aast::Expr_::Shape(shape_fields) => {
            let mut known_items = BTreeMap::new();

            for (shape_field_name, field_expr) in shape_fields {
                if let ast_defs::ShapeFieldName::SFlitStr((_, str)) = shape_field_name {
                    let field_type = infer(field_expr, resolved_names);

                    if let Some(field_type) = field_type {
                        known_items.insert(
                            DictKey::String(str.to_string()),
                            (false, Arc::new(wrap_atomic(field_type))),
                        );
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            Some(TAtomic::TDict {
                non_empty: !known_items.is_empty(),
                known_items: Some(known_items),
                params: None,
                shape_name: None,
            })
        }
        aast::Expr_::ValCollection(boxed) => {
            let mut entries = BTreeMap::new();

            for (i, entry_expr) in boxed.2.iter().enumerate() {
                let entry_type = infer(entry_expr, resolved_names);

                if let Some(entry_type) = entry_type {
                    entries.insert(i, (false, wrap_atomic(entry_type)));
                } else {
                    return None;
                }
            }

            match boxed.0 .1 {
                oxidized::tast::VcKind::Vec => Some(TAtomic::TVec {
                    known_count: Some(entries.len()),
                    known_items: Some(entries),
                    type_param: Box::new(get_nothing()),
                    non_empty: true,
                }),
                oxidized::tast::VcKind::Keyset => None,
                _ => panic!(),
            }
        }
        aast::Expr_::KeyValCollection(boxed) => {
            let mut known_items = BTreeMap::new();

            for entry_field in &boxed.2 {
                if let aast::Expr_::String(key_value) = &entry_field.0 .2 {
                    let value_type = infer(&entry_field.1, resolved_names);

                    if let Some(value_type) = value_type {
                        known_items.insert(
                            DictKey::String(key_value.to_string()),
                            (false, Arc::new(wrap_atomic(value_type))),
                        );
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            if known_items.len() < 100 {
                match boxed.0 .1 {
                    oxidized::tast::KvcKind::Dict => Some(TAtomic::TDict {
                        non_empty: !known_items.is_empty(),
                        known_items: Some(known_items),
                        params: None,
                        shape_name: None,
                    }),
                    _ => panic!(),
                }
            } else {
                None
            }
        }
        aast::Expr_::Null => Some(TAtomic::TNull),
        aast::Expr_::True => Some(TAtomic::TTrue),
        aast::Expr_::False => Some(TAtomic::TFalse),
        aast::Expr_::Int(value) => Some(TAtomic::TLiteralInt {
            value: int_from_string(value).unwrap(),
        }),
        aast::Expr_::Float(_) => Some(TAtomic::TFloat),
        aast::Expr_::String(value) => Some(if value.len() < 200 {
            TAtomic::TLiteralString {
                value: value.to_string(),
            }
        } else {
            TAtomic::TStringWithFlags(true, false, true)
        }),
        aast::Expr_::Tuple(values) => {
            let mut entries = BTreeMap::new();

            for (i, entry_expr) in values.iter().enumerate() {
                let entry_type = infer(entry_expr, resolved_names);

                if let Some(entry_type) = entry_type {
                    entries.insert(i, (false, wrap_atomic(entry_type)));
                } else {
                    return None;
                }
            }

            Some(TAtomic::TVec {
                known_count: Some(entries.len()),
                known_items: Some(entries),
                type_param: Box::new(get_nothing()),
                non_empty: true,
            })
        }
        aast::Expr_::Binop(boxed) => {
            if let ast_defs::Bop::Dot = boxed.bop {
                Some(TAtomic::TStringWithFlags(true, false, true))
            } else {
                None
            }
        }
        aast::Expr_::Unop(boxed) => {
            if let ast_defs::Uop::Uminus = boxed.0 {
                let number_type = infer(&boxed.1, resolved_names);

                if let Some(number_type) = number_type {
                    if let TAtomic::TLiteralInt { value, .. } = number_type {
                        Some(TAtomic::TLiteralInt { value: -value })
                    } else {
                        Some(number_type)
                    }
                } else {
                    None
                }
            } else if let ast_defs::Uop::Utild = boxed.0 {
                Some(TAtomic::TInt)
            } else {
                panic!()
            }
        }
        aast::Expr_::Id(name) => {
            if let Some(name_string) = resolved_names.get(&(name.0.start_offset() as u32)) {
                if *name_string == StrId::LIB_MATH_INT32_MAX {
                    return Some(TAtomic::TLiteralInt {
                        value: i32::MAX as i64,
                    });
                }
            }

            None
        }
        aast::Expr_::Eif(_) => None,
        aast::Expr_::New(..) => None,
        aast::Expr_::Omitted => None,
        _ => {
            println!("{:#?}", expr.2);
            panic!()
        }
    };
}

pub fn int_from_string(value: &str) -> Result<i64, ParseIntError> {
    if value.starts_with("0x") {
        i64::from_str_radix(value.trim_start_matches("0x"), 16)
    } else if value.starts_with("0b") {
        i64::from_str_radix(value.trim_start_matches("0b"), 2)
    } else {
        value.parse::<i64>()
    }
}
