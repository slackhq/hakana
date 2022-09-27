use crate::{
    codebase_info::Symbols,
    data_flow::node::DataFlowNode,
    t_atomic::{populate_atomic_type, DictKey, TAtomic},
};
use core::panic;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Eq)]
pub struct TUnion {
    pub types: BTreeMap<String, TAtomic>,
    pub parent_nodes: FxHashMap<String, DataFlowNode>,
    pub had_template: bool,

    // Whether or not the data in this type could have references to it.
    // Defaults to false, but newly created immutable objects definitionally
    // are reference-free
    pub reference_free: bool,

    // special case because try is a weird situation
    pub possibly_undefined_from_try: bool,

    pub ignore_falsable_issues: bool,

    // Whether or not this union comes from a template "as" default
    pub from_template_default: bool,

    pub has_mutations: bool,

    pub populated: bool,
}

#[derive(Clone, Debug)]
pub enum TypeNode<'a> {
    Union(&'a TUnion),
    Atomic(&'a TAtomic),
}

impl<'a> TypeNode<'a> {
    pub fn get_child_nodes(&self) -> Vec<TypeNode> {
        match self {
            TypeNode::Union(union) => union.get_child_nodes(),
            TypeNode::Atomic(atomic) => atomic.get_child_nodes(),
        }
    }
}

pub trait HasTypeNodes {
    fn get_child_nodes(&self) -> Vec<TypeNode>;
}

impl TUnion {
    pub fn new(types: Vec<TAtomic>) -> TUnion {
        let mut keyed_types = BTreeMap::new();

        for ttype in types.into_iter() {
            let key = ttype.get_key();
            keyed_types.insert(key, ttype);
        }

        TUnion {
            types: keyed_types,
            parent_nodes: FxHashMap::default(),
            had_template: false,
            reference_free: false,
            possibly_undefined_from_try: false,
            ignore_falsable_issues: false,
            from_template_default: false,
            has_mutations: true,
            populated: false,
        }
    }

    pub fn add_type(&mut self, new_type: TAtomic) {
        self.types.insert(new_type.get_key(), new_type);
    }

    pub fn is_int(&self) -> bool {
        for (_, atomic) in &self.types {
            let no_int = match atomic {
                TAtomic::TInt { .. } | TAtomic::TLiteralInt { .. } => false,
                _ => true,
            };

            if no_int {
                return false;
            }
        }

        return true;
    }

    pub fn has_int(&self) -> bool {
        for (_, atomic) in &self.types {
            match atomic {
                TAtomic::TInt { .. } | TAtomic::TLiteralInt { .. } => {
                    return true;
                }
                _ => {}
            };
        }

        return false;
    }

    pub fn has_float(&self) -> bool {
        for (_, atomic) in &self.types {
            match atomic {
                TAtomic::TFloat { .. } => {
                    return true;
                }
                _ => {}
            };
        }

        return false;
    }

    pub fn is_arraykey(&self) -> bool {
        for (_, atomic) in &self.types {
            if match atomic {
                TAtomic::TArraykey { .. } => false,
                _ => true,
            } {
                return false;
            }
        }

        return true;
    }

    pub fn has_string(&self) -> bool {
        for (_, atomic) in &self.types {
            match atomic {
                TAtomic::TString { .. }
                | TAtomic::TLiteralString { .. }
                | TAtomic::TStringWithFlags { .. } => {
                    return true;
                }
                _ => {}
            };
        }

        return false;
    }

    pub fn is_float(&self) -> bool {
        for (_, atomic) in &self.types {
            let no_int = match atomic {
                TAtomic::TFloat { .. } => false,
                _ => true,
            };

            if no_int {
                return false;
            }
        }

        return true;
    }

    pub fn is_nothing(&self) -> bool {
        for (_, atomic) in &self.types {
            if let &TAtomic::TNothing = atomic {
                return true;
            }

            return false;
        }

        return false;
    }

    pub fn is_placeholder(&self) -> bool {
        for (_, atomic) in &self.types {
            if let &TAtomic::TPlaceholder = atomic {
                return true;
            }

            return false;
        }

        return false;
    }

    pub fn is_true(&self) -> bool {
        for (_, atomic) in &self.types {
            if let &TAtomic::TTrue { .. } = atomic {
                return true;
            }

            return false;
        }

        return false;
    }

    pub fn is_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        for (_, atomic) in &self.types {
            return match atomic {
                &TAtomic::TMixed
                | &TAtomic::TMixedAny
                | &TAtomic::TNonnullMixed
                | &TAtomic::TMixedFromLoopIsset
                | &TAtomic::TFalsyMixed
                | &TAtomic::TTruthyMixed => true,
                _ => false,
            };
        }

        return true;
    }

    pub fn is_mixed_with_any(&self, has_any: &mut bool) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        for (_, atomic) in &self.types {
            return match atomic {
                &TAtomic::TMixedAny => {
                    *has_any = true;
                    true
                }
                &TAtomic::TMixed
                | &TAtomic::TNonnullMixed
                | &TAtomic::TMixedFromLoopIsset
                | &TAtomic::TFalsyMixed
                | &TAtomic::TTruthyMixed => true,
                _ => false,
            };
        }

        return true;
    }

    pub fn is_nullable_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }
        for (_, atomic) in &self.types {
            if let &TAtomic::TMixed
            | &TAtomic::TMixedAny
            | &TAtomic::TMixedFromLoopIsset
            | &TAtomic::TFalsyMixed = atomic
            {
                continue;
            }

            return false;
        }

        return true;
    }

    pub fn is_falsy_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }
        for (_, atomic) in &self.types {
            if let &TAtomic::TFalsyMixed { .. } = atomic {
                continue;
            }

            return false;
        }

        return true;
    }

    pub fn is_vanilla_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        if let Some(mixed) = self.types.get("mixed") {
            if matches!(mixed, TAtomic::TMixed) {
                return true;
            }
        }

        return false;
    }

    pub fn has_template_or_static(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TTemplateParam { .. } = atomic {
                return true;
            }

            if let TAtomic::TNamedObject {
                extra_types,
                is_this,
                ..
            } = atomic
            {
                if *is_this {
                    return true;
                }

                if let Some(extra_types) = extra_types {
                    for (_, extra_type) in extra_types {
                        if let TAtomic::TTemplateParam { .. } = extra_type {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub fn has_template(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TTemplateParam { .. } = atomic {
                return true;
            }

            if let TAtomic::TNamedObject { extra_types, .. } = atomic {
                if let Some(extra_types) = extra_types {
                    for (_, extra_type) in extra_types {
                        if let TAtomic::TTemplateParam { .. } = extra_type {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub fn has_template_types(&self) -> bool {
        let mut child_nodes = self.get_child_nodes();
        let mut all_child_nodes = vec![];

        while let Some(child_node) = child_nodes.pop() {
            let new_child_nodes = match child_node {
                TypeNode::Union(union) => union.get_child_nodes(),
                TypeNode::Atomic(atomic) => atomic.get_child_nodes(),
            };

            all_child_nodes.push(child_node);

            child_nodes.extend(new_child_nodes);
        }

        for child_node in all_child_nodes {
            if let TypeNode::Atomic(inner) = child_node {
                if let TAtomic::TTemplateParam { .. }
                | TAtomic::TTemplateParamClass { .. }
                | TAtomic::TTemplateParamType { .. } = inner
                {
                    return true;
                }
            }
        }

        return false;
    }

    pub fn get_template_types(&self) -> Vec<TAtomic> {
        let mut child_nodes = self.get_child_nodes();
        let mut all_child_nodes = vec![];

        while let Some(child_node) = child_nodes.pop() {
            let new_child_nodes = match child_node {
                TypeNode::Union(union) => union.get_child_nodes(),
                TypeNode::Atomic(atomic) => atomic.get_child_nodes(),
            };

            all_child_nodes.push(child_node);

            child_nodes.extend(new_child_nodes);
        }

        let mut template_types = Vec::new();

        for child_node in all_child_nodes {
            if let TypeNode::Atomic(inner) = child_node {
                if let TAtomic::TTemplateParam { .. }
                | TAtomic::TTemplateParamClass { .. }
                | TAtomic::TTemplateParamType { .. } = inner
                {
                    template_types.push(inner.clone());
                }
            }
        }

        template_types
    }

    pub fn is_objecty(&self) -> bool {
        for (_, atomic) in &self.types {
            if let &TAtomic::TObject { .. }
            | TAtomic::TNamedObject { .. }
            | TAtomic::TClosure { .. } = atomic
            {
                continue;
            }

            return false;
        }

        return true;
    }

    pub fn is_generator(&self) -> bool {
        for (_, atomic) in &self.types {
            if let &TAtomic::TNamedObject { name, .. } = &atomic {
                if name == "Generator" {
                    continue;
                }
            }

            return false;
        }

        return true;
    }

    pub fn is_null(&self) -> bool {
        self.types.len() == 1 && self.types.contains_key("null")
    }

    pub fn is_nullable(&self) -> bool {
        self.types.len() >= 2 && self.types.contains_key("null")
    }

    pub fn is_void(&self) -> bool {
        self.types.len() == 1 && self.types.contains_key("void")
    }

    pub fn is_vec(&self) -> bool {
        self.types.len() == 1 && self.types.contains_key("vec")
    }

    pub fn is_false(&self) -> bool {
        self.types.len() == 1 && self.types.contains_key("false")
    }

    pub fn is_falsable(&self) -> bool {
        self.types.len() >= 2 && self.types.contains_key("false")
    }

    pub fn has_bool(&self) -> bool {
        for (_, atomic) in &self.types {
            match atomic {
                TAtomic::TBool { .. } | TAtomic::TFalse { .. } | TAtomic::TTrue { .. } => {
                    return true;
                }
                _ => {}
            };
        }

        return false;
    }

    pub fn has_scalar(&self) -> bool {
        for (_, atomic) in &self.types {
            match atomic {
                TAtomic::TScalar { .. } => {
                    return true;
                }
                _ => {}
            };
        }

        return false;
    }

    pub fn is_always_truthy(&self) -> bool {
        for (_, atomic) in &self.types {
            if atomic.is_truthy() {
                continue;
            }

            return false;
        }

        return true;
    }

    pub fn is_always_falsy(&self) -> bool {
        for (_, atomic) in &self.types {
            if atomic.is_falsy() {
                continue;
            }

            return false;
        }

        return true;
    }

    pub fn is_literal_of(&self, other: &TUnion) -> bool {
        for (_, other_atomic_type) in &other.types {
            if let TAtomic::TString = other_atomic_type {
                for (_, self_atomic_type) in &self.types {
                    if self_atomic_type.is_string_subtype() {
                        continue;
                    }

                    return false;
                }

                return true;
            } else if let TAtomic::TInt = other_atomic_type {
                for (_, self_atomic_type) in &self.types {
                    if let TAtomic::TLiteralInt { .. } = self_atomic_type {
                        continue;
                    }

                    return false;
                }

                return true;
            } else {
                return false;
            }
        }

        false
    }

    pub fn all_literals(&self) -> bool {
        for (_, atomic) in &self.types {
            match atomic {
                TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TStringWithFlags(_, _, true)
                | TAtomic::TEnumLiteralCase { .. }
                | TAtomic::TEnum { .. } => {
                    continue;
                }
                _ => {
                    return false;
                }
            };
        }

        return true;
    }

    pub fn has_static_object(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TNamedObject { is_this: true, .. } = atomic {
                return true;
            }
        }

        false
    }

    pub fn has_typealias(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TTypeAlias { .. } = atomic {
                return true;
            }
        }

        false
    }

    pub fn is_static_object(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TNamedObject { is_this: true, .. } = atomic {
                continue;
            }

            return false;
        }

        true
    }

    pub fn get_key(&self) -> String {
        let mut tatomic_strings = (&self.types).into_iter().map(|(key, _)| key);
        tatomic_strings.join("|")
    }

    pub fn get_id(&self) -> String {
        let mut tatomic_strings = (&self.types).into_iter().map(|(_, atomic)| atomic.get_id());
        tatomic_strings.join("|")
    }

    #[inline]
    pub fn is_single(&self) -> bool {
        self.types.len() == 1
    }

    #[inline]
    pub fn get_single(&self) -> &TAtomic {
        for (_, atomic) in &self.types {
            return atomic;
        }

        panic!()
    }

    #[inline]
    pub fn get_single_owned(self) -> TAtomic {
        for (_, atomic) in self.types.into_iter() {
            return atomic;
        }

        panic!()
    }

    #[inline]
    pub fn has_named_object(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TNamedObject { .. } = atomic {
                return true;
            }
        }

        false
    }

    #[inline]
    pub fn has_object_type(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TNamedObject { .. } | TAtomic::TObject { .. } = atomic {
                return true;
            }
        }

        false
    }

    pub fn get_single_literal_int_value(&self) -> Option<i64> {
        if self.is_single() {
            match self.get_single() {
                TAtomic::TLiteralInt { value, .. } => Some(*value),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn get_single_literal_string_value(&self) -> Option<String> {
        if self.is_single() {
            match self.get_single() {
                TAtomic::TLiteralString { value, .. } => Some(value.clone()),
                TAtomic::TTypeAlias {
                    name,
                    as_type: Some(as_type),
                    type_params: Some(_),
                } => {
                    if name == "HH\\Lib\\Regex\\Pattern" {
                        if let TAtomic::TLiteralString { value, .. } = &**as_type {
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
        } else {
            None
        }
    }

    pub fn get_single_dict_key(&self) -> Option<DictKey> {
        if self.is_single() {
            match self.get_single() {
                TAtomic::TLiteralInt { value, .. } => Some(DictKey::Int(*value as u32)),
                TAtomic::TLiteralString { value, .. } => Some(DictKey::String(value.clone())),
                TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                    ..
                } => Some(DictKey::Enum(enum_name.clone(), member_name.clone())),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn get_literal_ints(&self) -> FxHashMap<&String, &TAtomic> {
        self.types
            .iter()
            .filter(|(_, a)| matches!(a, TAtomic::TLiteralInt { .. }))
            .collect()
    }

    pub fn get_literal_strings(&self) -> FxHashMap<&String, &TAtomic> {
        self.types
            .iter()
            .filter(|(_, a)| matches!(a, TAtomic::TLiteralString { .. }))
            .collect()
    }

    pub fn has_literal_value(&self) -> bool {
        for (_, atomic) in &self.types {
            if let TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TTrue { .. }
            | TAtomic::TFalse { .. }
            | TAtomic::TLiteralClassname { .. } = atomic
            {
                return true;
            }
        }

        false
    }

    pub fn has_taintable_value(&self) -> bool {
        let mut any_taintable = false;

        for (_, assignment_atomic_type) in &self.types {
            match assignment_atomic_type {
                TAtomic::TInt
                | TAtomic::TFloat
                | TAtomic::TNull
                | TAtomic::TLiteralClassname { .. }
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralString { .. }
                | TAtomic::TBool
                | TAtomic::TFalse
                | TAtomic::TFalsyMixed
                | TAtomic::TTrue
                | TAtomic::TEnum { .. }
                | TAtomic::TEnumLiteralCase { .. }
                | TAtomic::TNum => {
                    // do nothing
                }
                _ => {
                    any_taintable = true;
                }
            }
        }

        any_taintable
    }

    pub fn needs_population(&self) -> bool {
        !self.populated && self.types.iter().any(|(_, v)| v.needs_population())
    }
}

impl PartialEq for TUnion {
    fn eq(&self, other: &TUnion) -> bool {
        if self.types.len() != other.types.len()
            || !self.types.keys().all(|k| other.types.contains_key(k))
        {
            return false;
        }

        for (k, v) in &self.types {
            if let Some(other_type) = other.types.get(k) {
                if v != other_type {
                    return false;
                }
            } else {
                return false;
            }
        }

        if self.parent_nodes.len() != other.parent_nodes.len()
            || !self
                .parent_nodes
                .keys()
                .all(|k| other.parent_nodes.contains_key(k))
        {
            return false;
        }

        true
    }
}

impl HasTypeNodes for TUnion {
    fn get_child_nodes(&self) -> Vec<TypeNode> {
        self.types
            .iter()
            .map(|(_, t)| TypeNode::Atomic(t))
            .collect()
    }
}

pub fn populate_union_type(t_union: &mut self::TUnion, codebase_symbols: &Symbols) {
    if t_union.populated {
        return;
    }

    t_union.populated = true;

    let ref mut types = t_union.types;

    let mut swapped_keys = vec![];

    for (key, atomic) in types.iter_mut() {
        if let TAtomic::TClassname { ref mut as_type }
        | TAtomic::TTemplateParamClass {
            ref mut as_type, ..
        } = atomic
        {
            let mut new_as_type = (**as_type).clone();
            populate_atomic_type(&mut new_as_type, codebase_symbols);
            *as_type = Box::new(new_as_type);
        } else {
            populate_atomic_type(atomic, codebase_symbols);
        }

        let new_key = atomic.get_key();

        if &new_key != key {
            swapped_keys.push((key.clone(), new_key));
        }
    }

    for (old_key, new_key) in swapped_keys {
        let atomic = types.remove(&old_key).unwrap();
        types.insert(new_key, atomic);
    }
}
