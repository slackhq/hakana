use crate::{
    codebase_info::{symbols::SymbolKind, CodebaseInfo, Symbols},
    functionlike_parameter::FunctionLikeParameter,
    t_union::{populate_union_type, HasTypeNodes, TUnion, TypeNode},
};
use function_context::functionlike_identifier::FunctionLikeIdentifier;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
pub enum TAtomic {
    TArraykey {
        from_any: bool,
    },
    TBool,
    TClassname {
        as_type: Box<self::TAtomic>,
    },
    TDict {
        known_items: Option<BTreeMap<String, (bool, Arc<TUnion>)>>,
        enum_items: Option<BTreeMap<(String, String), (bool, TUnion)>>,
        key_param: TUnion,
        value_param: TUnion,
        non_empty: bool,
        shape_name: Option<String>,
    },
    TEnum {
        name: String,
    },
    TFalsyMixed,
    TFalse,
    TFloat,
    TClosure {
        params: Vec<FunctionLikeParameter>,
        return_type: Option<TUnion>,
        is_pure: Option<bool>,
    },
    TClosureAlias {
        id: FunctionLikeIdentifier,
    },
    TInt,
    TKeyset {
        type_param: TUnion,
    },
    TLiteralClassname {
        name: String,
    },
    TEnumLiteralCase {
        enum_name: String,
        member_name: String,
        constraint_type: Option<Box<TAtomic>>,
    },
    TLiteralInt {
        value: i64,
    },
    TLiteralString {
        value: String,
    },
    TMixedAny,
    TMixed,
    TMixedFromLoopIsset,
    TNamedObject {
        name: String,
        type_params: Option<Vec<TUnion>>,
        is_this: bool,
        extra_types: Option<FxHashMap<String, TAtomic>>,
        remapped_params: bool,
    },
    TNonnullMixed,
    TObject,
    TNothing,
    TNull,
    TNum,
    TReference {
        name: String,
        type_params: Option<Vec<TUnion>>,
    },
    TScalar,
    TString,
    // .0 => TTruthyString
    // .1 => TNonEmptyString
    // .2 => TNonspecificLiteralString
    TStringWithFlags(bool, bool, bool),
    TTemplateParam {
        param_name: String,
        as_type: TUnion,
        defining_entity: String,
        from_class: bool,
        extra_types: Option<FxHashMap<String, TAtomic>>,
    },
    TTemplateParamClass {
        param_name: String,
        as_type: Box<crate::t_atomic::TAtomic>,
        defining_entity: String,
    },
    TTemplateParamType {
        param_name: String,
        defining_entity: String,
    },
    TTrue,
    TTruthyMixed,
    TTypeAlias {
        name: String,
        type_params: Option<Vec<TUnion>>,
        as_type: Option<Box<TAtomic>>,
    },
    TVec {
        known_items: Option<BTreeMap<usize, (bool, TUnion)>>,
        type_param: TUnion,
        known_count: Option<usize>,
        non_empty: bool,
    },
    TVoid,
    TPlaceholder,
    TClassTypeConstant {
        class_type: Box<TAtomic>,
        member_name: String,
    },
    TRegexPattern {
        value: String,
    },
}

impl TAtomic {
    pub fn get_id(&self) -> String {
        match self {
            TAtomic::TArraykey { .. } => "arraykey".to_string(),
            TAtomic::TBool { .. } => "bool".to_string(),
            TAtomic::TClassname { as_type, .. } => {
                let mut str = String::new();
                str += "classname<";
                str += (&*as_type).get_id().as_str();
                str += ">";
                return str;
            }
            TAtomic::TDict {
                key_param,
                value_param,
                known_items,
                enum_items,
                shape_name,
                ..
            } => {
                if let Some(shape_name) = shape_name {
                    return format!("shape-from({})", shape_name);
                }

                let mut str = String::new();
                if let Some(known_items) = known_items {
                    str += "shape(";
                    str += known_items
                        .into_iter()
                        .map(|(property, (u, property_type))| {
                            format!(
                                "{}'{}' => {}",
                                if *u { "?" } else { "" },
                                property,
                                property_type.get_id()
                            )
                        })
                        .join(", ")
                        .as_str();

                    if !value_param.is_nothing() {
                        str += ", ...dict<";
                        str += key_param.get_id().as_str();
                        str += ",";
                        str += value_param.get_id().as_str();
                        str += ">";
                    }

                    str += ")";
                    return str;
                } else if let Some(enum_items) = enum_items {
                    str += "shape(";
                    str += enum_items
                        .into_iter()
                        .map(|((l, r), (u, property_type))| {
                            format!(
                                "{}{}::{} => {}",
                                if *u { "?" } else { "" },
                                l,
                                r,
                                property_type.get_id()
                            )
                        })
                        .join(", ")
                        .as_str();

                    if !value_param.is_nothing() {
                        str += ", ...dict<";
                        str += key_param.get_id().as_str();
                        str += ",";
                        str += value_param.get_id().as_str();
                        str += ">";
                    }

                    str += ")";
                    return str;
                }

                str += "dict<";
                str += key_param.get_id().as_str();
                str += ",";
                str += value_param.get_id().as_str();
                str += ">";
                return str;
            }
            TAtomic::TEnum { name } => name.clone(),
            TAtomic::TFalsyMixed { .. } => "falsy-mixed".to_string(),
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
                                param_type.get_id()
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
                    str += return_type.get_id().as_str();
                } else {
                    str += "mixed";
                }

                str += ")";

                str
            }
            TAtomic::TClosureAlias { id } => {
                format!("{}<>", id.to_string())
            }
            TAtomic::TInt { .. } => "int".to_string(),
            TAtomic::TObject => "object".to_string(),
            TAtomic::TNonnullMixed { .. } => "nonnull".to_string(),
            TAtomic::TKeyset { type_param, .. } => {
                let mut str = String::new();
                str += "keyset<";
                str += type_param.get_id().as_str();
                str += ">";
                return str;
            }
            TAtomic::TLiteralClassname { name } => {
                let mut str = String::new();
                str += name.as_str();
                str += "::class";
                return str;
            }
            TAtomic::TEnumLiteralCase {
                enum_name,
                member_name,
                ..
            } => {
                format!("{}::{}", enum_name, member_name)
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
            TAtomic::TMixedAny => "any".to_string(),
            TAtomic::TMixed | TAtomic::TMixedFromLoopIsset => "mixed".to_string(),
            TAtomic::TNamedObject {
                name,
                type_params,
                is_this,
                ..
            } => match type_params {
                None => format!("{}{}", name, if *is_this { "&static" } else { "" }),
                Some(type_params) => {
                    let mut str = String::new();
                    str += name.as_str();
                    if *is_this {
                        str += "&static";
                    }
                    str += "<";
                    str += type_params
                        .into_iter()
                        .map(|tunion| tunion.get_id())
                        .join(", ")
                        .as_str();
                    str += ">";
                    return str;
                }
            },
            TAtomic::TTypeAlias {
                name, type_params, ..
            } => match type_params {
                None => "type-alias(".to_string() + &name + ")",
                Some(type_params) => {
                    let mut str = String::new();
                    str += "type-alias(";
                    str += &name;
                    str += "<";
                    str += type_params
                        .into_iter()
                        .map(|tunion| tunion.get_id())
                        .join(", ")
                        .as_str();
                    str += ">)";
                    return str;
                }
            },
            TAtomic::TTruthyMixed { .. } => "truthy-mixed".to_string(),
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
            TAtomic::TTemplateParam {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += param_name.as_str();
                str += ":";
                str += defining_entity.as_str();
                return str;
            }
            TAtomic::TTemplateParamClass {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "classname<";
                str += param_name.as_str();
                str += ":";
                str += defining_entity.as_str();
                str += ">";
                return str;
            }
            TAtomic::TTemplateParamType {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "typename<";
                str += param_name.as_str();
                str += ":";
                str += defining_entity.as_str();
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
                        .map(|(_, (_, tunion))| tunion.get_id())
                        .join(", ")
                        .as_str();

                    if !type_param.is_nothing() {
                        str += ", ...vec<";
                        str += type_param.get_id().as_str();
                        str += ">";
                    }

                    str += ")";
                    return str;
                }
                let mut str = String::new();
                str += if *non_empty { "non-empty-vec<" } else { "vec<" };
                str += type_param.get_id().as_str();
                str += ">";
                return str;
            }
            TAtomic::TVoid => "void".to_string(),
            TAtomic::TReference { name, .. } => {
                let mut str = String::new();
                str += "unknown-ref(";
                str += name.as_str();
                str += ")";
                return str;
            }
            TAtomic::TPlaceholder => "_".to_string(),
            TAtomic::TClassTypeConstant {
                class_type,
                member_name,
                ..
            } => {
                format!("{}::{}", class_type.get_id(), member_name)
            }
            TAtomic::TRegexPattern { value } => "re\\\"".to_string() + value.as_str() + "\\\"",
        }
    }

    pub fn get_key(&self) -> String {
        match self {
            TAtomic::TDict { .. } => "dict".to_string(),
            TAtomic::TVec { .. } => "vec".to_string(),
            TAtomic::TKeyset { .. } => "keyset".to_string(),
            TAtomic::TArraykey { .. } => self.get_id(),
            TAtomic::TBool { .. } => self.get_id(),
            TAtomic::TClassname { as_type, .. } => {
                let mut str = String::new();
                str += "classname<";
                str += (&*as_type).get_key().as_str();
                str += ">";
                return str;
            }
            TAtomic::TFalsyMixed { .. }
            | TAtomic::TFalse { .. }
            | TAtomic::TFloat { .. }
            | TAtomic::TClosure { .. }
            | TAtomic::TClosureAlias { .. }
            | TAtomic::TInt { .. }
            | TAtomic::TTruthyMixed { .. }
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
            | TAtomic::TNonnullMixed { .. }
            | TAtomic::TTrue { .. }
            | TAtomic::TObject
            | TAtomic::TScalar
            | TAtomic::TReference { .. }
            | TAtomic::TRegexPattern { .. } => self.get_id(),

            TAtomic::TStringWithFlags(..) => "string".to_string(),

            TAtomic::TNamedObject {
                name, type_params, ..
            } => match type_params {
                None => name.clone(),
                Some(type_params) => {
                    let mut str = String::new();
                    str += name.as_str();
                    str += "<";
                    str += type_params
                        .into_iter()
                        .map(|tunion| tunion.get_key())
                        .join(", ")
                        .as_str();
                    str += ">";
                    return str;
                }
            },

            TAtomic::TTypeAlias {
                name, type_params, ..
            } => match type_params {
                None => "type-alias(".to_string() + &name + ")",
                Some(type_params) => {
                    let mut str = String::new();
                    str += "type-alias(";
                    str += &name;
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

            TAtomic::TTemplateParam {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += param_name.as_str();
                str += ":";
                str += defining_entity.as_str();
                return str;
            }
            TAtomic::TTemplateParamClass {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "classname<";
                str += param_name.as_str();
                str += ":";
                str += defining_entity.as_str();
                str += ">";
                return str;
            }
            TAtomic::TTemplateParamType {
                param_name,
                defining_entity,
                ..
            } => {
                let mut str = String::new();
                str += "typename<";
                str += param_name.as_str();
                str += ":";
                str += defining_entity.as_str();
                str += ">";
                return str;
            }
            TAtomic::TPlaceholder => "_".to_string(),
            TAtomic::TMixedAny => "mixed".to_string(),
        }
    }

    pub fn get_combiner_key(&self, codebase: &CodebaseInfo) -> String {
        match self {
            TAtomic::TNamedObject {
                name, type_params, ..
            } => match type_params {
                None => name.clone(),
                Some(type_params) => {
                    let covariants =
                        if let Some(classlike_storage) = codebase.classlike_infos.get(name) {
                            &classlike_storage.template_covariants
                        } else {
                            return self.get_key();
                        };

                    let mut str = String::new();
                    str += name.as_str();
                    str += "<";
                    str += type_params
                        .into_iter()
                        .enumerate()
                        .map(|(i, tunion)| {
                            if covariants.contains(&i) {
                                "*".to_string()
                            } else {
                                tunion.get_key()
                            }
                        })
                        .join(", ")
                        .as_str();
                    str += ">";
                    return str;
                }
            },
            _ => self.get_key(),
        }
    }

    pub fn is_mixed(&self) -> bool {
        match self {
            TAtomic::TMixed
            | TAtomic::TNonnullMixed
            | TAtomic::TMixedFromLoopIsset
            | TAtomic::TMixedAny
            | TAtomic::TFalsyMixed
            | TAtomic::TTruthyMixed => true,
            _ => false,
        }
    }

    pub fn is_mixed_with_any(&self, has_any: &mut bool) -> bool {
        match self {
            TAtomic::TMixedAny => {
                *has_any = true;
                true
            }
            TAtomic::TMixed
            | TAtomic::TNonnullMixed
            | TAtomic::TMixedFromLoopIsset
            | TAtomic::TFalsyMixed
            | TAtomic::TTruthyMixed => true,
            _ => false,
        }
    }

    pub fn is_templated_as_mixed(&self, has_any: &mut bool) -> bool {
        match self {
            TAtomic::TTemplateParam {
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
            TAtomic::TTemplateParam {
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
            TAtomic::TTemplateParam {
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

    pub fn get_dict_params(&self) -> Option<(&TUnion, &TUnion)> {
        match self {
            TAtomic::TDict {
                key_param,
                value_param,
                ..
            } => Some((key_param, value_param)),
            _ => None,
        }
    }

    pub fn get_shape_name(&self) -> Option<&String> {
        match self {
            TAtomic::TDict { shape_name, .. } => shape_name.as_ref(),
            _ => None,
        }
    }

    #[inline]
    pub fn is_some_scalar(&self) -> bool {
        match self {
            TAtomic::TTemplateParamClass { .. }
            | TAtomic::TTemplateParamType { .. }
            | TAtomic::TLiteralClassname { .. }
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TArraykey { .. }
            | TAtomic::TBool { .. }
            | TAtomic::TClassname { .. }
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
            | TAtomic::TTemplateParamClass { .. }
            | TAtomic::TTemplateParamType { .. }
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
            | TAtomic::TTemplateParamClass { .. }
            | TAtomic::TTemplateParamType { .. }
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
        if let TAtomic::TTemplateParam {
            param_name,
            defining_entity,
            extra_types,
            from_class,
            ..
        } = self
        {
            return TAtomic::TTemplateParam {
                as_type: new_as_type,
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

    pub fn add_known_items_to_dict(
        mut self,
        new_known_items: BTreeMap<String, (bool, Arc<TUnion>)>,
    ) -> TAtomic {
        if let TAtomic::TDict {
            ref mut known_items,
            ..
        } = self
        {
            *known_items = Some(new_known_items);

            return self;
        }

        panic!()
    }

    pub fn is_truthy(&self) -> bool {
        match &self {
            &TAtomic::TTrue { .. }
            | &TAtomic::TTruthyMixed { .. }
            | &TAtomic::TStringWithFlags(true, _, _)
            | &TAtomic::TObject { .. }
            | &TAtomic::TClosure { .. }
            | &TAtomic::TLiteralClassname { .. }
            | &TAtomic::TClassname { .. } => true,
            &TAtomic::TNamedObject { name, .. } => {
                name != "HH\\Container" && name != "HH\\KeyedContainer"
            }
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
            &TAtomic::TFalse { .. } | &TAtomic::TNull { .. } | &TAtomic::TFalsyMixed { .. } => true,
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
                value_param,
                ..
            } => {
                if let None = known_items {
                    if value_param.is_nothing() && !non_empty {
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

    pub fn is_array_accessible_with_string_key(&self) -> bool {
        match self {
            TAtomic::TDict { .. } | TAtomic::TKeyset { .. } => true,
            TAtomic::TNamedObject { name, .. } => name == "HH\\KeyedContainer",
            _ => false,
        }
    }

    pub fn is_array_accessible_with_int_or_string_key(&self) -> bool {
        match self {
            TAtomic::TDict { .. } | TAtomic::TVec { .. } | TAtomic::TKeyset { .. } => true,
            TAtomic::TNamedObject { name, .. } => {
                name == "HH\\KeyedContainer" || name == "HH\\Container"
            }
            _ => false,
        }
    }

    pub fn add_intersection_type(&mut self, atomic: TAtomic) {
        if let TAtomic::TNamedObject {
            ref mut extra_types,
            ..
        }
        | TAtomic::TTemplateParam {
            ref mut extra_types,
            ..
        } = self
        {
            if let Some(extra_types) = extra_types {
                extra_types.insert(atomic.get_key(), atomic);
            } else {
                let mut map = FxHashMap::default();
                map.insert(atomic.get_key(), atomic);
                *extra_types = Some(map);
            }
        }
    }

    pub fn clone_without_intersection_types(&self) -> TAtomic {
        let mut clone = self.clone();

        if let TAtomic::TNamedObject {
            ref mut extra_types,
            ..
        }
        | TAtomic::TTemplateParam {
            ref mut extra_types,
            ..
        } = clone
        {
            *extra_types = None
        }

        clone
    }

    pub fn get_intersection_types(
        &self,
    ) -> (FxHashMap<String, &TAtomic>, FxHashMap<String, TAtomic>) {
        match self {
            TAtomic::TNamedObject {
                extra_types: Some(extra_types),
                ..
            }
            | TAtomic::TTemplateParam {
                extra_types: Some(extra_types),
                ..
            } => {
                return (
                    extra_types
                        .iter()
                        .map(|(k, v)| (k.clone(), v))
                        .collect::<FxHashMap<_, _>>(),
                    FxHashMap::default(),
                )
            }
            _ => {
                if let TAtomic::TTemplateParam { as_type, .. } = self {
                    for (_, as_atomic) in &as_type.types {
                        // T1 as T2 as object becomes (T1 as object) & (T2 as object)
                        if let TAtomic::TTemplateParam {
                            as_type: extends_as_type,
                            ..
                        } = as_atomic
                        {
                            let mut new_intersection_types = FxHashMap::default();
                            let intersection_types = as_atomic.get_intersection_types();
                            new_intersection_types.extend(intersection_types.1);
                            let mut type_part = self.clone();
                            if let TAtomic::TTemplateParam {
                                ref mut as_type, ..
                            } = type_part
                            {
                                *as_type = extends_as_type.clone();
                            }
                            new_intersection_types.insert(type_part.get_key(), type_part);

                            return (intersection_types.0, new_intersection_types);
                        }
                    }
                }

                let mut intersection_types = FxHashMap::default();
                intersection_types.insert(self.get_key(), self);
                return (intersection_types, FxHashMap::default());
            }
        };
    }

    pub fn remove_placeholders(&mut self) {
        match self {
            TAtomic::TDict {
                ref mut key_param,
                ref mut value_param,
                ..
            } => {
                if let TAtomic::TPlaceholder = key_param.get_single() {
                    *key_param = TUnion::new(vec![TAtomic::TArraykey { from_any: true }]);
                }
                if let TAtomic::TPlaceholder = value_param.get_single() {
                    *value_param = TUnion::new(vec![TAtomic::TMixedAny]);
                }
            }
            TAtomic::TKeyset { ref mut type_param } => {
                if let TAtomic::TPlaceholder = type_param.get_single() {
                    *type_param = TUnion::new(vec![TAtomic::TArraykey { from_any: true }]);
                }
            }
            TAtomic::TNamedObject {
                ref mut name,
                ref mut type_params,
                ..
            } => {
                if let Some(type_params) = type_params {
                    if name == "HH\\KeyedContainer" {
                        if let Some(key_param) = type_params.get_mut(0) {
                            if let TAtomic::TPlaceholder = key_param.get_single() {
                                *key_param =
                                    TUnion::new(vec![TAtomic::TArraykey { from_any: true }]);
                            }
                        }

                        if let Some(value_param) = type_params.get_mut(1) {
                            if let TAtomic::TPlaceholder = value_param.get_single() {
                                *value_param = TUnion::new(vec![TAtomic::TMixedAny]);
                            }
                        }
                    } else if name == "HH\\Container" {
                        if let Some(key_param) = type_params.get_mut(0) {
                            if let TAtomic::TPlaceholder = key_param.get_single() {
                                *key_param =
                                    TUnion::new(vec![TAtomic::TArraykey { from_any: true }]);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl HasTypeNodes for TAtomic {
    fn get_child_nodes(&self) -> Vec<TypeNode> {
        match self {
            TAtomic::TDict {
                key_param,
                value_param,
                known_items,
                ..
            } => {
                let mut vec = vec![TypeNode::Union(key_param), TypeNode::Union(value_param)];
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
            TAtomic::TTemplateParam { as_type, .. } => {
                vec![TypeNode::Union(as_type)]
            }
            TAtomic::TTypeAlias { type_params, .. } => match type_params {
                None => vec![],
                Some(type_params) => {
                    let mut vec = vec![];
                    for type_param in type_params {
                        vec.push(TypeNode::Union(type_param));
                    }
                    vec
                }
            },
            _ => vec![],
        }
    }
}

pub fn populate_atomic_type(t_atomic: &mut self::TAtomic, codebase_symbols: &Symbols) {
    match t_atomic {
        TAtomic::TDict {
            ref mut key_param,
            ref mut value_param,
            ref mut known_items,
            ref mut enum_items,
            ..
        } => {
            populate_union_type(key_param, codebase_symbols);
            populate_union_type(value_param, codebase_symbols);
            if let Some(known_items) = known_items {
                for (_, (_, prop_type)) in known_items {
                    populate_union_type(Arc::make_mut(prop_type), codebase_symbols);
                }
            }
            if let Some(enum_items) = enum_items {
                for (_, (_, prop_type)) in enum_items {
                    populate_union_type(prop_type, codebase_symbols);
                }
            }
        }
        TAtomic::TClosure {
            ref mut params,
            ref mut return_type,
            ..
        } => {
            if let Some(return_type) = return_type {
                populate_union_type(return_type, codebase_symbols);
            }

            for param in params {
                if let Some(ref mut param_type) = param.signature_type {
                    populate_union_type(param_type, codebase_symbols);
                }
            }
        }
        TAtomic::TKeyset {
            ref mut type_param, ..
        } => {
            populate_union_type(type_param, codebase_symbols);
        }
        TAtomic::TNamedObject {
            ref mut type_params,
            ..
        } => match type_params {
            None => {}
            Some(type_params) => {
                for type_param in type_params {
                    populate_union_type(type_param, codebase_symbols);
                }
            }
        },
        TAtomic::TVec {
            ref mut type_param,
            ref mut known_items,
            ..
        } => {
            populate_union_type(type_param, codebase_symbols);

            if let Some(known_items) = known_items {
                for (_, (_, tuple_type)) in known_items {
                    populate_union_type(tuple_type, codebase_symbols);
                }
            }
        }
        TAtomic::TReference {
            ref name,
            ref mut type_params,
        } => {
            if let Some(type_params) = type_params {
                for type_param in type_params {
                    populate_union_type(type_param, codebase_symbols);
                }
            }

            if let Some(symbol_kind) = codebase_symbols.all.get(name) {
                match symbol_kind {
                    SymbolKind::Enum => {
                        *t_atomic = TAtomic::TEnum { name: name.clone() };
                        return;
                    }
                    SymbolKind::EnumClass => panic!(),
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
                // println!("Uknown symbol {}", name);
            }
        }
        TAtomic::TClassTypeConstant { class_type, .. } => {
            populate_atomic_type(class_type, codebase_symbols);
        }
        TAtomic::TTemplateParam {
            ref mut as_type, ..
        } => {
            populate_union_type(as_type, codebase_symbols);
        }
        _ => {}
    }
}
