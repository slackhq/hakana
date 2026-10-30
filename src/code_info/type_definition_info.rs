use std::sync::Arc;

use hakana_str::StrId;
use rustc_hash::FxHashMap;

use serde::{Deserialize, Serialize};

use crate::{
    GenericParent,
    attribute_info::AttributeInfo,
    classlike_info::Variance,
    code_location::{FilePath, HPos},
    t_atomic::DictKey,
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
}
