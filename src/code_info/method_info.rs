use rustc_hash::FxHashSet;

use serde::{Deserialize, Serialize};

use crate::member_visibility::MemberVisibility;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    pub is_static: bool,

    pub visibility: MemberVisibility,

    pub is_final: bool,

    pub is_abstract: bool,

    pub overridden_downstream: bool,

    pub overridden_somewhere: bool,

    pub defining_fqcln: Option<String>,

    pub external_mutation_free: bool,

    pub immutable: bool,

    pub mutation_free_inferred: bool,

    pub this_property_mutations: Option<FxHashSet<String>>,

    pub stubbed: bool,

    pub probably_fluent: bool,
}

impl MethodInfo {
    pub fn new() -> Self {
        Self {
            is_static: false,
            visibility: MemberVisibility::Public,
            is_final: false,
            is_abstract: false,
            overridden_downstream: false,
            overridden_somewhere: false,
            defining_fqcln: None,
            external_mutation_free: false,
            immutable: false,
            mutation_free_inferred: false,
            this_property_mutations: None,
            stubbed: false,
            probably_fluent: false,
        }
    }
}
