use crate::{
    codebase_info::Symbols,
    data_flow::node::DataFlowNode,
    symbol_references::{ReferenceSource, SymbolReferences},
    t_atomic::{populate_atomic_type, DictKey, TAtomic},
};
use derivative::Derivative;
use hakana_str::{Interner, StrId};
use itertools::Itertools;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Derivative)]
pub struct TUnion {
    pub types: Vec<TAtomic>,
    pub parent_nodes: Vec<DataFlowNode>,
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

    pub populated: bool,
}

impl Hash for TUnion {
    // for hashing we only care about the types, not anything else
    fn hash<H: Hasher>(&self, state: &mut H) {
        for t in &self.types {
            t.hash(state);
        }
    }
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
            types,
            parent_nodes: vec![],
            had_template: false,
            reference_free: false,
            possibly_undefined_from_try: false,
            ignore_falsable_issues: false,
            from_template_default: false,
            populated: false,
        }
    }

    pub fn remove_type(&mut self, bad_type: &TAtomic) {
        self.types.retain(|t| t != bad_type);
    }

    pub fn is_int(&self) -> bool {
        for atomic in &self.types {
            let no_int = !matches!(atomic, TAtomic::TInt { .. } | TAtomic::TLiteralInt { .. });

            if no_int {
                return false;
            }
        }

        true
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

        false
    }

    pub fn has_float(&self) -> bool {
        for atomic in &self.types {
            if let TAtomic::TFloat { .. } = atomic {
                return true;
            };
        }

        false
    }

    pub fn is_arraykey(&self) -> bool {
        for atomic in &self.types {
            if !matches!(atomic, TAtomic::TArraykey { .. }) {
                return false;
            }
        }

        true
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

        false
    }

    pub fn is_float(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TFloat)
    }

    pub fn is_bool(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TBool)
    }

    pub fn is_nothing(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TNothing)
    }

    pub fn is_placeholder(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TPlaceholder)
    }

    pub fn is_true(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TTrue)
    }

    pub fn is_nonnull(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TMixedWithFlags(_, _, _, true))
    }

    pub fn is_any(&self) -> bool {
        self.types.len() == 1 && matches!(self.types[0], TAtomic::TMixedWithFlags(true, _, _, _))
    }

    pub fn is_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        matches!(
            self.types.first().unwrap(),
            TAtomic::TMixed | TAtomic::TMixedWithFlags(..) | TAtomic::TMixedFromLoopIsset
        )
    }

    pub fn is_mixed_with_any(&self, has_any: &mut bool) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        match &self.types[0] {
            &TAtomic::TMixedWithFlags(is_any, ..) => {
                *has_any = is_any;
                true
            }
            &TAtomic::TMixed | &TAtomic::TMixedFromLoopIsset => true,
            _ => false,
        }
    }

    pub fn is_nullable_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        match &self.types[0] {
            // eliminate truthy-mixed and nonnull
            &TAtomic::TMixedWithFlags(_, true, _, _) | &TAtomic::TMixedWithFlags(_, _, _, true) => {
                false
            }
            &TAtomic::TMixed | &TAtomic::TMixedFromLoopIsset | &TAtomic::TMixedWithFlags(..) => {
                true
            }
            _ => false,
        }
    }

    pub fn is_falsy_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        matches!(&self.types[0], &TAtomic::TMixedWithFlags(_, _, true, _))
    }

    pub fn is_vanilla_mixed(&self) -> bool {
        if self.types.len() != 1 {
            return false;
        }

        matches!(&self.types[0], TAtomic::TMixed)
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
                    for extra_type in extra_types {
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

            if let TAtomic::TNamedObject {
                extra_types: Some(extra_types),
                ..
            } = atomic
            {
                for extra_type in extra_types {
                    if let TAtomic::TGenericParam { .. } = extra_type {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn get_all_child_nodes(&self) -> Vec<TypeNode> {
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

        all_child_nodes
    }

    pub fn has_template_types(&self) -> bool {
        let all_child_nodes = self.get_all_child_nodes();

        for child_node in all_child_nodes {
            if let TypeNode::Atomic(
                TAtomic::TGenericParam { .. }
                | TAtomic::TGenericClassname { .. }
                | TAtomic::TGenericTypename { .. },
            ) = child_node
            {
                return true;
            }
        }

        false
    }

    pub fn has_awaitable_types(&self) -> bool {
        self.get_all_child_nodes()
            .iter()
            .any(|a| matches!(a, TypeNode::Atomic(TAtomic::TAwaitable { .. })))
    }

    pub fn get_template_types(&self) -> Vec<&TAtomic> {
        let all_child_nodes = self.get_all_child_nodes();

        let mut template_types = Vec::new();

        for child_node in all_child_nodes {
            if let TypeNode::Atomic(inner) = child_node {
                if let TAtomic::TGenericParam { .. }
                | TAtomic::TGenericClassname { .. }
                | TAtomic::TGenericTypename { .. } = inner
                {
                    template_types.push(inner);
                }
            }
        }

        template_types
    }

    pub fn is_objecty(&self) -> bool {
        for atomic in &self.types {
            if let &TAtomic::TObject { .. }
            | TAtomic::TNamedObject { .. }
            | TAtomic::TAwaitable { .. }
            | TAtomic::TClosure(_) = atomic
            {
                continue;
            }

            return false;
        }

        true
    }

    pub fn is_generator(&self) -> bool {
        for atomic in &self.types {
            if let &TAtomic::TNamedObject { name, .. } = &atomic {
                if *name == StrId::GENERATOR {
                    continue;
                }
            }

            return false;
        }

        true
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
        self.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TBool { .. } | TAtomic::TFalse { .. } | TAtomic::TTrue { .. }
            )
        })
    }

    pub fn has_scalar(&self) -> bool {
        self.types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TScalar { .. }))
    }

    pub fn is_always_truthy(&self) -> bool {
        self.types.iter().all(|atomic| atomic.is_truthy())
    }

    pub fn is_always_falsy(&self) -> bool {
        self.types.iter().all(|atomic| atomic.is_falsy())
    }

    pub fn is_literal_of(&self, other: &TUnion) -> bool {
        let other_atomic_type = other.types.first().unwrap();

        match other_atomic_type {
            TAtomic::TString => {
                for self_atomic_type in &self.types {
                    if self_atomic_type.is_string_subtype() {
                        continue;
                    }

                    return false;
                }

                true
            }
            TAtomic::TInt => {
                for self_atomic_type in &self.types {
                    if let TAtomic::TLiteralInt { .. } = self_atomic_type {
                        continue;
                    }

                    return false;
                }

                true
            }
            TAtomic::TEnum { name, .. } => {
                for self_atomic_type in &self.types {
                    if let TAtomic::TEnumLiteralCase { enum_name, .. } = self_atomic_type {
                        if enum_name == name {
                            continue;
                        }
                    }

                    return false;
                }

                true
            }
            _ => false,
        }
    }

    pub fn all_literals(&self) -> bool {
        self.types.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TLiteralString { .. }
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TStringWithFlags(_, _, true)
                    | TAtomic::TEnumLiteralCase { .. }
                    | TAtomic::TEnum { .. }
            )
        })
    }

    pub fn has_static_object(&self) -> bool {
        self.types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TNamedObject { is_this: true, .. }))
    }

    pub fn has_typealias(&self) -> bool {
        self.types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TTypeAlias { .. }))
    }

    pub fn is_static_object(&self) -> bool {
        self.types
            .iter()
            .all(|atomic| matches!(atomic, TAtomic::TNamedObject { is_this: true, .. }))
    }

    pub fn get_key(&self) -> String {
        let mut tatomic_strings = self.types.iter().map(|atomic| atomic.get_key());
        tatomic_strings.join("|")
    }

    pub fn get_id(&self, interner: Option<&Interner>) -> String {
        self.get_id_with_refs(interner, &mut vec![], None)
    }

    pub fn get_id_with_refs(
        &self,
        interner: Option<&Interner>,
        refs: &mut Vec<StrId>,
        indent: Option<usize>,
    ) -> String {
        if self.types.len() == 2 {
            match (&self.types[0], &self.types[1]) {
                (TAtomic::TNull, a) | (a, TAtomic::TNull) => {
                    format!("?{}", a.get_id_with_refs(interner, refs, indent))
                }
                (a, b) => {
                    format!(
                        "{}|{}",
                        a.get_id_with_refs(interner, refs, indent),
                        b.get_id_with_refs(interner, refs, indent)
                    )
                }
            }
        } else {
            let mut tatomic_strings = self
                .types
                .iter()
                .map(|atomic| atomic.get_id_with_refs(interner, refs, indent));
            tatomic_strings.join("|")
        }
    }

    #[inline]
    pub fn is_single(&self) -> bool {
        self.types.len() == 1
    }

    #[inline]
    pub fn get_single(&self) -> &TAtomic {
        &self.types[0]
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

    pub fn get_single_literal_string_value(&self) -> Option<String> {
        if self.is_single() {
            self.get_single().get_literal_string_value()
        } else {
            None
        }
    }

    pub fn get_single_dict_key(&self) -> Option<DictKey> {
        if self.is_single() {
            match self.get_single() {
                TAtomic::TLiteralInt { value, .. } => Some(DictKey::Int(*value as u64)),
                TAtomic::TLiteralString { value, .. } => Some(DictKey::String(value.clone())),
                TAtomic::TEnumLiteralCase {
                    enum_name,
                    member_name,
                    ..
                } => Some(DictKey::Enum(*enum_name, *member_name)),
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

    pub fn get_literal_string_values(&self) -> Vec<Option<String>> {
        self.get_literal_strings()
            .into_iter()
            .map(|atom| match atom {
                TAtomic::TLiteralString { value, .. } => Some(value.clone()),
                TAtomic::TTypeAlias {
                    name,
                    as_type: Some(as_type),
                    type_params: Some(_),
                } => {
                    if name == &StrId::LIB_REGEX_PATTERN {
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
            })
            .collect()
    }

    pub fn has_literal_value(&self) -> bool {
        self.types.iter().any(|atomic| {
            matches!(
                atomic,
                TAtomic::TLiteralInt { .. }
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TTrue { .. }
                    | TAtomic::TFalse { .. }
                    | TAtomic::TLiteralClassname { .. }
            )
        })
    }

    pub fn has_taintable_value(&self) -> bool {
        self.types.iter().any(|assignment_atomic_type| {
            !matches!(
                assignment_atomic_type,
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
                    | TAtomic::TNum
            )
        })
    }

    pub fn needs_population(&self) -> bool {
        !self.populated || self.types.iter().any(|v| v.needs_population())
    }

    pub fn is_json_compatible(&self, banned_type_aliases: &Vec<StrId>) -> bool {
        self.types
            .iter()
            .all(|t| t.is_json_compatible(banned_type_aliases))
    }

    pub fn generalize_literals(mut self) -> TUnion {
        let old_types = self.types.drain(..);

        let mut generalized_literals = vec![];

        let mut types = vec![];

        for t in old_types {
            match t {
                TAtomic::TLiteralString { .. } => {
                    if !generalized_literals
                        .iter()
                        .any(|t| matches!(t, TAtomic::TStringWithFlags(false, false, true)))
                    {
                        generalized_literals.push(TAtomic::TStringWithFlags(false, false, true));
                    }
                }
                TAtomic::TLiteralInt { .. } => {
                    if !generalized_literals
                        .iter()
                        .any(|t| matches!(t, TAtomic::TInt))
                    {
                        generalized_literals.push(TAtomic::TInt);
                    }
                }
                _ => {
                    types.push(t);
                }
            }
        }

        types.extend(generalized_literals);

        self.types = types;

        self
    }
}

impl PartialEq for TUnion {
    fn eq(&self, other: &TUnion) -> bool {
        let len = self.types.len();

        if len != other.types.len() {
            return false;
        }

        if len == 0 {
            if self.types[0] != other.types[0] {
                return false;
            }
        } else {
            for i in 0..len {
                let mut has_match = false;
                for j in 0..len {
                    if self.types[i] == other.types[j] {
                        has_match = true;
                        break;
                    }
                }
                if !has_match {
                    return false;
                }
            }
        }

        self.parent_nodes == other.parent_nodes
    }
}

impl HasTypeNodes for TUnion {
    fn get_child_nodes(&self) -> Vec<TypeNode> {
        self.types.iter().map(TypeNode::Atomic).collect()
    }
}

pub fn populate_union_type(
    t_union: &mut self::TUnion,
    codebase_symbols: &Symbols,
    reference_source: &ReferenceSource,
    symbol_references: &mut SymbolReferences,
    force: bool,
) {
    if t_union.populated && !force {
        return;
    }

    t_union.populated = true;

    let types = &mut t_union.types;

    for atomic in types.iter_mut() {
        if let TAtomic::TClassname { ref mut as_type }
        | TAtomic::TTypename { ref mut as_type }
        | TAtomic::TGenericClassname {
            ref mut as_type, ..
        }
        | TAtomic::TGenericTypename {
            ref mut as_type, ..
        } = atomic
        {
            let mut new_as_type = (**as_type).clone();
            populate_atomic_type(
                &mut new_as_type,
                codebase_symbols,
                reference_source,
                symbol_references,
                force,
            );
            *as_type = Box::new(new_as_type);
        } else {
            populate_atomic_type(
                atomic,
                codebase_symbols,
                reference_source,
                symbol_references,
                force,
            );
        }
    }
}
