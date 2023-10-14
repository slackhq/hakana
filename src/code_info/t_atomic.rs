use crate::functionlike_identifier::FunctionLikeIdentifier;
use crate::functionlike_parameter::FnParameter;
use crate::symbol_references::{ReferenceSource, SymbolReferences};
use crate::{
    codebase_info::{symbols::SymbolKind, Symbols},
    t_union::{populate_union_type, HasTypeNodes, TUnion, TypeNode},
};
use crate::{
    Interner, StrId, STR_ANY_ARRAY, STR_CONTAINER, STR_KEYED_CONTAINER, STR_KEYED_TRAVERSABLE,
    STR_LIB_REGEX_PATTERN, STR_PHP_INCOMPLETE_CLASS, STR_TRAVERSABLE,
};
use derivative::Derivative;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, PartialOrd, Ord, Hash)]
pub enum DictKey {
    Int(u32),
    String(String),
    Enum(StrId, StrId),
}

impl DictKey {
    pub fn to_string(&self, interner: Option<&Interner>) -> String {
        match &self {
            DictKey::Int(i) => i.to_string(),
            DictKey::String(k) => "'".to_string() + k.as_str() + "'",
            DictKey::Enum(c, m) => {
                if let Some(interner) = interner {
                    interner.lookup(c).to_string() + "::" + interner.lookup(m)
                } else {
                    c.0.to_string() + "::" + m.0.to_string().as_str()
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Derivative)]
#[derivative(Hash)]
pub enum TAtomic {
    TArraykey {
        from_any: bool,
    },
    TBool,
    TClassname {
        as_type: Box<self::TAtomic>,
    },
    TDict {
        known_items: Option<BTreeMap<DictKey, (bool, Arc<TUnion>)>>,
        params: Option<(Box<TUnion>, Box<TUnion>)>,
        non_empty: bool,
        shape_name: Option<(StrId, Option<StrId>)>,
    },
    TEnum {
        name: StrId,
        base_type: Option<Box<TAtomic>>,
    },
    TFalse,
    TFloat,
    TClosure {
        params: Vec<FnParameter>,
        return_type: Option<Box<TUnion>>,
        effects: Option<u8>,
        closure_id: StrId,
    },
    TClosureAlias {
        id: FunctionLikeIdentifier,
    },
    TInt,
    TKeyset {
        type_param: Box<TUnion>,
    },
    TLiteralClassname {
        name: StrId,
    },
    TEnumLiteralCase {
        enum_name: StrId,
        member_name: StrId,
        constraint_type: Option<Box<TAtomic>>,
    },
    TLiteralInt {
        value: i64,
    },
    TLiteralString {
        value: String,
    },
    TMixed,
    TMixedFromLoopIsset,
    // .0 => TMixedAny
    // .1 => TTruthyMixed
    // .2 => TFalsyMixed
    // .3 => TNonnullMixed
    TMixedWithFlags(bool, bool, bool, bool),
    TNamedObject {
        name: StrId,
        type_params: Option<Vec<TUnion>>,
        is_this: bool,
        extra_types: Option<Vec<TAtomic>>,
        remapped_params: bool,
    },
    TObject,
    TNothing,
    TNull,
    TNum,
    TReference {
        name: StrId,
        type_params: Option<Vec<TUnion>>,
    },
    TScalar,
    TString,
    // .0 => TTruthyString
    // .1 => TNonEmptyString
    // .2 => TNonspecificLiteralString
    TStringWithFlags(bool, bool, bool),
    TGenericParam {
        param_name: StrId,
        as_type: Box<TUnion>,
        defining_entity: StrId,
        from_class: bool,
        extra_types: Option<Vec<TAtomic>>,
    },
    TGenericClassname {
        param_name: StrId,
        defining_entity: StrId,
        as_type: Box<TAtomic>,
    },
    TGenericTypename {
        param_name: StrId,
        defining_entity: StrId,
        as_type: Box<TAtomic>,
    },
    TTypeVariable {
        name: String,
    },
    TTrue,
    TTypeAlias {
        name: StrId,
        type_params: Option<Vec<TUnion>>,
        as_type: Option<Box<TUnion>>,
    },
    TTypename {
        as_type: Box<TAtomic>,
    },
    TVec {
        known_items: Option<BTreeMap<usize, (bool, TUnion)>>,
        type_param: Box<TUnion>,
        known_count: Option<usize>,
        non_empty: bool,
    },
    TVoid,
    TPlaceholder,
    TClassTypeConstant {
        class_type: Box<TAtomic>,
        member_name: StrId,
    },
    TEnumClassLabel {
        class_name: Option<StrId>,
        member_name: StrId,
    },
    TResource,
}

impl TAtomic {
    pub fn get_id(&self, interner: Option<&Interner>) -> String {
        match self {
            TAtomic::TArraykey { .. } => "arraykey".to_string(),
            TAtomic::TBool { .. } => "bool".to_string(),
            TAtomic::TClassname { as_type, .. } => {
                let mut str = String::new();
                str += "classname<";
                str += (&*as_type).get_id(interner).as_str();
                str += ">";
                return str;
            }
            TAtomic::TTypename { as_type, .. } => {
                let mut str = String::new();
                str += "typename<";
                str += (&*as_type).get_id(interner).as_str();
                str += ">";
                return str;
            }
            TAtomic::TDict {
                params,
                known_items,
                shape_name,
                ..
            } => {
                if let Some(shape_name) = shape_name {
                    return if let Some(interner) = interner {
                        if let Some(shape_member_name) = &shape_name.1 {
                            format!(
                                "shape-from({}::{})",
                                interner.lookup(&shape_name.0),
                                interner.lookup(shape_member_name)
                            )
                        } else {
                            format!("shape-from({})", interner.lookup(&shape_name.0),)
                        }
                    } else {
                        if let Some(shape_member_name) = &shape_name.1 {
                            format!("shape-from({}::{})", shape_name.0 .0, shape_member_name.0)
                        } else {
                            format!("shape-from({})", shape_name.0 .0)
                        }
                    };
                }

                let mut str = String::new();

                if let Some(known_items) = known_items {
                    str += "shape(";

                    str += known_items
                        .into_iter()
                        .map(|(property, (u, property_type))| {
                            format!(
                                "{}{} => {}",
                                if *u { "?" } else { "" },
                                property.to_string(interner),
                                property_type.get_id(interner)
                            )
                        })
                        .join(", ")
                        .as_str();

                    if let Some(params) = params {
                        str += ", ...dict<";
                        str += params.0.get_id(interner).as_str();
                        str += ",";
                        str += params.1.get_id(interner).as_str();
                        str += ">";
                    }

                    str += ")";
                    return str;
                }

                if let Some(params) = params {
                    str += "dict<";
                    str += params.0.get_id(interner).as_str();
                    str += ",";
                    str += params.1.get_id(interner).as_str();
                    str += ">";
                    str
                } else {
                    "dict<nothing, nothing>".to_string()
                }
            }
            TAtomic::TEnum { name, .. } => {
                if let Some(interner) = interner {
                    interner.lookup(name).to_string()
                } else {
                    name.0.to_string()
                }
            }
            TAtomic::TFalse { .. } => "false".to_string(),
            TAtomic::TFloat { .. } => "float".to_string(),
            TAtomic::TClosure {
                params,
                return_type,
                ..
            } => {
                let mut str = String::new();
                str += "(function(";

                str += params
                    .iter()
                    .map(|param| {
                        format!(
                            "{}{}",
                            if let Some(param_type) = &param.signature_type {
                                param_type.get_id(interner)
                            } else {
                                "mixed".to_string()
                            },
                            if param.is_optional { "=" } else { "" }.to_string()
                        )
                    })
                    .join(", ")
                    .as_str();

                str += "): ";
                if let Some(return_type) = return_type {
                    str += return_type.get_id(interner).as_str();
                } else {
                    str += "mixed";
                }

                str += ")";

                str
            }
            TAtomic::TClosureAlias { id } => {
                let mut str = String::new();
                if let Some(interner) = interner {
                    str += id.to_string(interner).as_str();
                } else {
                    str += id.to_hash().as_str();
                }
                str += "<>";
                str
            }
            TAtomic::TInt { .. } => "int".to_string(),
            TAtomic::TObject => "object".to_string(),
            TAtomic::TKeyset { type_param, .. } => {
                let mut str = String::new();
                str += "keyset<";
                str += type_param.get_id(interner).as_str();
                str += ">";
                return str;
            }
            TAtomic::TLiteralClassname { name } => {
                let mut str = String::new();
                if let Some(interner) = interner {
                    str += interner.lookup(name);
                } else {
                    str += name.0.to_string().as_str();
                }
                str += "::class";
                return str;
            }
            TAtomic::TEnumLiteralCase {
                enum_name,
                member_name,
                ..
            } => {
                let mut str = String::new();
                if let Some(interner) = interner {
                    str += interner.lookup(enum_name);
                } else {
                    str += enum_name.0.to_string().as_str();
                }
                str += "::";
                if let Some(interner) = interner {
                    str += interner.lookup(member_name);
                } else {
                    str += member_name.0.to_string().as_str();
                }
                str
            }
            TAtomic::TLiteralInt { value } => {
                let mut str = String::new();
                str += "int(";
                str += value.to_string().as_str();
                str += ")";
                return str;
            }
            TAtomic::TLiteralString { value } => {
                let mut str = String::new();
                str += "string(";
                str += value.as_str();
                str += ")";
                return str;
            }
            TAtomic::TMixed | TAtomic::TMixedFromLoopIsset => "mixed".to_string(),
            TAtomic::TMixedWithFlags(is_any, is_truthy, is_falsy, is_nonnull) => {
                return if *is_any {
                    if *is_truthy {
                        "truthy-from-any"
                    } else if *is_falsy {
                        "falsy-from-any"
                    } else if *is_nonnull {
                        "nonnull-from-any"
                    } else {
                        "any"
                    }
                } else if *is_truthy {
                    "truthy-mixed"
                } else if *is_falsy {
                    "falsy-mixed"
                } else if *is_nonnull {
                    "nonnull"
                } else {
                    "mixed"
                }
                .to_string()
            }
            TAtomic::TNamedObject {
                name,
                type_params,
                is_this,
                extra_types,
                ..
            } => match type_params {
                None => format!(
                    "{}{}{}",
                    if let Some(interner) = interner {
                        interner.lookup(name).to_string()
                    } else {
                        name.0.to_string()
                    },
                    if *is_this { "&static" } else { "" },
                    if let Some(extra_types) = extra_types {
                        "&".to_string()
                            + extra_types
                                .iter()
                                .map(|atomic| atomic.get_id(interner))
                                .join("&")
                                .as_str()
                    } else {
                        "".to_string()
                    }
                ),
                Some(type_params) => {
                    let mut str = String::new();
                    if let Some(interner) = interner {
                        str += interner.lookup(name);
                    } else {
                        str += name.0.to_string().as_str();
                    }
                    if *is_this {
                        str += "&static";
                    }
                    str += "<";
                    str += type_params
                        .into_iter()
                        .map(|tunion| tunion.get_id(interner))
                        .join(", ")
                        .as_str();
                    str += ">";
                    return str;
                }
            },
            TAtomic::TTypeAlias {
                name, type_params, ..
            } => match type_params {
                None => {
                    let mut str = "type-alias(".to_string();
                    if let Some(interner) = interner {
                        str += interner.lookup(name);
                    } else {
                        str += name.0.to_string().as_str();
                    }
                    str += ")";
                    str
                }
                Some(type_params) => {
                    let mut str = String::new();
                    str += "type-alias(";
                    if let Some(interner) = interner {
                        str += interner.lookup(name);
                    } else {
                        str += name.0.to_string().as_str();
                    }
                    str += "<";
                    str += type_params
                        .into_iter()
                        .map(|tunion| tunion.get_id(interner))
                        .join(", ")
                        .as_str();
                    str += ">)";
                    return str;
                }
            },
            TAtomic::TNothing => "nothing".to_string(),
            TAtomic::TNull { .. } => "null".to_string(),
            TAtomic::TNum { .. } => "num".to_string(),
            TAtomic::TScalar => "scalar".to_string(),
            TAtomic::TString { .. } => "string".to_string(),
            TAtomic::TStringWithFlags(is_truthy, is_non_empty, is_nonspecific_literal) => {
                let mut str = String::new();

                if *is_truthy {
                    str += "truthy-"
                } else if *is_non_empty {
                    str += "non-empty-"
                }

                if *is_nonspecific_literal {
                    str += "literal-"
                }

                return str + "string";
            }
            TAtomic::TGenericParam {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                if let Some(interner) = interner {
                    str += interner.lookup(param_name);
                } else {
                    str += param_name.0.to_string().as_str();
                };
                str += ":";
                if let Some(interner) = interner {
                    str += interner.lookup(defining_entity);
                } else {
                    str += defining_entity.0.to_string().as_str();
                }
                return str;
            }
            TAtomic::TGenericClassname {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "classname<";
                if let Some(interner) = interner {
                    str += interner.lookup(param_name);
                } else {
                    str += param_name.0.to_string().as_str();
                }
                str += ":";
                if let Some(interner) = interner {
                    str += interner.lookup(defining_entity);
                } else {
                    str += defining_entity.0.to_string().as_str();
                }
                str += ">";
                return str;
            }
            TAtomic::TGenericTypename {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "typename<";
                if let Some(interner) = interner {
                    str += interner.lookup(param_name);
                } else {
                    str += param_name.0.to_string().as_str();
                }
                str += ":";
                if let Some(interner) = interner {
                    str += interner.lookup(defining_entity);
                } else {
                    str += defining_entity.0.to_string().as_str();
                }
                str += ">";
                return str;
            }
            TAtomic::TTrue { .. } => "true".to_string(),
            TAtomic::TVec {
                type_param,
                known_items,
                non_empty,
                ..
            } => {
                if let Some(known_items) = known_items {
                    let mut str = String::new();
                    str += "tuple(";
                    str += known_items
                        .into_iter()
                        .map(|(_, (_, tunion))| tunion.get_id(interner))
                        .join(", ")
                        .as_str();

                    if !type_param.is_nothing() {
                        str += ", ...vec<";
                        str += type_param.get_id(interner).as_str();
                        str += ">";
                    }

                    str += ")";
                    return str;
                }
                let mut str = String::new();
                str += if *non_empty { "non-empty-vec<" } else { "vec<" };
                str += type_param.get_id(interner).as_str();
                str += ">";
                return str;
            }
            TAtomic::TVoid => "void".to_string(),
            TAtomic::TReference { name, .. } => {
                let mut str = String::new();
                str += "unknown-ref(";
                if let Some(interner) = interner {
                    str += interner.lookup(name);
                } else {
                    str += name.0.to_string().as_str();
                }
                str += ")";
                return str;
            }
            TAtomic::TPlaceholder => "_".to_string(),
            TAtomic::TClassTypeConstant {
                class_type,
                member_name,
                ..
            } => {
                format!(
                    "{}::{}",
                    class_type.get_id(interner),
                    if let Some(interner) = interner {
                        interner.lookup(member_name).to_string()
                    } else {
                        member_name.0.to_string()
                    }
                )
            }
            TAtomic::TEnumClassLabel {
                class_name,
                member_name,
            } => {
                if let Some(class_name) = class_name {
                    if let Some(interner) = interner {
                        format!(
                            "#{}::{}",
                            interner.lookup(class_name),
                            interner.lookup(member_name)
                        )
                    } else {
                        format!("#{}::{}", class_name.0, member_name.0)
                    }
                } else {
                    if let Some(interner) = interner {
                        format!("#{}", interner.lookup(member_name))
                    } else {
                        format!("#{}", member_name.0)
                    }
                }
            }
            TAtomic::TResource => "resource".to_string(),
            TAtomic::TTypeVariable { name } => name.clone(),
        }
    }

    pub fn get_key(&self) -> String {
        match self {
            TAtomic::TDict { .. } => "dict".to_string(),
            TAtomic::TVec { .. } => "vec".to_string(),
            TAtomic::TKeyset { .. } => "keyset".to_string(),
            TAtomic::TClassname { as_type, .. } => {
                let mut str = String::new();
                str += "classname<";
                str += (&*as_type).get_key().as_str();
                str += ">";
                return str;
            }
            TAtomic::TTypename { as_type, .. } => {
                let mut str = String::new();
                str += "typename<";
                str += (&*as_type).get_key().as_str();
                str += ">";
                return str;
            }
            TAtomic::TFalse { .. }
            | TAtomic::TFloat { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TClosureAlias { .. }
            | TAtomic::TInt { .. }
            | TAtomic::TNothing
            | TAtomic::TNull { .. }
            | TAtomic::TNum { .. }
            | TAtomic::TMixed
            | TAtomic::TMixedFromLoopIsset
            | TAtomic::TString { .. }
            | TAtomic::TEnum { .. }
            | TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TEnumLiteralCase { .. }
            | TAtomic::TClassTypeConstant { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TVoid
            | TAtomic::TTrue { .. }
            | TAtomic::TObject
            | TAtomic::TScalar
            | TAtomic::TResource
            | TAtomic::TReference { .. }
            | TAtomic::TArraykey { .. }
            | TAtomic::TBool { .. }
            | TAtomic::TEnumClassLabel { .. }
            | TAtomic::TMixedWithFlags(..)
            | TAtomic::TTypeVariable { .. } => self.get_id(None),

            TAtomic::TStringWithFlags(..) => "string".to_string(),

            TAtomic::TNamedObject {
                name,
                type_params,
                extra_types,
                ..
            } => {
                let mut start = match type_params {
                    None => name.0.to_string(),
                    Some(type_params) => {
                        let mut str = String::new();
                        str += name.0.to_string().as_str();
                        str += "<";
                        str += type_params
                            .into_iter()
                            .map(|tunion| tunion.get_key())
                            .join(", ")
                            .as_str();
                        str += ">";
                        return str;
                    }
                };

                if let Some(extra_types) = extra_types {
                    start += "&";
                    start += &extra_types.iter().map(|a| a.get_key()).join("&");
                }

                start
            }

            TAtomic::TTypeAlias {
                name, type_params, ..
            } => match type_params {
                None => "type-alias(".to_string() + name.0.to_string().as_str() + ")",
                Some(type_params) => {
                    let mut str = String::new();
                    str += "type-alias(";
                    str += name.0.to_string().as_str();
                    str += "<";
                    str += type_params
                        .into_iter()
                        .map(|tunion| tunion.get_key())
                        .join(", ")
                        .as_str();
                    str += ">)";
                    return str;
                }
            },

            TAtomic::TGenericParam {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += param_name.0.to_string().as_str();
                str += ":";
                str += defining_entity.0.to_string().as_str();
                return str;
            }
            TAtomic::TGenericClassname {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "classname<";
                str += param_name.0.to_string().as_str();
                str += ":";
                str += defining_entity.0.to_string().as_str();
                str += ">";
                return str;
            }
            TAtomic::TGenericTypename {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "typename<";
                str += param_name.0.to_string().as_str();
                str += ":";
                str += defining_entity.0.to_string().as_str();
                str += ">";
                return str;
            }
            TAtomic::TPlaceholder => "_".to_string(),
        }
    }

    pub fn is_mixed(&self) -> bool {
        match self {
            TAtomic::TMixed | TAtomic::TMixedFromLoopIsset | TAtomic::TMixedWithFlags(..) => true,
            _ => false,
        }
    }

    pub fn is_mixed_with_any(&self, has_any: &mut bool) -> bool {
        match self {
            TAtomic::TMixed | TAtomic::TMixedFromLoopIsset => true,
            TAtomic::TMixedWithFlags(is_any, ..) => {
                *has_any = *is_any;
                true
            }
            _ => false,
        }
    }

    pub fn is_templated_as_mixed(&self, has_any: &mut bool) -> bool {
        match self {
            TAtomic::TGenericParam {
                as_type,
                extra_types: None,
                ..
            } => as_type.is_mixed_with_any(has_any),
            _ => false,
        }
    }

    pub fn is_object_type(&self) -> bool {
        match self {
            TAtomic::TObject { .. } => true,
            TAtomic::TClosure { .. } => true,
            TAtomic::TNamedObject { .. } => true,
            TAtomic::TGenericParam {
                as_type,
                extra_types: None,
                ..
            } => as_type.is_objecty(),
            _ => false,
        }
    }

    pub fn is_named_object(&self) -> bool {
        match self {
            TAtomic::TNamedObject { .. } => true,
            _ => false,
        }
    }

    pub fn is_templated_as_object(&self) -> bool {
        match self {
            TAtomic::TGenericParam {
                as_type,
                extra_types: None,
                ..
            } => as_type.is_objecty(),
            _ => false,
        }
    }

    pub fn is_vec(&self) -> bool {
        match self {
            TAtomic::TVec { .. } => true,
            _ => false,
        }
    }

    pub fn get_vec_param(&self) -> Option<&TUnion> {
        match self {
            TAtomic::TVec { type_param, .. } => Some(type_param),
            _ => None,
        }
    }

    pub fn is_non_empty_vec(&self) -> bool {
        match self {
            TAtomic::TVec { non_empty, .. } => *non_empty,
            _ => false,
        }
    }

    pub fn is_dict(&self) -> bool {
        match self {
            TAtomic::TDict { .. } => true,
            _ => false,
        }
    }

    pub fn is_non_empty_dict(&self) -> bool {
        match self {
            TAtomic::TDict { non_empty, .. } => *non_empty,
            _ => false,
        }
    }

    pub fn get_shape_name(&self) -> Option<&StrId> {
        match self {
            TAtomic::TDict {
                shape_name: Some((shape_name, None)),
                ..
            } => Some(shape_name),
            _ => None,
        }
    }

    #[inline]
    pub fn is_some_scalar(&self) -> bool {
        match self {
            TAtomic::TGenericClassname { .. }
            | TAtomic::TGenericTypename { .. }
            | TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TArraykey { .. }
            | TAtomic::TBool { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TTypename { .. }
            | TAtomic::TFalse { .. }
            | TAtomic::TFloat { .. }
            | TAtomic::TInt { .. }
            | TAtomic::TNum { .. }
            | TAtomic::TString { .. }
            | TAtomic::TStringWithFlags(..)
            | TAtomic::TTrue { .. }
            | TAtomic::TEnum { .. }
            | TAtomic::TEnumLiteralCase { .. } => true,

            _ => false,
        }
    }

    #[inline]
    pub fn is_boring_scalar(&self) -> bool {
        match self {
            TAtomic::TArraykey { .. }
            | TAtomic::TBool { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TTypename { .. }
            | TAtomic::TFalse { .. }
            | TAtomic::TFloat { .. }
            | TAtomic::TInt { .. }
            | TAtomic::TNum { .. }
            | TAtomic::TString { .. } => true,

            _ => false,
        }
    }

    #[inline]
    pub fn is_xhpchild_scalar_or_collection(&self) -> bool {
        if self.is_string()
            || self.is_int()
            || matches!(
                self,
                TAtomic::TFloat | TAtomic::TNum | TAtomic::TArraykey { .. }
            )
        {
            return true;
        }

        return false;
    }

    #[inline]
    pub fn is_string(&self) -> bool {
        match self {
            TAtomic::TString { .. }
            | TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TTypename { .. }
            | TAtomic::TGenericClassname { .. }
            | TAtomic::TGenericTypename { .. }
            | TAtomic::TStringWithFlags { .. } => true,

            _ => false,
        }
    }

    #[inline]
    pub fn is_string_subtype(&self) -> bool {
        match self {
            TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TTypename { .. }
            | TAtomic::TGenericClassname { .. }
            | TAtomic::TGenericTypename { .. }
            | TAtomic::TStringWithFlags { .. } => true,

            _ => false,
        }
    }

    #[inline]
    pub fn is_int(&self) -> bool {
        match self {
            TAtomic::TLiteralInt { .. } | TAtomic::TInt { .. } => true,

            _ => false,
        }
    }

    #[inline]
    pub fn is_bool(&self) -> bool {
        match self {
            TAtomic::TFalse { .. } | TAtomic::TTrue { .. } | TAtomic::TBool { .. } => true,

            _ => false,
        }
    }

    pub fn is_literal(&self) -> bool {
        match self {
            TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TEnumLiteralCase { .. }
            | TAtomic::TFalse { .. }
            | TAtomic::TTrue { .. }
            | TAtomic::TBool { .. }
            | TAtomic::TNull { .. } => true,
            _ => false,
        }
    }

    pub fn replace_template_extends(&self, new_as_type: TUnion) -> TAtomic {
        if let TAtomic::TGenericParam {
            param_name,
            defining_entity,
            extra_types,
            from_class,
            ..
        } = self
        {
            return TAtomic::TGenericParam {
                as_type: Box::new(new_as_type),
                param_name: param_name.clone(),
                defining_entity: defining_entity.clone(),
                extra_types: extra_types.clone(),
                from_class: from_class.clone(),
            };
        }

        panic!()
    }

    pub fn get_non_empty_vec(&self, known_count: Option<usize>) -> TAtomic {
        if let TAtomic::TVec {
            known_items,
            type_param,
            ..
        } = self
        {
            return TAtomic::TVec {
                known_items: known_items.clone(),
                type_param: type_param.clone(),
                known_count: known_count,
                non_empty: true,
            };
        }

        panic!()
    }

    pub fn make_non_empty_dict(mut self) -> TAtomic {
        if let TAtomic::TDict {
            ref mut non_empty, ..
        } = self
        {
            *non_empty = true;

            return self;
        }

        panic!()
    }

    pub fn is_truthy(&self) -> bool {
        match &self {
            &TAtomic::TTrue { .. }
            | &TAtomic::TMixedWithFlags(_, true, _, _)
            | &TAtomic::TStringWithFlags(true, _, _)
            | &TAtomic::TObject { .. }
            | &TAtomic::TClosure { .. }
            | &TAtomic::TLiteralClassname { .. }
            | &TAtomic::TClassname { .. }
            | &TAtomic::TTypename { .. } => true,
            &TAtomic::TNamedObject { name, .. } => match name {
                &STR_CONTAINER
                | &STR_KEYED_CONTAINER
                | &STR_ANY_ARRAY
                | &STR_TRAVERSABLE
                | &STR_KEYED_TRAVERSABLE => false,
                _ => true,
            },
            &TAtomic::TLiteralInt { value, .. } => {
                if *value != 0 {
                    return true;
                }
                false
            }
            &TAtomic::TLiteralString { value, .. } => {
                if value != "" && value != "0" {
                    return true;
                }
                false
            }
            &TAtomic::TDict {
                known_items,
                non_empty,
                ..
            } => {
                if *non_empty {
                    return true;
                }

                if let Some(known_items) = known_items {
                    for (_, (u, _)) in known_items {
                        if !u {
                            return true;
                        }
                    }
                }

                false
            }
            &TAtomic::TVec {
                known_items,
                non_empty,
                ..
            } => {
                if *non_empty {
                    return true;
                }

                if let Some(known_items) = known_items {
                    for (_, (possibly_undefined, _)) in known_items {
                        if !possibly_undefined {
                            return true;
                        }
                    }
                }

                false
            }
            _ => false,
        }
    }

    pub fn is_falsy(&self) -> bool {
        match &self {
            &TAtomic::TFalse { .. }
            | &TAtomic::TNull { .. }
            | &TAtomic::TMixedWithFlags(_, _, true, _) => true,
            &TAtomic::TLiteralInt { value, .. } => {
                if *value == 0 {
                    return true;
                }
                false
            }
            &TAtomic::TLiteralString { value, .. } => {
                if value == "" || value == "0" {
                    return true;
                }
                false
            }
            &TAtomic::TDict {
                known_items,
                non_empty,
                params,
                ..
            } => {
                if let None = known_items {
                    if params.is_none() && !non_empty {
                        return true;
                    }
                }

                false
            }
            &TAtomic::TVec {
                known_items,
                non_empty,
                type_param,
                ..
            } => {
                if let None = known_items {
                    if type_param.is_nothing() && !non_empty {
                        return true;
                    }
                }

                false
            }
            &TAtomic::TKeyset { type_param, .. } => {
                if type_param.is_nothing() {
                    return true;
                }

                false
            }
            _ => false,
        }
    }

    pub fn is_array_accessible_with_string_key(&self, interner: &Interner) -> bool {
        match self {
            TAtomic::TDict { .. } | TAtomic::TKeyset { .. } => true,
            TAtomic::TNamedObject { name, .. } => match interner.lookup(name) {
                "HH\\KeyedContainer" | "HH\\AnyArray" => true,
                _ => false,
            },
            _ => false,
        }
    }

    pub fn is_array_accessible_with_int_or_string_key(&self, interner: &Interner) -> bool {
        match self {
            TAtomic::TDict { .. } | TAtomic::TVec { .. } | TAtomic::TKeyset { .. } => true,
            TAtomic::TNamedObject { name, .. } => match interner.lookup(name) {
                "HH\\KeyedContainer" | "HH\\Container" | "HH\\AnyArray" => true,
                _ => false,
            },
            _ => false,
        }
    }

    #[inline]
    pub fn needs_population(&self) -> bool {
        match self {
            TAtomic::TGenericClassname { .. }
            | TAtomic::TGenericTypename { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TTypename { .. }
            | TAtomic::TDict { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TKeyset { .. }
            | TAtomic::TNamedObject { .. }
            | TAtomic::TVec { .. }
            | TAtomic::TReference { .. }
            | TAtomic::TClassTypeConstant { .. }
            | TAtomic::TGenericParam { .. } => true,
            _ => false,
        }
    }

    pub fn add_intersection_type(&mut self, atomic: TAtomic) {
        if let TAtomic::TNamedObject {
            ref mut extra_types,
            ..
        }
        | TAtomic::TGenericParam {
            ref mut extra_types,
            ..
        } = self
        {
            if let Some(extra_types) = extra_types {
                extra_types.push(atomic);
            } else {
                *extra_types = Some(vec![atomic]);
            }
        }
    }

    pub fn clone_without_intersection_types(&self) -> TAtomic {
        let mut clone = self.clone();

        if let TAtomic::TNamedObject {
            ref mut extra_types,
            ..
        }
        | TAtomic::TGenericParam {
            ref mut extra_types,
            ..
        } = clone
        {
            *extra_types = None
        }

        clone
    }

    pub fn get_intersection_types(&self) -> (Vec<&TAtomic>, Vec<TAtomic>) {
        match self {
            TAtomic::TNamedObject {
                extra_types: Some(extra_types),
                ..
            }
            | TAtomic::TGenericParam {
                extra_types: Some(extra_types),
                ..
            } => {
                let mut intersection_types = vec![];
                intersection_types.push(self);
                intersection_types.extend(extra_types);
                return (intersection_types, vec![]);
            }
            _ => {
                if let TAtomic::TGenericParam { as_type, .. } = self {
                    for as_atomic in &as_type.types {
                        // T1 as T2 as object becomes (T1 as object) & (T2 as object)
                        if let TAtomic::TGenericParam {
                            as_type: extends_as_type,
                            ..
                        } = as_atomic
                        {
                            let mut new_intersection_types = vec![];
                            let intersection_types = as_atomic.get_intersection_types();
                            new_intersection_types.extend(intersection_types.1);
                            let mut type_part = self.clone();
                            if let TAtomic::TGenericParam {
                                ref mut as_type, ..
                            } = type_part
                            {
                                *as_type = extends_as_type.clone();
                            }
                            new_intersection_types.push(type_part);

                            return (intersection_types.0, new_intersection_types);
                        }
                    }
                }

                return (vec![self], vec![]);
            }
        };
    }

    pub fn remove_placeholders(&mut self) {
        match self {
            TAtomic::TDict {
                params: Some(ref mut params),
                ..
            } => {
                if let TAtomic::TPlaceholder = params.0.get_single() {
                    params.0 = Box::new(TUnion::new(vec![TAtomic::TArraykey { from_any: true }]));
                }
                if let TAtomic::TPlaceholder = params.1.get_single() {
                    params.1 = Box::new(TUnion::new(vec![TAtomic::TMixedWithFlags(
                        true, false, false, false,
                    )]));
                }
            }
            TAtomic::TVec { type_param, .. } => {
                if let TAtomic::TPlaceholder = type_param.get_single() {
                    *type_param = Box::new(TUnion::new(vec![TAtomic::TMixedWithFlags(
                        true, false, false, false,
                    )]));
                }
            }
            TAtomic::TKeyset { ref mut type_param } => {
                if let TAtomic::TPlaceholder = type_param.get_single() {
                    *type_param =
                        Box::new(TUnion::new(vec![TAtomic::TArraykey { from_any: true }]));
                }
            }
            TAtomic::TNamedObject {
                ref mut name,
                ref mut type_params,
                ..
            } => {
                if let Some(type_params) = type_params {
                    if name == &STR_KEYED_CONTAINER
                        || name == &STR_ANY_ARRAY
                        || name == &STR_KEYED_TRAVERSABLE
                    {
                        if let Some(key_param) = type_params.get_mut(0) {
                            if let TAtomic::TPlaceholder = key_param.get_single() {
                                *key_param =
                                    TUnion::new(vec![TAtomic::TArraykey { from_any: true }]);
                            }
                        }

                        if let Some(value_param) = type_params.get_mut(1) {
                            if let TAtomic::TPlaceholder = value_param.get_single() {
                                *value_param = TUnion::new(vec![TAtomic::TMixedWithFlags(
                                    true, false, false, false,
                                )]);
                            }
                        }
                    } else if name == &STR_CONTAINER || name == &STR_TRAVERSABLE {
                        if let Some(value_param) = type_params.get_mut(0) {
                            if let TAtomic::TPlaceholder = value_param.get_single() {
                                *value_param = TUnion::new(vec![TAtomic::TMixedWithFlags(
                                    true, false, false, false,
                                )]);
                            }
                        }
                    } else {
                        for type_param in type_params {
                            if let TAtomic::TPlaceholder = type_param.get_single() {
                                *type_param =
                                    TUnion::new(vec![TAtomic::TArraykey { from_any: true }]);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn get_literal_string_value(&self) -> Option<String> {
        match self {
            TAtomic::TLiteralString { value, .. } => Some(value.clone()),
            TAtomic::TTypeAlias {
                name,
                as_type: Some(as_type),
                type_params: Some(_),
            } => {
                if name == &STR_LIB_REGEX_PATTERN {
                    if let TAtomic::TLiteralString { value, .. } = as_type.get_single() {
                        Some(value.clone())
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

    pub fn get_literal_int_value(&self) -> Option<i64> {
        match self {
            TAtomic::TLiteralInt { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn is_json_compatible(&self, banned_type_aliases: &Vec<StrId>) -> bool {
        if self.is_some_scalar() {
            return true;
        }

        if self.is_mixed() {
            // maybe in the future don't give up here
            return true;
        }

        match self {
            TAtomic::TNamedObject {
                name, type_params, ..
            } => {
                if let Some(type_params) = type_params {
                    if name == &STR_ANY_ARRAY || name == &STR_KEYED_CONTAINER {
                        return type_params[1].is_json_compatible(banned_type_aliases);
                    } else if name == &STR_CONTAINER {
                        return type_params[0].is_json_compatible(banned_type_aliases);
                    }
                }

                return false;
            }
            TAtomic::TNull => true,
            TAtomic::TNothing => true,
            TAtomic::TDict {
                known_items,
                params,
                shape_name,
                ..
            } => {
                if let Some((shape_name, None)) = shape_name {
                    if banned_type_aliases.contains(&shape_name) {
                        return false;
                    }
                }

                if let Some(params) = params {
                    if !params.1.is_json_compatible(banned_type_aliases) {
                        return false;
                    }
                }

                if let Some(known_items) = known_items {
                    for (_, (_, item_type)) in known_items {
                        if !item_type.is_json_compatible(banned_type_aliases) {
                            return false;
                        }
                    }
                }

                true
            }
            TAtomic::TKeyset { type_param } => type_param.is_json_compatible(banned_type_aliases),
            TAtomic::TVec {
                known_items,
                type_param,
                ..
            } => {
                if !type_param.is_json_compatible(banned_type_aliases) {
                    return false;
                }

                if let Some(known_items) = known_items {
                    for (_, (_, item_type)) in known_items {
                        if !item_type.is_json_compatible(banned_type_aliases) {
                            return false;
                        }
                    }
                }

                true
            }
            TAtomic::TTypeAlias {
                as_type: Some(as_type),
                ..
            } => as_type.is_json_compatible(banned_type_aliases),
            TAtomic::TGenericParam { as_type, .. } => {
                as_type.is_json_compatible(banned_type_aliases)
            }
            _ => false,
        }
    }
}

impl HasTypeNodes for TAtomic {
    fn get_child_nodes(&self) -> Vec<TypeNode> {
        match self {
            TAtomic::TDict {
                params,
                known_items,
                ..
            } => {
                let mut vec = vec![];

                if let Some(params) = params {
                    vec.push(TypeNode::Union(&params.0));
                    vec.push(TypeNode::Union(&params.1));
                }
                if let Some(known_items) = known_items {
                    for (_, (_, prop_type)) in known_items {
                        vec.push(TypeNode::Union(prop_type));
                    }
                }

                vec
            }
            TAtomic::TClosure {
                params,
                return_type,
                ..
            } => {
                let mut vec = vec![];
                if let Some(return_type) = return_type {
                    vec.push(TypeNode::Union(return_type));
                }

                for param in params {
                    if let Some(param_type) = &param.signature_type {
                        vec.push(TypeNode::Union(param_type));
                    }
                }

                vec
            }
            TAtomic::TKeyset { type_param, .. } => {
                vec![TypeNode::Union(type_param)]
            }
            TAtomic::TNamedObject { type_params, .. } => match type_params {
                None => vec![],
                Some(type_params) => {
                    let mut vec = vec![];
                    for type_param in type_params {
                        vec.push(TypeNode::Union(type_param));
                    }
                    vec
                }
            },
            TAtomic::TVec {
                type_param,
                known_items,
                ..
            } => {
                let mut vec = vec![TypeNode::Union(type_param)];
                if let Some(known_items) = known_items {
                    for (_, (_, prop_type)) in known_items {
                        vec.push(TypeNode::Union(prop_type));
                    }
                }

                vec
            }
            TAtomic::TReference { type_params, .. } => match type_params {
                None => vec![],
                Some(type_params) => {
                    let mut vec = vec![];
                    for type_param in type_params {
                        vec.push(TypeNode::Union(type_param));
                    }
                    vec
                }
            },
            TAtomic::TGenericParam { as_type, .. } => {
                vec![TypeNode::Union(as_type)]
            }
            TAtomic::TTypeAlias {
                type_params,
                as_type,
                ..
            } => {
                let mut nodes = vec![];

                match type_params {
                    None => {}
                    Some(type_params) => {
                        for type_param in type_params {
                            nodes.push(TypeNode::Union(type_param));
                        }
                    }
                };

                match as_type {
                    None => {}
                    Some(as_type) => {
                        nodes.push(TypeNode::Union(as_type));
                    }
                };

                nodes
            }
            TAtomic::TClassname { as_type } | TAtomic::TTypename { as_type } => {
                vec![TypeNode::Atomic(&as_type)]
            }
            _ => vec![],
        }
    }
}

pub fn populate_atomic_type(
    t_atomic: &mut self::TAtomic,
    codebase_symbols: &Symbols,
    reference_source: &ReferenceSource,
    symbol_references: &mut SymbolReferences,
    force: bool,
) {
    match t_atomic {
        TAtomic::TDict {
            ref mut params,
            ref mut known_items,
            ..
        } => {
            if let Some(params) = params {
                populate_union_type(
                    &mut params.0,
                    codebase_symbols,
                    reference_source,
                    symbol_references,
                    force,
                );
                populate_union_type(
                    &mut params.1,
                    codebase_symbols,
                    reference_source,
                    symbol_references,
                    force,
                );
            }

            if let Some(known_items) = known_items {
                for (_, (_, prop_type)) in known_items {
                    populate_union_type(
                        Arc::make_mut(prop_type),
                        codebase_symbols,
                        reference_source,
                        symbol_references,
                        force,
                    );
                }
            }
        }
        TAtomic::TClosure {
            ref mut params,
            ref mut return_type,
            ..
        } => {
            if let Some(return_type) = return_type {
                populate_union_type(
                    return_type,
                    codebase_symbols,
                    reference_source,
                    symbol_references,
                    force,
                );
            }

            for param in params {
                if let Some(ref mut param_type) = param.signature_type {
                    populate_union_type(
                        param_type,
                        codebase_symbols,
                        reference_source,
                        symbol_references,
                        force,
                    );
                }
            }
        }
        TAtomic::TKeyset {
            ref mut type_param, ..
        } => {
            populate_union_type(
                type_param,
                codebase_symbols,
                reference_source,
                symbol_references,
                force,
            );
        }
        TAtomic::TNamedObject {
            ref mut type_params,
            ..
        } => match type_params {
            None => {}
            Some(type_params) => {
                for type_param in type_params {
                    populate_union_type(
                        type_param,
                        codebase_symbols,
                        reference_source,
                        symbol_references,
                        force,
                    );
                }
            }
        },
        TAtomic::TTypeAlias {
            ref mut type_params,
            ..
        } => {
            if let Some(type_params) = type_params {
                for type_param in type_params {
                    populate_union_type(
                        type_param,
                        codebase_symbols,
                        reference_source,
                        symbol_references,
                        force,
                    );
                }
            }
        }
        TAtomic::TVec {
            ref mut type_param,
            ref mut known_items,
            ..
        } => {
            populate_union_type(
                type_param,
                codebase_symbols,
                reference_source,
                symbol_references,
                force,
            );

            if let Some(known_items) = known_items {
                for (_, (_, tuple_type)) in known_items {
                    populate_union_type(
                        tuple_type,
                        codebase_symbols,
                        reference_source,
                        symbol_references,
                        force,
                    );
                }
            }
        }
        TAtomic::TReference {
            ref name,
            ref mut type_params,
        } => {
            if let Some(type_params) = type_params {
                for type_param in type_params {
                    populate_union_type(
                        type_param,
                        codebase_symbols,
                        reference_source,
                        symbol_references,
                        force,
                    );
                }
            }

            match reference_source {
                ReferenceSource::Symbol(in_signature, a) => {
                    symbol_references.add_symbol_reference_to_symbol(*a, *name, *in_signature)
                }
                ReferenceSource::ClasslikeMember(in_signature, a, b) => symbol_references
                    .add_class_member_reference_to_symbol((*a, *b), *name, *in_signature),
            }

            if let Some(symbol_kind) = codebase_symbols.all.get(name) {
                match symbol_kind {
                    SymbolKind::Enum => {
                        *t_atomic = TAtomic::TEnum {
                            name: name.clone(),
                            base_type: None,
                        };
                        return;
                    }
                    SymbolKind::TypeDefinition => {
                        *t_atomic = TAtomic::TTypeAlias {
                            name: name.clone(),
                            type_params: type_params.clone(),
                            as_type: None,
                        };
                        return;
                    }
                    _ => {
                        *t_atomic = TAtomic::TNamedObject {
                            name: name.clone(),
                            type_params: type_params.clone(),
                            is_this: false,
                            extra_types: None,
                            remapped_params: false,
                        };
                        return;
                    }
                };
            } else {
                if *name == STR_PHP_INCOMPLETE_CLASS {
                    *t_atomic = TAtomic::TNamedObject {
                        name: *name,
                        type_params: None,
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    };
                    return;
                }
                // println!("Uknown symbol {}", name);
            }
        }
        TAtomic::TClassname { as_type } | TAtomic::TTypename { as_type } => {
            populate_atomic_type(
                as_type,
                codebase_symbols,
                reference_source,
                symbol_references,
                force,
            );
        }
        TAtomic::TClassTypeConstant { class_type, .. } => {
            populate_atomic_type(
                class_type,
                codebase_symbols,
                reference_source,
                symbol_references,
                force,
            );
        }
        TAtomic::TGenericParam {
            ref mut as_type, ..
        } => {
            populate_union_type(
                as_type,
                codebase_symbols,
                reference_source,
                symbol_references,
                force,
            );
        }
        _ => {}
    }
}
