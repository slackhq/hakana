use rustc_hash::FxHashMap;

pub use crate::functionlike_identifier::FunctionLikeIdentifier;
use crate::{symbol_references::ReferenceSource, StrId};

#[derive(Clone, Debug)]
pub struct FunctionContext {
    pub namespace: Option<String>,

    pub calling_class: Option<StrId>,

    pub aliased_namespaces: FxHashMap<String, String>,

    pub aliased_types: FxHashMap<String, String>,

    pub aliased_functions: FxHashMap<String, String>,

    pub aliased_constants: FxHashMap<String, String>,

    /**
     * Whether or not to do a deep analysis and collect initializations from private or final methods
     */
    pub collect_initializations: bool,

    /**
     * Whether or not to do a deep analysis and collect initializations from public non-final methods
     */
    pub collect_nonprivate_initializations: bool,

    /**
     * Stored to prevent re-analysing methods when checking for initialised properties
     *
     * @var array<string, bool>|null
     */
    pub initialized_methods: Option<FxHashMap<String, bool>>,

    pub is_static: bool,

    pub is_async: bool,

    pub calling_functionlike_id: Option<FunctionLikeIdentifier>,
    pub pure: bool,

    pub mutation_free: bool,

    pub external_mutation_free: bool,
    
}

impl FunctionContext {
    pub fn new() -> Self {
        Self {
            namespace: None,
            calling_class: None,
            aliased_namespaces: FxHashMap::default(),
            aliased_types: FxHashMap::default(),
            aliased_functions: FxHashMap::default(),
            aliased_constants: FxHashMap::default(),

            pure: false,
            mutation_free: false,
            external_mutation_free: false,
            collect_initializations: false,
            collect_nonprivate_initializations: false,
            initialized_methods: None,
            is_static: false,
            calling_functionlike_id: None,
            is_async: false,
        }
    }

    pub fn get_reference_source(&self, file_path: &StrId) -> ReferenceSource {
        if let Some(calling_functionlike_id) = &self.calling_functionlike_id {
            match calling_functionlike_id {
                FunctionLikeIdentifier::Function(name) => ReferenceSource::Symbol(false, *name),
                FunctionLikeIdentifier::Method(a, b) => {
                    ReferenceSource::ClasslikeMember(false, *a, *b)
                }
            }
        } else {
            ReferenceSource::Symbol(false, *file_path)
        }
    }
}
