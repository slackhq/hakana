use std::sync::{Arc, OnceLock};

use hakana_str::StrId;
use rustc_hash::FxHashMap;

use serde::{Deserialize, Serialize};

use crate::{
    GenericParent,
    attribute_info::AttributeInfo,
    classlike_info::Variance,
    code_location::{FilePath, HPos},
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
    taint::SourceType,
};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "memory-profile", derive(allocative::Allocative))]
pub struct TypeDefinitionInfo {
    pub newtype_file: Option<FilePath>,
    pub as_type: Option<TUnion>,
    pub actual_type: TUnion,

    /**
     * An array holding the function template "as" types.
     *
     * It's the de-facto list of all templates on a given function.
     *
     * The name of the template is the first key. The nested array is keyed by a unique
     * function identifier. This allows operations with the same-named template defined
     * across multiple classes and/or functions to not run into trouble.
     */
    pub template_types: Vec<(StrId, Vec<(GenericParent, Arc<TUnion>)>)>,

    #[cfg_attr(feature = "memory-profile", allocative(visit = hakana_str::memory_profile::hash_map))]
    pub generic_variance: FxHashMap<usize, Variance>,

    #[cfg_attr(feature = "memory-profile", allocative(visit = hakana_str::memory_profile::option_hash_map))]
    pub shape_field_taints: Option<FxHashMap<DictKey, (HPos, Vec<SourceType>)>>,

    pub is_literal_string: bool,
    pub location: HPos,
    pub user_defined: bool,
    pub generated: bool,

    pub attributes: Vec<AttributeInfo>,

    /// What this alias expands to, worked out on first use during analysis.
    /// Derived from the whole codebase, so it is neither cached on disk nor
    /// copied, and `populate_codebase` clears it whenever the codebase changes.
    #[serde(skip)]
    #[cfg_attr(feature = "memory-profile", allocative(skip))]
    pub expansions: AliasExpansions,
}

/// What a non-generic type alias expanded to (see `type_expander`).
#[derive(Debug)]
pub struct ExpandedAlias {
    pub types: Vec<TAtomic>,
    /// What the expansion added to the caller's cost counter
    pub cost: u32,
    /// Files of the newtypes that stayed opaque in `types`. Inside those
    /// files they are transparent, so this expansion must not be used there.
    pub opaque_newtype_files: Vec<FilePath>,
}

/// One remembered expansion per combination of the expansion options that
/// can change the result.
#[derive(Debug, Default)]
pub struct AliasExpansions {
    slots: [OnceLock<Box<ExpandedAlias>>; 4],
}

impl AliasExpansions {
    pub fn get(&self, slot: usize) -> Option<&ExpandedAlias> {
        self.slots[slot].get().map(|expanded| &**expanded)
    }

    /// Keeps the first expansion offered: threads that race here have all
    /// computed the same thing.
    pub fn set(&self, slot: usize, expanded: ExpandedAlias) {
        let _ = self.slots[slot].set(Box::new(expanded));
    }
}

impl Clone for AliasExpansions {
    fn clone(&self) -> Self {
        Self::default()
    }
}
