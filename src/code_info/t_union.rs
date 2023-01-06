use crate::{
    codebase_info::Symbols,
    data_flow::node::DataFlowNode,
    symbol_references::{SymbolReferences, ReferenceSource},
    t_atomic::{populate_atomic_type, DictKey, TAtomic},
    Interner, StrId,
};
use core::panic;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq)]
pub struct TUnion {
    pub types: Vec<TAtomic>,
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
        TUnion {
            types: types,
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

    pub fn remove_type(&mut self, bad_type: &TAtomic) {
        self.types.retain(|t| t != bad_type);
    }

    pub fn is_int(&self) -> bool {
        for atomic in &self.types {
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
        for atomic in &self.types {
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
        for atomic in &self.types {
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
        for atomic in &self.types {
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
        for atomic in &self.types {
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
        for atomic in &self.types {
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
        for atomic in &self.types {
            if let &TAtomic::TNothing = atomic {
                return true;
            }

            return false;
        }

        return false;
    }

    pub fn is_placeholder(&self) -> bool {
        for atomic in &self.types {
            if let &TAtomic::TPlaceholder = atomic {
                return true;
            }

            return false;
        }

        return false;
    }

    pub fn is_true(&self) -> bool {
        for atomic in &self.types {
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

        for atomic in &self.types {
            return match atomic {
                &TAtomic::TMixed
                | &TAtomic::TMixedWithFlags(..)
                | &TAtomic::TMixedFromLoopIsset => true,
                _ => false,
            };
        }

        return true;
    }

    pub fn is_mixed_with_any(&self, has_any: &mut bool) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        for atomic in &self.types {
            return match atomic {
                &TAtomic::TMixedWithFlags(is_any, ..) => {
                    *has_any = is_any;
                    true
                }
                &TAtomic::TMixed | &TAtomic::TMixedFromLoopIsset => true,
                _ => false,
            };
        }

        return true;
    }

    pub fn is_nullable_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }
        for atomic in &self.types {
            match atomic {
                &TAtomic::TMixed
                | &TAtomic::TMixedFromLoopIsset
                | &TAtomic::TMixedWithFlags(_, _, true, _) => continue,
                TAtomic::TMixedWithFlags(is_any, _, is_falsy, _) => {
                    if *is_any || *is_falsy {
                        continue;
                    }
                }
                _ => (),
            }

            return false;
        }

        return true;
    }

    pub fn is_falsy_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }
        for atomic in &self.types {
            if let &TAtomic::TMixedWithFlags(_, _, true, _) = atomic {
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

        if let Some(mixed) = self.types.get(0) {
            if matches!(mixed, TAtomic::TMixed) {
                return true;
            }
        }

        return false;
    }

    pub fn has_template_or_static(&self) -> bool {
        for atomic in &self.types {
            if let TAtomic::TGenericParam { .. } = atomic {
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
                        if let TAtomic::TGenericParam { .. } = extra_type {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub fn has_template(&self) -> bool {
        for atomic in &self.types {
            if let TAtomic::TGenericParam { .. } = atomic {
                return true;
            }

            if let TAtomic::TNamedObject { extra_types, .. } = atomic {
                if let Some(extra_types) = extra_types {
                    for (_, extra_type) in extra_types {
                        if let TAtomic::TGenericParam { .. } = extra_type {
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
                if let TAtomic::TGenericParam { .. }
                | TAtomic::TGenericClassname { .. }
                | TAtomic::TGenericTypename { .. } = inner
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
                if let TAtomic::TGenericParam { .. }
                | TAtomic::TGenericClassname { .. }
                | TAtomic::TGenericTypename { .. } = inner
                {
                    template_types.push(inner.clone());
                }
            }
        }

        template_types
    }

    pub fn is_objecty(&self) -> bool {
        for atomic in &self.types {
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

    pub fn is_generator(&self, interner: &Interner) -> bool {
        for atomic in &self.types {
            if let &TAtomic::TNamedObject { name, .. } = &atomic {
                if name == &interner.get("Generator").unwrap() {
                    continue;
                }
            }

            return false;
        }

        return true;
    }

    pub fn is_null(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TNull)
    }

    pub fn is_nullable(&self) -> bool {
        self.types.len() >= 2 && self.types.iter().any(|t| matches!(t, TAtomic::TNull))
    }

    pub fn is_void(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TVoid)
    }

    pub fn is_vec(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TVec { .. })
    }

    pub fn is_false(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TFalse)
    }

    pub fn is_falsable(&self) -> bool {
        self.types.len() >= 2 && self.types.iter().any(|t| matches!(t, TAtomic::TFalse))
    }

    pub fn has_bool(&self) -> bool {
        self.types.iter().any(|atomic| match atomic {
            TAtomic::TBool { .. } | TAtomic::TFalse { .. } | TAtomic::TTrue { .. } => true,
            _ => false,
        })
    }

    pub fn has_scalar(&self) -> bool {
        self.types.iter().any(|atomic| match atomic {
            TAtomic::TScalar { .. } => true,
            _ => false,
        })
    }

    pub fn is_always_truthy(&self, interner: &Interner) -> bool {
        self.types.iter().all(|atomic| atomic.is_truthy(interner))
    }

    pub fn is_always_falsy(&self) -> bool {
        self.types.iter().all(|atomic| atomic.is_falsy())
    }

    pub fn is_literal_of(&self, other: &TUnion) -> bool {
        for other_atomic_type in &other.types {
            if let TAtomic::TString = other_atomic_type {
                for self_atomic_type in &self.types {
                    if self_atomic_type.is_string_subtype() {
                        continue;
                    }

                    return false;
                }

                return true;
            } else if let TAtomic::TInt = other_atomic_type {
                for self_atomic_type in &self.types {
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
        self.types.iter().all(|atomic| match atomic {
            TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TStringWithFlags(_, _, true)
            | TAtomic::TEnumLiteralCase { .. }
            | TAtomic::TEnum { .. } => true,
            _ => false,
        })
    }

    pub fn has_static_object(&self) -> bool {
        self.types.iter().any(|atomic| match atomic {
            TAtomic::TNamedObject { is_this: true, .. } => true,
            _ => false,
        })
    }

    pub fn has_typealias(&self) -> bool {
        self.types.iter().any(|atomic| match atomic {
            TAtomic::TTypeAlias { .. } => true,
            _ => false,
        })
    }

    pub fn is_static_object(&self) -> bool {
        self.types.iter().all(|atomic| match atomic {
            TAtomic::TNamedObject { is_this: true, .. } => true,
            _ => false,
        })
    }

    pub fn get_key(&self) -> String {
        let mut tatomic_strings = self.types.iter().map(|atomic| atomic.get_key());
        tatomic_strings.join("|")
    }

    pub fn get_id(&self, interner: Option<&Interner>) -> String {
        let mut tatomic_strings = self.types.iter().map(|atomic| atomic.get_id(interner));
        tatomic_strings.join("|")
    }

    #[inline]
    pub fn is_single(&self) -> bool {
        self.types.len() == 1
    }

    #[inline]
    pub fn get_single(&self) -> &TAtomic {
        for atomic in &self.types {
            return atomic;
        }

        panic!()
    }

    #[inline]
    pub fn get_single_owned(self) -> TAtomic {
        self.types[0].to_owned()
    }

    #[inline]
    pub fn has_named_object(&self) -> bool {
        self.types
            .iter()
            .any(|t| matches!(t, TAtomic::TNamedObject { .. }))
    }

    #[inline]
    pub fn has_object(&self) -> bool {
        self.types
            .iter()
            .any(|t| matches!(t, TAtomic::TObject { .. }))
    }

    #[inline]
    pub fn has_object_type(&self) -> bool {
        self.types
            .iter()
            .any(|t| matches!(t, TAtomic::TObject | TAtomic::TNamedObject { .. }))
    }

    pub fn get_single_literal_int_value(&self) -> Option<i64> {
        if self.is_single() {
            self.get_single().get_literal_int_value()
        } else {
            None
        }
    }

    pub fn get_single_literal_string_value(&self, interner: &Interner) -> Option<String> {
        if self.is_single() {
            self.get_single().get_literal_string_value(interner)
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

    pub fn get_literal_ints(&self) -> Vec<&TAtomic> {
        self.types
            .iter()
            .filter(|a| matches!(a, TAtomic::TLiteralInt { .. }))
            .collect()
    }

    pub fn get_literal_strings(&self) -> Vec<&TAtomic> {
        self.types
            .iter()
            .filter(|a| matches!(a, TAtomic::TLiteralString { .. }))
            .collect()
    }

    pub fn get_literal_string_values(&self, interner: &Interner) -> Vec<Option<String>> {
        self.get_literal_strings()
            .into_iter()
            .map(|atom| match atom {
                TAtomic::TLiteralString { value, .. } => Some(value.clone()),
                TAtomic::TTypeAlias {
                    name,
                    as_type: Some(as_type),
                    type_params: Some(_),
                } => {
                    if name == &interner.get("HH\\Lib\\Regex\\Pattern").unwrap() {
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
            })
            .collect()
    }

    pub fn has_literal_value(&self) -> bool {
        self.types.iter().any(|atomic| match atomic {
            TAtomic::TLiteralInt { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TTrue { .. }
            | TAtomic::TFalse { .. }
            | TAtomic::TLiteralClassname { .. } => true,
            _ => false,
        })
    }

    pub fn has_taintable_value(&self) -> bool {
        self.types
            .iter()
            .any(|assignment_atomic_type| match assignment_atomic_type {
                TAtomic::TInt
                | TAtomic::TFloat
                | TAtomic::TNull
                | TAtomic::TLiteralClassname { .. }
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TLiteralString { .. }
                | TAtomic::TBool
                | TAtomic::TFalse
                | TAtomic::TMixedWithFlags(_, _, true, _)
                | TAtomic::TTrue
                | TAtomic::TEnum { .. }
                | TAtomic::TEnumLiteralCase { .. }
                | TAtomic::TNum => false,
                _ => true,
            })
    }

    pub fn needs_population(&self) -> bool {
        !self.populated && self.types.iter().any(|v| v.needs_population())
    }

    pub fn is_json_compatible(&self, banned_type_aliases: &Vec<&str>) -> bool {
        self.types
            .iter()
            .all(|t| t.is_json_compatible(banned_type_aliases))
    }

    pub fn get_all_references(&self) -> Vec<(StrId, Option<StrId>)> {
        let mut all_references = vec![];

        for atomic in &self.types {
            all_references.extend(atomic.get_all_references())
        }

        all_references
    }
}

impl PartialEq for TUnion {
    fn eq(&self, other: &TUnion) -> bool {
        let len = self.types.len();

        if len != other.types.len() {
            return false;
        }

        if len == 0 {
            if &self.types[0] != &other.types[0] {
                return false;
            }
        } else {
            for i in 0..len {
                let mut has_match = false;
                for j in 0..len {
                    if &self.types[i] == &other.types[j] {
                        has_match = true;
                        break;
                    }
                }
                if !has_match {
                    return false;
                }
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
        self.types.iter().map(|t| TypeNode::Atomic(t)).collect()
    }
}

pub fn populate_union_type(
    t_union: &mut self::TUnion,
    codebase_symbols: &Symbols,
    reference_source: &ReferenceSource,
    symbol_references: &mut SymbolReferences,
) {
    if t_union.populated {
        return;
    }

    t_union.populated = true;

    let ref mut types = t_union.types;

    for atomic in types.iter_mut() {
        if let TAtomic::TClassname { ref mut as_type }
        | TAtomic::TGenericClassname {
            ref mut as_type, ..
        } = atomic
        {
            let mut new_as_type = (**as_type).clone();
            populate_atomic_type(&mut new_as_type, codebase_symbols, reference_source, symbol_references);
            *as_type = Box::new(new_as_type);
        } else {
            populate_atomic_type(atomic, codebase_symbols, reference_source, symbol_references);
        }
    }
}
